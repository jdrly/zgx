//! Scratch (not shipped): a frame's measured floor. A program that knows
//! its layout and what changes patches one full frame's instances where
//! values change and submits them through the renderer's own direct path
//! (`render_direct`), with no scene, layout, damage or retained shapes.
use crate::*;
use cosmic_text::{Buffer, Ellipsize, Hinting, LayoutRunIter, Wrap};
use std::cell::RefCell;

thread_local! {
    /// `Some(None)` asks `render_flat` to keep the next direct frame.
    pub(crate) static CAPTURE: RefCell<Option<Option<Captured>>> = const { RefCell::new(None) };
}

/// One direct frame as `render_flat` handed it to `render_direct`.
pub(crate) struct Captured {
    pub(crate) quads: Vec<Quad>,
    pub(crate) draws: Vec<Draw>,
    pub(crate) regions: Vec<(Rect, Scissor)>,
    pub(crate) clips: Vec<Region>,
}

/// A text whose glyphs are written in place: room for `cap` glyphs from
/// `start`, unused ones empty.
struct Slot {
    start: u32,
    cap: u32,
    /// Its box, snapped as `render_flat` places glyphs.
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    font: zgx::text_layout::FontStyle,
    size: f32,
    /// A glyph quad of the text as captured: colour, fade, mask.
    template: Quad,
    buffer: Buffer,
}

/// A captured frame being patched.
pub struct Floor {
    quads: Vec<Quad>,
    draws: Vec<Draw>,
    regions: Vec<(Rect, Scissor)>,
    clips: Vec<Region>,
    slots: Vec<Slot>,
    /// Room to add after a captured index, while slots are being found.
    room: Vec<(u32, u32)>,
    glyphs: Vec<Glyph>,
    stats: GpuStats,
}

impl GpuRenderer {
    /// Render `scene` whole and keep the frame for patching.
    pub fn floor_capture(&mut self, scene: &Scene) -> Result<Floor, GpuError> {
        CAPTURE.with(|c| *c.borrow_mut() = Some(None));
        self.fresh = true;
        let viewport = Rect::new(
            0.,
            0.,
            self.width as f32 / self.scale,
            self.height as f32 / self.scale,
        );
        self.render(scene, &[viewport])?;
        let captured = CAPTURE
            .with(|c| c.borrow_mut().take().flatten())
            .ok_or_else(|| GpuError("the frame took no direct path".into()))?;
        Ok(Floor {
            quads: captured.quads,
            draws: captured.draws,
            regions: captured.regions,
            clips: captured.clips,
            slots: Vec::new(),
            room: Vec::new(),
            glyphs: Vec::new(),
            stats: GpuStats::default(),
        })
    }
}

impl Floor {
    /// The text drawn in `bounds` (logical, world): its glyphs become a
    /// slot of `cap`. Its glyph quads are the run of glyph quads inside it.
    pub fn text_slot(
        &mut self,
        bounds: Rect,
        font: &zgx::text_layout::FontStyle,
        size: f32,
        cap: u32,
        scale: f32,
    ) -> Option<usize> {
        let inside = |q: &Quad| {
            let (cx, cy) = (q.rect[0] + q.rect[2] / 2., q.rect[1] + q.rect[3] / 2.);
            (q.options[1] - 1.).abs() < 1e-6
                && cx >= bounds.x
                && cx <= bounds.x + bounds.width
                && cy >= bounds.y
                && cy <= bounds.y + bounds.height
        };
        let first = self.quads.iter().position(inside)?;
        let count = self.quads[first..].iter().take_while(|q| inside(q)).count();
        let snap = |value: f32| (value * scale).round() / scale;
        let size = size.max(1.);
        let buffer = Buffer::new_empty(cosmic_text::Metrics::new(
            size,
            font.line_height.resolve(size),
        ));
        self.room
            .push(((first + count) as u32, cap.saturating_sub(count as u32)));
        self.slots.push(Slot {
            start: first as u32,
            cap: cap.max(count as u32),
            x: snap(bounds.x),
            y: snap(bounds.y),
            width: bounds.width,
            height: bounds.height,
            font: font.clone(),
            size,
            template: self.quads[first],
            buffer,
        });
        Some(self.slots.len() - 1)
    }
    /// Make the room slots asked for, moving draws and slots with it.
    pub fn finish(&mut self) {
        self.room.sort_unstable();
        // Room at `at` goes before the captured quad `at`: after a draw or
        // slot ending there, before one starting there.
        let shift = |index: u32, room: &[(u32, u32)]| {
            index
                + room
                    .iter()
                    .filter(|(at, _)| *at <= index)
                    .map(|(_, n)| n)
                    .sum::<u32>()
        };
        let mut quads = Vec::with_capacity(self.quads.len() + 64 * self.room.len());
        let mut next = 0;
        for (at, n) in &self.room {
            quads.extend_from_slice(&self.quads[next..*at as usize]);
            quads.extend(std::iter::repeat_n(
                <Quad as bytemuck::Zeroable>::zeroed(),
                *n as usize,
            ));
            next = *at as usize;
        }
        quads.extend_from_slice(&self.quads[next..]);
        for draw in &mut self.draws {
            draw.start = shift(draw.start, &self.room);
            draw.end = shift(draw.end, &self.room);
            draw.bounds = draw.clip;
        }
        for slot in &mut self.slots {
            slot.start = shift(slot.start, &self.room);
        }
        self.quads = quads;
        self.room.clear();
    }
    /// The quads whose rectangle and colour `is` accepts.
    pub fn find(&self, is: impl Fn([f32; 4], [f32; 4]) -> bool) -> Vec<u32> {
        (0..self.quads.len() as u32)
            .filter(|i| {
                let q = &self.quads[*i as usize];
                is(q.rect, q.color)
            })
            .collect()
    }
    pub fn rect(&self, index: u32) -> [f32; 4] {
        self.quads[index as usize].rect
    }
    pub fn debug(&self, index: u32) -> String {
        let q = &self.quads[index as usize];
        format!(
            "rect {:?} uv {:?} color {:?} fade {:?} options {:?} shape {:?} border {:?} mask {:?}",
            q.rect, q.uv, q.color, q.fade, q.options, q.shape, q.border, q.mask
        )
    }
    /// Quad `index` will move: its draw shades edges at any fraction (a
    /// draw captured with whole-pixel edges took the basic pipeline).
    pub fn moves(&mut self, index: u32) {
        for draw in &mut self.draws {
            if draw.start <= index
                && index < draw.end
                && matches!(draw.shading, Shading::Basic | Shading::Opaque)
            {
                draw.shading = Shading::Full;
            }
        }
    }
    /// Move quad `index` to `rect`: a box carries its box in `fade` too.
    pub fn set_rect(&mut self, index: u32, rect: [f32; 4]) {
        let quad = &mut self.quads[index as usize];
        if quad.shape[3] > 0.5 && quad.fade == quad.rect {
            quad.fade = rect;
        }
        quad.rect = rect;
    }
    pub fn set_color(&mut self, index: u32, color: Color) {
        self.quads[index as usize].color = rgba(color, 1.);
    }
    /// A colour as quads carry it, to compare with `find`'s.
    pub fn color(color: Color) -> [f32; 4] {
        rgba(color, 1.)
    }
    /// Write `text` into slot `slot`: shaped (words from the word cache),
    /// laid out and placed as `render_flat` places plain text.
    pub fn set_text(&mut self, gpu: &mut GpuRenderer, slot: usize, text: &str) {
        let fonts = gpu.fonts.clone();
        let mut fonts = fonts.borrow_mut();
        let s = &mut self.slots[slot];
        crate::text::set_buffer_text(&mut fonts, &mut s.buffer, text, &s.font, s.size);
        for line in s.buffer.lines.iter_mut() {
            line.layout(
                &mut fonts,
                s.size,
                crate::text::wrap_width(Some(s.width)),
                Wrap::WordOrGlyph,
                Ellipsize::None,
                None,
                8,
                Hinting::default(),
            );
        }
        drop(fonts);
        let line_height = s.buffer.metrics().line_height;
        let runs = LayoutRunIter::from_lines(&s.buffer.lines, Some(s.height), line_height, 0., 0);
        self.glyphs = glyphs_of(std::mem::take(&mut self.glyphs), runs, &[], None, gpu.scale);
        let (start, cap, x, y, template) = (s.start, s.cap, s.x, s.y, s.template);
        let mut at = start;
        for glyph in &self.glyphs {
            let Ok(Some(a)) = gpu.glyph(glyph.key, &mut self.stats) else {
                continue;
            };
            if at == start + cap {
                break;
            }
            self.quads[at as usize] = Quad {
                rect: [
                    (glyph.x + a.left as f32) / gpu.scale + x,
                    (glyph.y - a.top as f32) / gpu.scale + y,
                    a.width as f32 / gpu.scale,
                    a.height as f32 / gpu.scale,
                ],
                uv: [
                    a.x as f32 / gpu.atlas_size as f32,
                    a.y as f32 / gpu.atlas_size as f32,
                    a.width as f32 / gpu.atlas_size as f32,
                    a.height as f32 / gpu.atlas_size as f32,
                ],
                options: [template.options[0], 1., 0., 0.],
                ..template
            };
            at += 1;
        }
        self.glyphs.clear();
        for quad in &mut self.quads[at as usize..(start + cap) as usize] {
            *quad = bytemuck::Zeroable::zeroed();
        }
    }
    /// Draw the frame through the renderer's direct path.
    pub fn submit(&mut self, gpu: &mut GpuRenderer) -> GpuStats {
        gpu.flush_pending();
        gpu.frame = gpu.frame.wrapping_add(1);
        if !gpu.texture_uploads.is_empty() {
            let mut encoder = gpu.device.create_command_encoder(&Default::default());
            gpu.texture_uploads.encode(&gpu.device, &mut encoder);
            gpu.queue.submit([encoder.finish()]);
        }
        gpu.fonts.borrow_mut().shape_run_cache.trim(SHAPE_RUN_AGES);
        let quads = std::mem::take(&mut self.quads);
        let stats = gpu.render_direct(
            quads,
            self.draws.clone(),
            &self.regions,
            &[],
            None,
            None,
            (&[], 0),
            &self.clips,
            None,
            std::mem::take(&mut self.stats),
        );
        self.quads = std::mem::take(&mut gpu.frame_quads);
        stats
    }
}
