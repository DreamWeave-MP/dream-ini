use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::ControllerAction;

const DEVICE_RESCAN_INTERVAL: Duration = Duration::from_secs(2);
const STICK_DEADZONE: i32 = 16_384;

const EV_KEY: u16 = 0x01;
const EV_ABS: u16 = 0x03;

const BTN_SOUTH: u16 = 0x130;
const BTN_EAST: u16 = 0x131;
const BTN_DPAD_UP: u16 = 0x220;
const BTN_DPAD_DOWN: u16 = 0x221;
const BTN_DPAD_LEFT: u16 = 0x222;
const BTN_DPAD_RIGHT: u16 = 0x223;

const ABS_X: u16 = 0x00;
const ABS_Y: u16 = 0x01;
const ABS_HAT0X: u16 = 0x10;
const ABS_HAT0Y: u16 = 0x11;

#[derive(Debug)]
pub(super) struct ControllerBackend {
    devices: Vec<InputDevice>,
    last_scan: Instant,
}

impl Default for ControllerBackend {
    fn default() -> Self {
        Self {
            devices: open_devices(),
            last_scan: Instant::now(),
        }
    }
}

impl ControllerBackend {
    pub(super) fn poll(&mut self) -> Vec<ControllerAction> {
        self.rescan_if_needed();

        let mut actions = Vec::new();
        self.devices.retain_mut(|device| match device.poll() {
            Ok(device_actions) => {
                actions.extend(device_actions);
                true
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => true,
            Err(_) => false,
        });
        deduplicate(actions)
    }

    fn rescan_if_needed(&mut self) {
        if self.last_scan.elapsed() < DEVICE_RESCAN_INTERVAL {
            return;
        }

        self.devices = open_devices();
        self.last_scan = Instant::now();
    }
}

#[derive(Debug)]
struct InputDevice {
    file: File,
    axes: AxisState,
}

impl InputDevice {
    fn open(path: &Path) -> io::Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
            .open(path)?;
        Ok(Self {
            file,
            axes: AxisState::default(),
        })
    }

    fn poll(&mut self) -> io::Result<Vec<ControllerAction>> {
        let mut buffer = [0_u8; input_event_size() * 32];
        let mut actions = Vec::new();

        loop {
            match self.file.read(&mut buffer) {
                Ok(0) => break,
                Ok(bytes_read) => {
                    for event in parse_events(&buffer[..bytes_read]) {
                        if let Some(action) = self.handle_event(event) {
                            actions.push(action);
                        }
                    }
                    if bytes_read < buffer.len() {
                        break;
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) => return Err(error),
            }
        }

        Ok(actions)
    }

    fn handle_event(&mut self, event: InputEvent) -> Option<ControllerAction> {
        match event.kind {
            EV_KEY if event.value == 1 => key_action(event.code),
            EV_ABS => self.axes.update(event.code, event.value),
            _ => None,
        }
    }
}

#[derive(Debug, Default)]
struct AxisState {
    left_x: AxisDirection,
    left_y: AxisDirection,
    hat_x: AxisDirection,
    hat_y: AxisDirection,
}

impl AxisState {
    fn update(&mut self, code: u16, value: i32) -> Option<ControllerAction> {
        match code {
            ABS_X => self.left_x.update(stick_direction(value), horizontal_action),
            ABS_Y => self.left_y.update(stick_direction(value), vertical_action),
            ABS_HAT0X => self.hat_x.update(hat_direction(value), horizontal_action),
            ABS_HAT0Y => self.hat_y.update(hat_direction(value), vertical_action),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct InputEvent {
    kind: u16,
    code: u16,
    value: i32,
}

fn open_devices() -> Vec<InputDevice> {
    candidate_device_paths()
        .into_iter()
        .filter_map(|path| InputDevice::open(&path).ok())
        .collect()
}

fn candidate_device_paths() -> Vec<PathBuf> {
    let mut by_id = joystick_event_paths(Path::new("/dev/input/by-id"));
    if !by_id.is_empty() {
        by_id.sort();
        by_id.dedup();
        return by_id;
    }

    event_device_paths(Path::new("/dev/input"))
}

fn joystick_event_paths(directory: &Path) -> Vec<PathBuf> {
    read_directory_paths(directory)
        .into_iter()
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with("event-joystick"))
        })
        .collect()
}

fn event_device_paths(directory: &Path) -> Vec<PathBuf> {
    read_directory_paths(directory)
        .into_iter()
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    name
                        .strip_prefix("event")
                        .is_some_and(|suffix| suffix.chars().all(|character| character.is_ascii_digit()))
                })
        })
        .collect()
}

fn read_directory_paths(directory: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(directory) else {
        return Vec::new();
    };

    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect()
}

fn parse_events(bytes: &[u8]) -> impl Iterator<Item = InputEvent> + '_ {
    bytes.chunks_exact(input_event_size()).map(parse_event)
}

fn parse_event(bytes: &[u8]) -> InputEvent {
    let offset = std::mem::size_of::<libc::timeval>();
    InputEvent {
        kind: u16::from_ne_bytes([bytes[offset], bytes[offset + 1]]),
        code: u16::from_ne_bytes([bytes[offset + 2], bytes[offset + 3]]),
        value: i32::from_ne_bytes([
            bytes[offset + 4],
            bytes[offset + 5],
            bytes[offset + 6],
            bytes[offset + 7],
        ]),
    }
}

const fn input_event_size() -> usize {
    std::mem::size_of::<libc::timeval>() + 8
}

fn key_action(code: u16) -> Option<ControllerAction> {
    match code {
        BTN_SOUTH => Some(ControllerAction::Accept),
        BTN_EAST => Some(ControllerAction::Cancel),
        BTN_DPAD_UP => Some(ControllerAction::Up),
        BTN_DPAD_DOWN => Some(ControllerAction::Down),
        BTN_DPAD_LEFT => Some(ControllerAction::Left),
        BTN_DPAD_RIGHT => Some(ControllerAction::Right),
        _ => None,
    }
}

fn stick_direction(value: i32) -> AxisDirection {
    if value < -STICK_DEADZONE {
        AxisDirection::Negative
    } else if value > STICK_DEADZONE {
        AxisDirection::Positive
    } else {
        AxisDirection::Neutral
    }
}

fn hat_direction(value: i32) -> AxisDirection {
    match value.cmp(&0) {
        std::cmp::Ordering::Less => AxisDirection::Negative,
        std::cmp::Ordering::Equal => AxisDirection::Neutral,
        std::cmp::Ordering::Greater => AxisDirection::Positive,
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

fn deduplicate(actions: Vec<ControllerAction>) -> Vec<ControllerAction> {
    let mut seen = BTreeSet::new();
    actions
        .into_iter()
        .filter(|action| seen.insert(*action))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_events_map_to_controller_actions() {
        assert_eq!(key_action(BTN_SOUTH), Some(ControllerAction::Accept));
        assert_eq!(key_action(BTN_EAST), Some(ControllerAction::Cancel));
        assert_eq!(key_action(BTN_DPAD_UP), Some(ControllerAction::Up));
        assert_eq!(key_action(BTN_DPAD_DOWN), Some(ControllerAction::Down));
        assert_eq!(key_action(BTN_DPAD_LEFT), Some(ControllerAction::Left));
        assert_eq!(key_action(BTN_DPAD_RIGHT), Some(ControllerAction::Right));
    }

    #[test]
    fn stick_axes_are_edge_triggered() {
        let mut axes = AxisState::default();

        assert_eq!(axes.update(ABS_X, STICK_DEADZONE + 1), Some(ControllerAction::Right));
        assert_eq!(axes.update(ABS_X, STICK_DEADZONE + 2), None);
        assert_eq!(axes.update(ABS_X, 0), None);
        assert_eq!(axes.update(ABS_X, -STICK_DEADZONE - 1), Some(ControllerAction::Left));
    }

    #[test]
    fn hat_axes_map_directly() {
        let mut axes = AxisState::default();

        assert_eq!(axes.update(ABS_HAT0Y, -1), Some(ControllerAction::Up));
        assert_eq!(axes.update(ABS_HAT0Y, 1), Some(ControllerAction::Down));
        assert_eq!(axes.update(ABS_HAT0X, -1), Some(ControllerAction::Left));
        assert_eq!(axes.update(ABS_HAT0X, 1), Some(ControllerAction::Right));
    }

    #[test]
    fn duplicate_actions_are_removed_without_reordering() {
        assert_eq!(
            deduplicate(vec![
                ControllerAction::Down,
                ControllerAction::Down,
                ControllerAction::Accept,
            ]),
            vec![ControllerAction::Down, ControllerAction::Accept]
        );
    }
}
