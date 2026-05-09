// SPDX-License-Identifier: GPL-3.0-only

#[cfg(target_os = "linux")]
use std::collections::HashMap;
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
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(target_os = "linux")]
use super::{GuiApp, GuiShell};

const LOG_FILE_NAME: &str = "dream-ini-portmaster.log";
#[cfg(target_os = "linux")]
const FBIOGET_VSCREENINFO: libc::c_ulong = 0x4600;
#[cfg(target_os = "linux")]
const FBIOGET_FSCREENINFO: libc::c_ulong = 0x4602;
#[cfg(target_os = "linux")]
const FRAME_DELAY: Duration = Duration::from_millis(33);
#[cfg(target_os = "linux")]
const FRAMEBUFFER_PATHS: [&str; 2] = ["/dev/fb0", "/dev/graphics/fb0"];
#[cfg(target_os = "linux")]
const DRAW_ENV_VAR: &str = "DREAM_INI_FB_DRAW";
#[cfg(target_os = "linux")]
const MAX_SNAPSHOT_BYTES: usize = 8 * 1024 * 1024;
#[cfg(target_os = "linux")]
const MAX_RENDER_PIXELS: usize = 1024 * 768;
#[cfg(target_os = "linux")]
const MAX_TEXTURE_BYTES: usize = 8 * 1024 * 1024;
type SharedLog = Arc<Mutex<File>>;

pub(crate) fn run() -> ExitCode {
    let log = open_log().map(Mutex::new).map(Arc::new);
    install_panic_hook(log.clone());
    log_startup(log.as_ref());

    match run_gui(log.as_ref()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            write_log(log.as_ref(), format!("fatal error: {error}"));
            ExitCode::FAILURE
        }
    }
}

#[cfg(target_os = "linux")]
fn run_gui(log: Option<&SharedLog>) -> io::Result<()> {
    let mut framebuffer = Framebuffer::open()?;
    framebuffer.log_info(log);
    framebuffer.validate_format()?;
    let draw_enabled = framebuffer_draw_enabled();
    write_log(
        log,
        format!(
            "framebuffer drawing enabled={draw_enabled} env_{}={:?}",
            DRAW_ENV_VAR,
            env::var_os(DRAW_ENV_VAR)
        ),
    );
    if !draw_enabled {
        write_log(
            log,
            "framebuffer drawing disabled; exiting without launching GUI",
        );
        return Ok(());
    }

    let egui_context = egui::Context::default();
    let mut app = GuiApp::new(egui_context.clone());
    let mut shell = PortMasterGuiShell::new(log);
    write_log(log, "shared GUI and controller worker started");

    let mut frame_count = 0_u64;
    let mut snapshots = Vec::new();
    let mut renderer = SoftwareRenderer::default();
    let mut gui_error = None;
    let exit_reason = 'gui: loop {
        let log_frame = frame_count == 0 || frame_count.is_multiple_of(30);
        let mut frame = GuiFrame {
            context: &egui_context,
            app: &mut app,
            shell: &mut shell,
            snapshots: &mut snapshots,
            log,
            log_frame,
        };
        if let Err(error) = framebuffer.draw_egui_gui(&mut renderer, &mut frame) {
            write_log(log, format!("draw failed: {error}"));
            gui_error = Some(error);
            break 'gui "draw-error";
        }
        frame_count = frame_count.saturating_add(1);

        if shell.exit_requested() {
            write_log(log, "quit requested by GUI shell");
            break 'gui "exit-requested";
        }

        thread::sleep(FRAME_DELAY);
    };

    write_log(
        log,
        format!("leaving framebuffer GUI reason={exit_reason} frames={frame_count}"),
    );
    let restore_result = framebuffer.restore_snapshots(&snapshots, log);
    if let Err(error) = &restore_result {
        write_log(log, format!("framebuffer restore failed: {error}"));
    }
    write_log(log, "dropping shared GUI and controller worker");
    drop(app);
    write_log(log, "controller worker dropped");
    write_log(log, "framebuffer GUI complete");
    if let Some(error) = gui_error {
        Err(error)
    } else {
        restore_result
    }
}

#[cfg(target_os = "linux")]
#[derive(Debug, Default)]
struct PortMasterGuiShell {
    exit_requested: bool,
    clipboard_unsupported_logged: bool,
    log: Option<SharedLog>,
}

#[cfg(target_os = "linux")]
impl PortMasterGuiShell {
    fn new(log: Option<&SharedLog>) -> Self {
        Self {
            exit_requested: false,
            clipboard_unsupported_logged: false,
            log: log.cloned(),
        }
    }

    const fn exit_requested(&self) -> bool {
        self.exit_requested
    }
}

#[cfg(target_os = "linux")]
impl GuiShell for PortMasterGuiShell {
    fn request_exit(&mut self, _context: &egui::Context) {
        self.exit_requested = true;
    }

    fn copy_text(&mut self, _context: &egui::Context, _text: String) {
        if !self.clipboard_unsupported_logged {
            write_log(
                self.log.as_ref(),
                "clipboard requested, but PortMaster framebuffer shell has no clipboard",
            );
            self.clipboard_unsupported_logged = true;
        }
    }
}

#[cfg(target_os = "linux")]
struct GuiFrame<'a, S: GuiShell> {
    context: &'a egui::Context,
    app: &'a mut GuiApp,
    shell: &'a mut S,
    snapshots: &'a mut Vec<FramebufferSnapshot>,
    log: Option<&'a SharedLog>,
    log_frame: bool,
}

#[cfg(not(target_os = "linux"))]
fn run_gui(log: Option<&SharedLog>) -> io::Result<()> {
    let message = "PortMaster framebuffer GUI is only supported on Linux";
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

    fn draw_egui_gui<S: GuiShell>(
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

    fn restore_snapshots(
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

#[cfg(target_os = "linux")]
impl Drop for Framebuffer {
    fn drop(&mut self) {
        // SAFETY: memory and memory_len are the same mapping returned by mmap in
        // Framebuffer::open_path and have not been unmapped yet.
        let _ = unsafe { libc::munmap(self.memory.as_ptr().cast(), self.memory_len.get()) };
    }
}

#[cfg(target_os = "linux")]
#[derive(Debug, Default)]
struct SoftwareRenderer {
    surface: SoftwareSurface,
    textures: TextureStore,
}

#[cfg(target_os = "linux")]
impl SoftwareRenderer {
    fn render<S: GuiShell>(
        &mut self,
        width: usize,
        height: usize,
        frame: &mut GuiFrame<'_, S>,
    ) -> io::Result<()> {
        self.surface.resize(width, height)?;
        self.surface.clear([17, 20, 28, 255]);

        let raw_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(usize_to_f32(width), usize_to_f32(height)),
            )),
            max_texture_side: Some(1024),
            ..Default::default()
        };
        let output = frame
            .context
            .run_ui(raw_input, |ui| frame.app.ui(ui, frame.shell));
        self.textures.apply(&output.textures_delta)?;
        let primitives = frame.context.tessellate(output.shapes, 1.0);
        if frame.log_frame {
            write_log(
                frame.log,
                format!(
                    "software renderer surface={}x{} bytes={} textures={} texture_bytes={} primitives={}",
                    self.surface.width,
                    self.surface.height,
                    self.surface.pixels.len(),
                    self.textures.len(),
                    self.textures.bytes_used(),
                    primitives.len()
                ),
            );
        }
        self.rasterize(&primitives)?;
        for id in output.textures_delta.free {
            self.textures.free(id);
        }
        Ok(())
    }

    const fn surface(&self) -> &SoftwareSurface {
        &self.surface
    }

    fn rasterize(&mut self, primitives: &[egui::ClippedPrimitive]) -> io::Result<()> {
        for primitive in primitives {
            match &primitive.primitive {
                egui::epaint::Primitive::Mesh(mesh) => {
                    self.rasterize_mesh(mesh, primitive.clip_rect)?;
                }
                egui::epaint::Primitive::Callback(_) => {
                    return Err(io::Error::other(
                        "unsupported egui paint callback in software renderer",
                    ));
                }
            }
        }
        Ok(())
    }

    fn rasterize_mesh(&mut self, mesh: &egui::Mesh, clip_rect: egui::Rect) -> io::Result<()> {
        let Some(texture) = self.textures.get(&mesh.texture_id) else {
            return Ok(());
        };
        let clip = ClipBounds::new(clip_rect, self.surface.width, self.surface.height)?;
        if clip.is_empty() {
            return Ok(());
        }
        let surface = &mut self.surface;
        for triangle in mesh.indices.chunks_exact(3) {
            let i0 = usize::try_from(triangle[0])
                .map_err(|_| io::Error::other("mesh index does not fit usize"))?;
            let i1 = usize::try_from(triangle[1])
                .map_err(|_| io::Error::other("mesh index does not fit usize"))?;
            let i2 = usize::try_from(triangle[2])
                .map_err(|_| io::Error::other("mesh index does not fit usize"))?;
            let Some(v0) = mesh.vertices.get(i0) else {
                continue;
            };
            let Some(v1) = mesh.vertices.get(i1) else {
                continue;
            };
            let Some(v2) = mesh.vertices.get(i2) else {
                continue;
            };
            rasterize_triangle(surface, v0, v1, v2, texture, clip);
        }
        Ok(())
    }
}

#[cfg(target_os = "linux")]
#[derive(Debug, Default)]
struct SoftwareSurface {
    width: usize,
    height: usize,
    pixels: Vec<u8>,
}

#[cfg(target_os = "linux")]
impl SoftwareSurface {
    fn resize(&mut self, width: usize, height: usize) -> io::Result<()> {
        let pixels = width
            .checked_mul(height)
            .ok_or_else(|| io::Error::other("software surface pixel count overflow"))?;
        if pixels > MAX_RENDER_PIXELS {
            return Err(io::Error::other(format!(
                "software surface pixel budget exceeded: {pixels} > {MAX_RENDER_PIXELS}"
            )));
        }
        let bytes = pixels
            .checked_mul(4)
            .ok_or_else(|| io::Error::other("software surface byte count overflow"))?;
        if self.width != width || self.height != height || self.pixels.len() != bytes {
            self.pixels.resize(bytes, 0);
            self.width = width;
            self.height = height;
        }
        Ok(())
    }

    fn clear(&mut self, color: [u8; 4]) {
        for pixel in self.pixels.chunks_exact_mut(4) {
            pixel.copy_from_slice(&color);
        }
    }

    fn blend_pixel(&mut self, x: usize, y: usize, color: [u8; 4]) {
        let offset = (y * self.width + x) * 4;
        alpha_blend(&mut self.pixels[offset..offset + 4], color);
    }
}

#[cfg(target_os = "linux")]
#[derive(Debug, Default)]
struct TextureStore {
    textures: HashMap<egui::TextureId, TextureImage>,
}

#[cfg(target_os = "linux")]
impl TextureStore {
    fn apply(&mut self, delta: &egui::TexturesDelta) -> io::Result<()> {
        for (id, image_delta) in &delta.set {
            self.set(*id, image_delta)?;
        }
        Ok(())
    }

    fn set(&mut self, id: egui::TextureId, delta: &egui::epaint::ImageDelta) -> io::Result<()> {
        let metadata = TextureImageMetadata::from_image_data(&delta.image)?;
        if let Some(pos) = delta.pos {
            self.textures
                .get(&id)
                .ok_or_else(|| io::Error::other("partial texture update for missing texture"))?
                .validate_update_bounds(pos, metadata.width, metadata.height)?;
            let image = TextureImage::from_image_data(&delta.image, metadata);
            let texture = self
                .textures
                .get_mut(&id)
                .ok_or_else(|| io::Error::other("partial texture update for missing texture"))?;
            texture.update(pos, &image)
        } else {
            let bytes_used = self.bytes_used();
            let old_len = self
                .textures
                .get(&id)
                .map_or(0, |texture| texture.pixels.len());
            check_texture_budget(bytes_used, old_len, metadata.byte_len)?;
            let image = TextureImage::from_image_data(&delta.image, metadata);
            self.textures.insert(id, image);
            Ok(())
        }
    }

    fn free(&mut self, id: egui::TextureId) {
        self.textures.remove(&id);
    }

    fn get(&self, id: &egui::TextureId) -> Option<&TextureImage> {
        self.textures.get(id)
    }

    fn len(&self) -> usize {
        self.textures.len()
    }

    fn bytes_used(&self) -> usize {
        self.textures
            .values()
            .map(|texture| texture.pixels.len())
            .sum()
    }
}

#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy)]
struct TextureImageMetadata {
    width: usize,
    height: usize,
    byte_len: usize,
}

#[cfg(target_os = "linux")]
impl TextureImageMetadata {
    fn from_image_data(image: &egui::ImageData) -> io::Result<Self> {
        match image {
            egui::ImageData::Color(color_image) => {
                let [width, height] = color_image.size;
                let pixel_count = width
                    .checked_mul(height)
                    .ok_or_else(|| io::Error::other("texture pixel count overflow"))?;
                if color_image.pixels.len() != pixel_count {
                    return Err(io::Error::other(format!(
                        "texture pixel count mismatch: {} != {pixel_count}",
                        color_image.pixels.len()
                    )));
                }
                let byte_len = pixel_count
                    .checked_mul(4)
                    .ok_or_else(|| io::Error::other("texture byte count overflow"))?;
                Ok(Self {
                    width,
                    height,
                    byte_len,
                })
            }
        }
    }
}

#[cfg(target_os = "linux")]
#[derive(Debug, Clone)]
struct TextureImage {
    width: usize,
    height: usize,
    pixels: Vec<u8>,
}

#[cfg(target_os = "linux")]
impl TextureImage {
    fn from_image_data(image: &egui::ImageData, metadata: TextureImageMetadata) -> Self {
        match image {
            egui::ImageData::Color(color_image) => {
                let mut pixels = Vec::with_capacity(metadata.byte_len);
                for color in &color_image.pixels {
                    pixels.extend_from_slice(&[color.r(), color.g(), color.b(), color.a()]);
                }
                Self {
                    width: metadata.width,
                    height: metadata.height,
                    pixels,
                }
            }
        }
    }

    fn update(&mut self, pos: [usize; 2], image: &Self) -> io::Result<()> {
        self.validate_update_bounds(pos, image.width, image.height)?;
        for row in 0..image.height {
            let destination = ((pos[1] + row) * self.width + pos[0]) * 4;
            let source = row * image.width * 4;
            let byte_count = image.width * 4;
            self.pixels[destination..destination + byte_count]
                .copy_from_slice(&image.pixels[source..source + byte_count]);
        }
        Ok(())
    }

    fn validate_update_bounds(
        &self,
        pos: [usize; 2],
        width: usize,
        height: usize,
    ) -> io::Result<()> {
        let x_end = pos[0]
            .checked_add(width)
            .ok_or_else(|| io::Error::other("partial texture x range overflow"))?;
        let y_end = pos[1]
            .checked_add(height)
            .ok_or_else(|| io::Error::other("partial texture y range overflow"))?;
        if x_end > self.width || y_end > self.height {
            return Err(io::Error::other(
                "partial texture update exceeds texture bounds",
            ));
        }
        Ok(())
    }

    fn sample_nearest(&self, uv: egui::Pos2) -> [u8; 4] {
        if self.width == 0 || self.height == 0 {
            return [255, 255, 255, 255];
        }
        let x = f32_to_usize_round_clamped(
            uv.x.clamp(0.0, 1.0) * usize_to_f32(self.width.saturating_sub(1)),
            self.width.saturating_sub(1),
        );
        let y = f32_to_usize_round_clamped(
            uv.y.clamp(0.0, 1.0) * usize_to_f32(self.height.saturating_sub(1)),
            self.height.saturating_sub(1),
        );
        let offset = (y * self.width + x) * 4;
        [
            self.pixels[offset],
            self.pixels[offset + 1],
            self.pixels[offset + 2],
            self.pixels[offset + 3],
        ]
    }
}

#[cfg(target_os = "linux")]
#[derive(Clone, Copy, Debug)]
struct ClipBounds {
    min_x: usize,
    min_y: usize,
    max_x: usize,
    max_y: usize,
}

#[cfg(target_os = "linux")]
impl ClipBounds {
    fn new(rect: egui::Rect, width: usize, height: usize) -> io::Result<Self> {
        let min_x = clamp_rect_value(rect.min.x.floor(), width)?;
        let min_y = clamp_rect_value(rect.min.y.floor(), height)?;
        let max_x = clamp_rect_value(rect.max.x.ceil(), width)?;
        let max_y = clamp_rect_value(rect.max.y.ceil(), height)?;
        Ok(Self {
            min_x,
            min_y,
            max_x,
            max_y,
        })
    }

    const fn is_empty(self) -> bool {
        self.min_x >= self.max_x || self.min_y >= self.max_y
    }
}

#[cfg(target_os = "linux")]
fn clamp_rect_value(value: f32, max: usize) -> io::Result<usize> {
    if !value.is_finite() {
        return Err(io::Error::other("non-finite clip rectangle value"));
    }
    Ok(f32_to_usize_floor_clamped(value, max))
}

#[cfg(target_os = "linux")]
fn usize_to_f32(value: usize) -> f32 {
    f32::from(u16::try_from(value).unwrap_or(u16::MAX))
}

#[cfg(target_os = "linux")]
fn f32_to_usize_floor_clamped(value: f32, max: usize) -> usize {
    f32_to_usize_threshold_clamped(value.floor(), max)
}

#[cfg(target_os = "linux")]
fn f32_to_usize_ceil_clamped(value: f32, max: usize) -> usize {
    f32_to_usize_threshold_clamped(value.ceil(), max)
}

#[cfg(target_os = "linux")]
fn f32_to_usize_round_clamped(value: f32, max: usize) -> usize {
    f32_to_usize_threshold_clamped(value.round(), max)
}

#[cfg(target_os = "linux")]
fn f32_to_usize_threshold_clamped(value: f32, max: usize) -> usize {
    if value <= 0.0 {
        return 0;
    }
    if value >= usize_to_f32(max) {
        return max;
    }
    let mut low = 0_usize;
    let mut high = max;
    while low < high {
        let mid = (low + high).div_ceil(2);
        if usize_to_f32(mid) <= value {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    low
}

#[cfg(target_os = "linux")]
fn f32_to_u8_round_clamped(value: f32) -> u8 {
    let value = value.round().clamp(0.0, 255.0);
    let mut low = 0_u8;
    let mut high = u8::MAX;
    while low < high {
        let mid = low + (high - low).div_ceil(2);
        if f32::from(mid) <= value {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    low
}

#[cfg(target_os = "linux")]
fn edge(a: egui::Pos2, b: egui::Pos2, c: egui::Pos2) -> f32 {
    (c.x - a.x).mul_add(b.y - a.y, -((c.y - a.y) * (b.x - a.x)))
}

#[cfg(target_os = "linux")]
fn rasterize_triangle(
    surface: &mut SoftwareSurface,
    v0: &egui::epaint::Vertex,
    v1: &egui::epaint::Vertex,
    v2: &egui::epaint::Vertex,
    texture: &TextureImage,
    clip: ClipBounds,
) {
    let area = edge(v0.pos, v1.pos, v2.pos);
    if area.abs() <= f32::EPSILON {
        return;
    }
    let min_x = f32_to_usize_floor_clamped(v0.pos.x.min(v1.pos.x).min(v2.pos.x), clip.max_x)
        .max(clip.min_x);
    let max_x =
        f32_to_usize_ceil_clamped(v0.pos.x.max(v1.pos.x).max(v2.pos.x), clip.max_x).min(clip.max_x);
    let min_y = f32_to_usize_floor_clamped(v0.pos.y.min(v1.pos.y).min(v2.pos.y), clip.max_y)
        .max(clip.min_y);
    let max_y =
        f32_to_usize_ceil_clamped(v0.pos.y.max(v1.pos.y).max(v2.pos.y), clip.max_y).min(clip.max_y);
    if min_x >= max_x || min_y >= max_y {
        return;
    }

    for y in min_y..max_y {
        for x in min_x..max_x {
            let point = egui::pos2(usize_to_f32(x) + 0.5, usize_to_f32(y) + 0.5);
            let w0 = edge(v1.pos, v2.pos, point) / area;
            let w1 = edge(v2.pos, v0.pos, point) / area;
            let w2 = edge(v0.pos, v1.pos, point) / area;
            if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                continue;
            }
            let uv = egui::pos2(
                v0.uv.x.mul_add(w0, v1.uv.x.mul_add(w1, v2.uv.x * w2)),
                v0.uv.y.mul_add(w0, v1.uv.y.mul_add(w1, v2.uv.y * w2)),
            );
            let vertex_color = interpolate_color(v0.color, v1.color, v2.color, w0, w1, w2);
            let texture_color = texture.sample_nearest(uv);
            let color = modulate_color(vertex_color, texture_color);
            surface.blend_pixel(x, y, color);
        }
    }
}

#[cfg(target_os = "linux")]
fn check_texture_budget(bytes_used: usize, old_len: usize, new_len: usize) -> io::Result<()> {
    let used = bytes_used
        .checked_sub(old_len)
        .and_then(|bytes| bytes.checked_add(new_len))
        .ok_or_else(|| io::Error::other("texture byte budget overflow"))?;
    if used > MAX_TEXTURE_BYTES {
        return Err(io::Error::other(format!(
            "texture byte budget exceeded: {used} > {MAX_TEXTURE_BYTES}"
        )));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn interpolate_color(
    c0: egui::Color32,
    c1: egui::Color32,
    c2: egui::Color32,
    w0: f32,
    w1: f32,
    w2: f32,
) -> [u8; 4] {
    [
        interpolate_channel(c0.r(), c1.r(), c2.r(), w0, w1, w2),
        interpolate_channel(c0.g(), c1.g(), c2.g(), w0, w1, w2),
        interpolate_channel(c0.b(), c1.b(), c2.b(), w0, w1, w2),
        interpolate_channel(c0.a(), c1.a(), c2.a(), w0, w1, w2),
    ]
}

#[cfg(target_os = "linux")]
fn interpolate_channel(c0: u8, c1: u8, c2: u8, w0: f32, w1: f32, w2: f32) -> u8 {
    let value = f32::from(c0).mul_add(w0, f32::from(c1).mul_add(w1, f32::from(c2) * w2));
    f32_to_u8_round_clamped(value)
}

#[cfg(target_os = "linux")]
fn modulate_color(vertex: [u8; 4], texture: [u8; 4]) -> [u8; 4] {
    [
        multiply_u8(vertex[0], texture[0]),
        multiply_u8(vertex[1], texture[1]),
        multiply_u8(vertex[2], texture[2]),
        multiply_u8(vertex[3], texture[3]),
    ]
}

#[cfg(target_os = "linux")]
fn multiply_u8(a: u8, b: u8) -> u8 {
    u8::try_from((u16::from(a) * u16::from(b) + 127) / 255).unwrap_or(u8::MAX)
}

#[cfg(target_os = "linux")]
fn alpha_blend(destination: &mut [u8], source: [u8; 4]) {
    let inverse_alpha = u16::from(u8::MAX - source[3]);
    // egui::Color32 stores premultiplied-alpha sRGBA. Do not multiply the
    // source channels by alpha again here unless darker fringes around every
    // translucent primitive sound like entertainment.
    destination[0] = blend_premultiplied_channel(source[0], destination[0], inverse_alpha);
    destination[1] = blend_premultiplied_channel(source[1], destination[1], inverse_alpha);
    destination[2] = blend_premultiplied_channel(source[2], destination[2], inverse_alpha);
    destination[3] = u8::MAX;
}

#[cfg(target_os = "linux")]
fn blend_premultiplied_channel(source: u8, destination: u8, inverse_alpha: u16) -> u8 {
    u8::try_from(u16::from(source) + ((u16::from(destination) * inverse_alpha + 127) / 255))
        .unwrap_or(u8::MAX)
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

#[cfg(target_os = "linux")]
#[derive(Debug)]
struct FramebufferSnapshot {
    viewport: VisibleViewport,
    bytes: Vec<u8>,
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
fn viewport_row_bytes(viewport: &VisibleViewport) -> io::Result<usize> {
    viewport
        .width
        .checked_mul(viewport.bytes_per_pixel)
        .ok_or_else(|| io::Error::other("framebuffer viewport row byte count overflow"))
}

#[cfg(target_os = "linux")]
fn viewport_snapshot_bytes(viewport: &VisibleViewport) -> io::Result<usize> {
    viewport_row_bytes(viewport)?
        .checked_mul(viewport.height)
        .ok_or_else(|| io::Error::other("framebuffer snapshot byte count overflow"))
}

#[cfg(target_os = "linux")]
fn snapshot_bytes_used(snapshots: &[FramebufferSnapshot]) -> io::Result<usize> {
    snapshots.iter().try_fold(0_usize, |total, snapshot| {
        total
            .checked_add(snapshot.bytes.len())
            .ok_or_else(|| io::Error::other("framebuffer snapshot bytes used overflow"))
    })
}

#[cfg(target_os = "linux")]
fn viewport_row_offset(viewport: &VisibleViewport, y: usize) -> io::Result<usize> {
    y.checked_mul(viewport.line_length)
        .and_then(|offset| viewport.base_offset.checked_add(offset))
        .ok_or_else(|| io::Error::other("framebuffer viewport row offset overflow"))
}

#[cfg(target_os = "linux")]
fn framebuffer_draw_enabled() -> bool {
    env::var(DRAW_ENV_VAR).map_or(true, |value| {
        !matches!(
            value.as_str(),
            "0" | "false" | "False" | "FALSE" | "off" | "Off" | "OFF"
        )
    })
}

#[cfg(target_os = "linux")]
fn pack_color(var: &FbVarScreeninfo, (red, green, blue): (u8, u8, u8)) -> u32 {
    pack_channel(red, &var.red)
        | pack_channel(green, &var.green)
        | pack_channel(blue, &var.blue)
        | pack_channel(u8::MAX, &var.transp)
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
    use std::sync::Arc;

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
    fn portmaster_shell_records_exit_and_unsupported_clipboard() {
        let mut shell = PortMasterGuiShell::new(None);

        shell.copy_text(&egui::Context::default(), "fallback=1\n".to_owned());
        shell.request_exit(&egui::Context::default());

        assert!(shell.clipboard_unsupported_logged);
        assert!(shell.exit_requested());
    }

    #[test]
    fn alpha_blend_places_premultiplied_half_alpha_over_opaque_destination() {
        let mut destination = [0, 0, 255, 255];

        alpha_blend(&mut destination, [128, 0, 0, 128]);

        assert_eq!(destination, [128, 0, 127, 255]);
    }

    #[test]
    fn software_surface_rejects_pixel_budget_overflow() {
        let mut surface = SoftwareSurface::default();

        let error = surface
            .resize(MAX_RENDER_PIXELS + 1, 1)
            .expect_err("oversized surface");

        assert_eq!(
            error.to_string(),
            format!(
                "software surface pixel budget exceeded: {} > {MAX_RENDER_PIXELS}",
                MAX_RENDER_PIXELS + 1
            )
        );
    }

    #[test]
    fn texture_partial_update_checks_bounds() {
        let mut texture = TextureImage {
            width: 2,
            height: 2,
            pixels: vec![0; 16],
        };
        let patch = TextureImage {
            width: 2,
            height: 1,
            pixels: vec![255; 8],
        };

        let error = texture
            .update([1, 1], &patch)
            .expect_err("patch crosses bounds");

        assert_eq!(
            error.to_string(),
            "partial texture update exceeds texture bounds"
        );
    }

    #[test]
    fn texture_budget_rejects_excess_bytes() {
        let error = check_texture_budget(MAX_TEXTURE_BYTES, 0, 4).expect_err("over budget");

        assert_eq!(
            error.to_string(),
            format!(
                "texture byte budget exceeded: {} > {MAX_TEXTURE_BYTES}",
                MAX_TEXTURE_BYTES + 4
            )
        );
    }

    #[test]
    fn texture_preflight_rejects_mismatched_pixel_count() {
        let image = egui::ImageData::Color(Arc::new(egui::ColorImage {
            size: [2, 2],
            source_size: egui::vec2(2.0, 2.0),
            pixels: vec![egui::Color32::WHITE; 3],
        }));

        let error = TextureImageMetadata::from_image_data(&image).expect_err("mismatched pixels");

        assert_eq!(error.to_string(), "texture pixel count mismatch: 3 != 4");
    }

    #[test]
    fn texture_store_rejects_over_budget_before_storing() {
        let mut store = TextureStore::default();
        let pixel_count = (MAX_TEXTURE_BYTES / 4) + 1;
        let image =
            egui::ColorImage::new([pixel_count, 1], vec![egui::Color32::WHITE; pixel_count]);
        let delta = egui::epaint::ImageDelta::full(image, egui::TextureOptions::NEAREST);

        let error = store
            .set(egui::TextureId::Managed(0), &delta)
            .expect_err("over-budget texture");

        assert_eq!(
            error.to_string(),
            format!(
                "texture byte budget exceeded: {} > {MAX_TEXTURE_BYTES}",
                MAX_TEXTURE_BYTES + 4
            )
        );
        assert_eq!(store.len(), 0);
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
    fn triangle_rasterizer_draws_into_tiny_surface() {
        let mut surface = SoftwareSurface::default();
        surface.resize(2, 2).expect("surface");
        surface.clear([0, 0, 0, 255]);
        let texture = TextureImage {
            width: 1,
            height: 1,
            pixels: vec![255, 255, 255, 255],
        };
        let v0 = test_vertex(0.0, 0.0);
        let v1 = test_vertex(2.0, 0.0);
        let v2 = test_vertex(0.0, 2.0);

        rasterize_triangle(
            &mut surface,
            &v0,
            &v1,
            &v2,
            &texture,
            ClipBounds {
                min_x: 0,
                min_y: 0,
                max_x: 2,
                max_y: 2,
            },
        );

        assert!(surface.pixels.chunks_exact(4).any(|pixel| pixel[0] == 255));
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

    fn test_vertex(x: f32, y: f32) -> egui::epaint::Vertex {
        egui::epaint::Vertex {
            pos: egui::pos2(x, y),
            color: egui::Color32::WHITE,
            uv: egui::Pos2::ZERO,
        }
    }
}
