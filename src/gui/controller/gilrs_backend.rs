use gilrs::{Axis, Button, EventType, Gilrs};

use super::ControllerAction;

const STICK_DEADZONE: f32 = 0.5;

pub(super) struct ControllerBackend {
    gilrs: Option<Gilrs>,
    axes: AxisState,
}

impl std::fmt::Debug for ControllerBackend {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ControllerBackend")
            .field("gilrs_available", &self.gilrs.is_some())
            .field("axes", &self.axes)
            .finish()
    }
}

impl Default for ControllerBackend {
    fn default() -> Self {
        Self {
            gilrs: Gilrs::new().ok(),
            axes: AxisState::default(),
        }
    }
}

impl ControllerBackend {
    pub(super) fn poll(&mut self) -> Vec<ControllerAction> {
        let Some(gilrs) = &mut self.gilrs else {
            return Vec::new();
        };

        let mut actions = Vec::new();
        while let Some(event) = gilrs.next_event() {
            if let Some(action) = self.axes.handle_event(event.event) {
                actions.push(action);
            }
        }
        actions
    }
}

#[derive(Debug, Default)]
struct AxisState {
    left_x: AxisDirection,
    left_y: AxisDirection,
}

impl AxisState {
    fn handle_event(&mut self, event: EventType) -> Option<ControllerAction> {
        match event {
            EventType::ButtonPressed(button, _) => button_action(button),
            EventType::AxisChanged(Axis::LeftStickX, value, _) => self
                .left_x
                .update(stick_direction(value), horizontal_action),
            EventType::AxisChanged(Axis::LeftStickY, value, _) => {
                self.left_y.update(stick_direction(value), vertical_action)
            }
            _ => None,
        }
    }
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
        action: impl FnOnce(Self) -> Option<ControllerAction>,
    ) -> Option<ControllerAction> {
        if *self == next {
            return None;
        }
        *self = next;
        action(next)
    }
}

fn button_action(button: Button) -> Option<ControllerAction> {
    match button {
        Button::South => Some(ControllerAction::Accept),
        Button::East => Some(ControllerAction::Cancel),
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
        AxisDirection::Negative => Some(ControllerAction::Up),
        AxisDirection::Neutral => None,
        AxisDirection::Positive => Some(ControllerAction::Down),
    }
}
