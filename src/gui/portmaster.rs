// SPDX-License-Identifier: GPL-3.0-only

use std::env;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
#[cfg(target_os = "linux")]
use std::num::NonZeroUsize;
#[cfg(target_os = "linux")]
use std::os::fd::AsRawFd;
#[cfg(target_os = "linux")]
use std::path::Path;
use std::path::PathBuf;
use std::process::ExitCode;
#[cfg(target_os = "linux")]
use std::ptr::NonNull;
use std::sync::{Arc, Mutex};
#[cfg(target_os = "linux")]
use std::thread;
#[cfg(target_os = "linux")]
use std::time::{Duration, Instant};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(target_os = "linux")]
use super::controller::{Controller, ControllerAction, ControllerEvent};

const LOG_FILE_NAME: &str = "dream-ini-portmaster.log";
#[cfg(target_os = "linux")]
const FBIOGET_VSCREENINFO: libc::c_ulong = 0x4600;
#[cfg(target_os = "linux")]
const FBIOGET_FSCREENINFO: libc::c_ulong = 0x4602;
#[cfg(target_os = "linux")]
const FRAME_DELAY: Duration = Duration::from_millis(33);
#[cfg(target_os = "linux")]
const AUTO_EXIT_AFTER: Duration = Duration::from_mins(2);
#[cfg(target_os = "linux")]
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

#[cfg(target_os = "linux")]
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
        framebuffer.draw_pattern(phase, log)?;
        frame_count = frame_count.saturating_add(1);

        if start.elapsed() >= AUTO_EXIT_AFTER {
            write_log(log, format!("auto-exit after {frame_count} frames"));
            return Ok(());
        }

        thread::sleep(FRAME_DELAY);
    }
}

#[cfg(not(target_os = "linux"))]
fn run_probe(log: Option<&SharedLog>) -> io::Result<()> {
    let message = "PortMaster framebuffer probe is only supported on Linux";
    write_log(log, message);
    eprintln!("{message}");
    Err(io::Error::other(message))
}

#[cfg(target_os = "linux")]
#[derive(Debug)]
struct Framebuffer {
    path: PathBuf,
    file: File,
    fix: FbFixScreeninfo,
    var: FbVarScreeninfo,
    memory: NonNull<u8>,
    memory_len: NonZeroUsize,
}

#[cfg(target_os = "linux")]
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
            file,
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
                "framebuffer variable xres={} yres={} xres_virtual={} yres_virtual={} xoffset={} yoffset={} bits_per_pixel={}",
                self.var.xres,
                self.var.yres,
                self.var.xres_virtual,
                self.var.yres_virtual,
                self.var.xoffset,
                self.var.yoffset,
                self.var.bits_per_pixel
            ),
        );
        log_derived_page_info(log, &self.var);
        match visible_viewport(&self.fix, &self.var) {
            Ok(viewport) => log_visible_viewport(log, &viewport),
            Err(error) => write_log(log, format!("framebuffer viewport unavailable: {error}")),
        }
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

    fn draw_pattern(&mut self, phase: u32, log: Option<&SharedLog>) -> io::Result<()> {
        self.var = get_var_info(&self.file).map_err(|error| {
            write_log(
                log,
                format!("failed to refresh framebuffer variable info: {error}"),
            );
            error
        })?;
        self.validate_format()?;
        let viewport = visible_viewport(&self.fix, &self.var)?;
        log_visible_viewport(log, &viewport);
        log_derived_page_info(log, &self.var);

        // SAFETY: memory points to a live mmap of memory_len bytes owned by self.
        let pixels =
            unsafe { std::slice::from_raw_parts_mut(self.memory.as_ptr(), self.memory_len.get()) };
        let border = viewport.width.min(viewport.height).clamp(1, 10);
        let bar_width = (viewport.width / 6).max(1);
        for y in 0..viewport.height {
            let row_offset = y
                .checked_mul(viewport.line_length)
                .and_then(|offset| viewport.base_offset.checked_add(offset))
                .ok_or_else(|| io::Error::other("framebuffer row offset overflow"))?;
            for x in 0..viewport.width {
                let x_offset = x
                    .checked_mul(viewport.bytes_per_pixel)
                    .ok_or_else(|| io::Error::other("framebuffer pixel offset overflow"))?;
                let pixel_offset = row_offset
                    .checked_add(x_offset)
                    .ok_or_else(|| io::Error::other("framebuffer pixel offset overflow"))?;
                let pixel_end = pixel_offset
                    .checked_add(viewport.bytes_per_pixel)
                    .ok_or_else(|| io::Error::other("framebuffer pixel end overflow"))?;
                let row_pixel_end = viewport
                    .x_offset_bytes
                    .checked_add(x_offset)
                    .and_then(|offset| offset.checked_add(viewport.bytes_per_pixel))
                    .ok_or_else(|| io::Error::other("framebuffer row pixel offset overflow"))?;
                if pixel_end > pixels.len() || row_pixel_end > viewport.line_length {
                    continue;
                }

                let color = pattern_color(
                    x,
                    y,
                    viewport.width,
                    viewport.height,
                    bar_width,
                    border,
                    phase,
                );
                let packed = pack_color(&self.var, color);
                write_pixel(
                    &mut pixels[pixel_offset..pixel_end],
                    viewport.bytes_per_pixel,
                    packed,
                );
            }
        }

        Ok(())
    }
}

#[cfg(target_os = "linux")]
impl Drop for Framebuffer {
    fn drop(&mut self) {
        // SAFETY: memory and memory_len are the same mapping returned by mmap in
        // Framebuffer::open_path and have not been unmapped yet.
        let _ = unsafe { libc::munmap(self.memory.as_ptr().cast(), self.memory_len.get()) };
    }
}

#[cfg(target_os = "linux")]
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct FbBitfield {
    offset: u32,
    length: u32,
    msb_right: u32,
}

#[cfg(target_os = "linux")]
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

#[cfg(target_os = "linux")]
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

#[cfg(target_os = "linux")]
fn get_fix_info(file: &File) -> io::Result<FbFixScreeninfo> {
    let mut fix = FbFixScreeninfo::default();
    // SAFETY: fix points to writable memory matching Linux fb_fix_screeninfo.
    let result = unsafe { libc::ioctl(file.as_raw_fd(), FBIOGET_FSCREENINFO, &mut fix) };
    if result < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(fix)
}

#[cfg(target_os = "linux")]
fn get_var_info(file: &File) -> io::Result<FbVarScreeninfo> {
    let mut var = FbVarScreeninfo::default();
    // SAFETY: var points to writable memory matching Linux fb_var_screeninfo.
    let result = unsafe { libc::ioctl(file.as_raw_fd(), FBIOGET_VSCREENINFO, &mut var) };
    if result < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(var)
}

#[cfg(target_os = "linux")]
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

#[cfg(target_os = "linux")]
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

#[cfg(target_os = "linux")]
fn bitfield_info(field: &FbBitfield) -> String {
    format!(
        "offset={} length={} msb_right={}",
        field.offset, field.length, field.msb_right
    )
}

#[cfg(target_os = "linux")]
#[derive(Debug, PartialEq, Eq)]
struct VisibleViewport {
    xoffset: u32,
    yoffset: u32,
    width: usize,
    height: usize,
    line_length: usize,
    bytes_per_pixel: usize,
    x_offset_bytes: usize,
    base_offset: usize,
}

#[cfg(target_os = "linux")]
fn visible_viewport(fix: &FbFixScreeninfo, var: &FbVarScreeninfo) -> io::Result<VisibleViewport> {
    let bytes_per_pixel = usize::try_from(var.bits_per_pixel / 8)
        .map_err(|_| io::Error::other("framebuffer bytes per pixel does not fit usize"))?;
    let line_length = usize::try_from(fix.line_length)
        .map_err(|_| io::Error::other("framebuffer line length does not fit usize"))?;
    let width = var.xres.min(var.xres_virtual.saturating_sub(var.xoffset));
    let height = var.yres.min(var.yres_virtual.saturating_sub(var.yoffset));
    let width = usize::try_from(width)
        .map_err(|_| io::Error::other("framebuffer width does not fit usize"))?;
    let height = usize::try_from(height)
        .map_err(|_| io::Error::other("framebuffer height does not fit usize"))?;
    if width == 0 || height == 0 || line_length == 0 || bytes_per_pixel == 0 {
        return Err(io::Error::other("framebuffer dimensions are empty"));
    }

    let yoffset = usize::try_from(var.yoffset)
        .map_err(|_| io::Error::other("framebuffer yoffset does not fit usize"))?;
    let xoffset = usize::try_from(var.xoffset)
        .map_err(|_| io::Error::other("framebuffer xoffset does not fit usize"))?;
    let y_offset_bytes = yoffset
        .checked_mul(line_length)
        .ok_or_else(|| io::Error::other("framebuffer yoffset byte offset overflow"))?;
    let x_offset_bytes = xoffset
        .checked_mul(bytes_per_pixel)
        .ok_or_else(|| io::Error::other("framebuffer xoffset byte offset overflow"))?;
    let base_offset = y_offset_bytes
        .checked_add(x_offset_bytes)
        .ok_or_else(|| io::Error::other("framebuffer visible base offset overflow"))?;

    Ok(VisibleViewport {
        xoffset: var.xoffset,
        yoffset: var.yoffset,
        width,
        height,
        line_length,
        bytes_per_pixel,
        x_offset_bytes,
        base_offset,
    })
}

#[cfg(target_os = "linux")]
fn log_visible_viewport(log: Option<&SharedLog>, viewport: &VisibleViewport) {
    write_log(
        log,
        format!(
            "framebuffer visible viewport xoffset={} yoffset={} width={} height={} base_offset={} line_length={} bytes_per_pixel={}",
            viewport.xoffset,
            viewport.yoffset,
            viewport.width,
            viewport.height,
            viewport.base_offset,
            viewport.line_length,
            viewport.bytes_per_pixel
        ),
    );
}

#[cfg(target_os = "linux")]
fn log_derived_page_info(log: Option<&SharedLog>, var: &FbVarScreeninfo) {
    if var.xoffset == 0
        && var.yres > 0
        && var.yres_virtual > 0
        && var.yoffset.is_multiple_of(var.yres)
        && var.yres_virtual.is_multiple_of(var.yres)
    {
        write_log(
            log,
            format!(
                "framebuffer derived page info page_index={} page_count={}",
                var.yoffset / var.yres,
                var.yres_virtual / var.yres
            ),
        );
    }
}

#[cfg(target_os = "linux")]
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

#[cfg(target_os = "linux")]
fn pack_color(var: &FbVarScreeninfo, (red, green, blue): (u8, u8, u8)) -> u32 {
    pack_channel(red, &var.red) | pack_channel(green, &var.green) | pack_channel(blue, &var.blue)
}

#[cfg(target_os = "linux")]
fn pack_channel(value: u8, field: &FbBitfield) -> u32 {
    if field.length == 0 || field.offset >= 32 {
        return 0;
    }
    let length = field.length.min(32 - field.offset);
    let max_value = (1_u64 << length) - 1;
    let scaled = (u64::from(value) * max_value + 127) / 255;
    u32::try_from(scaled << field.offset).unwrap_or(0)
}

#[cfg(target_os = "linux")]
fn write_pixel(pixel: &mut [u8], bytes_per_pixel: usize, packed: u32) {
    let bytes = packed.to_ne_bytes();
    pixel[..bytes_per_pixel].copy_from_slice(&bytes[..bytes_per_pixel]);
}

#[cfg(target_os = "linux")]
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

#[cfg(target_os = "linux")]
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

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn visible_viewport_uses_panned_page_base_offset() {
        let fix = FbFixScreeninfo {
            line_length: 2_560,
            ..Default::default()
        };
        let var = FbVarScreeninfo {
            xres: 640,
            yres: 480,
            xres_virtual: 640,
            yres_virtual: 960,
            yoffset: 480,
            bits_per_pixel: 32,
            ..Default::default()
        };

        let viewport = visible_viewport(&fix, &var).expect("visible viewport");

        assert_eq!(viewport.width, 640);
        assert_eq!(viewport.height, 480);
        assert_eq!(viewport.bytes_per_pixel, 4);
        assert_eq!(viewport.base_offset, 480 * 2_560);
    }

    #[test]
    fn visible_viewport_clips_width_to_remaining_virtual_row() {
        let fix = FbFixScreeninfo {
            line_length: 3_200,
            ..Default::default()
        };
        let var = FbVarScreeninfo {
            xres: 640,
            yres: 480,
            xres_virtual: 800,
            yres_virtual: 480,
            xoffset: 200,
            bits_per_pixel: 32,
            ..Default::default()
        };

        let viewport = visible_viewport(&fix, &var).expect("visible viewport");

        assert_eq!(viewport.width, 600);
        assert_eq!(viewport.height, 480);
        assert_eq!(viewport.x_offset_bytes, 800);
        assert_eq!(viewport.base_offset, 800);
    }
}
