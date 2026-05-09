use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::{AsRawFd, RawFd};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{SyncSender, TrySendError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use eframe::egui;

use super::ControllerAction;

const DEVICE_RESCAN_INTERVAL: Duration = Duration::from_secs(2);
const IDLE_POLL_TIMEOUT: Duration = Duration::from_millis(500);
const INITIAL_REPEAT_DELAY: Duration = Duration::from_millis(350);
const REPEAT_INTERVAL: Duration = Duration::from_millis(90);
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

pub(super) struct ControllerWorker {
    stop: Arc<AtomicBool>,
    wake: WakeFd,
    handle: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for ControllerWorker {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ControllerWorker")
            .field("stop_requested", &self.stop.load(Ordering::Relaxed))
            .field("wake_fd", &self.wake.as_raw_fd())
            .field("running", &self.handle.is_some())
            .finish()
    }
}

impl ControllerWorker {
    pub(super) fn spawn(sender: SyncSender<ControllerAction>, context: egui::Context) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let (worker_wake, wake) = WakeFd::new_pair().expect("Linux controller wake fd should open");
        let handle = thread::Builder::new()
            .name("dream-ini-linux-controller".to_owned())
            .spawn(move || run_worker(&sender, &context, &worker_stop, worker_wake))
            .expect("Linux controller worker thread should spawn");

        Self {
            stop,
            wake,
            handle: Some(handle),
        }
    }
}

impl Drop for ControllerWorker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.wake.wake();
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

#[derive(Debug)]
struct WorkerState {
    devices: Vec<InputDevice>,
    poll_fds: Vec<libc::pollfd>,
    last_scan: Instant,
    wake: WakeFd,
}

impl WorkerState {
    fn new(wake: WakeFd) -> Self {
        Self {
            devices: open_devices(),
            poll_fds: Vec::new(),
            last_scan: Instant::now(),
            wake,
        }
    }

    fn poll(&mut self) -> Vec<ControllerAction> {
        self.rescan_if_needed();
        if self.devices.is_empty() {
            self.wait_for_input();
            return Vec::new();
        }

        self.wait_for_input();
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

    fn wait_for_input(&mut self) {
        let timeout = self.next_repeat_delay().unwrap_or(IDLE_POLL_TIMEOUT);
        let timeout_ms = duration_to_poll_timeout(timeout);
        self.poll_fds.clear();
        self.poll_fds.push(libc::pollfd {
            fd: self.wake.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        });
        self.poll_fds
            .extend(self.devices.iter().map(|device| libc::pollfd {
                fd: device.file.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            }));
        // SAFETY: poll_fds points to a valid mutable array for the duration of
        // the call. poll(2) only writes revents fields in that array.
        let result = unsafe {
            libc::poll(
                self.poll_fds.as_mut_ptr(),
                self.poll_fds.len() as libc::nfds_t,
                timeout_ms,
            )
        };
        if result > 0 && self.poll_fds[0].revents & libc::POLLIN != 0 {
            self.wake.drain();
        }
    }

    fn next_repeat_delay(&self) -> Option<Duration> {
        let now = Instant::now();
        self.devices
            .iter()
            .filter_map(InputDevice::next_repeat)
            .min()
            .map(|instant| instant.saturating_duration_since(now))
    }

    fn rescan_if_needed(&mut self) {
        if self.last_scan.elapsed() < DEVICE_RESCAN_INTERVAL {
            return;
        }

        let known_paths = self
            .devices
            .iter()
            .map(|device| device.path.clone())
            .collect::<BTreeSet<_>>();
        self.devices.extend(
            candidate_device_paths()
                .into_iter()
                .filter(|path| !known_paths.contains(path))
                .filter_map(|path| InputDevice::open(&path).ok()),
        );
        self.last_scan = Instant::now();
    }
}

fn run_worker(
    sender: &SyncSender<ControllerAction>,
    context: &egui::Context,
    stop: &AtomicBool,
    wake: WakeFd,
) {
    let mut state = WorkerState::new(wake);
    while !stop.load(Ordering::Relaxed) {
        let actions = state.poll();
        if actions.is_empty() {
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

fn duration_to_poll_timeout(duration: Duration) -> i32 {
    let milliseconds = duration.as_millis().min(i32::MAX as u128);
    i32::try_from(milliseconds).expect("duration was clamped to i32::MAX")
}

#[derive(Debug)]
struct WakeFd {
    fd: RawFd,
}

impl WakeFd {
    fn new_pair() -> io::Result<(Self, Self)> {
        // SAFETY: eventfd creates a new file descriptor or returns -1 with errno.
        let fd = unsafe { libc::eventfd(0, libc::EFD_CLOEXEC | libc::EFD_NONBLOCK) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: fd is a valid file descriptor here. dup either creates a new
        // descriptor referring to the same eventfd or returns -1 with errno.
        let duplicate = unsafe { libc::dup(fd) };
        if duplicate < 0 {
            // SAFETY: fd was opened by eventfd above and has not been moved.
            unsafe {
                libc::close(fd);
            }
            return Err(io::Error::last_os_error());
        }

        Ok((Self { fd }, Self { fd: duplicate }))
    }

    const fn as_raw_fd(&self) -> RawFd {
        self.fd
    }

    fn wake(&self) {
        let value = 1_u64.to_ne_bytes();
        // SAFETY: value points to a valid u64-sized buffer as required by eventfd.
        let _ = unsafe { libc::write(self.fd, value.as_ptr().cast(), value.len()) };
    }

    fn drain(&self) {
        let mut value = [0_u8; std::mem::size_of::<u64>()];
        loop {
            // SAFETY: value points to a valid u64-sized buffer as required by eventfd.
            let result = unsafe { libc::read(self.fd, value.as_mut_ptr().cast(), value.len()) };
            if result < 0 {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::WouldBlock {
                    return;
                }
                return;
            }
            if result == 0 {
                return;
            }
        }
    }
}

impl Drop for WakeFd {
    fn drop(&mut self) {
        // SAFETY: fd is owned by this WakeFd and is closed exactly once here.
        let _ = unsafe { libc::close(self.fd) };
    }
}

#[derive(Debug)]
struct InputDevice {
    path: PathBuf,
    file: File,
    axes: AxisState,
    repeater: ActionRepeater,
}

impl InputDevice {
    fn open(path: &Path) -> io::Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
            .open(path)?;
        Ok(Self {
            path: path.to_owned(),
            file,
            axes: AxisState::default(),
            repeater: ActionRepeater::default(),
        })
    }

    fn poll(&mut self) -> io::Result<Vec<ControllerAction>> {
        let mut buffer = [0_u8; input_event_size() * 32];
        let mut actions = Vec::new();
        let now = Instant::now();

        loop {
            match self.file.read(&mut buffer) {
                Ok(0) => break,
                Ok(bytes_read) => {
                    for event in parse_events(&buffer[..bytes_read]) {
                        actions.extend(self.handle_event(event, now));
                    }
                    if bytes_read < buffer.len() {
                        break;
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) => return Err(error),
            }
        }

        actions.extend(self.repeater.poll(now));
        Ok(actions)
    }

    fn next_repeat(&self) -> Option<Instant> {
        self.repeater.next_repeat()
    }

    fn handle_event(&mut self, event: InputEvent, now: Instant) -> Vec<ControllerAction> {
        match event.kind {
            EV_KEY => key_actions(event.code, event.value, &mut self.repeater, now),
            EV_ABS => self
                .axes
                .update(event.code, event.value, &mut self.repeater, now),
            _ => Vec::new(),
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
    fn update(
        &mut self,
        code: u16,
        value: i32,
        repeater: &mut ActionRepeater,
        now: Instant,
    ) -> Vec<ControllerAction> {
        match code {
            ABS_X => self
                .left_x
                .update(stick_direction(value), horizontal_action, repeater, now),
            ABS_Y => self
                .left_y
                .update(stick_direction(value), vertical_action, repeater, now),
            ABS_HAT0X => self
                .hat_x
                .update(hat_direction(value), horizontal_action, repeater, now),
            ABS_HAT0Y => self
                .hat_y
                .update(hat_direction(value), vertical_action, repeater, now),
            _ => Vec::new(),
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
                    name.strip_prefix("event").is_some_and(|suffix| {
                        suffix.chars().all(|character| character.is_ascii_digit())
                    })
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

fn key_actions(
    code: u16,
    value: i32,
    repeater: &mut ActionRepeater,
    now: Instant,
) -> Vec<ControllerAction> {
    match (key_action(code), value) {
        (Some(ControllerAction::Accept | ControllerAction::Cancel), 1) => {
            vec![key_action(code).expect("checked above")]
        }
        (Some(action), 1) => repeater.start(action, now),
        (Some(action), 0) => {
            repeater.stop(action);
            Vec::new()
        }
        (Some(_) | None, _) => Vec::new(),
    }
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
        let mut repeater = ActionRepeater::default();
        let now = Instant::now();

        assert_eq!(
            axes.update(ABS_X, STICK_DEADZONE + 1, &mut repeater, now),
            vec![ControllerAction::Right]
        );
        assert_eq!(
            axes.update(ABS_X, STICK_DEADZONE + 2, &mut repeater, now),
            Vec::new()
        );
        assert_eq!(axes.update(ABS_X, 0, &mut repeater, now), Vec::new());
        assert_eq!(
            axes.update(ABS_X, -STICK_DEADZONE - 1, &mut repeater, now),
            vec![ControllerAction::Left]
        );
    }

    #[test]
    fn hat_axes_map_directly() {
        let mut axes = AxisState::default();
        let mut repeater = ActionRepeater::default();
        let now = Instant::now();

        assert_eq!(
            axes.update(ABS_HAT0Y, -1, &mut repeater, now),
            vec![ControllerAction::Up]
        );
        assert_eq!(
            axes.update(ABS_HAT0Y, 1, &mut repeater, now),
            vec![ControllerAction::Down]
        );
        assert_eq!(
            axes.update(ABS_HAT0X, -1, &mut repeater, now),
            vec![ControllerAction::Left]
        );
        assert_eq!(
            axes.update(ABS_HAT0X, 1, &mut repeater, now),
            vec![ControllerAction::Right]
        );
    }

    #[test]
    fn held_direction_repeats_after_initial_delay() {
        let mut repeater = ActionRepeater::default();
        let now = Instant::now();

        assert_eq!(
            repeater.start(ControllerAction::Down, now),
            vec![ControllerAction::Down]
        );
        let before_initial_repeat = (now + INITIAL_REPEAT_DELAY)
            .checked_sub(Duration::from_millis(1))
            .unwrap();
        assert!(repeater.poll(before_initial_repeat).is_empty());
        assert_eq!(
            repeater.poll(now + INITIAL_REPEAT_DELAY),
            vec![ControllerAction::Down]
        );
        assert!(
            repeater
                .poll(now + INITIAL_REPEAT_DELAY + Duration::from_millis(1))
                .is_empty()
        );
        assert_eq!(
            repeater.poll(now + INITIAL_REPEAT_DELAY + REPEAT_INTERVAL),
            vec![ControllerAction::Down]
        );
    }

    #[test]
    fn released_direction_stops_repeating() {
        let mut repeater = ActionRepeater::default();
        let now = Instant::now();

        repeater.start(ControllerAction::Down, now);
        repeater.stop(ControllerAction::Down);

        assert!(repeater.poll(now + INITIAL_REPEAT_DELAY).is_empty());
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
