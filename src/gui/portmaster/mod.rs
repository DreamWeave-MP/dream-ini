// SPDX-License-Identifier: GPL-3.0-only

use std::env;
use std::io;
use std::process::ExitCode;
use std::sync::{Arc, Mutex};
#[cfg(target_os = "linux")]
use std::time::{Duration, Instant};

#[cfg(target_os = "linux")]
mod fbdev;
mod log;
#[cfg(target_os = "linux")]
mod pacing;
#[cfg(target_os = "linux")]
mod raster;
#[cfg(target_os = "linux")]
mod renderer;
#[cfg(target_os = "linux")]
mod shell;
#[cfg(target_os = "linux")]
mod surface;
#[cfg(target_os = "linux")]
mod texture;

#[cfg(target_os = "linux")]
use super::{GuiApp, GuiShell};
#[cfg(target_os = "linux")]
use fbdev::{DRAW_ENV_VAR, Framebuffer, FramebufferSnapshot, framebuffer_draw_enabled};
use log::{SharedLog, install_panic_hook, log_startup, open_log, write_log};
#[cfg(target_os = "linux")]
use pacing::{
    FrameScheduleAction, IDLE_POLL_INTERVAL, REFRESH_ENV_VAR, earliest_repaint_deadline,
    frame_schedule_action, repaint_deadline, select_refresh_rate, sleep_for_frame_schedule,
};
#[cfg(target_os = "linux")]
use renderer::SoftwareRenderer;
#[cfg(target_os = "linux")]
use shell::PortMasterGuiShell;

pub(crate) fn run() -> ExitCode {
    let log = open_log().map(Mutex::new).map(Arc::new);
    install_panic_hook(log.clone());
    log_startup(log.as_ref());

    match run_gui(log.as_ref()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            write_log(log.as_ref(), format!("fatal error: {error}"));
            ExitCode::FAILURE
        }
    }
}

#[cfg(target_os = "linux")]
fn run_gui(log: Option<&SharedLog>) -> io::Result<()> {
    let mut framebuffer = Framebuffer::open()?;
    framebuffer.log_info(log);
    framebuffer.validate_format()?;
    let draw_enabled = framebuffer_draw_enabled();
    write_log(
        log,
        format!(
            "framebuffer drawing enabled={draw_enabled} env_{}={:?}",
            DRAW_ENV_VAR,
            env::var_os(DRAW_ENV_VAR)
        ),
    );
    if !draw_enabled {
        write_log(
            log,
            "framebuffer drawing disabled; exiting without launching GUI",
        );
        return Ok(());
    }
    let refresh_env_raw = env::var_os(REFRESH_ENV_VAR);
    let selected_refresh = select_refresh_rate(
        refresh_env_raw.as_deref().and_then(std::ffi::OsStr::to_str),
        framebuffer.refresh_timing(),
    );
    let frame_interval = selected_refresh.frame_interval();
    write_log(
        log,
        format!(
            "framebuffer refresh env_{}={:?} source={} hz={} interval={:?}",
            REFRESH_ENV_VAR,
            refresh_env_raw,
            selected_refresh.source.as_str(),
            selected_refresh.hz(),
            frame_interval
        ),
    );

    let egui_context = egui::Context::default();
    let pending_repaint_delay = Arc::new(Mutex::new(None));
    egui_context.set_request_repaint_callback({
        let pending_repaint_delay = Arc::clone(&pending_repaint_delay);
        move |info| {
            if info.viewport_id != egui::ViewportId::ROOT {
                return;
            }
            if let Ok(mut pending) = pending_repaint_delay.lock() {
                *pending =
                    Some(pending.map_or(info.delay, |delay: Duration| delay.min(info.delay)));
            }
        }
    });
    let mut app = GuiApp::new(egui_context.clone());
    let mut shell = PortMasterGuiShell::new(log);
    write_log(log, "shared GUI and controller worker started");

    let mut frame_count = 0_u64;
    let mut snapshots = Vec::new();
    let mut renderer = SoftwareRenderer::default();
    let mut gui_error = None;
    let mut next_repaint_at = Some(Instant::now());
    let mut skipped_idle_polls = 0_u64;
    let exit_reason = 'gui: loop {
        let now = Instant::now();
        if let Some(requested_delay) = take_pending_repaint_delay(&pending_repaint_delay) {
            next_repaint_at =
                earliest_repaint_deadline(next_repaint_at, repaint_deadline(now, requested_delay));
        }
        let requested_repaint_now = egui_context.has_requested_repaint()
            && next_repaint_at.is_some_and(|deadline| deadline <= now);
        match frame_schedule_action(
            now,
            next_repaint_at,
            requested_repaint_now,
            IDLE_POLL_INTERVAL,
        ) {
            FrameScheduleAction::Draw => {}
            FrameScheduleAction::Sleep(duration) => {
                skipped_idle_polls = skipped_idle_polls.saturating_add(1);
                if skipped_idle_polls == 1 || skipped_idle_polls.is_multiple_of(600) {
                    write_log(
                        log,
                        format!(
                            "framebuffer idle poll skipped_render_count={skipped_idle_polls} sleep_for={duration:?} next_repaint_at={next_repaint_at:?} has_requested_repaint={}",
                            egui_context.has_requested_repaint()
                        ),
                    );
                }
                sleep_for_frame_schedule(duration);
                continue 'gui;
            }
        }

        skipped_idle_polls = 0;
        let frame_started_at = Instant::now();
        let log_frame = frame_count == 0 || frame_count.is_multiple_of(30);
        let mut frame = GuiFrame {
            context: &egui_context,
            app: &mut app,
            shell: &mut shell,
            snapshots: &mut snapshots,
            log,
            log_frame,
        };
        let draw_outcome = match framebuffer.draw_egui_gui(&mut renderer, &mut frame) {
            Ok(outcome) => outcome,
            Err(error) => {
                write_log(log, format!("draw failed: {error}"));
                gui_error = Some(error);
                break 'gui "draw-error";
            }
        };
        frame_count = frame_count.saturating_add(1);
        let frame_finished_at = Instant::now();
        let pending_during_frame = take_pending_repaint_delay(&pending_repaint_delay);
        next_repaint_at = next_frame_repaint_deadline(
            frame_started_at,
            frame_finished_at,
            draw_outcome.repaint_delay,
            frame_interval,
        );
        if draw_outcome.repaint_delay != Duration::ZERO {
            if let Some(requested_delay) = pending_during_frame {
                next_repaint_at = earliest_repaint_deadline(
                    next_repaint_at,
                    repaint_deadline(frame_finished_at, requested_delay),
                );
            }
        } else if let Some(requested_delay) = pending_during_frame
            && requested_delay != Duration::ZERO
        {
            next_repaint_at = earliest_repaint_deadline(
                next_repaint_at,
                repaint_deadline(frame_finished_at, requested_delay),
            );
        }

        if shell.exit_requested() {
            write_log(log, "quit requested by GUI shell");
            break 'gui "exit-requested";
        }
    };

    write_log(
        log,
        format!("leaving framebuffer GUI reason={exit_reason} frames={frame_count}"),
    );
    let restore_result = framebuffer.restore_snapshots(&snapshots, log);
    if let Err(error) = &restore_result {
        write_log(log, format!("framebuffer restore failed: {error}"));
    }
    write_log(log, "dropping shared GUI and controller worker");
    drop(app);
    write_log(log, "controller worker dropped");
    write_log(log, "framebuffer GUI complete");
    if let Some(error) = gui_error {
        Err(error)
    } else {
        restore_result
    }
}

#[cfg(target_os = "linux")]
fn take_pending_repaint_delay(pending_repaint_delay: &Mutex<Option<Duration>>) -> Option<Duration> {
    pending_repaint_delay
        .lock()
        .map(|mut pending| pending.take())
        .unwrap_or(None)
}

#[cfg(target_os = "linux")]
fn next_frame_repaint_deadline(
    frame_started_at: Instant,
    frame_finished_at: Instant,
    repaint_delay: Duration,
    frame_interval: Duration,
) -> Option<Instant> {
    if repaint_delay.is_zero() {
        Some(
            frame_started_at
                .checked_add(frame_interval)
                .unwrap_or(frame_finished_at),
        )
    } else {
        repaint_deadline(frame_finished_at, repaint_delay)
    }
}

#[cfg(target_os = "linux")]
struct GuiFrame<'a, S: GuiShell> {
    context: &'a egui::Context,
    app: &'a mut GuiApp,
    shell: &'a mut S,
    snapshots: &'a mut Vec<FramebufferSnapshot>,
    log: Option<&'a SharedLog>,
    log_frame: bool,
}

#[cfg(not(target_os = "linux"))]
fn run_gui(log: Option<&SharedLog>) -> io::Result<()> {
    let message = "PortMaster framebuffer GUI is only supported on Linux";
    write_log(log, message);
    eprintln!("{message}");
    Err(io::Error::other(message))
}
