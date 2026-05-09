// SPDX-License-Identifier: GPL-3.0-only

use std::env;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use miniquad::{EventHandler, KeyCode, KeyMods, PassAction, RenderingBackend, conf, window};

use super::controller::{Controller, ControllerAction, ControllerEvent};
use crate::desktop_entry::{APP_ID, APP_NAME};

const LOG_FILE_NAME: &str = "dream-ini-portmaster.log";

type SharedLog = Arc<Mutex<File>>;

pub(crate) fn run() -> ExitCode {
    let log = open_log().map(Mutex::new).map(Arc::new);
    install_panic_hook(log.clone());
    log_startup(log.as_ref());

    let options = conf::Conf {
        window_title: format!("{APP_NAME} PortMaster Probe"),
        window_width: 854,
        window_height: 480,
        fullscreen: true,
        platform: conf::Platform {
            linux_wm_class: APP_ID,
            ..Default::default()
        },
        ..Default::default()
    };

    miniquad::start(options, move || {
        let mut app = ProbeApp::new(log);
        app.log("miniquad probe started");
        Box::new(app)
    });

    ExitCode::SUCCESS
}

struct ProbeApp {
    graphics: Box<dyn RenderingBackend>,
    controller: Controller,
    log: Option<SharedLog>,
    input_count: u64,
    last_action: &'static str,
}

impl ProbeApp {
    fn new(log: Option<SharedLog>) -> Self {
        Self {
            graphics: window::new_rendering_backend(),
            controller: Controller::new(egui::Context::default()),
            log,
            input_count: 0,
            last_action: "startup",
        }
    }

    fn log(&mut self, message: impl AsRef<str>) {
        write_log(self.log.as_ref(), message);
    }

    fn record_action(&mut self, source: &str, action: ControllerAction) {
        self.input_count = self.input_count.saturating_add(1);
        self.last_action = action_name(action);
        self.log(format!(
            "input source={source} action={} count={}",
            self.last_action, self.input_count
        ));
        if action == ControllerAction::Cancel {
            self.log("quit requested by cancel action");
            window::request_quit();
        }
    }

    fn record_key(&mut self, keycode: KeyCode, repeat: bool) {
        if repeat {
            return;
        }
        let Some(action) = key_action(keycode) else {
            self.log(format!("key ignored keycode={keycode:?}"));
            return;
        };
        self.record_action("keyboard", action);
    }
}

impl EventHandler for ProbeApp {
    fn update(&mut self) {
        for event in self.controller.drain_events() {
            match event {
                ControllerEvent::Action(action) => self.record_action("controller", action),
                ControllerEvent::Available(available) => {
                    self.log(format!(
                        "controller availability changed available={available}"
                    ));
                }
                ControllerEvent::PurgeQueuedActions => self.log("controller purge queued actions"),
            }
        }
    }

    fn draw(&mut self) {
        let pulse = match self.input_count % 6 {
            0 => 0.0,
            1 => 1.0 / 6.0,
            2 => 2.0 / 6.0,
            3 => 3.0 / 6.0,
            4 => 4.0 / 6.0,
            _ => 5.0 / 6.0,
        };
        let color = match self.last_action {
            "up" => (0.10, 0.22 + pulse, 0.55, 1.0),
            "down" => (0.12, 0.42, 0.20 + pulse, 1.0),
            "left" => (0.35 + pulse, 0.12, 0.32, 1.0),
            "right" => (0.55, 0.30 + pulse, 0.08, 1.0),
            "accept" => (0.08, 0.45 + pulse, 0.52, 1.0),
            "cancel" => (0.52 + pulse, 0.06, 0.06, 1.0),
            _ => (0.08, 0.04, 0.18 + pulse, 1.0),
        };
        self.graphics
            .begin_default_pass(PassAction::clear_color(color.0, color.1, color.2, color.3));
        self.graphics.end_render_pass();
        self.graphics.commit_frame();
    }

    fn key_down_event(&mut self, keycode: KeyCode, _keymods: KeyMods, repeat: bool) {
        self.log(format!("key down keycode={keycode:?} repeat={repeat}"));
        self.record_key(keycode, repeat);
    }

    fn resize_event(&mut self, width: f32, height: f32) {
        self.log(format!("resize width={width} height={height}"));
    }

    fn quit_requested_event(&mut self) {
        self.log("quit requested by window system");
    }
}

fn key_action(keycode: KeyCode) -> Option<ControllerAction> {
    match keycode {
        KeyCode::Up => Some(ControllerAction::Up),
        KeyCode::Down => Some(ControllerAction::Down),
        KeyCode::Left => Some(ControllerAction::Left),
        KeyCode::Right => Some(ControllerAction::Right),
        KeyCode::Enter | KeyCode::KpEnter | KeyCode::Space => Some(ControllerAction::Accept),
        KeyCode::Escape | KeyCode::Back => Some(ControllerAction::Cancel),
        KeyCode::Backspace => Some(ControllerAction::ClearCurrent),
        _ => None,
    }
}

const fn action_name(action: ControllerAction) -> &'static str {
    match action {
        ControllerAction::Up => "up",
        ControllerAction::Down => "down",
        ControllerAction::Left => "left",
        ControllerAction::Right => "right",
        ControllerAction::Accept => "accept",
        ControllerAction::Cancel => "cancel",
        ControllerAction::ClearCurrent => "clear-current",
        ControllerAction::SelectCurrent => "select-current",
        ControllerAction::ToggleHiddenDirectories => "toggle-hidden-directories",
        ControllerAction::PagePreviewDown => "page-preview-down",
        ControllerAction::ScrollPreviewLeft => "scroll-preview-left",
        ControllerAction::ScrollPreviewRight => "scroll-preview-right",
        ControllerAction::ScrollPreviewUp => "scroll-preview-up",
        ControllerAction::ScrollPreviewDown => "scroll-preview-down",
    }
}

fn open_log() -> Option<File> {
    let paths = log_paths();
    for path in paths {
        match OpenOptions::new().create(true).append(true).open(&path) {
            Ok(file) => return Some(file),
            Err(error) => eprintln!(
                "failed to open PortMaster log at {}: {error}",
                path.display()
            ),
        }
    }
    None
}

fn log_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(exe) = env::current_exe()
        && let Some(parent) = exe.parent()
    {
        paths.push(parent.join(LOG_FILE_NAME));
    }
    if let Ok(cwd) = env::current_dir() {
        paths.push(cwd.join(LOG_FILE_NAME));
    }
    paths.push(PathBuf::from(LOG_FILE_NAME));
    paths
}

fn install_panic_hook(log: Option<SharedLog>) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        write_log(log.as_ref(), format!("panic: {panic_info}"));
        previous(panic_info);
    }));
}

fn log_startup(log: Option<&SharedLog>) {
    write_log(log, "startup compile_feature=portmaster-gui");
    write_log(
        log,
        format!("argv={:?}", env::args_os().collect::<Vec<_>>()),
    );
    write_log(log, format!("cwd={:?}", env::current_dir()));
    write_log(log, format!("current_exe={:?}", env::current_exe()));
    write_log(log, format!("unix_timestamp={}", unix_timestamp()));
}

fn write_log(log: Option<&SharedLog>, message: impl AsRef<str>) {
    let Some(log) = log else {
        return;
    };
    if let Ok(mut file) = log.lock() {
        let _ = writeln!(file, "{} {}", unix_timestamp(), message.as_ref());
        let _ = file.flush();
    }
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}
