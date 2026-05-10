// SPDX-License-Identifier: GPL-3.0-only

use std::env;
use std::io;
use std::process::ExitCode;
use std::sync::{Arc, Mutex};
#[cfg(target_os = "linux")]
use std::time::Instant;

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
use pacing::{REFRESH_ENV_VAR, select_refresh_rate, sleep_after_frame};
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
    let mut app = GuiApp::new(egui_context.clone());
    let mut shell = PortMasterGuiShell::new(log);
    write_log(log, "shared GUI and controller worker started");

    let mut frame_count = 0_u64;
    let mut snapshots = Vec::new();
    let mut renderer = SoftwareRenderer::default();
    let mut gui_error = None;
    let mut next_frame_at = Instant::now();
    let exit_reason = 'gui: loop {
        let log_frame = frame_count == 0 || frame_count.is_multiple_of(30);
        let mut frame = GuiFrame {
            context: &egui_context,
            app: &mut app,
            shell: &mut shell,
            snapshots: &mut snapshots,
            log,
            log_frame,
        };
        if let Err(error) = framebuffer.draw_egui_gui(&mut renderer, &mut frame) {
            write_log(log, format!("draw failed: {error}"));
            gui_error = Some(error);
            break 'gui "draw-error";
        }
        frame_count = frame_count.saturating_add(1);

        if shell.exit_requested() {
            write_log(log, "quit requested by GUI shell");
            break 'gui "exit-requested";
        }

        next_frame_at = sleep_after_frame(Instant::now(), next_frame_at, frame_interval);
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
