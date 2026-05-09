use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use eframe::egui;
use gilrs::{Axis, Button, EventType, Gilrs};

use super::{ControllerAction, ControllerEvent, ControllerEventSender};

const GILRS_POLL_INTERVAL: Duration = Duration::from_millis(8);
const GILRS_RETRY_INTERVAL: Duration = Duration::from_millis(500);
const INITIAL_REPEAT_DELAY: Duration = Duration::from_millis(350);
const MAX_GILRS_EVENTS_PER_TICK: usize = 64;
const MAX_WORKER_ACTIONS_PER_TICK: usize = 32;
const REPEAT_INTERVAL: Duration = Duration::from_millis(90);
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
    axes: AxisState,
    repeater: ActionRepeater,
}

impl WorkerState {
    fn new() -> Self {
        Self {
            gilrs: Gilrs::new().ok(),
            axes: AxisState::default(),
            repeater: ActionRepeater::default(),
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
            input.extend(self.handle_event(event.event, now));
            if input.actions.len() >= MAX_WORKER_ACTIONS_PER_TICK {
                break;
            }
        }
        input.actions.extend(self.repeater.poll(now));
        input.actions.truncate(MAX_WORKER_ACTIONS_PER_TICK);
        input
    }

    fn has_gamepads(&self) -> bool {
        self.gilrs
            .as_ref()
            .is_some_and(|gilrs| gilrs.gamepads().any(|(_, gamepad)| gamepad.is_connected()))
    }

    fn handle_event(&mut self, event: EventType, now: Instant) -> InputActions {
        match event {
            EventType::ButtonPressed(button, _) => button_pressed(button, &mut self.repeater, now),
            EventType::Connected => InputActions::default(),
            EventType::Disconnected => {
                self.axes = AxisState::default();
                self.repeater.clear();
                InputActions::released()
            }
            EventType::ButtonReleased(button, _) => {
                if let Some(action) = repeatable_button_action(button) {
                    self.repeater.stop(action);
                    return InputActions::released();
                }
                InputActions::default()
            }
            EventType::AxisChanged(Axis::LeftStickX, value, _) => self.axes.left_x.update(
                stick_direction(value),
                horizontal_action,
                &mut self.repeater,
                now,
            ),
            EventType::AxisChanged(Axis::LeftStickY, value, _) => self.axes.left_y.update(
                stick_direction(value),
                vertical_action,
                &mut self.repeater,
                now,
            ),
            EventType::AxisChanged(Axis::RightStickX, value, _) => self.axes.right_x.update(
                stick_direction(value),
                preview_horizontal_scroll_action,
                &mut self.repeater,
                now,
            ),
            EventType::AxisChanged(Axis::RightStickY, value, _) => self.axes.right_y.update(
                stick_direction(value),
                preview_vertical_scroll_action,
                &mut self.repeater,
                now,
            ),
            _ => InputActions::default(),
        }
    }

    fn sleep_duration(&self) -> Duration {
        self.repeater
            .next_repeat()
            .map(|instant| instant.saturating_duration_since(Instant::now()))
            .unwrap_or(GILRS_POLL_INTERVAL)
            .min(GILRS_POLL_INTERVAL)
    }
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

#[derive(Debug, Default)]
struct InputActions {
    actions: Vec<ControllerAction>,
    released: bool,
}

impl InputActions {
    fn action(action: ControllerAction) -> Self {
        Self {
            actions: vec![action],
            released: false,
        }
    }

    fn released() -> Self {
        Self {
            actions: Vec::new(),
            released: true,
        }
    }

    fn extend(&mut self, other: Self) {
        self.actions.extend(other.actions);
        self.released |= other.released;
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

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum AxisDirection {
    Negative,
    #[default]
    Neutral,
    Positive,
}

impl AxisDirection {
    fn update(
        &mut self,
        next: Self,
        action: impl Fn(Self) -> Option<ControllerAction>,
        repeater: &mut ActionRepeater,
        now: Instant,
    ) -> InputActions {
        if *self == next {
            return InputActions::default();
        }
        let mut input = InputActions::default();
        if let Some(action) = action(*self) {
            repeater.stop(action);
            input.released = true;
        }
        *self = next;
        if let Some(action) = action(next) {
            input.actions.extend(repeater.start(action, now));
        }
        input
    }
}

#[derive(Debug, Default)]
struct ActionRepeater {
    up: HeldAction,
    down: HeldAction,
    left: HeldAction,
    right: HeldAction,
    scroll_preview_left: HeldAction,
    scroll_preview_right: HeldAction,
    scroll_preview_up: HeldAction,
    scroll_preview_down: HeldAction,
}

impl ActionRepeater {
    fn start(&mut self, action: ControllerAction, now: Instant) -> Vec<ControllerAction> {
        let Some(held) = self.held_mut(action) else {
            return vec![action];
        };
        held.start(now);
        vec![action]
    }

    fn stop(&mut self, action: ControllerAction) {
        if let Some(held) = self.held_mut(action) {
            held.stop();
        }
    }

    fn clear(&mut self) {
        *self = Self::default();
    }

    fn poll(&mut self, now: Instant) -> Vec<ControllerAction> {
        [
            (ControllerAction::Up, &mut self.up),
            (ControllerAction::Down, &mut self.down),
            (ControllerAction::Left, &mut self.left),
            (ControllerAction::Right, &mut self.right),
            (
                ControllerAction::ScrollPreviewLeft,
                &mut self.scroll_preview_left,
            ),
            (
                ControllerAction::ScrollPreviewRight,
                &mut self.scroll_preview_right,
            ),
            (
                ControllerAction::ScrollPreviewUp,
                &mut self.scroll_preview_up,
            ),
            (
                ControllerAction::ScrollPreviewDown,
                &mut self.scroll_preview_down,
            ),
        ]
        .into_iter()
        .filter_map(|(action, held)| held.poll(now).then_some(action))
        .collect()
    }

    fn next_repeat(&self) -> Option<Instant> {
        [
            &self.up,
            &self.down,
            &self.left,
            &self.right,
            &self.scroll_preview_left,
            &self.scroll_preview_right,
            &self.scroll_preview_up,
            &self.scroll_preview_down,
        ]
        .into_iter()
        .filter_map(HeldAction::next_repeat)
        .min()
    }

    fn held_mut(&mut self, action: ControllerAction) -> Option<&mut HeldAction> {
        match action {
            ControllerAction::Up => Some(&mut self.up),
            ControllerAction::Down => Some(&mut self.down),
            ControllerAction::Left => Some(&mut self.left),
            ControllerAction::Right => Some(&mut self.right),
            ControllerAction::ScrollPreviewLeft => Some(&mut self.scroll_preview_left),
            ControllerAction::ScrollPreviewRight => Some(&mut self.scroll_preview_right),
            ControllerAction::ScrollPreviewUp => Some(&mut self.scroll_preview_up),
            ControllerAction::ScrollPreviewDown => Some(&mut self.scroll_preview_down),
            ControllerAction::Accept
            | ControllerAction::Cancel
            | ControllerAction::ClearCurrent
            | ControllerAction::SelectCurrent
            | ControllerAction::ToggleHiddenDirectories => None,
        }
    }
}

#[derive(Debug, Default)]
struct HeldAction {
    source_count: u8,
    next_repeat: Option<Instant>,
}

impl HeldAction {
    fn start(&mut self, now: Instant) {
        self.source_count = self.source_count.saturating_add(1);
        self.next_repeat = Some(now + INITIAL_REPEAT_DELAY);
    }

    fn stop(&mut self) {
        self.source_count = self.source_count.saturating_sub(1);
        if self.source_count == 0 {
            self.next_repeat = None;
        }
    }

    fn poll(&mut self, now: Instant) -> bool {
        let Some(next_repeat) = self.next_repeat else {
            return false;
        };
        if now < next_repeat {
            return false;
        }
        self.next_repeat = Some(now + REPEAT_INTERVAL);
        true
    }

    const fn next_repeat(&self) -> Option<Instant> {
        self.next_repeat
    }
}

fn button_pressed(button: Button, repeater: &mut ActionRepeater, now: Instant) -> InputActions {
    if let Some(action) = immediate_button_action(button) {
        return InputActions::action(action);
    }
    repeatable_button_action(button).map_or_else(InputActions::default, |action| InputActions {
        actions: repeater.start(action, now),
        released: false,
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
