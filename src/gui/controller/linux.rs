// SPDX-License-Identifier: GPL-3.0-only

use std::collections::BTreeSet;
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::{AsRawFd, RawFd};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use super::common::{ActionRepeater, AxisDirection, InputActions};
use super::{ControllerAction, ControllerEvent, ControllerEventSender};

const DEVICE_RESCAN_INTERVAL: Duration = Duration::from_secs(2);
const IDLE_POLL_TIMEOUT: Duration = Duration::from_millis(500);
const MAX_DEVICE_READ_BATCHES: usize = 4;
const MAX_WORKER_ACTIONS_PER_POLL: usize = 32;
const CONTROLLER_LOG_ENV_VAR: &str = "DREAM_INI_CONTROLLER_LOG";

const EV_KEY: u16 = 0x01;
const EV_ABS: u16 = 0x03;
const EV_MAX: u16 = 0x1f;
const KEY_MAX: u16 = 0x2ff;
const ABS_MAX: u16 = 0x3f;

const BTN_SOUTH: u16 = 0x130;
const BTN_EAST: u16 = 0x131;
const BTN_WEST: u16 = 0x133;
const BTN_NORTH: u16 = 0x134;
const BTN_TL: u16 = 0x136;
const BTN_TR: u16 = 0x137;
const BTN_TL2: u16 = 0x138;
const BTN_TR2: u16 = 0x139;
const BTN_SELECT: u16 = 0x13a;
const BTN_START: u16 = 0x13b;
const BTN_DPAD_UP: u16 = 0x220;
const BTN_DPAD_DOWN: u16 = 0x221;
const BTN_DPAD_LEFT: u16 = 0x222;
const BTN_DPAD_RIGHT: u16 = 0x223;

const ABS_X: u16 = 0x00;
const ABS_Y: u16 = 0x01;
const ABS_RX: u16 = 0x03;
const ABS_RY: u16 = 0x04;
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
    pub(super) fn spawn(sender: ControllerEventSender, context: egui::Context) -> Option<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let (worker_wake, wake) = WakeFd::new_pair().ok()?;
        let handle = thread::Builder::new()
            .name("dream-ini-linux-controller".to_owned())
            .spawn(move || run_worker(&sender, &context, &worker_stop, worker_wake))
            .ok()?;

        Some(Self {
            stop,
            wake,
            handle: Some(handle),
        })
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

    fn poll(&mut self) -> InputActions {
        self.rescan_if_needed();
        if self.devices.is_empty() {
            self.wait_for_input();
            return InputActions::default();
        }

        self.wait_for_input();
        let mut input = InputActions::default();
        self.devices.retain_mut(|device| match device.poll() {
            Ok(device_input) => {
                input.extend(device_input);
                input.actions.truncate(MAX_WORKER_ACTIONS_PER_POLL);
                true
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => true,
            Err(_) => {
                device.repeater.clear();
                input.released = true;
                false
            }
        });
        input
    }

    fn has_devices(&self) -> bool {
        !self.devices.is_empty()
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
            .map(|device| device.identity.clone())
            .collect::<BTreeSet<_>>();
        let mut seen_paths = known_paths;
        for path in candidate_device_paths() {
            let identity = device_identity(&path);
            if seen_paths.contains(&identity) {
                continue;
            }
            if let Ok(device) = InputDevice::open(&path) {
                seen_paths.insert(device.identity.clone());
                self.devices.push(device);
            }
        }
        self.last_scan = Instant::now();
    }
}

fn run_worker(
    sender: &ControllerEventSender,
    context: &egui::Context,
    stop: &AtomicBool,
    wake: WakeFd,
) {
    let mut state = WorkerState::new(wake);
    let mut last_availability = state.has_devices();
    while !stop.load(Ordering::Relaxed) {
        let input = state.poll();
        let available = state.has_devices();
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
            continue;
        }

        let mut sent_action = false;
        for action in input.actions {
            if send_event(sender, ControllerEvent::Action(action)) {
                sent_action = true;
            } else {
                log_controller_event(format_args!("dropped queued action={action:?}"));
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

fn controller_log_enabled() -> bool {
    env::var_os(CONTROLLER_LOG_ENV_VAR).is_some_and(|value| value != "0")
}

fn log_controller_event(arguments: std::fmt::Arguments<'_>) {
    if controller_log_enabled() {
        eprintln!("dream-ini controller: {arguments}");
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
    identity: PathBuf,
    file: File,
    capabilities: DeviceCapabilities,
    axes: AxisState,
    held_buttons: BTreeSet<u16>,
    repeater: ActionRepeater,
    log_events: bool,
}

impl InputDevice {
    fn open(path: &Path) -> io::Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
            .open(path)?;
        let Some(capabilities) = read_controller_capabilities(file.as_raw_fd()) else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "input device does not expose controller capabilities",
            ));
        };
        let axes = AxisState::new(AxisCalibration::read(file.as_raw_fd()));
        let identity = device_identity(path);
        let log_events = controller_log_enabled();
        if log_events {
            log_controller_event(format_args!(
                "opened device={} has_hat_axes={}",
                identity.display(),
                capabilities.has_hat_axes
            ));
        }
        Ok(Self {
            identity,
            file,
            capabilities,
            axes,
            held_buttons: BTreeSet::new(),
            repeater: ActionRepeater::default(),
            log_events,
        })
    }

    fn poll(&mut self) -> io::Result<InputActions> {
        let mut buffer = [0_u8; input_event_size() * 32];
        let mut input = InputActions::default();
        let now = Instant::now();

        for _ in 0..MAX_DEVICE_READ_BATCHES {
            match self.file.read(&mut buffer) {
                Ok(0) => {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "input device reached EOF",
                    ));
                }
                Ok(bytes_read) => {
                    for event in parse_events(&buffer[..bytes_read]) {
                        input.extend(self.handle_event(event, now));
                    }
                    if bytes_read < buffer.len() {
                        break;
                    }
                    if input.actions.len() >= MAX_WORKER_ACTIONS_PER_POLL {
                        break;
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) => return Err(error),
            }
        }

        input.actions.extend(self.repeater.poll(now));
        input.actions.truncate(MAX_WORKER_ACTIONS_PER_POLL);
        Ok(input)
    }

    fn next_repeat(&self) -> Option<Instant> {
        self.repeater.next_repeat()
    }

    fn handle_event(&mut self, event: InputEvent, now: Instant) -> InputActions {
        match event.kind {
            EV_KEY => {
                let input = key_actions(
                    event.code,
                    event.value,
                    self.capabilities.has_hat_axes,
                    &mut self.held_buttons,
                    &mut self.repeater,
                    now,
                );
                if self.log_events {
                    log_controller_event(format_args!(
                        "device={} key code=0x{:03x} name={} value={} mapped={:?} emitted={:?} ignored_dpad_key={}",
                        self.identity.display(),
                        event.code,
                        key_name(event.code),
                        event.value,
                        key_action(event.code),
                        input.actions,
                        self.capabilities.has_hat_axes && is_dpad_key(event.code)
                    ));
                }
                input
            }
            EV_ABS => self
                .axes
                .update(event.code, event.value, &mut self.repeater, now),
            _ => InputActions::default(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct DeviceCapabilities {
    has_hat_axes: bool,
}

#[derive(Debug)]
struct AxisState {
    left_x: AxisDirection,
    left_y: AxisDirection,
    right_x: AxisDirection,
    right_y: AxisDirection,
    hat_x: AxisDirection,
    hat_y: AxisDirection,
    calibration: AxisCalibration,
}

impl AxisState {
    fn new(calibration: AxisCalibration) -> Self {
        Self {
            left_x: AxisDirection::Neutral,
            left_y: AxisDirection::Neutral,
            right_x: AxisDirection::Neutral,
            right_y: AxisDirection::Neutral,
            hat_x: AxisDirection::Neutral,
            hat_y: AxisDirection::Neutral,
            calibration,
        }
    }

    fn update(
        &mut self,
        code: u16,
        value: i32,
        repeater: &mut ActionRepeater,
        now: Instant,
    ) -> InputActions {
        match code {
            ABS_X => self.left_x.update(
                self.calibration.x.direction(value),
                horizontal_action,
                repeater,
                now,
            ),
            ABS_Y => self.left_y.update(
                self.calibration.y.direction(value),
                vertical_action,
                repeater,
                now,
            ),
            ABS_RX => self.right_x.update(
                self.calibration.rx.direction(value),
                preview_horizontal_scroll_action,
                repeater,
                now,
            ),
            ABS_RY => self.right_y.update(
                self.calibration.ry.direction(value),
                preview_vertical_scroll_action,
                repeater,
                now,
            ),
            ABS_HAT0X => self
                .hat_x
                .update(hat_direction(value), horizontal_action, repeater, now),
            ABS_HAT0Y => self
                .hat_y
                .update(hat_direction(value), vertical_action, repeater, now),
            _ => InputActions::default(),
        }
    }
}

impl Default for AxisState {
    fn default() -> Self {
        Self::new(AxisCalibration::default())
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct AxisCalibration {
    x: AxisInfo,
    y: AxisInfo,
    rx: AxisInfo,
    ry: AxisInfo,
}

impl AxisCalibration {
    fn read(fd: RawFd) -> Self {
        Self {
            x: AxisInfo::read(fd, ABS_X),
            y: AxisInfo::read(fd, ABS_Y),
            rx: AxisInfo::read(fd, ABS_RX),
            ry: AxisInfo::read(fd, ABS_RY),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct AxisInfo {
    minimum: i32,
    maximum: i32,
    flat: i32,
}

impl AxisInfo {
    fn read(fd: RawFd, axis: u16) -> Self {
        let mut info = InputAbsInfo::default();
        // SAFETY: info points to a valid input_absinfo-sized buffer for the ioctl to fill.
        let result = unsafe { libc::ioctl(fd, eviocgabs(axis), &mut info) };
        if result < 0 || info.minimum >= info.maximum {
            return Self::default();
        }
        Self {
            minimum: info.minimum,
            maximum: info.maximum,
            flat: info.flat.max(0),
        }
    }

    fn direction(self, value: i32) -> AxisDirection {
        let center = self.minimum + (self.maximum - self.minimum) / 2;
        let range = self.maximum - self.minimum;
        let deadzone = self.flat.max(range / 4);
        if value < center - deadzone {
            AxisDirection::Negative
        } else if value > center + deadzone {
            AxisDirection::Positive
        } else {
            AxisDirection::Neutral
        }
    }
}

impl Default for AxisInfo {
    fn default() -> Self {
        Self {
            minimum: i16::MIN.into(),
            maximum: i16::MAX.into(),
            flat: 0,
        }
    }
}

#[repr(C)]
#[derive(Debug, Default)]
struct InputAbsInfo {
    value: i32,
    minimum: i32,
    maximum: i32,
    fuzz: i32,
    flat: i32,
    resolution: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct InputEvent {
    kind: u16,
    code: u16,
    value: i32,
}

fn open_devices() -> Vec<InputDevice> {
    let mut seen = BTreeSet::new();
    let mut devices = Vec::new();
    for path in candidate_device_paths() {
        let Ok(device) = InputDevice::open(&path) else {
            continue;
        };
        if seen.insert(device.identity.clone()) {
            devices.push(device);
        }
    }
    devices
}

fn candidate_device_paths() -> Vec<PathBuf> {
    let mut by_id = joystick_event_paths(Path::new("/dev/input/by-id"));
    by_id.extend(event_device_paths(Path::new("/dev/input")));
    by_id.sort();
    by_id.dedup();
    by_id
}

fn device_identity(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_owned())
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

fn read_controller_capabilities(fd: RawFd) -> Option<DeviceCapabilities> {
    let event_bits = ioctl_bitset(fd, 0, EV_MAX).unwrap_or_default();
    let key_bits = ioctl_bitset(fd, EV_KEY, KEY_MAX).unwrap_or_default();
    let abs_bits = if test_bit(&event_bits, EV_ABS) {
        ioctl_bitset(fd, EV_ABS, ABS_MAX).unwrap_or_default()
    } else {
        Vec::new()
    };

    controller_capabilities_from_bits(&event_bits, &key_bits, &abs_bits)
}

fn controller_capabilities_from_bits(
    event_bits: &[u8],
    key_bits: &[u8],
    abs_bits: &[u8],
) -> Option<DeviceCapabilities> {
    if !test_bit(event_bits, EV_KEY) {
        return None;
    }

    let has_controller_button = [
        BTN_SOUTH,
        BTN_EAST,
        BTN_WEST,
        BTN_TL,
        BTN_TR,
        BTN_TL2,
        BTN_TR2,
        BTN_SELECT,
        BTN_START,
        BTN_DPAD_UP,
        BTN_DPAD_DOWN,
        BTN_DPAD_LEFT,
        BTN_DPAD_RIGHT,
    ]
    .into_iter()
    .any(|code| test_bit(key_bits, code));
    let has_dpad_buttons = [BTN_DPAD_UP, BTN_DPAD_DOWN, BTN_DPAD_LEFT, BTN_DPAD_RIGHT]
        .into_iter()
        .any(|code| test_bit(key_bits, code));
    let has_abs = test_bit(event_bits, EV_ABS);
    let has_stick_axes = has_abs && test_bit(abs_bits, ABS_X) && test_bit(abs_bits, ABS_Y);
    let has_hat_axes = has_abs && test_bit(abs_bits, ABS_HAT0X) && test_bit(abs_bits, ABS_HAT0Y);
    let has_navigation = has_stick_axes || has_hat_axes || has_dpad_buttons;

    (has_controller_button && has_navigation).then_some(DeviceCapabilities { has_hat_axes })
}

fn ioctl_bitset(fd: RawFd, event_type: u16, max_bit: u16) -> io::Result<Vec<u8>> {
    let mut bits = vec![0_u8; usize::from(max_bit / 8 + 1)];
    let request = eviocgbit(event_type, bits.len());
    // SAFETY: bits points to a valid mutable buffer of the size encoded in the request.
    let result = unsafe { libc::ioctl(fd, request, bits.as_mut_ptr()) };
    if result < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(bits)
}

fn test_bit(bits: &[u8], bit: u16) -> bool {
    let byte = usize::from(bit / 8);
    let mask = 1_u8 << (bit % 8);
    bits.get(byte).is_some_and(|value| value & mask != 0)
}

const IOC_READ: u8 = 2;
const IOC_NRSHIFT: u8 = 0;
const IOC_TYPESHIFT: u8 = 8;
const IOC_SIZESHIFT: u8 = 16;
const IOC_DIRSHIFT: u8 = 30;

const fn eviocgbit(event_type: u16, size: usize) -> libc::c_ulong {
    ioc(IOC_READ, b'E', 0x20 + event_type, size)
}

const fn eviocgabs(axis: u16) -> libc::c_ulong {
    ioc(
        IOC_READ,
        b'E',
        0x40 + axis,
        std::mem::size_of::<InputAbsInfo>(),
    )
}

const fn ioc(direction: u8, ioctl_type: u8, number: u16, size: usize) -> libc::c_ulong {
    ((direction as libc::c_ulong) << IOC_DIRSHIFT)
        | ((ioctl_type as libc::c_ulong) << IOC_TYPESHIFT)
        | ((number as libc::c_ulong) << IOC_NRSHIFT)
        | ((size as libc::c_ulong) << IOC_SIZESHIFT)
}

fn key_actions(
    code: u16,
    value: i32,
    ignore_dpad_keys: bool,
    held_buttons: &mut BTreeSet<u16>,
    repeater: &mut ActionRepeater,
    now: Instant,
) -> InputActions {
    if ignore_dpad_keys && is_dpad_key(code) {
        return InputActions::default();
    }
    match (key_action(code), value) {
        (
            Some(
                action @ (ControllerAction::Accept
                | ControllerAction::Cancel
                | ControllerAction::ClearCurrent
                | ControllerAction::Shift
                | ControllerAction::SelectCurrent
                | ControllerAction::PagePreviewDown
                | ControllerAction::ToggleHiddenDirectories),
            ),
            1,
        ) => InputActions::action(action),
        (Some(action), 1) if action.is_repeatable() && held_buttons.insert(code) => {
            InputActions::repeated(repeater.start(action, now))
        }
        (Some(action), 1) if action.is_repeatable() => InputActions::default(),
        (Some(action), 0) if action_repeats(action) => {
            if held_buttons.remove(&code) {
                repeater.stop(action);
                InputActions::released()
            } else {
                InputActions::default()
            }
        }
        (Some(_) | None, _) => InputActions::default(),
    }
}

const fn action_repeats(action: ControllerAction) -> bool {
    action.is_repeatable()
}

fn is_dpad_key(code: u16) -> bool {
    matches!(
        code,
        BTN_DPAD_UP | BTN_DPAD_DOWN | BTN_DPAD_LEFT | BTN_DPAD_RIGHT
    )
}

#[cfg(all(feature = "portmaster-gui", not(feature = "gui")))]
fn key_action(code: u16) -> Option<ControllerAction> {
    portmaster_key_action(code)
}

#[cfg(not(all(feature = "portmaster-gui", not(feature = "gui"))))]
fn key_action(code: u16) -> Option<ControllerAction> {
    default_key_action(code)
}

const fn default_key_action(code: u16) -> Option<ControllerAction> {
    match code {
        BTN_SOUTH => Some(ControllerAction::Accept),
        BTN_EAST | BTN_SELECT => Some(ControllerAction::Cancel),
        BTN_WEST => Some(ControllerAction::ClearCurrent),
        BTN_NORTH => Some(ControllerAction::Shift),
        BTN_START => Some(ControllerAction::SelectCurrent),
        BTN_TL => Some(ControllerAction::ToggleHiddenDirectories),
        BTN_TR => Some(ControllerAction::PagePreviewDown),
        BTN_DPAD_UP => Some(ControllerAction::Up),
        BTN_DPAD_DOWN => Some(ControllerAction::Down),
        BTN_DPAD_LEFT => Some(ControllerAction::Left),
        BTN_DPAD_RIGHT => Some(ControllerAction::Right),
        _ => None,
    }
}

#[cfg(all(feature = "portmaster-gui", not(feature = "gui")))]
const fn portmaster_key_action(code: u16) -> Option<ControllerAction> {
    // Anbernic PortMaster images report the menu buttons and rear triggers through
    // each other's evdev codes. Keep this scoped to the fbdev PortMaster build;
    // desktop Linux controllers are not obliged to share that charming lie.
    match code {
        BTN_SELECT => Some(ControllerAction::ToggleHiddenDirectories),
        BTN_START => Some(ControllerAction::PagePreviewDown),
        BTN_TL => Some(ControllerAction::Cancel),
        BTN_TR => Some(ControllerAction::SelectCurrent),
        _ => default_key_action(code),
    }
}

const fn key_name(code: u16) -> &'static str {
    match code {
        BTN_SOUTH => "BTN_SOUTH",
        BTN_EAST => "BTN_EAST",
        BTN_WEST => "BTN_WEST",
        BTN_NORTH => "BTN_NORTH",
        BTN_TL => "BTN_TL",
        BTN_TR => "BTN_TR",
        BTN_TL2 => "BTN_TL2",
        BTN_TR2 => "BTN_TR2",
        BTN_SELECT => "BTN_SELECT",
        BTN_START => "BTN_START",
        BTN_DPAD_UP => "BTN_DPAD_UP",
        BTN_DPAD_DOWN => "BTN_DPAD_DOWN",
        BTN_DPAD_LEFT => "BTN_DPAD_LEFT",
        BTN_DPAD_RIGHT => "BTN_DPAD_RIGHT",
        _ => "unknown",
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

fn preview_horizontal_scroll_action(direction: AxisDirection) -> Option<ControllerAction> {
    match direction {
        AxisDirection::Negative => Some(ControllerAction::ScrollPreviewLeft),
        AxisDirection::Neutral => None,
        AxisDirection::Positive => Some(ControllerAction::ScrollPreviewRight),
    }
}

fn preview_vertical_scroll_action(direction: AxisDirection) -> Option<ControllerAction> {
    match direction {
        AxisDirection::Negative => Some(ControllerAction::ScrollPreviewUp),
        AxisDirection::Neutral => None,
        AxisDirection::Positive => Some(ControllerAction::ScrollPreviewDown),
    }
}

#[cfg(test)]
mod tests {
    use super::super::common::INITIAL_REPEAT_DELAY;
    use super::*;

    #[test]
    fn key_events_map_to_controller_actions() {
        assert_eq!(key_action(BTN_SOUTH), Some(ControllerAction::Accept));
        assert_eq!(key_action(BTN_EAST), Some(ControllerAction::Cancel));
        assert_eq!(key_action(BTN_WEST), Some(ControllerAction::ClearCurrent));
        assert_eq!(key_action(BTN_NORTH), Some(ControllerAction::Shift));
        #[cfg(not(all(feature = "portmaster-gui", not(feature = "gui"))))]
        {
            assert_eq!(key_action(BTN_SELECT), Some(ControllerAction::Cancel));
            assert_eq!(key_action(BTN_START), Some(ControllerAction::SelectCurrent));
            assert_eq!(
                key_action(BTN_TL),
                Some(ControllerAction::ToggleHiddenDirectories)
            );
            assert_eq!(key_action(BTN_TR), Some(ControllerAction::PagePreviewDown));
        }
        #[cfg(all(feature = "portmaster-gui", not(feature = "gui")))]
        {
            assert_eq!(
                key_action(BTN_SELECT),
                Some(ControllerAction::ToggleHiddenDirectories)
            );
            assert_eq!(
                key_action(BTN_START),
                Some(ControllerAction::PagePreviewDown)
            );
            assert_eq!(key_action(BTN_TL), Some(ControllerAction::Cancel));
            assert_eq!(key_action(BTN_TR), Some(ControllerAction::SelectCurrent));
        }
        assert_eq!(key_action(BTN_DPAD_UP), Some(ControllerAction::Up));
        assert_eq!(key_action(BTN_DPAD_DOWN), Some(ControllerAction::Down));
        assert_eq!(key_action(BTN_DPAD_LEFT), Some(ControllerAction::Left));
        assert_eq!(key_action(BTN_DPAD_RIGHT), Some(ControllerAction::Right));
    }

    #[test]
    fn default_key_events_map_to_controller_actions() {
        assert_eq!(
            default_key_action(BTN_SELECT),
            Some(ControllerAction::Cancel)
        );
        assert_eq!(
            default_key_action(BTN_START),
            Some(ControllerAction::SelectCurrent)
        );
        assert_eq!(
            default_key_action(BTN_TL),
            Some(ControllerAction::ToggleHiddenDirectories)
        );
        assert_eq!(
            default_key_action(BTN_TR),
            Some(ControllerAction::PagePreviewDown)
        );
    }

    #[test]
    fn key_names_cover_trigger_and_menu_codes() {
        assert_eq!(key_name(BTN_TL2), "BTN_TL2");
        assert_eq!(key_name(BTN_TR2), "BTN_TR2");
        assert_eq!(key_name(BTN_NORTH), "BTN_NORTH");
        assert_eq!(key_name(BTN_SELECT), "BTN_SELECT");
        assert_eq!(key_name(BTN_START), "BTN_START");
    }

    #[test]
    fn dpad_key_events_are_ignored_when_hat_axes_are_present() {
        let mut repeater = ActionRepeater::default();
        let mut held_buttons = BTreeSet::new();
        let now = Instant::now();

        assert!(
            key_actions(
                BTN_DPAD_DOWN,
                1,
                true,
                &mut held_buttons,
                &mut repeater,
                now
            )
            .actions
            .is_empty()
        );
        assert_eq!(
            key_actions(BTN_SOUTH, 1, true, &mut held_buttons, &mut repeater, now).actions,
            vec![ControllerAction::Accept]
        );
    }

    #[test]
    fn immediate_button_release_does_not_purge_queued_input() {
        let mut repeater = ActionRepeater::default();
        let mut held_buttons = BTreeSet::new();
        let now = Instant::now();

        let input = key_actions(BTN_SOUTH, 0, false, &mut held_buttons, &mut repeater, now);

        assert!(input.actions.is_empty());
        assert!(!input.released);
    }

    #[test]
    fn duplicate_repeatable_button_press_is_ignored_until_release() {
        let mut repeater = ActionRepeater::default();
        let mut held_buttons = BTreeSet::new();
        let now = Instant::now();

        assert_eq!(
            key_actions(
                BTN_DPAD_DOWN,
                1,
                false,
                &mut held_buttons,
                &mut repeater,
                now
            )
            .actions,
            vec![ControllerAction::Down]
        );
        assert!(
            key_actions(
                BTN_DPAD_DOWN,
                1,
                false,
                &mut held_buttons,
                &mut repeater,
                now
            )
            .actions
            .is_empty()
        );
        assert!(
            key_actions(
                BTN_DPAD_DOWN,
                0,
                false,
                &mut held_buttons,
                &mut repeater,
                now
            )
            .released
        );
        assert!(repeater.poll(now + INITIAL_REPEAT_DELAY).is_empty());
    }

    #[test]
    fn capability_filter_rejects_absolute_pointer_without_controller_buttons() {
        let event_bits = bitset(&[EV_KEY, EV_ABS], EV_ABS);
        let key_bits = bitset(&[], KEY_MAX);
        let abs_bits = bitset(&[ABS_X, ABS_Y], ABS_MAX);

        assert!(controller_capabilities_from_bits(&event_bits, &key_bits, &abs_bits).is_none());
    }

    #[test]
    fn capability_filter_accepts_buttons_with_navigation_source() {
        let event_bits = bitset(&[EV_KEY, EV_ABS], EV_ABS);
        let key_bits = bitset(&[BTN_SOUTH], KEY_MAX);
        let abs_bits = bitset(&[ABS_X, ABS_Y], ABS_MAX);

        assert!(controller_capabilities_from_bits(&event_bits, &key_bits, &abs_bits).is_some());
    }

    #[test]
    fn capability_filter_rejects_button_without_navigation_source() {
        let event_bits = bitset(&[EV_KEY], EV_ABS);
        let key_bits = bitset(&[BTN_SOUTH], KEY_MAX);
        let abs_bits = bitset(&[], ABS_MAX);

        assert!(controller_capabilities_from_bits(&event_bits, &key_bits, &abs_bits).is_none());
    }

    #[test]
    fn stick_axes_are_edge_triggered() {
        let mut axes = AxisState::default();
        let mut repeater = ActionRepeater::default();
        let now = Instant::now();

        assert_eq!(
            axes.update(ABS_X, 20_000, &mut repeater, now).actions,
            vec![ControllerAction::Right]
        );
        assert_eq!(
            axes.update(ABS_X, 20_001, &mut repeater, now).actions,
            Vec::new()
        );
        assert_eq!(
            axes.update(ABS_X, 0, &mut repeater, now).actions,
            Vec::new()
        );
        assert_eq!(
            axes.update(ABS_X, -20_000, &mut repeater, now).actions,
            vec![ControllerAction::Left]
        );
    }

    #[test]
    fn right_stick_vertical_scrolls_preview() {
        let mut axes = AxisState::default();
        let mut repeater = ActionRepeater::default();
        let now = Instant::now();

        assert_eq!(
            axes.update(ABS_RY, 20_000, &mut repeater, now).actions,
            vec![ControllerAction::ScrollPreviewDown]
        );
        assert_eq!(
            axes.update(ABS_RY, 0, &mut repeater, now).actions,
            Vec::new()
        );
        assert_eq!(
            axes.update(ABS_RY, -20_000, &mut repeater, now).actions,
            vec![ControllerAction::ScrollPreviewUp]
        );
    }

    #[test]
    fn right_stick_horizontal_scrolls_preview() {
        let mut axes = AxisState::default();
        let mut repeater = ActionRepeater::default();
        let now = Instant::now();

        assert_eq!(
            axes.update(ABS_RX, 20_000, &mut repeater, now).actions,
            vec![ControllerAction::ScrollPreviewRight]
        );
        assert_eq!(
            axes.update(ABS_RX, 0, &mut repeater, now).actions,
            Vec::new()
        );
        assert_eq!(
            axes.update(ABS_RX, -20_000, &mut repeater, now).actions,
            vec![ControllerAction::ScrollPreviewLeft]
        );
    }

    #[test]
    fn hat_axes_map_directly() {
        let mut axes = AxisState::default();
        let mut repeater = ActionRepeater::default();
        let now = Instant::now();

        assert_eq!(
            axes.update(ABS_HAT0Y, -1, &mut repeater, now).actions,
            vec![ControllerAction::Up]
        );
        assert_eq!(
            axes.update(ABS_HAT0Y, 1, &mut repeater, now).actions,
            vec![ControllerAction::Down]
        );
        assert_eq!(
            axes.update(ABS_HAT0X, -1, &mut repeater, now).actions,
            vec![ControllerAction::Left]
        );
        assert_eq!(
            axes.update(ABS_HAT0X, 1, &mut repeater, now).actions,
            vec![ControllerAction::Right]
        );
    }

    fn bitset(bits: &[u16], max_bit: u16) -> Vec<u8> {
        let mut values = vec![0_u8; usize::from(max_bit / 8 + 1)];
        for bit in bits {
            let byte = usize::from(bit / 8);
            values[byte] |= 1_u8 << (bit % 8);
        }
        values
    }
}
