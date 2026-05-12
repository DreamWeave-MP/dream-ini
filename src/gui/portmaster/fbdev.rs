// SPDX-License-Identifier: GPL-3.0-only

use std::env;
use std::fs::{File, OpenOptions};
use std::io;
use std::num::NonZeroUsize;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::ptr::NonNull;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use super::log::{SharedLog, write_log};
use super::pacing::{DisplayTiming, format_repaint_delay};
use super::renderer::SoftwareRenderer;
use super::surface::SoftwareSurface;
use super::{GuiFrame, GuiShell};

const FBIOGET_VSCREENINFO: libc::c_ulong = 0x4600;
const FBIOGET_FSCREENINFO: libc::c_ulong = 0x4602;
const FRAMEBUFFER_PATHS: [&str; 2] = ["/dev/fb0", "/dev/graphics/fb0"];
pub(super) const DRAW_ENV_VAR: &str = "DREAM_INI_FB_DRAW";
const FORCE_GENERIC_BLIT_ENV_VAR: &str = "DREAM_INI_PM_FORCE_GENERIC_BLIT";
const MAX_SNAPSHOT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct FramebufferDrawOutcome {
    pub(super) repaint_delay: Duration,
}

#[derive(Debug)]
pub(super) struct Framebuffer {
    path: PathBuf,
    file: File,
    fix: FbFixScreeninfo,
    var: FbVarScreeninfo,
    memory: NonNull<u8>,
    memory_len: NonZeroUsize,
}

impl Framebuffer {
    pub(super) fn open() -> io::Result<Self> {
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

    pub(super) fn validate_format(&self) -> io::Result<()> {
        match self.var.bits_per_pixel {
            16 | 32 => Ok(()),
            bits_per_pixel => Err(io::Error::other(format!(
                "unsupported framebuffer format: {bits_per_pixel} bits per pixel"
            ))),
        }
    }

    pub(super) const fn refresh_timing(&self) -> DisplayTiming {
        DisplayTiming::new(
            self.var.pixclock,
            self.var.xres,
            self.var.yres,
            [
                self.var.left_margin,
                self.var.right_margin,
                self.var.hsync_len,
            ],
            [
                self.var.upper_margin,
                self.var.lower_margin,
                self.var.vsync_len,
            ],
        )
    }

    pub(super) fn log_info(&self, log: Option<&SharedLog>) {
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

    pub(super) fn draw_egui_gui<S: GuiShell>(
        &mut self,
        renderer: &mut SoftwareRenderer,
        frame: &mut GuiFrame<'_, S>,
    ) -> io::Result<FramebufferDrawOutcome> {
        let log_frame = frame.log_frame;
        let total_start = log_frame.then(Instant::now);
        let stage_start = log_frame.then(Instant::now);
        self.var = get_var_info(&self.file).map_err(|error| {
            write_log(
                frame.log,
                format!("failed to refresh framebuffer variable info: {error}"),
            );
            error
        })?;
        let var_refresh_elapsed = elapsed_micros(stage_start);

        let stage_start = log_frame.then(Instant::now);
        self.validate_format()?;
        let viewport = visible_viewport(&self.fix, &self.var)?;
        let validate_viewport_elapsed = elapsed_micros(stage_start);
        if log_frame {
            log_visible_viewport(frame.log, &viewport);
            log_derived_page_info(frame.log, &self.var);
        }

        let stage_start = log_frame.then(Instant::now);
        let repaint_delay = renderer.render(viewport.width, viewport.height, frame)?;
        let render_elapsed = elapsed_micros(stage_start);

        let stage_start = log_frame.then(Instant::now);
        self.capture_snapshot_if_needed(frame.snapshots, &viewport, frame.log)?;
        let snapshot_elapsed = elapsed_micros(stage_start);

        let stage_start = log_frame.then(Instant::now);
        let blit_format = self.blit_rgba_surface(renderer.surface(), &viewport)?;
        let blit_elapsed = elapsed_micros(stage_start);
        let total_elapsed = elapsed_micros(total_start);
        if frame.log_render_stats {
            log_present_stats(
                frame.log,
                frame.frame_index,
                renderer.surface(),
                &viewport,
                &self.var,
                blit_format,
            )?;
        }
        if log_frame {
            let blit_format_name = blit_format.name();
            let repaint_delay = format_repaint_delay(repaint_delay);
            write_log(
                frame.log,
                format!(
                    "framebuffer draw timings frame={} blit_format={blit_format_name} var_refresh_us={var_refresh_elapsed} validate_viewport_us={validate_viewport_elapsed} render_us={render_elapsed} snapshot_us={snapshot_elapsed} blit_us={blit_elapsed} repaint_delay={repaint_delay} total_us={total_elapsed}",
                    frame.frame_index,
                ),
            );
        }
        Ok(FramebufferDrawOutcome { repaint_delay })
    }

    fn blit_rgba_surface(
        &mut self,
        surface: &SoftwareSurface,
        viewport: &VisibleViewport,
    ) -> io::Result<BlitFormat> {
        if surface.width != viewport.width || surface.height != viewport.height {
            return Err(io::Error::other(
                "software surface size does not match viewport",
            ));
        }

        let blit_format = if force_generic_blit_enabled() {
            BlitFormat::GenericPackColor
        } else {
            detect_fast_blit_format(&self.var).map_or(BlitFormat::GenericPackColor, |format| {
                BlitFormat::Fast32(Fast32BlitMode::from_byte_aligned(format))
            })
        };

        // SAFETY: memory points to a live mmap of memory_len bytes owned by self.
        let pixels =
            unsafe { std::slice::from_raw_parts_mut(self.memory.as_ptr(), self.memory_len.get()) };
        if let BlitFormat::Fast32(format) = blit_format {
            blit_fast32_rows(pixels, surface, viewport, format)?;
            return Ok(blit_format);
        }

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

                let source_offset = y
                    .checked_mul(surface.width)
                    .and_then(|offset| offset.checked_add(x))
                    .and_then(|offset| offset.checked_mul(4))
                    .ok_or_else(|| io::Error::other("software surface pixel offset overflow"))?;
                let color = (
                    surface.pixels[source_offset],
                    surface.pixels[source_offset + 1],
                    surface.pixels[source_offset + 2],
                );
                let pixel = &mut pixels[pixel_offset..pixel_end];
                let packed = pack_color(&self.var, color);
                write_pixel(pixel, viewport.bytes_per_pixel, packed);
            }
        }

        Ok(blit_format)
    }

    fn capture_snapshot_if_needed(
        &self,
        snapshots: &mut Vec<FramebufferSnapshot>,
        viewport: &VisibleViewport,
        log: Option<&SharedLog>,
    ) -> io::Result<()> {
        if snapshots
            .iter()
            .any(|snapshot| snapshot.viewport == *viewport)
        {
            return Ok(());
        }
        let snapshot_bytes = viewport_snapshot_bytes(viewport)?;
        let used_bytes = snapshot_bytes_used(snapshots)?;
        let new_total = used_bytes
            .checked_add(snapshot_bytes)
            .ok_or_else(|| io::Error::other("framebuffer snapshot budget overflow"))?;
        if new_total > MAX_SNAPSHOT_BYTES {
            write_log(
                log,
                format!(
                    "refusing framebuffer snapshot bytes_used={used_bytes} new_snapshot_bytes={snapshot_bytes} budget={MAX_SNAPSHOT_BYTES}"
                ),
            );
            return Err(io::Error::other("framebuffer snapshot budget exceeded"));
        }

        let snapshot = self.capture_snapshot(viewport)?;
        write_log(
            log,
            format!(
                "captured framebuffer snapshot base_offset={} width={} height={} bytes={}",
                snapshot.viewport.base_offset,
                snapshot.viewport.width,
                snapshot.viewport.height,
                snapshot.bytes.len()
            ),
        );
        snapshots.push(snapshot);
        Ok(())
    }

    fn capture_snapshot(&self, viewport: &VisibleViewport) -> io::Result<FramebufferSnapshot> {
        // SAFETY: memory points to a live mmap of memory_len bytes owned by self.
        let pixels =
            unsafe { std::slice::from_raw_parts(self.memory.as_ptr(), self.memory_len.get()) };
        let row_bytes = viewport_row_bytes(viewport)?;
        let total_bytes = viewport_snapshot_bytes(viewport)?;
        let mut bytes = Vec::with_capacity(total_bytes);
        for y in 0..viewport.height {
            let row_offset = viewport_row_offset(viewport, y)?;
            let row_end = row_offset
                .checked_add(row_bytes)
                .ok_or_else(|| io::Error::other("framebuffer snapshot row end overflow"))?;
            if row_end > pixels.len() {
                return Err(io::Error::other("framebuffer snapshot exceeds mmap length"));
            }
            bytes.extend_from_slice(&pixels[row_offset..row_end]);
        }

        Ok(FramebufferSnapshot {
            viewport: viewport.clone(),
            bytes,
        })
    }

    pub(super) fn restore_snapshots(
        &mut self,
        snapshots: &[FramebufferSnapshot],
        log: Option<&SharedLog>,
    ) -> io::Result<()> {
        if snapshots.is_empty() {
            write_log(log, "no framebuffer snapshots to restore");
            return Ok(());
        }

        write_log(
            log,
            format!("restoring {} framebuffer snapshot(s)", snapshots.len()),
        );
        for snapshot in snapshots.iter().rev() {
            self.restore_snapshot(snapshot)?;
            write_log(
                log,
                format!(
                    "restored framebuffer snapshot base_offset={} width={} height={} bytes={}",
                    snapshot.viewport.base_offset,
                    snapshot.viewport.width,
                    snapshot.viewport.height,
                    snapshot.bytes.len()
                ),
            );
        }
        Ok(())
    }

    fn restore_snapshot(&mut self, snapshot: &FramebufferSnapshot) -> io::Result<()> {
        // SAFETY: memory points to a live mmap of memory_len bytes owned by self.
        let pixels =
            unsafe { std::slice::from_raw_parts_mut(self.memory.as_ptr(), self.memory_len.get()) };
        let row_bytes = viewport_row_bytes(&snapshot.viewport)?;
        let expected_bytes = viewport_snapshot_bytes(&snapshot.viewport)?;
        if snapshot.bytes.len() != expected_bytes {
            return Err(io::Error::other(
                "framebuffer snapshot size does not match viewport",
            ));
        }

        for y in 0..snapshot.viewport.height {
            let destination_offset = viewport_row_offset(&snapshot.viewport, y)?;
            let destination_end = destination_offset
                .checked_add(row_bytes)
                .ok_or_else(|| io::Error::other("framebuffer restore row end overflow"))?;
            let source_offset = y
                .checked_mul(row_bytes)
                .ok_or_else(|| io::Error::other("framebuffer restore source offset overflow"))?;
            let source_end = source_offset
                .checked_add(row_bytes)
                .ok_or_else(|| io::Error::other("framebuffer restore source end overflow"))?;
            if destination_end > pixels.len() || source_end > snapshot.bytes.len() {
                return Err(io::Error::other(
                    "framebuffer restore exceeds buffer length",
                ));
            }
            pixels[destination_offset..destination_end]
                .copy_from_slice(&snapshot.bytes[source_offset..source_end]);
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

#[derive(Clone, Debug, PartialEq, Eq)]
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

#[derive(Debug)]
pub(super) struct FramebufferSnapshot {
    viewport: VisibleViewport,
    bytes: Vec<u8>,
}

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
    let row_bytes = width
        .checked_mul(bytes_per_pixel)
        .ok_or_else(|| io::Error::other("framebuffer visible row byte count overflow"))?;
    let row_end = x_offset_bytes
        .checked_add(row_bytes)
        .ok_or_else(|| io::Error::other("framebuffer visible row end overflow"))?;
    if row_end > line_length {
        return Err(io::Error::other(
            "framebuffer visible viewport crosses scanline boundary",
        ));
    }

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

fn log_present_stats(
    log: Option<&SharedLog>,
    frame_index: u64,
    surface: &SoftwareSurface,
    viewport: &VisibleViewport,
    var: &FbVarScreeninfo,
    blit_format: BlitFormat,
) -> io::Result<()> {
    let source_bytes_read = surface.pixels.len();
    let framebuffer_bytes_written = viewport_snapshot_bytes(viewport)?;
    write_log(
        log,
        format!(
            "framebuffer present frame={frame_index} mode=full-blit blit_format={} source_format=rgba8888 source_bytes_read={source_bytes_read} framebuffer_bytes_written={framebuffer_bytes_written} surface={}x{} visible_width={} visible_height={} stride={} xoffset={} yoffset={} bits_per_pixel={} bytes_per_pixel={} red={} green={} blue={} transp={}",
            blit_format.name(),
            surface.width,
            surface.height,
            viewport.width,
            viewport.height,
            viewport.line_length,
            viewport.xoffset,
            viewport.yoffset,
            var.bits_per_pixel,
            viewport.bytes_per_pixel,
            bitfield_info(&var.red),
            bitfield_info(&var.green),
            bitfield_info(&var.blue),
            bitfield_info(&var.transp),
        ),
    );
    Ok(())
}

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

fn viewport_row_bytes(viewport: &VisibleViewport) -> io::Result<usize> {
    viewport
        .width
        .checked_mul(viewport.bytes_per_pixel)
        .ok_or_else(|| io::Error::other("framebuffer viewport row byte count overflow"))
}

fn viewport_snapshot_bytes(viewport: &VisibleViewport) -> io::Result<usize> {
    viewport_row_bytes(viewport)?
        .checked_mul(viewport.height)
        .ok_or_else(|| io::Error::other("framebuffer snapshot byte count overflow"))
}

fn snapshot_bytes_used(snapshots: &[FramebufferSnapshot]) -> io::Result<usize> {
    snapshots.iter().try_fold(0_usize, |total, snapshot| {
        total
            .checked_add(snapshot.bytes.len())
            .ok_or_else(|| io::Error::other("framebuffer snapshot bytes used overflow"))
    })
}

fn viewport_row_offset(viewport: &VisibleViewport, y: usize) -> io::Result<usize> {
    y.checked_mul(viewport.line_length)
        .and_then(|offset| viewport.base_offset.checked_add(offset))
        .ok_or_else(|| io::Error::other("framebuffer viewport row offset overflow"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BlitFormat {
    Fast32(Fast32BlitMode),
    GenericPackColor,
}

impl BlitFormat {
    const fn name(self) -> &'static str {
        match self {
            Self::Fast32(format) => format.name(),
            Self::GenericPackColor => "generic-pack-color",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fast32BlitMode {
    BgrxZero,
    BgraOpaque,
    RgbxZero,
    RgbaOpaque,
    ByteAligned(Fast32ByteAlignedBlit),
}

impl Fast32BlitMode {
    const fn from_byte_aligned(format: Fast32ByteAlignedBlit) -> Self {
        match (format.red, format.green, format.blue, format.alpha) {
            (2, 1, 0, None) => Self::BgrxZero,
            (2, 1, 0, Some(3)) => Self::BgraOpaque,
            (0, 1, 2, None) => Self::RgbxZero,
            (0, 1, 2, Some(3)) => Self::RgbaOpaque,
            _ => Self::ByteAligned(format),
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::BgrxZero => "fast32-bgrx-zero",
            Self::BgraOpaque => "fast32-bgra-opaque",
            Self::RgbxZero => "fast32-rgbx-zero",
            Self::RgbaOpaque => "fast32-rgba-opaque",
            Self::ByteAligned(_) => "fast32-byte-shuffle",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Fast32ByteAlignedBlit {
    red: usize,
    green: usize,
    blue: usize,
    alpha: Option<usize>,
}

pub(super) fn framebuffer_draw_enabled() -> bool {
    env::var(DRAW_ENV_VAR).map_or(true, |value| parse_env_bool(&value))
}

fn force_generic_blit_enabled() -> bool {
    static FORCE_GENERIC_BLIT: OnceLock<bool> = OnceLock::new();
    *FORCE_GENERIC_BLIT.get_or_init(|| {
        env::var(FORCE_GENERIC_BLIT_ENV_VAR).is_ok_and(|value| parse_env_bool(&value))
    })
}

fn parse_env_bool(value: &str) -> bool {
    !matches!(
        value,
        "0" | "false" | "False" | "FALSE" | "off" | "Off" | "OFF"
    )
}

fn pack_color(var: &FbVarScreeninfo, (red, green, blue): (u8, u8, u8)) -> u32 {
    pack_channel(red, &var.red)
        | pack_channel(green, &var.green)
        | pack_channel(blue, &var.blue)
        | pack_channel(u8::MAX, &var.transp)
}

fn detect_fast_blit_format(var: &FbVarScreeninfo) -> Option<Fast32ByteAlignedBlit> {
    if var.bits_per_pixel != 32 {
        return None;
    }
    let red = byte_aligned_channel(&var.red)?;
    let green = byte_aligned_channel(&var.green)?;
    let blue = byte_aligned_channel(&var.blue)?;
    if !distinct_bytes([Some(red), Some(green), Some(blue), None]) {
        return None;
    }
    let alpha = if var.transp.length == 0 {
        None
    } else {
        Some(byte_aligned_channel(&var.transp)?)
    };
    if !distinct_bytes([Some(red), Some(green), Some(blue), alpha]) {
        return None;
    }

    Some(Fast32ByteAlignedBlit {
        red,
        green,
        blue,
        alpha,
    })
}

fn byte_aligned_channel(field: &FbBitfield) -> Option<usize> {
    if field.length != 8 || !field.offset.is_multiple_of(8) || field.msb_right != 0 {
        return None;
    }
    let byte = usize::try_from(field.offset / 8).ok()?;
    if byte >= 4 {
        return None;
    }
    Some(native_endian_byte_index(byte))
}

const fn native_endian_byte_index(byte: usize) -> usize {
    if cfg!(target_endian = "little") {
        byte
    } else {
        3 - byte
    }
}

fn distinct_bytes(bytes: [Option<usize>; 4]) -> bool {
    for (index, byte) in bytes.iter().enumerate() {
        let Some(byte) = byte else {
            continue;
        };
        if bytes[index + 1..]
            .iter()
            .flatten()
            .any(|other| other == byte)
        {
            return false;
        }
    }
    true
}

#[cfg(test)]
fn write_fast_pixel(
    pixel: &mut [u8],
    format: Fast32ByteAlignedBlit,
    (red, green, blue): (u8, u8, u8),
) {
    pixel[..4].fill(0);
    pixel[format.red] = red;
    pixel[format.green] = green;
    pixel[format.blue] = blue;
    if let Some(alpha) = format.alpha {
        pixel[alpha] = u8::MAX;
    }
}

fn blit_fast32_rows(
    pixels: &mut [u8],
    surface: &SoftwareSurface,
    viewport: &VisibleViewport,
    format: Fast32BlitMode,
) -> io::Result<()> {
    if viewport.bytes_per_pixel != 4 {
        return Err(io::Error::other(
            "fast32 framebuffer viewport is not 4 bytes per pixel",
        ));
    }
    let row_bytes = viewport
        .width
        .checked_mul(4)
        .ok_or_else(|| io::Error::other("fast32 framebuffer row byte count overflow"))?;
    let source_len = surface
        .width
        .checked_mul(surface.height)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| io::Error::other("software surface byte count overflow"))?;
    if source_len > surface.pixels.len() {
        return Err(io::Error::other("software surface pixels are truncated"));
    }
    for y in 0..viewport.height {
        let framebuffer_row_start = viewport_row_offset(viewport, y)?;
        let framebuffer_row_end = framebuffer_row_start
            .checked_add(row_bytes)
            .ok_or_else(|| io::Error::other("fast32 framebuffer row end overflow"))?;
        let row_scanline_end = viewport
            .x_offset_bytes
            .checked_add(row_bytes)
            .ok_or_else(|| io::Error::other("fast32 framebuffer scanline end overflow"))?;
        if framebuffer_row_end > pixels.len() || row_scanline_end > viewport.line_length {
            continue;
        }

        let source_row_start = y
            .checked_mul(surface.width)
            .and_then(|offset| offset.checked_mul(4))
            .ok_or_else(|| io::Error::other("software surface row offset overflow"))?;
        let source_row_end = source_row_start
            .checked_add(row_bytes)
            .ok_or_else(|| io::Error::other("software surface row end overflow"))?;
        let source_row = surface
            .pixels
            .get(source_row_start..source_row_end)
            .ok_or_else(|| io::Error::other("software surface row exceeds pixel buffer"))?;

        let framebuffer_row = &mut pixels[framebuffer_row_start..framebuffer_row_end];
        convert_rgba_row_to_fast32(framebuffer_row, source_row, format);
    }

    Ok(())
}

fn convert_rgba_row_to_fast32(destination: &mut [u8], source: &[u8], format: Fast32BlitMode) {
    match format {
        Fast32BlitMode::BgrxZero => convert_rgba_row_to_bgrx_zero(destination, source),
        Fast32BlitMode::BgraOpaque => convert_rgba_row_to_bgra_opaque(destination, source),
        Fast32BlitMode::RgbxZero => convert_rgba_row_to_rgbx_zero(destination, source),
        Fast32BlitMode::RgbaOpaque => convert_rgba_row_to_rgba_opaque(destination, source),
        Fast32BlitMode::ByteAligned(format) => {
            convert_rgba_row_to_byte_aligned(destination, source, format);
        }
    }
}

fn convert_rgba_row_to_bgrx_zero(destination: &mut [u8], source: &[u8]) {
    for (source, destination) in source.chunks_exact(4).zip(destination.chunks_exact_mut(4)) {
        destination[0] = source[2];
        destination[1] = source[1];
        destination[2] = source[0];
        destination[3] = 0;
    }
}

fn convert_rgba_row_to_bgra_opaque(destination: &mut [u8], source: &[u8]) {
    for (source, destination) in source.chunks_exact(4).zip(destination.chunks_exact_mut(4)) {
        destination[0] = source[2];
        destination[1] = source[1];
        destination[2] = source[0];
        destination[3] = u8::MAX;
    }
}

fn convert_rgba_row_to_rgbx_zero(destination: &mut [u8], source: &[u8]) {
    for (source, destination) in source.chunks_exact(4).zip(destination.chunks_exact_mut(4)) {
        destination[0] = source[0];
        destination[1] = source[1];
        destination[2] = source[2];
        destination[3] = 0;
    }
}

fn convert_rgba_row_to_rgba_opaque(destination: &mut [u8], source: &[u8]) {
    for (source, destination) in source.chunks_exact(4).zip(destination.chunks_exact_mut(4)) {
        destination[0] = source[0];
        destination[1] = source[1];
        destination[2] = source[2];
        destination[3] = u8::MAX;
    }
}

fn convert_rgba_row_to_byte_aligned(
    destination: &mut [u8],
    source: &[u8],
    format: Fast32ByteAlignedBlit,
) {
    for (source, destination) in source.chunks_exact(4).zip(destination.chunks_exact_mut(4)) {
        destination.fill(0);
        destination[format.red] = source[0];
        destination[format.green] = source[1];
        destination[format.blue] = source[2];
        if let Some(alpha) = format.alpha {
            destination[alpha] = u8::MAX;
        }
    }
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

fn elapsed_micros(start: Option<Instant>) -> u128 {
    start.map_or(0, |start| start.elapsed().as_micros())
}

#[cfg(test)]
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

    #[test]
    fn visible_viewport_rejects_scanline_crossing() {
        let fix = FbFixScreeninfo {
            line_length: 2_000,
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

        let error = visible_viewport(&fix, &var).expect_err("viewport crosses scanline");

        assert_eq!(
            error.to_string(),
            "framebuffer visible viewport crosses scanline boundary"
        );
    }

    #[test]
    fn snapshot_bytes_used_sums_existing_snapshots() {
        let snapshots = vec![
            FramebufferSnapshot {
                viewport: test_viewport(0),
                bytes: vec![0; 16],
            },
            FramebufferSnapshot {
                viewport: test_viewport(64),
                bytes: vec![0; 24],
            },
        ];

        assert_eq!(snapshot_bytes_used(&snapshots).expect("bytes used"), 40);
    }

    #[test]
    fn pack_color_sets_opaque_transparency_bits() {
        let var = FbVarScreeninfo {
            red: FbBitfield {
                offset: 16,
                length: 8,
                ..Default::default()
            },
            green: FbBitfield {
                offset: 8,
                length: 8,
                ..Default::default()
            },
            blue: FbBitfield {
                offset: 0,
                length: 8,
                ..Default::default()
            },
            transp: FbBitfield {
                offset: 24,
                length: 8,
                ..Default::default()
            },
            ..Default::default()
        };

        assert_eq!(pack_color(&var, (0x12, 0x34, 0x56)), 0xff12_3456);
    }

    #[test]
    fn portmaster_fast_blit_matches_pack_color_for_common_rgb32_layout() {
        let var = rgb32_var_without_alpha();
        let format = detect_fast_blit_format(&var).expect("fast blit format");
        let mode = Fast32BlitMode::from_byte_aligned(format);
        let color = (0x12, 0x34, 0x56);
        let mut fast_pixel = [0xff; 4];
        let mut generic_pixel = [0xff; 4];

        write_fast_pixel(&mut fast_pixel, format, color);
        write_pixel(&mut generic_pixel, 4, pack_color(&var, color));

        assert_eq!(fast_pixel, generic_pixel);
        if cfg!(target_endian = "little") {
            assert_eq!(mode, Fast32BlitMode::BgrxZero);
        }
    }

    #[test]
    fn portmaster_fast32_specialized_converters_match_byte_shuffle_and_pack_color() {
        assert_fast32_mode_matches_byte_shuffle_and_pack_color(
            rgb32_var_without_alpha(),
            Fast32BlitMode::BgrxZero,
        );
        assert_fast32_mode_matches_byte_shuffle_and_pack_color(
            rgb32_var_with_alpha(),
            Fast32BlitMode::BgraOpaque,
        );
        assert_fast32_mode_matches_byte_shuffle_and_pack_color(
            reversed_rgb32_var_without_alpha(),
            Fast32BlitMode::RgbxZero,
        );
        assert_fast32_mode_matches_byte_shuffle_and_pack_color(
            reversed_rgb32_var_with_alpha(),
            Fast32BlitMode::RgbaOpaque,
        );
    }

    #[test]
    fn framebuffer_env_bool_treats_only_known_false_values_as_false() {
        for value in ["0", "false", "False", "FALSE", "off", "Off", "OFF"] {
            assert!(!parse_env_bool(value), "{value:?} should disable");
        }

        for value in ["", "1", "true", "on", "yes", "no", "anything"] {
            assert!(parse_env_bool(value), "{value:?} should enable");
        }
    }

    #[test]
    fn portmaster_fast32_row_blit_matches_generic_pack_color() {
        let var = rgb32_var_without_alpha();
        let format = Fast32BlitMode::from_byte_aligned(
            detect_fast_blit_format(&var).expect("fast blit format"),
        );
        let viewport = VisibleViewport {
            width: 3,
            height: 2,
            line_length: 12,
            ..test_viewport(0)
        };
        let surface = test_surface(viewport.width, viewport.height);
        let mut fast = vec![0xee; viewport.line_length * viewport.height];
        let mut generic = fast.clone();

        blit_fast32_rows(&mut fast, &surface, &viewport, format).expect("fast row blit");
        blit_generic_for_test(&mut generic, &surface, &viewport, &var).expect("generic blit");

        assert_eq!(fast, generic);
    }

    #[test]
    fn portmaster_fast32_row_blit_preserves_stride_padding() {
        let var = rgb32_var_without_alpha();
        let format = Fast32BlitMode::from_byte_aligned(
            detect_fast_blit_format(&var).expect("fast blit format"),
        );
        let viewport = VisibleViewport {
            xoffset: 1,
            width: 3,
            height: 2,
            line_length: 20,
            x_offset_bytes: 4,
            base_offset: 4,
            ..test_viewport(0)
        };
        let surface = test_surface(viewport.width, viewport.height);
        let mut fast = vec![0xaa; viewport.line_length * viewport.height];
        let mut generic = fast.clone();

        blit_fast32_rows(&mut fast, &surface, &viewport, format).expect("fast row blit");
        blit_generic_for_test(&mut generic, &surface, &viewport, &var).expect("generic blit");

        assert_eq!(fast, generic);
        assert_eq!(&fast[0..4], &[0xaa; 4]);
        assert_eq!(&fast[16..20], &[0xaa; 4]);
        assert_eq!(&fast[20..24], &[0xaa; 4]);
        assert_eq!(&fast[36..40], &[0xaa; 4]);
    }

    #[test]
    fn portmaster_fast32_alpha_row_blit_preserves_stride_padding() {
        let var = rgb32_var_with_alpha();
        let format = Fast32BlitMode::from_byte_aligned(
            detect_fast_blit_format(&var).expect("fast blit format"),
        );
        let viewport = VisibleViewport {
            xoffset: 1,
            width: 3,
            height: 2,
            line_length: 20,
            x_offset_bytes: 4,
            base_offset: 4,
            ..test_viewport(0)
        };
        let surface = test_surface(viewport.width, viewport.height);
        let mut fast = vec![0xaa; viewport.line_length * viewport.height];
        let mut generic = fast.clone();

        blit_fast32_rows(&mut fast, &surface, &viewport, format).expect("fast row blit");
        blit_generic_for_test(&mut generic, &surface, &viewport, &var).expect("generic blit");

        assert_eq!(fast, generic);
        assert_eq!(&fast[0..4], &[0xaa; 4]);
        assert_eq!(&fast[16..20], &[0xaa; 4]);
        assert_eq!(&fast[20..24], &[0xaa; 4]);
        assert_eq!(&fast[36..40], &[0xaa; 4]);
    }

    #[test]
    fn portmaster_fast_blit_rejects_non_byte_aligned_layout() {
        let mut var = rgb32_var_without_alpha();
        var.red.offset = 17;

        assert_eq!(detect_fast_blit_format(&var), None);
    }

    #[test]
    fn portmaster_fast_blit_rejects_non_32bpp_layout() {
        let mut var = rgb32_var_without_alpha();
        var.bits_per_pixel = 16;

        assert_eq!(detect_fast_blit_format(&var), None);
    }

    fn rgb32_var_without_alpha() -> FbVarScreeninfo {
        FbVarScreeninfo {
            bits_per_pixel: 32,
            red: FbBitfield {
                offset: 16,
                length: 8,
                ..Default::default()
            },
            green: FbBitfield {
                offset: 8,
                length: 8,
                ..Default::default()
            },
            blue: FbBitfield {
                offset: 0,
                length: 8,
                ..Default::default()
            },
            transp: FbBitfield::default(),
            ..Default::default()
        }
    }

    fn rgb32_var_with_alpha() -> FbVarScreeninfo {
        FbVarScreeninfo {
            transp: FbBitfield {
                offset: 24,
                length: 8,
                ..Default::default()
            },
            ..rgb32_var_without_alpha()
        }
    }

    fn reversed_rgb32_var_without_alpha() -> FbVarScreeninfo {
        FbVarScreeninfo {
            bits_per_pixel: 32,
            red: FbBitfield {
                offset: 0,
                length: 8,
                ..Default::default()
            },
            green: FbBitfield {
                offset: 8,
                length: 8,
                ..Default::default()
            },
            blue: FbBitfield {
                offset: 16,
                length: 8,
                ..Default::default()
            },
            transp: FbBitfield::default(),
            ..Default::default()
        }
    }

    fn reversed_rgb32_var_with_alpha() -> FbVarScreeninfo {
        FbVarScreeninfo {
            transp: FbBitfield {
                offset: 24,
                length: 8,
                ..Default::default()
            },
            ..reversed_rgb32_var_without_alpha()
        }
    }

    fn assert_fast32_mode_matches_byte_shuffle_and_pack_color(
        var: FbVarScreeninfo,
        expected_mode: Fast32BlitMode,
    ) {
        let byte_aligned = detect_fast_blit_format(&var).expect("fast blit format");
        let mode = Fast32BlitMode::from_byte_aligned(byte_aligned);
        let source = test_surface(3, 1).pixels;
        let mut specialized = vec![0xee; source.len()];
        let mut byte_shuffle = specialized.clone();

        convert_rgba_row_to_fast32(&mut specialized, &source, mode);
        convert_rgba_row_to_byte_aligned(&mut byte_shuffle, &source, byte_aligned);

        if cfg!(target_endian = "little") {
            assert_eq!(mode, expected_mode);
        }
        assert_eq!(specialized, byte_shuffle);
        for (source, destination) in source.chunks_exact(4).zip(specialized.chunks_exact(4)) {
            let color = (source[0], source[1], source[2]);
            let mut packed = [0; 4];
            write_pixel(&mut packed, 4, pack_color(&var, color));
            assert_eq!(destination, packed);
        }
    }

    fn test_surface(width: usize, height: usize) -> SoftwareSurface {
        let mut pixels = Vec::with_capacity(width * height * 4);
        for index in 0..width * height {
            let base = u8::try_from(index * 17).expect("test color fits u8");
            pixels.extend_from_slice(&[base, base.wrapping_add(3), base.wrapping_add(7), 0x80]);
        }
        SoftwareSurface {
            width,
            height,
            pixels,
        }
    }

    fn blit_generic_for_test(
        pixels: &mut [u8],
        surface: &SoftwareSurface,
        viewport: &VisibleViewport,
        var: &FbVarScreeninfo,
    ) -> io::Result<()> {
        for y in 0..viewport.height {
            let row_offset = viewport_row_offset(viewport, y)?;
            for x in 0..viewport.width {
                let pixel_offset = row_offset + x * viewport.bytes_per_pixel;
                let pixel_end = pixel_offset + viewport.bytes_per_pixel;
                let source_offset = (y * surface.width + x) * 4;
                let color = (
                    surface.pixels[source_offset],
                    surface.pixels[source_offset + 1],
                    surface.pixels[source_offset + 2],
                );
                write_pixel(
                    &mut pixels[pixel_offset..pixel_end],
                    viewport.bytes_per_pixel,
                    pack_color(var, color),
                );
            }
        }
        Ok(())
    }

    fn test_viewport(base_offset: usize) -> VisibleViewport {
        VisibleViewport {
            xoffset: 0,
            yoffset: 0,
            width: 4,
            height: 4,
            line_length: 16,
            bytes_per_pixel: 4,
            x_offset_bytes: 0,
            base_offset,
        }
    }
}
