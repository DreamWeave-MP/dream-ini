// SPDX-License-Identifier: GPL-3.0-only

use std::env;
use std::fs::{File, OpenOptions};
use std::io;
use std::num::NonZeroUsize;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::ptr::NonNull;

use super::log::{SharedLog, write_log};
use super::pacing::DisplayTiming;
use super::renderer::SoftwareRenderer;
use super::surface::SoftwareSurface;
use super::{GuiFrame, GuiShell};

const FBIOGET_VSCREENINFO: libc::c_ulong = 0x4600;
const FBIOGET_FSCREENINFO: libc::c_ulong = 0x4602;
const FRAMEBUFFER_PATHS: [&str; 2] = ["/dev/fb0", "/dev/graphics/fb0"];
pub(super) const DRAW_ENV_VAR: &str = "DREAM_INI_FB_DRAW";
const MAX_SNAPSHOT_BYTES: usize = 8 * 1024 * 1024;

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
    ) -> io::Result<()> {
        self.var = get_var_info(&self.file).map_err(|error| {
            write_log(
                frame.log,
                format!("failed to refresh framebuffer variable info: {error}"),
            );
            error
        })?;
        self.validate_format()?;
        let viewport = visible_viewport(&self.fix, &self.var)?;
        if frame.log_frame {
            log_visible_viewport(frame.log, &viewport);
            log_derived_page_info(frame.log, &self.var);
        }
        renderer.render(viewport.width, viewport.height, frame)?;
        self.capture_snapshot_if_needed(frame.snapshots, &viewport, frame.log)?;
        self.blit_rgba_surface(renderer.surface(), &viewport)
    }

    fn blit_rgba_surface(
        &mut self,
        surface: &SoftwareSurface,
        viewport: &VisibleViewport,
    ) -> io::Result<()> {
        if surface.width != viewport.width || surface.height != viewport.height {
            return Err(io::Error::other(
                "software surface size does not match viewport",
            ));
        }

        // SAFETY: memory points to a live mmap of memory_len bytes owned by self.
        let pixels =
            unsafe { std::slice::from_raw_parts_mut(self.memory.as_ptr(), self.memory_len.get()) };
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

pub(super) fn framebuffer_draw_enabled() -> bool {
    env::var(DRAW_ENV_VAR).map_or(true, |value| {
        !matches!(
            value.as_str(),
            "0" | "false" | "False" | "FALSE" | "off" | "Off" | "OFF"
        )
    })
}

fn pack_color(var: &FbVarScreeninfo, (red, green, blue): (u8, u8, u8)) -> u32 {
    pack_channel(red, &var.red)
        | pack_channel(green, &var.green)
        | pack_channel(blue, &var.blue)
        | pack_channel(u8::MAX, &var.transp)
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
