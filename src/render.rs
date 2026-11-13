//! CPU rendering into a softbuffer frame: 0x00RRGGBB pixels.

use crate::assets;

const BACKGROUND: u32 = 0x2b2d42;
const TITLE_IDLE: u32 = 0x8d99ae;
const TITLE_LIT: u32 = 0xffd166;
/// The button fills this fraction of the window's shorter side.
const BUTTON_FRACTION: f32 = 0.8;

fn smoothstep(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

fn channels(c: u32) -> [f32; 3] {
    [(c >> 16 & 0xff) as f32, (c >> 8 & 0xff) as f32, (c & 0xff) as f32]
}

fn pack([r, g, b]: [f32; 3]) -> u32 {
    (r.round() as u32) << 16 | (g.round() as u32) << 8 | b.round() as u32
}

fn lerp_color(a: u32, b: u32, t: f32) -> u32 {
    let (a, b) = (channels(a), channels(b));
    pack([0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t))
}

/// Where things go for a given window size.
#[derive(Clone, Copy)]
pub struct Layout {
    width: usize,
    height: usize,
    side: f32,
    font: f32,
    center_x: f32,
    button_center_y: f32,
}

impl Layout {
    pub fn new(width: f32, height: f32) -> Self {
        // Button and title together are about 1.2 button-heights tall.
        let side = (width * BUTTON_FRACTION).min(height * BUTTON_FRACTION / 1.2);
        let font = side * 0.12;
        Self {
            width: width as usize,
            height: height as usize,
            side,
            font,
            center_x: width / 2.0,
            button_center_y: height / 2.0 - font * 0.8,
        }
    }

    pub fn button_contains(&self, x: f32, y: f32) -> bool {
        let half = self.side / 2.0;
        (x - self.center_x).abs() <= half && (y - self.button_center_y).abs() <= half
    }
}

pub struct Image {
    size: usize,
    pixels: Vec<u32>,
}

impl Image {
    /// Decodes RGB rows stored as left-deltas (see `build.rs`).
    pub fn from_filtered_rgb(data: &[u8], size: usize) -> Self {
        let mut pixels = Vec::with_capacity(size * size);
        for row in data.chunks_exact(size * 3) {
            let mut prev = [0u8; 3];
            for px in row.chunks_exact(3) {
                let cur = [0, 1, 2].map(|i| px[i].wrapping_add(prev[i]));
                pixels.push((cur[0] as u32) << 16 | (cur[1] as u32) << 8 | cur[2] as u32);
                prev = cur;
            }
        }
        Self { size, pixels }
    }

    fn sample(&self, x: f32, y: f32) -> [f32; 3] {
        let max = (self.size - 1) as f32;
        let (x, y) = (x.clamp(0.0, max), y.clamp(0.0, max));
        let (x0, y0) = (x as usize, y as usize);
        let (x1, y1) = ((x0 + 1).min(self.size - 1), (y0 + 1).min(self.size - 1));
        let (fx, fy) = (x - x0 as f32, y - y0 as f32);
        let px = |x, y| channels(self.pixels[y * self.size + x]);
        let (a, b, c, d) = (px(x0, y0), px(x1, y0), px(x0, y1), px(x1, y1));
        [0, 1, 2].map(|i| {
            let top = a[i] + (b[i] - a[i]) * fx;
            let bottom = c[i] + (d[i] - c[i]) * fx;
            top + (bottom - top) * fy
        })
    }
}

/// The button image scaled to one size, with rounded-corner coverage.
struct Scaled {
    open: bool,
    size: usize,
    pixels: Vec<u32>,
    coverage: Vec<u8>,
}

impl Scaled {
    fn new(image: &Image, open: bool, size: usize) -> Self {
        let scale = image.size as f32 / size as f32;
        let radius = size as f32 * 0.075;
        let mut pixels = Vec::with_capacity(size * size);
        let mut coverage = Vec::with_capacity(size * size);
        for y in 0..size {
            for x in 0..size {
                let (cx, cy) = (x as f32 + 0.5, y as f32 + 0.5);
                pixels.push(pack(image.sample(cx * scale - 0.5, cy * scale - 0.5)));
                // Distance into the corner arc, antialiased over one pixel.
                let dx = (radius - cx).max(cx - (size as f32 - radius)).max(0.0);
                let dy = (radius - cy).max(cy - (size as f32 - radius)).max(0.0);
                let outside = (dx * dx + dy * dy).sqrt() - radius;
                coverage.push(((0.5 - outside).clamp(0.0, 1.0) * 255.0) as u8);
            }
        }
        Self { open, size, pixels, coverage }
    }
}

pub struct Scene {
    closed: Image,
    open: Image,
    glyph_masks: Vec<u8>,
    cache: Option<Scaled>,
}

impl Scene {
    pub fn new(closed: Image, open: Image, glyph_masks: Vec<u8>) -> Self {
        Self { closed, open, glyph_masks, cache: None }
    }

    /// `press` and `active` are animation progress in 0..=1, `time` in seconds.
    pub fn draw(&mut self, frame: &mut [u32], layout: Layout, open: bool, press: f32, active: f32, time: f32) {
        frame.fill(BACKGROUND);

        // Slight shrink while held, so it feels like a button.
        let size = (layout.side * (1.0 - 0.06 * smoothstep(press))).round().max(1.0) as usize;
        if !self.cache.as_ref().is_some_and(|c| c.open == open && c.size == size) {
            let image = if open { &self.open } else { &self.closed };
            self.cache = Some(Scaled::new(image, open, size));
        }
        let scaled = self.cache.as_ref().unwrap();
        let left = (layout.center_x - size as f32 / 2.0).round() as isize;
        let top = (layout.button_center_y - size as f32 / 2.0).round() as isize;
        for y in 0..size {
            let fy = top + y as isize;
            if fy < 0 || fy >= layout.height as isize {
                continue;
            }
            for x in 0..size {
                let fx = left + x as isize;
                if fx < 0 || fx >= layout.width as isize {
                    continue;
                }
                let i = y * size + x;
                let dst = &mut frame[fy as usize * layout.width + fx as usize];
                *dst = match scaled.coverage[i] {
                    255 => scaled.pixels[i],
                    0 => continue,
                    c => lerp_color(*dst, scaled.pixels[i], c as f32 / 255.0),
                };
            }
        }

        self.draw_title(frame, layout, smoothstep(active), time);
    }

    /// While `activity` is above zero the letters hop in a wave and the text
    /// grows and brightens.
    fn draw_title(&self, frame: &mut [u32], layout: Layout, activity: f32, time: f32) {
        let px = layout.font * (1.0 + 0.12 * activity);
        let scale = px / assets::GLYPH_PX;
        let color = lerp_color(TITLE_IDLE, TITLE_LIT, activity);
        let spacing = px * 0.04;
        let total = assets::GLYPHS.iter().map(|g| g.advance * scale + spacing).sum::<f32>() - spacing;
        let line = (assets::GLYPH_ASCENT - assets::GLYPH_DESCENT) * scale;
        let title_y = layout.button_center_y + layout.side / 2.0 + layout.font * 1.1;
        let baseline = title_y - line / 2.0 + assets::GLYPH_ASCENT * scale;

        let mut pen = layout.center_x - total / 2.0;
        for (i, glyph) in assets::GLYPHS.iter().enumerate() {
            let phase = time * 9.0 - i as f32 * 0.7;
            let hop = phase.sin().max(0.0) * px * 0.35 * activity;
            let x0 = pen + glyph.left * scale;
            let y0 = baseline + glyph.top * scale - hop;
            pen += glyph.advance * scale + spacing;

            let mask = &self.glyph_masks[glyph.offset..glyph.offset + glyph.width * glyph.height];
            let coverage = |mx: f32, my: f32| -> f32 {
                let (ix, iy) = (mx.floor() as isize, my.floor() as isize);
                let (fx, fy) = (mx - ix as f32, my - iy as f32);
                let at = |x: isize, y: isize| {
                    if x < 0 || y < 0 || x >= glyph.width as isize || y >= glyph.height as isize {
                        0.0
                    } else {
                        mask[y as usize * glyph.width + x as usize] as f32
                    }
                };
                let top = at(ix, iy) + (at(ix + 1, iy) - at(ix, iy)) * fx;
                let bottom = at(ix, iy + 1) + (at(ix + 1, iy + 1) - at(ix, iy + 1)) * fx;
                (top + (bottom - top) * fy) / 255.0
            };

            let x_start = x0.floor().max(0.0) as usize;
            let y_start = y0.floor().max(0.0) as usize;
            let x_end = ((x0 + glyph.width as f32 * scale).ceil() as usize).min(layout.width);
            let y_end = ((y0 + glyph.height as f32 * scale).ceil() as usize).min(layout.height);
            for fy in y_start..y_end {
                for fx in x_start..x_end {
                    // 2×2 supersampling keeps small sizes smooth.
                    let mut a = 0.0;
                    for (sx, sy) in [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)] {
                        let mx = (fx as f32 + sx - x0) / scale - 0.5;
                        let my = (fy as f32 + sy - y0) / scale - 0.5;
                        a += coverage(mx, my);
                    }
                    if a > 0.0 {
                        let dst = &mut frame[fy * layout.width + fx];
                        *dst = lerp_color(*dst, color, (a / 4.0).min(1.0));
                    }
                }
            }
        }
    }
}
