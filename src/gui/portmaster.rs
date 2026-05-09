// SPDX-License-Identifier: GPL-3.0-only

use std::env;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::num::NonZeroUsize;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::ptr::NonNull;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::controller::{Controller, ControllerAction, ControllerEvent};

const LOG_FILE_NAME: &str = "dream-ini-portmaster.log";
const FBIOGET_VSCREENINFO: libc::c_ulong = 0x4600;
const FBIOGET_FSCREENINFO: libc::c_ulong = 0x4602;
const FRAME_DELAY: Duration = Duration::from_millis(33);
const AUTO_EXIT_AFTER: Duration = Duration::from_secs(120);
const FRAMEBUFFER_PATHS: [&str; 2] = ["/dev/fb0", "/dev/graphics/fb0"];

type SharedLog = Arc<Mutex<File>>;

pub(crate) fn run() -> ExitCode {
    let log = open_log().map(Mutex::new).map(Arc::new);
    install_panic_hook(log.clone());
    log_startup(log.as_ref());

    match run_probe(log.as_ref()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            write_log(log.as_ref(), format!("fatal error: {error}"));
            ExitCode::FAILURE
        }
    }
}

fn run_probe(log: Option<&SharedLog>) -> io::Result<()> {
    let mut framebuffer = Framebuffer::open()?;
    framebuffer.log_info(log);
    framebuffer.validate_format()?;

    let mut controller = Controller::new(egui::Context::default());
    write_log(log, "controller worker started");

    let start = Instant::now();
    let mut phase = 0_u32;
    let mut input_count = 0_u64;
    let mut frame_count = 0_u64;
    loop {
        for event in controller.drain_events() {
            match event {
                ControllerEvent::Action(action) => {
                    input_count = input_count.saturating_add(1);
                    phase = phase.wrapping_add(action_phase_step(action));
                    write_log(
                        log,
                        format!(
                            "controller action={} count={input_count}",
                            action_name(action)
                        ),
                    );
                    if action == ControllerAction::Cancel {
                        write_log(log, "quit requested by cancel action");
                        return Ok(());
                    }
                }
                ControllerEvent::Available(available) => {
                    write_log(
                        log,
                        format!("controller availability changed available={available}"),
                    );
                }
                ControllerEvent::PurgeQueuedActions => {
                    write_log(log, "controller purge queued actions");
                }
            }
        }

        phase = phase.wrapping_add(1);
        framebuffer.draw_pattern(phase)?;
        frame_count = frame_count.saturating_add(1);

        if start.elapsed() >= AUTO_EXIT_AFTER {
            write_log(log, format!("auto-exit after {frame_count} frames"));
            return Ok(());
        }

        thread::sleep(FRAME_DELAY);
    }
}

#[derive(Debug)]
struct Framebuffer {
    path: PathBuf,
    _file: File,
    fix: FbFixScreeninfo,
    var: FbVarScreeninfo,
    memory: NonNull<u8>,
    memory_len: NonZeroUsize,
}

impl Framebuffer {
    fn open() -> io::Result<Self> {
        let mut last_error = None;
        for path in FRAMEBUFFER_PATHS {
            match Self::open_path(Path::new(path)) {
                Ok(framebuffer) => return Ok(framebuffer),
                Err(error) => last_error = Some((path, error)),
            }
        }

        let Some((path, error)) = last_error else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "no framebuffer paths configured",
            ));
        };
        Err(io::Error::new(
            error.kind(),
            format!("failed to open framebuffer; last path {path}: {error}"),
        ))
    }

    fn open_path(path: &Path) -> io::Result<Self> {
        let file = OpenOptions::new().read(true).write(true).open(path)?;
        let fix = get_fix_info(&file)?;
        let var = get_var_info(&file)?;
        let memory_len = framebuffer_memory_len(&fix, &var)?;
        // SAFETY: mmap is called with a valid framebuffer file descriptor.  On
        // success it returns a mapping at least memory_len bytes long, which is
        // retained until Drop calls munmap with the same pointer and length.
        let memory = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                memory_len.get(),
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                file.as_raw_fd(),
                0,
            )
        };
        if memory == libc::MAP_FAILED {
            return Err(io::Error::last_os_error());
        }
        let memory = NonNull::new(memory.cast::<u8>())
            .ok_or_else(|| io::Error::other("framebuffer mmap returned a null pointer"))?;

        Ok(Self {
            path: path.to_owned(),
            _file: file,
            fix,
            var,
            memory,
            memory_len,
        })
    }

    fn validate_format(&self) -> io::Result<()> {
        match self.var.bits_per_pixel {
            16 | 32 => Ok(()),
            bits_per_pixel => Err(io::Error::other(format!(
                "unsupported framebuffer format: {bits_per_pixel} bits per pixel"
            ))),
        }
    }

    fn log_info(&self, log: Option<&SharedLog>) {
        write_log(log, format!("framebuffer path={}", self.path.display()));
        write_log(log, format!("framebuffer id={}", fb_id(&self.fix)));
        write_log(
            log,
            format!(
                "framebuffer fixed smem_len={} line_length={}",
                self.fix.smem_len, self.fix.line_length
            ),
        );
        write_log(
            log,
            format!(
                "framebuffer variable xres={} yres={} xres_virtual={} yres_virtual={} bits_per_pixel={}",
                self.var.xres,
                self.var.yres,
                self.var.xres_virtual,
                self.var.yres_virtual,
                self.var.bits_per_pixel
            ),
        );
        write_log(
            log,
            format!("framebuffer red {}", bitfield_info(&self.var.red)),
        );
        write_log(
            log,
            format!("framebuffer green {}", bitfield_info(&self.var.green)),
        );
        write_log(
            log,
            format!("framebuffer blue {}", bitfield_info(&self.var.blue)),
        );
        write_log(
            log,
            format!("framebuffer transp {}", bitfield_info(&self.var.transp)),
        );
        write_log(
            log,
            format!(
                "framebuffer grayscale={} activate={} rotate={}",
                self.var.grayscale, self.var.activate, self.var.rotate
            ),
        );
    }

    fn draw_pattern(&mut self, phase: u32) -> io::Result<()> {
        self.validate_format()?;
        let bytes_per_pixel = usize::try_from(self.var.bits_per_pixel / 8)
            .expect("supported bits per pixel fits usize");
        let line_length = usize::try_from(self.fix.line_length)
            .map_err(|_| io::Error::other("framebuffer line length does not fit usize"))?;
        let width = self.var.xres.min(self.var.xres_virtual);
        let height = self.var.yres.min(self.var.yres_virtual);
        let width = usize::try_from(width)
            .map_err(|_| io::Error::other("framebuffer width does not fit usize"))?;
        let height = usize::try_from(height)
            .map_err(|_| io::Error::other("framebuffer height does not fit usize"))?;
        if width == 0 || height == 0 || line_length == 0 {
            return Err(io::Error::other("framebuffer dimensions are empty"));
        }

        // SAFETY: memory points to a live mmap of memory_len bytes owned by self.
        let pixels =
            unsafe { std::slice::from_raw_parts_mut(self.memory.as_ptr(), self.memory_len.get()) };
        let border = width.min(height).clamp(1, 10);
        let bar_width = (width / 6).max(1);
        for y in 0..height {
            let row_offset = y
                .checked_mul(line_length)
                .ok_or_else(|| io::Error::other("framebuffer row offset overflow"))?;
            for x in 0..width {
                let pixel_offset = row_offset
                    .checked_add(
                        x.checked_mul(bytes_per_pixel)
                            .ok_or_else(|| io::Error::other("framebuffer pixel offset overflow"))?,
                    )
                    .ok_or_else(|| io::Error::other("framebuffer pixel offset overflow"))?;
                let pixel_end = pixel_offset
                    .checked_add(bytes_per_pixel)
                    .ok_or_else(|| io::Error::other("framebuffer pixel end overflow"))?;
                if pixel_end > pixels.len() || x * bytes_per_pixel >= line_length {
                    continue;
                }

                let color = pattern_color(x, y, width, height, bar_width, border, phase);
                let packed = pack_color(&self.var, color);
                write_pixel(
                    &mut pixels[pixel_offset..pixel_end],
                    bytes_per_pixel,
                    packed,
                );
            }
        }

        Ok(())
    }
}

impl Drop for Framebuffer {
    fn drop(&mut self) {
        // SAFETY: memory and memory_len are the same mapping returned by mmap in
        // Framebuffer::open_path and have not been unmapped yet.
        let _ = unsafe { libc::munmap(self.memory.as_ptr().cast(), self.memory_len.get()) };
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct FbBitfield {
    offset: u32,
    length: u32,
    msb_right: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct FbFixScreeninfo {
    id: [libc::c_char; 16],
    smem_start: libc::c_ulong,
    smem_len: u32,
    type_: u32,
    type_aux: u32,
    visual: u32,
    xpanstep: u16,
    ypanstep: u16,
    ywrapstep: u16,
    line_length: u32,
    mmio_start: libc::c_ulong,
    mmio_len: u32,
    accel: u32,
    capabilities: u16,
    reserved: [u16; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct FbVarScreeninfo {
    xres: u32,
    yres: u32,
    xres_virtual: u32,
    yres_virtual: u32,
    xoffset: u32,
    yoffset: u32,
    bits_per_pixel: u32,
    grayscale: u32,
    red: FbBitfield,
    green: FbBitfield,
    blue: FbBitfield,
    transp: FbBitfield,
    nonstd: u32,
    activate: u32,
    height: u32,
    width: u32,
    accel_flags: u32,
    pixclock: u32,
    left_margin: u32,
    right_margin: u32,
    upper_margin: u32,
    lower_margin: u32,
    hsync_len: u32,
    vsync_len: u32,
    sync: u32,
    vmode: u32,
    rotate: u32,
    colorspace: u32,
    reserved: [u32; 4],
}

fn get_fix_info(file: &File) -> io::Result<FbFixScreeninfo> {
    let mut fix = FbFixScreeninfo::default();
    // SAFETY: fix points to writable memory matching Linux fb_fix_screeninfo.
    let result = unsafe { libc::ioctl(file.as_raw_fd(), FBIOGET_FSCREENINFO, &mut fix) };
    if result < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(fix)
}

fn get_var_info(file: &File) -> io::Result<FbVarScreeninfo> {
    let mut var = FbVarScreeninfo::default();
    // SAFETY: var points to writable memory matching Linux fb_var_screeninfo.
    let result = unsafe { libc::ioctl(file.as_raw_fd(), FBIOGET_VSCREENINFO, &mut var) };
    if result < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(var)
}

fn framebuffer_memory_len(
    fix: &FbFixScreeninfo,
    var: &FbVarScreeninfo,
) -> io::Result<NonZeroUsize> {
    let length = if fix.smem_len != 0 {
        usize::try_from(fix.smem_len)
            .map_err(|_| io::Error::other("framebuffer smem_len does not fit usize"))?
    } else {
        let line_length = usize::try_from(fix.line_length)
            .map_err(|_| io::Error::other("framebuffer line_length does not fit usize"))?;
        let yres_virtual = usize::try_from(var.yres_virtual)
            .map_err(|_| io::Error::other("framebuffer yres_virtual does not fit usize"))?;
        line_length
            .checked_mul(yres_virtual)
            .ok_or_else(|| io::Error::other("framebuffer computed memory length overflow"))?
    };
    NonZeroUsize::new(length).ok_or_else(|| io::Error::other("framebuffer memory length is zero"))
}

fn fb_id(fix: &FbFixScreeninfo) -> String {
    let nul = fix
        .id
        .iter()
        .position(|character| *character == 0)
        .unwrap_or(fix.id.len());
    let bytes = fix.id[..nul]
        .iter()
        .map(|character| u8::try_from(*character).unwrap_or(b'?'))
        .collect::<Vec<_>>();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn bitfield_info(field: &FbBitfield) -> String {
    format!(
        "offset={} length={} msb_right={}",
        field.offset, field.length, field.msb_right
    )
}

fn pattern_color(
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    bar_width: usize,
    border: usize,
    phase: u32,
) -> (u8, u8, u8) {
    if x < border || y < border || width - x <= border || height - y <= border {
        return (255, 255, 255);
    }

    let pulse = u8::try_from((phase.wrapping_mul(3)) & 0xff).expect("pulse is masked to u8");
    match ((x / bar_width) + usize::try_from(phase / 30).unwrap_or(0)) % 6 {
        0 => (255, pulse / 3, pulse / 3),
        1 => (pulse / 3, 255, pulse / 3),
        2 => (pulse / 3, pulse / 3, 255),
        3 => (255, 255, pulse / 4),
        4 => (pulse / 4, 255, 255),
        _ => (255, pulse / 4, 255),
    }
}

fn pack_color(var: &FbVarScreeninfo, (red, green, blue): (u8, u8, u8)) -> u32 {
    pack_channel(red, &var.red) | pack_channel(green, &var.green) | pack_channel(blue, &var.blue)
}

fn pack_channel(value: u8, field: &FbBitfield) -> u32 {
    if field.length == 0 || field.offset >= 32 {
        return 0;
    }
    let length = field.length.min(32 - field.offset);
    let max_value = (1_u64 << length) - 1;
    let scaled = (u64::from(value) * max_value + 127) / 255;
    u32::try_from(scaled << field.offset).unwrap_or(0)
}

fn write_pixel(pixel: &mut [u8], bytes_per_pixel: usize, packed: u32) {
    let bytes = packed.to_ne_bytes();
    pixel[..bytes_per_pixel].copy_from_slice(&bytes[..bytes_per_pixel]);
}

const fn action_phase_step(action: ControllerAction) -> u32 {
    match action {
        ControllerAction::Up => 17,
        ControllerAction::Down => 29,
        ControllerAction::Left => 41,
        ControllerAction::Right => 53,
        ControllerAction::Accept => 83,
        ControllerAction::Cancel => 127,
        ControllerAction::ClearCurrent => 67,
        ControllerAction::SelectCurrent => 71,
        ControllerAction::ToggleHiddenDirectories => 73,
        ControllerAction::PagePreviewDown => 79,
        ControllerAction::ScrollPreviewLeft => 89,
        ControllerAction::ScrollPreviewRight => 97,
        ControllerAction::ScrollPreviewUp => 101,
        ControllerAction::ScrollPreviewDown => 103,
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
