// SPDX-License-Identifier: GPL-3.0-only

#[cfg(target_os = "linux")]
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
use fbdev::{
    DRAW_ENV_VAR, Framebuffer, FramebufferDrawOutcome, FramebufferSnapshot,
    framebuffer_draw_enabled,
};
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
    if !framebuffer_drawing_enabled(log) {
        write_log(
            log,
            "framebuffer drawing disabled; exiting without launching GUI",
        );
        return Ok(());
    }

    let frame_interval = log_selected_refresh_rate(log, &framebuffer);
    let mut runtime = FramebufferGuiRuntime::new(log, frame_interval);
    write_log(log, "shared GUI and controller worker started");

    let loop_outcome = runtime.run_loop(&mut framebuffer);

    write_log(
        log,
        format!(
            "leaving framebuffer GUI reason={} frames={}",
            loop_outcome.exit_reason, loop_outcome.frame_count
        ),
    );
    let restore_result = framebuffer.restore_snapshots(&runtime.snapshots, log);
    if let Err(error) = &restore_result {
        write_log(log, format!("framebuffer restore failed: {error}"));
    }
    write_log(log, "dropping shared GUI and controller worker");
    drop(runtime);
    write_log(log, "controller worker dropped");
    write_log(log, "framebuffer GUI complete");
    if let Some(error) = loop_outcome.gui_error {
        Err(error)
    } else {
        restore_result
    }
}

#[cfg(target_os = "linux")]
fn framebuffer_drawing_enabled(log: Option<&SharedLog>) -> bool {
    let draw_enabled = framebuffer_draw_enabled();
    write_log(
        log,
        format!(
            "framebuffer drawing enabled={draw_enabled} env_{}={:?}",
            DRAW_ENV_VAR,
            env::var_os(DRAW_ENV_VAR)
        ),
    );
    draw_enabled
}

#[cfg(target_os = "linux")]
fn log_selected_refresh_rate(log: Option<&SharedLog>, framebuffer: &Framebuffer) -> Duration {
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
    frame_interval
}

#[cfg(target_os = "linux")]
struct FramebufferGuiRuntime<'a> {
    egui_context: egui::Context,
    pending_repaint_delay: Arc<Mutex<Option<Duration>>>,
    app: GuiApp,
    shell: PortMasterGuiShell,
    snapshots: Vec<FramebufferSnapshot>,
    renderer: SoftwareRenderer,
    log: Option<&'a SharedLog>,
    frame_interval: Duration,
}

#[cfg(target_os = "linux")]
impl<'a> FramebufferGuiRuntime<'a> {
    fn new(log: Option<&'a SharedLog>, frame_interval: Duration) -> Self {
        let egui_context = egui::Context::default();
        let pending_repaint_delay = Arc::new(Mutex::new(None));
        install_repaint_callback(&egui_context, &pending_repaint_delay);
        let app = GuiApp::new(egui_context.clone());
        let shell = PortMasterGuiShell::new(log);
        Self {
            egui_context,
            pending_repaint_delay,
            app,
            shell,
            snapshots: Vec::new(),
            renderer: SoftwareRenderer::default(),
            log,
            frame_interval,
        }
    }

    fn run_loop(&mut self, framebuffer: &mut Framebuffer) -> GuiLoopOutcome {
        let mut frame_count = 0_u64;
        let mut next_repaint_at = Some(Instant::now());
        let mut skipped_idle_polls = 0_u64;
        let mut gui_error = None;
        let exit_reason = 'gui: loop {
            let now = Instant::now();
            if let Some(requested_delay) = take_pending_repaint_delay(&self.pending_repaint_delay) {
                next_repaint_at = earliest_repaint_deadline(
                    next_repaint_at,
                    repaint_deadline(now, requested_delay),
                );
            }
            let requested_repaint_now = self.egui_context.has_requested_repaint()
                && next_repaint_at.is_some_and(|deadline| deadline <= now);
            match frame_schedule_action(
                now,
                next_repaint_at,
                requested_repaint_now,
                IDLE_POLL_INTERVAL,
            ) {
                FrameScheduleAction::Draw => {}
                FrameScheduleAction::Sleep(duration) => {
                    skipped_idle_polls =
                        self.sleep_until_next_frame(duration, skipped_idle_polls, next_repaint_at);
                    continue 'gui;
                }
            }

            skipped_idle_polls = 0;
            let frame_started_at = Instant::now();
            match self.draw_frame(framebuffer, frame_count) {
                Ok(draw_outcome) => {
                    frame_count = frame_count.saturating_add(1);
                    next_repaint_at = self.next_repaint_after_frame(
                        frame_started_at,
                        Instant::now(),
                        draw_outcome.repaint_delay,
                    );
                }
                Err(error) => {
                    write_log(self.log, format!("draw failed: {error}"));
                    gui_error = Some(error);
                    break 'gui "draw-error";
                }
            }

            if self.shell.exit_requested() {
                write_log(self.log, "quit requested by GUI shell");
                break 'gui "exit-requested";
            }
        };

        GuiLoopOutcome {
            exit_reason,
            frame_count,
            gui_error,
        }
    }

    fn sleep_until_next_frame(
        &self,
        duration: Duration,
        skipped_idle_polls: u64,
        next_repaint_at: Option<Instant>,
    ) -> u64 {
        let skipped_idle_polls = skipped_idle_polls.saturating_add(1);
        if skipped_idle_polls == 1 || skipped_idle_polls.is_multiple_of(600) {
            write_log(
                self.log,
                format!(
                    "framebuffer idle poll skipped_render_count={skipped_idle_polls} sleep_for={duration:?} next_repaint_at={next_repaint_at:?} has_requested_repaint={}",
                    self.egui_context.has_requested_repaint()
                ),
            );
        }
        sleep_for_frame_schedule(duration);
        skipped_idle_polls
    }

    fn draw_frame(
        &mut self,
        framebuffer: &mut Framebuffer,
        frame_count: u64,
    ) -> io::Result<FramebufferDrawOutcome> {
        let log_frame = frame_count == 0 || frame_count.is_multiple_of(30);
        let mut frame = GuiFrame {
            context: &self.egui_context,
            app: &mut self.app,
            shell: &mut self.shell,
            snapshots: &mut self.snapshots,
            log: self.log,
            log_frame,
        };
        framebuffer.draw_egui_gui(&mut self.renderer, &mut frame)
    }

    fn next_repaint_after_frame(
        &self,
        frame_started_at: Instant,
        frame_finished_at: Instant,
        repaint_delay: Duration,
    ) -> Option<Instant> {
        let pending_during_frame = take_pending_repaint_delay(&self.pending_repaint_delay);
        let mut next_repaint_at = next_frame_repaint_deadline(
            frame_started_at,
            frame_finished_at,
            repaint_delay,
            self.frame_interval,
        );
        if repaint_delay != Duration::ZERO {
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
        next_repaint_at
    }
}

#[cfg(target_os = "linux")]
struct GuiLoopOutcome {
    exit_reason: &'static str,
    frame_count: u64,
    gui_error: Option<io::Error>,
}

#[cfg(target_os = "linux")]
fn install_repaint_callback(
    egui_context: &egui::Context,
    pending_repaint_delay: &Arc<Mutex<Option<Duration>>>,
) {
    egui_context.set_request_repaint_callback({
        let pending_repaint_delay = Arc::clone(pending_repaint_delay);
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
}

#[cfg(target_os = "linux")]
fn take_pending_repaint_delay(pending_repaint_delay: &Mutex<Option<Duration>>) -> Option<Duration> {
    pending_repaint_delay
        .lock()
        .map_or(None, |mut pending| pending.take())
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
