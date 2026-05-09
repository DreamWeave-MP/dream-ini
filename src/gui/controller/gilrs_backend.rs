use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{SyncSender, TrySendError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use eframe::egui;
use gilrs::{Axis, Button, EventType, Gilrs};

use super::ControllerAction;

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
    pub(super) fn spawn(sender: SyncSender<ControllerAction>, context: egui::Context) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let handle = thread::Builder::new()
            .name("dream-ini-gilrs-controller".to_owned())
            .spawn(move || run_worker(&sender, &context, &worker_stop))
            .expect("gilrs controller worker thread should spawn");

        Self {
            stop,
            handle: Some(handle),
        }
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

    fn poll(&mut self) -> Vec<ControllerAction> {
        let Some(gilrs) = &mut self.gilrs else {
            self.gilrs = Gilrs::new().ok();
            thread::park_timeout(GILRS_RETRY_INTERVAL);
            return Vec::new();
        };

        let now = Instant::now();
        let mut actions = Vec::new();
        for _ in 0..MAX_GILRS_EVENTS_PER_TICK {
            let Some(event) = gilrs.next_event() else {
                break;
            };
            actions.extend(self.handle_event(event.event, now));
            if actions.len() >= MAX_WORKER_ACTIONS_PER_TICK {
                break;
            }
        }
        actions.extend(self.repeater.poll(now));
        actions.truncate(MAX_WORKER_ACTIONS_PER_TICK);
        actions
    }

    fn handle_event(&mut self, event: EventType, now: Instant) -> Vec<ControllerAction> {
        match event {
            EventType::ButtonPressed(button, _) => button_pressed(button, &mut self.repeater, now),
            EventType::ButtonReleased(button, _) => {
                if let Some(action) = repeatable_button_action(button) {
                    self.repeater.stop(action);
                }
                Vec::new()
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
            _ => Vec::new(),
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

fn run_worker(sender: &SyncSender<ControllerAction>, context: &egui::Context, stop: &AtomicBool) {
    let mut state = WorkerState::new();
    while !stop.load(Ordering::Relaxed) {
        let actions = state.poll();
        if actions.is_empty() {
            thread::park_timeout(state.sleep_duration());
            continue;
        }

        let mut sent_action = false;
        for action in actions {
            match sender.try_send(action) {
                Ok(()) => sent_action = true,
                Err(TrySendError::Full(_)) => {}
                Err(TrySendError::Disconnected(_)) => return,
            }
        }
        if sent_action {
            context.request_repaint();
        }
    }
}

#[derive(Debug, Default)]
struct AxisState {
    left_x: AxisDirection,
    left_y: AxisDirection,
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
    ) -> Vec<ControllerAction> {
        if *self == next {
            return Vec::new();
        }
        if let Some(action) = action(*self) {
            repeater.stop(action);
        }
        *self = next;
        action(next).map_or_else(Vec::new, |action| repeater.start(action, now))
    }
}

#[derive(Debug, Default)]
struct ActionRepeater {
    up: HeldAction,
    down: HeldAction,
    left: HeldAction,
    right: HeldAction,
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

    fn poll(&mut self, now: Instant) -> Vec<ControllerAction> {
        [
            (ControllerAction::Up, &mut self.up),
            (ControllerAction::Down, &mut self.down),
            (ControllerAction::Left, &mut self.left),
            (ControllerAction::Right, &mut self.right),
        ]
        .into_iter()
        .filter_map(|(action, held)| held.poll(now).then_some(action))
        .collect()
    }

    fn next_repeat(&self) -> Option<Instant> {
        [&self.up, &self.down, &self.left, &self.right]
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
            ControllerAction::Accept | ControllerAction::Cancel => None,
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

fn button_pressed(
    button: Button,
    repeater: &mut ActionRepeater,
    now: Instant,
) -> Vec<ControllerAction> {
    if let Some(action) = immediate_button_action(button) {
        return vec![action];
    }
    repeatable_button_action(button).map_or_else(Vec::new, |action| repeater.start(action, now))
}

fn immediate_button_action(button: Button) -> Option<ControllerAction> {
    match button {
        Button::South => Some(ControllerAction::Accept),
        Button::East => Some(ControllerAction::Cancel),
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
