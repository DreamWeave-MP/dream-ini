use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use eframe::egui;
use gilrs::{Axis, Button, Event, EventType, GamepadId, Gilrs};

use super::common::{ActionRepeater, AxisDirection, InputActions};
use super::{ControllerAction, ControllerEvent, ControllerEventSender};

const GILRS_POLL_INTERVAL: Duration = Duration::from_millis(8);
const GILRS_RETRY_INTERVAL: Duration = Duration::from_millis(500);
const MAX_GILRS_EVENTS_PER_TICK: usize = 64;
const MAX_WORKER_ACTIONS_PER_TICK: usize = 32;
const STICK_DEADZONE: f32 = 0.5;

pub(super) struct ControllerWorker {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for ControllerWorker {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ControllerWorker")
            .field("stop_requested", &self.stop.load(Ordering::Relaxed))
            .field("running", &self.handle.is_some())
            .finish()
    }
}

impl ControllerWorker {
    pub(super) fn spawn(sender: ControllerEventSender, context: egui::Context) -> Option<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let handle = thread::Builder::new()
            .name("dream-ini-gilrs-controller".to_owned())
            .spawn(move || run_worker(&sender, &context, &worker_stop))
            .ok()?;

        Some(Self {
            stop,
            handle: Some(handle),
        })
    }
}

impl Drop for ControllerWorker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            handle.thread().unpark();
            let _ = handle.join();
        }
    }
}

struct WorkerState {
    gilrs: Option<Gilrs>,
    gamepads: HashMap<GamepadId, GamepadInputState>,
}

impl WorkerState {
    fn new() -> Self {
        Self {
            gilrs: Gilrs::new().ok(),
            gamepads: HashMap::new(),
        }
    }

    fn poll(&mut self) -> InputActions {
        let Some(gilrs) = &mut self.gilrs else {
            self.gilrs = Gilrs::new().ok();
            thread::park_timeout(GILRS_RETRY_INTERVAL);
            return InputActions::default();
        };

        let now = Instant::now();
        let mut input = InputActions::default();
        for _ in 0..MAX_GILRS_EVENTS_PER_TICK {
            let Some(event) = gilrs.next_event() else {
                break;
            };
            input.extend(self.handle_event(event, now));
            if input.actions.len() >= MAX_WORKER_ACTIONS_PER_TICK {
                break;
            }
        }
        for gamepad in self.gamepads.values_mut() {
            input.actions.extend(gamepad.repeater.poll(now));
            if input.actions.len() >= MAX_WORKER_ACTIONS_PER_TICK {
                break;
            }
        }
        input.actions.truncate(MAX_WORKER_ACTIONS_PER_TICK);
        input
    }

    fn has_gamepads(&self) -> bool {
        self.gilrs
            .as_ref()
            .is_some_and(|gilrs| gilrs.gamepads().any(|(_, gamepad)| gamepad.is_connected()))
    }

    fn handle_event(&mut self, event: Event, now: Instant) -> InputActions {
        match event.event {
            EventType::ButtonPressed(button, _) => {
                let gamepad = self.gamepad_state(event.id);
                button_pressed(button, &mut gamepad.repeater, now)
            }
            EventType::Connected => InputActions::default(),
            EventType::Disconnected => {
                self.gamepads.remove(&event.id);
                InputActions::released()
            }
            EventType::ButtonReleased(button, _) => {
                if let Some(action) = repeatable_button_action(button) {
                    let gamepad = self.gamepad_state(event.id);
                    gamepad.repeater.stop(action);
                    return InputActions::released();
                }
                InputActions::default()
            }
            EventType::AxisChanged(Axis::LeftStickX, value, _) => {
                let gamepad = self.gamepad_state(event.id);
                gamepad.axes.left_x.update(
                    stick_direction(value),
                    horizontal_action,
                    &mut gamepad.repeater,
                    now,
                )
            }
            EventType::AxisChanged(Axis::LeftStickY, value, _) => {
                let gamepad = self.gamepad_state(event.id);
                gamepad.axes.left_y.update(
                    stick_direction(value),
                    vertical_action,
                    &mut gamepad.repeater,
                    now,
                )
            }
            EventType::AxisChanged(Axis::RightStickX, value, _) => {
                let gamepad = self.gamepad_state(event.id);
                gamepad.axes.right_x.update(
                    stick_direction(value),
                    preview_horizontal_scroll_action,
                    &mut gamepad.repeater,
                    now,
                )
            }
            EventType::AxisChanged(Axis::RightStickY, value, _) => {
                let gamepad = self.gamepad_state(event.id);
                gamepad.axes.right_y.update(
                    stick_direction(value),
                    preview_vertical_scroll_action,
                    &mut gamepad.repeater,
                    now,
                )
            }
            _ => InputActions::default(),
        }
    }

    fn gamepad_state(&mut self, id: GamepadId) -> &mut GamepadInputState {
        self.gamepads.entry(id).or_default()
    }

    fn sleep_duration(&self) -> Duration {
        self.gamepads
            .values()
            .filter_map(|gamepad| gamepad.repeater.next_repeat())
            .min()
            .map(|instant| instant.saturating_duration_since(Instant::now()))
            .unwrap_or(GILRS_POLL_INTERVAL)
            .min(GILRS_POLL_INTERVAL)
    }
}

#[derive(Debug, Default)]
struct GamepadInputState {
    axes: AxisState,
    repeater: ActionRepeater,
}

fn run_worker(sender: &ControllerEventSender, context: &egui::Context, stop: &AtomicBool) {
    let mut state = WorkerState::new();
    let mut last_availability = state.has_gamepads();
    while !stop.load(Ordering::Relaxed) {
        let input = state.poll();
        let available = state.has_gamepads();
        if available != last_availability {
            if send_event(sender, ControllerEvent::Available(available)) {
                context.request_repaint();
            }
            last_availability = available;
        }
        if input.released {
            sender.purge_actions();
            send_event(sender, ControllerEvent::PurgeQueuedActions);
        }
        if input.actions.is_empty() {
            thread::park_timeout(state.sleep_duration());
            continue;
        }

        let mut sent_action = false;
        for action in input.actions {
            if send_event(sender, ControllerEvent::Action(action)) {
                sent_action = true;
            }
        }
        if sent_action {
            context.request_repaint();
        }
    }
}

fn send_event(sender: &ControllerEventSender, event: ControllerEvent) -> bool {
    sender.send(event)
}

#[derive(Debug, Default)]
struct AxisState {
    left_x: AxisDirection,
    left_y: AxisDirection,
    right_x: AxisDirection,
    right_y: AxisDirection,
}

fn button_pressed(button: Button, repeater: &mut ActionRepeater, now: Instant) -> InputActions {
    if let Some(action) = immediate_button_action(button) {
        return InputActions::action(action);
    }
    repeatable_button_action(button).map_or_else(InputActions::default, |action| {
        InputActions::repeated(repeater.start(action, now))
    })
}

fn immediate_button_action(button: Button) -> Option<ControllerAction> {
    match button {
        Button::South => Some(ControllerAction::Accept),
        Button::East => Some(ControllerAction::Cancel),
        Button::West => Some(ControllerAction::ClearCurrent),
        Button::Select => Some(ControllerAction::Cancel),
        Button::Start => Some(ControllerAction::SelectCurrent),
        Button::LeftTrigger => Some(ControllerAction::ToggleHiddenDirectories),
        _ => None,
    }
}

fn repeatable_button_action(button: Button) -> Option<ControllerAction> {
    match button {
        Button::DPadUp => Some(ControllerAction::Up),
        Button::DPadDown => Some(ControllerAction::Down),
        Button::DPadLeft => Some(ControllerAction::Left),
        Button::DPadRight => Some(ControllerAction::Right),
        _ => None,
    }
}

fn stick_direction(value: f32) -> AxisDirection {
    if value < -STICK_DEADZONE {
        AxisDirection::Negative
    } else if value > STICK_DEADZONE {
        AxisDirection::Positive
    } else {
        AxisDirection::Neutral
    }
}

fn horizontal_action(direction: AxisDirection) -> Option<ControllerAction> {
    match direction {
        AxisDirection::Negative => Some(ControllerAction::Left),
        AxisDirection::Neutral => None,
        AxisDirection::Positive => Some(ControllerAction::Right),
    }
}

fn vertical_action(direction: AxisDirection) -> Option<ControllerAction> {
    match direction {
        AxisDirection::Negative => Some(ControllerAction::Down),
        AxisDirection::Neutral => None,
        AxisDirection::Positive => Some(ControllerAction::Up),
    }
}

fn preview_horizontal_scroll_action(direction: AxisDirection) -> Option<ControllerAction> {
    match direction {
        AxisDirection::Negative => Some(ControllerAction::ScrollPreviewLeft),
        AxisDirection::Neutral => None,
        AxisDirection::Positive => Some(ControllerAction::ScrollPreviewRight),
    }
}

fn preview_vertical_scroll_action(direction: AxisDirection) -> Option<ControllerAction> {
    match direction {
        AxisDirection::Negative => Some(ControllerAction::ScrollPreviewDown),
        AxisDirection::Neutral => None,
        AxisDirection::Positive => Some(ControllerAction::ScrollPreviewUp),
    }
}
