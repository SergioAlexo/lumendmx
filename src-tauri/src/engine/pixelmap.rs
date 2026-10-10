//! Pixel maps (BUILD_PLAN Phase 7): a W×H grid of cells, each optionally
//! mapped to a fixture (or one cell of a multi-cell fixture), driven by a
//! built-in generator or an imported image/GIF.
//!
//! Rendering has two halves: `PixelMapRuntime::render_frame` produces one
//! RGB colour per grid position (so a map with no fixtures mapped — a
//! "virtual matrix" — still renders, and the editor can preview it), and
//! `frame_to_channels` turns that frame into channel values for whichever
//! cells are mapped, resolving fixtures through the patch index every tick.
//! A running map is a playback layer, merged like any other.

use std::collections::HashMap;
use std::f64::consts::PI;

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::effects::{rate_cycles, EffectClock, EffectRate};
use super::manager::PatchIndexEntry;
use super::state::ChannelKey;

pub type Rgb = [f32; 3];

/// Largest grid either side; keeps a typo from allocating a huge frame.
pub const MAX_MAP_SIDE: u32 = 256;
/// Imported media is capped at this many frames.
pub const MAX_MEDIA_FRAMES: usize = 300;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub enum Generator {
    /// The map's colours, one per cell, stepping along the direction.
    ColourChase,
    Rainbow,
    /// A soft band of the first colour moving along the direction.
    Wave,
    Plasma,
    /// The map's text in the first colour, scrolling horizontally.
    TextScroll,
    Fire,
    /// Drifting value noise in the first colour.
    Noise,
    /// The imported image/GIF.
    Media,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub enum MapDirection {
    Right,
    Left,
    Up,
    Down,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct PixelCell {
    pub x: u32,
    pub y: u32,
    pub fixture_number: u32,
    /// 0 = the whole fixture; n ≥ 1 = the fixture's nth cell (see
    /// `PatchIndexEntry::cells`).
    pub cell: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct MediaFrame {
    /// `width × height × 3` RGB bytes, base64-encoded (keeps show files a
    /// sane size compared to a JSON number array).
    pub rgb: String,
    pub delay_ms: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct PixelMedia {
    pub width: u32,
    pub height: u32,
    pub frames: Vec<MediaFrame>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct PixelMap {
    pub id: u32,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub cells: Vec<PixelCell>,
    pub generator: Generator,
    /// One cycle = one full pass of the pattern (a rainbow scrolling all the
    /// way across, the text scrolling past once, one loop of the media).
    pub rate: EffectRate,
    pub direction: MapDirection,
    /// Hex colours ("#ff0000"); used by ColourChase, Wave, TextScroll, Noise.
    pub colors: Vec<String>,
    pub text: String,
    pub media: Option<PixelMedia>,
}

impl PixelMap {
    pub fn new(id: u32, name: String, width: u32, height: u32) -> Self {
        Self {
            id,
            name,
            width: width.clamp(1, MAX_MAP_SIDE),
            height: height.clamp(1, MAX_MAP_SIDE),
            cells: Vec::new(),
            generator: Generator::Rainbow,
            rate: EffectRate::Beats { beats: 4.0 },
            direction: MapDirection::Right,
            colors: vec!["#ff0000".into(), "#00ff00".into(), "#0000ff".into()],
            text: "LUMEN".into(),
            media: None,
        }
    }
}

#[derive(Default)]
pub struct PixelMapStore {
    maps: HashMap<u32, PixelMap>,
    next_id: u32,
}

impl PixelMapStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create(&mut self, name: String, width: u32, height: u32) -> u32 {
        self.next_id += 1;
        let id = self.next_id;
        self.maps.insert(id, PixelMap::new(id, name, width, height));
        id
    }

    pub fn get(&self, id: u32) -> Option<&PixelMap> {
        self.maps.get(&id)
    }

    /// Replaces a map's definition (the editor sends the whole map back).
    pub fn update(&mut self, mut map: PixelMap) -> Result<(), String> {
        if !self.maps.contains_key(&map.id) {
            return Err(format!("No pixel map {}", map.id));
        }
        map.width = map.width.clamp(1, MAX_MAP_SIDE);
        map.height = map.height.clamp(1, MAX_MAP_SIDE);
        let (w, h) = (map.width, map.height);
        map.cells.retain(|c| c.x < w && c.y < h);
        self.maps.insert(map.id, map);
        Ok(())
    }

    pub fn delete(&mut self, id: u32) {
        self.maps.remove(&id);
    }

    pub fn list(&self) -> Vec<PixelMap> {
        let mut maps: Vec<PixelMap> = self.maps.values().cloned().collect();
        maps.sort_by_key(|m| m.id);
        maps
    }

    /// Replaces every map (loading a show).
    pub fn replace_all(&mut self, maps: Vec<PixelMap>) {
        self.next_id = maps.iter().map(|m| m.id).max().unwrap_or(0);
        self.maps = maps.into_iter().map(|m| (m.id, m)).collect();
    }
}

/// A running pixel map. Holds the little state some generators need (fire's
/// heat field) and the last rendered frame, for the editor's preview.
pub struct PixelMapRuntime {
    pub map_id: u32,
    pub priority: i64,
    /// Master intensity, 0.0-1.0.
    pub intensity: f32,
    heat: Vec<f32>,
    fire_steps: i64,
    seed: u64,
    pub last_frame: Vec<Rgb>,
}

impl PixelMapRuntime {
    pub fn new(map_id: u32, priority: i64) -> Self {
        Self { map_id, priority, intensity: 1.0, heat: Vec::new(), fire_steps: 0, seed: 0x5EED, last_frame: Vec::new() }
    }

    /// One RGB colour (0-255) per grid position, row-major.
    pub fn render_frame(&mut self, map: &PixelMap, clock: EffectClock) -> Vec<Rgb> {
        let (w, h) = (map.width as usize, map.height as usize);
        let t = rate_cycles(map.rate, clock);
        let colors: Vec<Rgb> = map.colors.iter().filter_map(|c| parse_hex(c)).collect();
        let first = colors.first().copied().unwrap_or([255.0, 255.0, 255.0]);

        let frame: Vec<Rgb> = match map.generator {
            Generator::Fire => self.render_fire(w, h, t),
            Generator::TextScroll => render_text(&map.text, w, h, t, map.direction, first),
            Generator::Media => render_media(map, t),
            _ => {
                let mut frame = Vec::with_capacity(w * h);
                for y in 0..h {
                    for x in 0..w {
                        frame.push(pixel(map, &colors, first, x, y, t));
                    }
                }
                frame
            }
        };
        self.last_frame = frame.clone();
        frame
    }

    /// The classic fire effect: random heat fed in along the bottom row,
    /// rising and cooling. Steps `height` times per cycle, so the rate
    /// controls how fast the flames rise.
    fn render_fire(&mut self, w: usize, h: usize, t: f64) -> Vec<Rgb> {
        if self.heat.len() != w * h {
            self.heat = vec![0.0; w * h];
        }
        let target_steps = (t * h as f64).floor() as i64;
        // Catch up at most a few steps per tick (e.g. after a long pause).
        let steps = (target_steps - self.fire_steps).clamp(0, 4);
        self.fire_steps = target_steps;
        for _ in 0..steps {
            for x in 0..w {
                self.seed = next_rand(self.seed);
                self.heat[(h - 1) * w + x] = 0.6 + 0.4 * unit(self.seed);
            }
            for y in 0..h.saturating_sub(1) {
                for x in 0..w {
                    let below = (y + 1) * w + x;
                    let left = (y + 1) * w + x.saturating_sub(1);
                    let right = (y + 1) * w + (x + 1).min(w - 1);
                    self.seed = next_rand(self.seed);
                    let cooling = 0.08 + 0.12 * unit(self.seed) + 1.0 / h as f32 * 0.5;
                    let avg = (self.heat[below] * 2.0 + self.heat[left] + self.heat[right]) / 4.0;
                    self.heat[y * w + x] = (avg - cooling * 0.5).max(0.0);
                }
            }
        }
        self.heat.iter().map(|&heat| fire_color(heat)).collect()
    }
}

/// Position along the map's direction of travel, 0.0-1.0 (the pattern moves
/// toward increasing values).
fn along(map: &PixelMap, x: usize, y: usize) -> f64 {
    let u = (x as f64 + 0.5) / map.width as f64;
    let v = (y as f64 + 0.5) / map.height as f64;
    match map.direction {
        MapDirection::Right => u,
        MapDirection::Left => 1.0 - u,
        MapDirection::Down => v,
        MapDirection::Up => 1.0 - v,
    }
}

/// The stateless generators, per pixel.
fn pixel(map: &PixelMap, colors: &[Rgb], first: Rgb, x: usize, y: usize, t: f64) -> Rgb {
    let a = along(map, x, y);
    match map.generator {
        Generator::ColourChase => {
            if colors.is_empty() {
                return [0.0; 3];
            }
            let cells_along = match map.direction {
                MapDirection::Right | MapDirection::Left => map.width,
                MapDirection::Up | MapDirection::Down => map.height,
            } as f64;
            let position = (a * cells_along).floor();
            let offset = (t * cells_along).floor();
            colors[((position - offset).rem_euclid(colors.len() as f64)) as usize]
        }
        Generator::Rainbow => hsv_to_rgb((a - t).rem_euclid(1.0), 1.0, 1.0),
        Generator::Wave => {
            let level = 0.5 + 0.5 * (2.0 * PI * (a - t)).sin();
            scale(first, level as f32)
        }
        Generator::Plasma => {
            let u = x as f64 / map.width.max(1) as f64 * 2.0 - 1.0;
            let v = y as f64 / map.height.max(1) as f64 * 2.0 - 1.0;
            let time = 2.0 * PI * t;
            let value = (u * 5.0 + time).sin()
                + ((u * (time / 2.0).sin() + v * (time / 3.0).cos()) * 5.0 + time).sin()
                + ((u * u + v * v).sqrt() * 8.0 - time).sin();
            hsv_to_rgb((value / 6.0 + 0.5).rem_euclid(1.0), 1.0, 1.0)
        }
        Generator::Noise => {
            let level = value_noise(x as f64 * 0.35, y as f64 * 0.35, t * 4.0);
            scale(first, level as f32)
        }
        Generator::Fire | Generator::TextScroll | Generator::Media => [0.0; 3],
    }
}

fn render_text(text: &str, w: usize, h: usize, t: f64, direction: MapDirection, color: Rgb) -> Vec<Rgb> {
    let columns: Vec<u8> = text
        .chars()
        .flat_map(|c| glyph(c).iter().copied().chain(std::iter::once(0)))
        .collect();
    let text_width = columns.len() as f64;
    // One cycle scrolls the text all the way in from one edge and out the
    // other.
    let travel = text_width + w as f64;
    let shift = (t.rem_euclid(1.0) * travel).floor() as i64;
    let top = (h as i64 - 7) / 2;
    let mut frame = vec![[0.0; 3]; w * h];
    for x in 0..w {
        // Moving left: text enters from the right edge.
        let column = match direction {
            MapDirection::Right => x as i64 - (shift - text_width as i64),
            _ => x as i64 + shift - w as i64,
        };
        let Some(&bits) = usize::try_from(column).ok().and_then(|c| columns.get(c)) else { continue };
        for row in 0..7i64 {
            let y = top + row;
            if (0..h as i64).contains(&y) && bits & (1 << row) != 0 {
                frame[y as usize * w + x] = color;
            }
        }
    }
    frame
}

fn render_media(map: &PixelMap, t: f64) -> Vec<Rgb> {
    let (w, h) = (map.width as usize, map.height as usize);
    let Some(media) = map.media.as_ref().filter(|m| !m.frames.is_empty()) else {
        return vec![[0.0; 3]; w * h];
    };
    let total: f64 = media.frames.iter().map(|f| f.delay_ms.max(1.0)).sum();
    let mut position = t.rem_euclid(1.0) * total;
    let frame = media
        .frames
        .iter()
        .find(|f| {
            position -= f.delay_ms.max(1.0);
            position < 0.0
        })
        .unwrap_or(&media.frames[media.frames.len() - 1]);
    let bytes = base64::engine::general_purpose::STANDARD.decode(&frame.rgb).unwrap_or_default();
    let (mw, mh) = (media.width.max(1) as usize, media.height.max(1) as usize);
    let mut out = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            // Nearest-neighbour, in case the map was resized after import.
            let sx = x * mw / w;
            let sy = y * mh / h;
            let i = (sy * mw + sx) * 3;
            out.push(match bytes.get(i..i + 3) {
                Some(px) => [px[0] as f32, px[1] as f32, px[2] as f32],
                None => [0.0; 3],
            });
        }
    }
    out
}

/// Decodes an image or (animated) GIF and scales every frame to `width` ×
/// `height`.
pub fn import_media(bytes: &[u8], width: u32, height: u32) -> Result<PixelMedia, String> {
    use image::AnimationDecoder;

    let (width, height) = (width.clamp(1, MAX_MAP_SIDE), height.clamp(1, MAX_MAP_SIDE));
    let format = image::guess_format(bytes).map_err(|e| format!("Unrecognised image: {e}"))?;
    let encode = |img: image::RgbaImage, delay_ms: f64| {
        let scaled = image::imageops::resize(&img, width, height, image::imageops::FilterType::Triangle);
        let rgb: Vec<u8> = scaled.pixels().flat_map(|p| {
            // Transparent pixels go dark, not white.
            let a = p[3] as f32 / 255.0;
            [(p[0] as f32 * a) as u8, (p[1] as f32 * a) as u8, (p[2] as f32 * a) as u8]
        }).collect();
        MediaFrame { rgb: base64::engine::general_purpose::STANDARD.encode(rgb), delay_ms }
    };

    let frames = if format == image::ImageFormat::Gif {
        let decoder = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
        let mut frames = Vec::new();
        for frame in decoder.into_frames().take(MAX_MEDIA_FRAMES) {
            let frame = frame.map_err(|e| e.to_string())?;
            let (num, den) = frame.delay().numer_denom_ms();
            let delay = if den == 0 { 100.0 } else { num as f64 / den as f64 };
            frames.push(encode(frame.into_buffer(), if delay <= 0.0 { 100.0 } else { delay }));
        }
        frames
    } else {
        let img = image::load_from_memory_with_format(bytes, format).map_err(|e| e.to_string())?;
        vec![encode(img.to_rgba8(), 1000.0)]
    };
    if frames.is_empty() {
        return Err("The image has no frames".to_string());
    }
    Ok(PixelMedia { width, height, frames })
}

/// Turns a rendered frame into channel values for every mapped cell.
/// RGB fixtures get red/green/blue; CMY fixtures the inverse; a cell with
/// only a dimmer gets the colour's brightness. A fixture's own dimmer (the
/// user's pars need one up to show any colour) is driven to full, and
/// scaled by the layer's intensity master at merge time like any other
/// intensity channel; a fixture without a dimmer has the master applied to
/// its colour channels directly.
pub fn frame_to_channels(
    map: &PixelMap,
    frame: &[Rgb],
    patch: &HashMap<u32, &PatchIndexEntry>,
    intensity: f32,
) -> HashMap<ChannelKey, f32> {
    let mut out = HashMap::new();
    for cell in &map.cells {
        let Some(entry) = patch.get(&cell.fixture_number) else { continue };
        let Some(&color) = frame.get((cell.y * map.width + cell.x) as usize) else { continue };
        let attrs = if cell.cell == 0 {
            &entry.attribute_channels
        } else {
            match entry.cells.get(cell.cell as usize - 1) {
                Some(attrs) => attrs,
                None => continue,
            }
        };
        let fixture_dimmer = entry.attribute_channels.get("dimmer").copied();
        let has_color = ["red", "green", "blue", "cyan", "magenta", "yellow"].iter().any(|a| attrs.contains_key(*a));
        let color_scale = if fixture_dimmer.is_some() { 1.0 } else { intensity };
        let color = scale(color, color_scale);
        let mut set = |attr: &str, value: f32| {
            if let Some(&channel) = attrs.get(attr) {
                out.insert((entry.universe, channel), value.clamp(0.0, 255.0));
            }
        };
        if has_color {
            set("red", color[0]);
            set("green", color[1]);
            set("blue", color[2]);
            set("cyan", 255.0 - color[0]);
            set("magenta", 255.0 - color[1]);
            set("yellow", 255.0 - color[2]);
            if let Some(dimmer) = fixture_dimmer {
                out.insert((entry.universe, dimmer), 255.0);
            }
        } else if let Some(&dimmer) = attrs.get("dimmer") {
            out.insert((entry.universe, dimmer), luminance(color));
        }
    }
    out
}

// --- Colour and noise helpers ---

fn scale(c: Rgb, k: f32) -> Rgb {
    [c[0] * k, c[1] * k, c[2] * k]
}

fn luminance(c: Rgb) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

pub fn parse_hex(hex: &str) -> Option<Rgb> {
    let hex = hex.trim().trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(hex, 16).ok()?;
    Some([((v >> 16) & 0xFF) as f32, ((v >> 8) & 0xFF) as f32, (v & 0xFF) as f32])
}

/// h, s, v in [0,1] → RGB 0-255.
pub fn hsv_to_rgb(h: f64, s: f64, v: f64) -> Rgb {
    let h6 = h.rem_euclid(1.0) * 6.0;
    let i = h6.floor();
    let f = h6 - i;
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    let (r, g, b) = match i as u8 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    [(r * 255.0) as f32, (g * 255.0) as f32, (b * 255.0) as f32]
}

fn fire_color(heat: f32) -> Rgb {
    let h = heat.clamp(0.0, 1.0) * 3.0;
    [(h.min(1.0)) * 255.0, (h - 1.0).clamp(0.0, 1.0) * 255.0, (h - 2.0).clamp(0.0, 1.0) * 255.0]
}

fn next_rand(seed: u64) -> u64 {
    // xorshift64
    let mut x = seed;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    x
}

fn unit(seed: u64) -> f32 {
    (seed >> 40) as f32 / (1u64 << 24) as f32
}

fn lattice(x: i64, y: i64, z: i64) -> f64 {
    let mut h = (x as u64).wrapping_mul(0x8DA6_B343) ^ (y as u64).wrapping_mul(0xD816_3841) ^ (z as u64).wrapping_mul(0xCB1A_B31F);
    h = next_rand(h | 1);
    (h >> 11) as f64 / (1u64 << 53) as f64
}

fn smooth(t: f64) -> f64 {
    t * t * (3.0 - 2.0 * t)
}

/// Smooth value noise in [0,1].
fn value_noise(x: f64, y: f64, z: f64) -> f64 {
    let (x0, y0, z0) = (x.floor(), y.floor(), z.floor());
    let (fx, fy, fz) = (smooth(x - x0), smooth(y - y0), smooth(z - z0));
    let (x0, y0, z0) = (x0 as i64, y0 as i64, z0 as i64);
    let lerp = |a: f64, b: f64, t: f64| a + (b - a) * t;
    let layer = |z: i64| {
        let top = lerp(lattice(x0, y0, z), lattice(x0 + 1, y0, z), fx);
        let bottom = lerp(lattice(x0, y0 + 1, z), lattice(x0 + 1, y0 + 1, z), fx);
        lerp(top, bottom, fy)
    };
    lerp(layer(z0), layer(z0 + 1), fz)
}

/// A 5×7 font, one byte per column, bit 0 = top row. Lowercase is drawn as
/// uppercase; anything else unknown as '?'.
fn glyph(c: char) -> &'static [u8; 5] {
    const SPACE: [u8; 5] = [0x00, 0x00, 0x00, 0x00, 0x00];
    const QUESTION: [u8; 5] = [0x02, 0x01, 0x51, 0x09, 0x06];
    const DIGITS: [[u8; 5]; 10] = [
        [0x3E, 0x51, 0x49, 0x45, 0x3E],
        [0x00, 0x42, 0x7F, 0x40, 0x00],
        [0x42, 0x61, 0x51, 0x49, 0x46],
        [0x21, 0x41, 0x45, 0x4B, 0x31],
        [0x18, 0x14, 0x12, 0x7F, 0x10],
        [0x27, 0x45, 0x45, 0x45, 0x39],
        [0x3C, 0x4A, 0x49, 0x49, 0x30],
        [0x01, 0x71, 0x09, 0x05, 0x03],
        [0x36, 0x49, 0x49, 0x49, 0x36],
        [0x06, 0x49, 0x49, 0x29, 0x1E],
    ];
    const LETTERS: [[u8; 5]; 26] = [
        [0x7E, 0x11, 0x11, 0x11, 0x7E],
        [0x7F, 0x49, 0x49, 0x49, 0x36],
        [0x3E, 0x41, 0x41, 0x41, 0x22],
        [0x7F, 0x41, 0x41, 0x22, 0x1C],
        [0x7F, 0x49, 0x49, 0x49, 0x41],
        [0x7F, 0x09, 0x09, 0x09, 0x01],
        [0x3E, 0x41, 0x49, 0x49, 0x7A],
        [0x7F, 0x08, 0x08, 0x08, 0x7F],
        [0x00, 0x41, 0x7F, 0x41, 0x00],
        [0x20, 0x40, 0x41, 0x3F, 0x01],
        [0x7F, 0x08, 0x14, 0x22, 0x41],
        [0x7F, 0x40, 0x40, 0x40, 0x40],
        [0x7F, 0x02, 0x0C, 0x02, 0x7F],
        [0x7F, 0x04, 0x08, 0x10, 0x7F],
        [0x3E, 0x41, 0x41, 0x41, 0x3E],
        [0x7F, 0x09, 0x09, 0x09, 0x06],
        [0x3E, 0x41, 0x51, 0x21, 0x5E],
        [0x7F, 0x09, 0x19, 0x29, 0x46],
        [0x46, 0x49, 0x49, 0x49, 0x31],
        [0x01, 0x01, 0x7F, 0x01, 0x01],
        [0x3F, 0x40, 0x40, 0x40, 0x3F],
        [0x1F, 0x20, 0x40, 0x20, 0x1F],
        [0x3F, 0x40, 0x38, 0x40, 0x3F],
        [0x63, 0x14, 0x08, 0x14, 0x63],
        [0x07, 0x08, 0x70, 0x08, 0x07],
        [0x61, 0x51, 0x49, 0x45, 0x43],
    ];
    const PUNCT: [(char, [u8; 5]); 6] = [
        ('!', [0x00, 0x00, 0x5F, 0x00, 0x00]),
        ('-', [0x08, 0x08, 0x08, 0x08, 0x08]),
        ('.', [0x00, 0x60, 0x60, 0x00, 0x00]),
        (':', [0x00, 0x36, 0x36, 0x00, 0x00]),
        ('+', [0x08, 0x08, 0x3E, 0x08, 0x08]),
        ('/', [0x20, 0x10, 0x08, 0x04, 0x02]),
    ];
    let c = c.to_ascii_uppercase();
    match c {
        ' ' => &SPACE,
        '0'..='9' => &DIGITS[c as usize - '0' as usize],
        'A'..='Z' => &LETTERS[c as usize - 'A' as usize],
        _ => PUNCT.iter().find(|(p, _)| *p == c).map_or(&QUESTION, |(_, g)| g),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgb_fixture(number: u32, base: u16) -> PatchIndexEntry {
        PatchIndexEntry {
            fixture_number: number,
            universe: 1,
            intensity_channel: Some(base),
            attribute_channels: [("dimmer", base), ("red", base + 1), ("green", base + 2), ("blue", base + 3)]
                .into_iter()
                .map(|(a, c)| (a.to_string(), c))
                .collect(),
            cells: Vec::new(),
        }
    }

    fn render(map: &PixelMap, beats: f64) -> Vec<Rgb> {
        PixelMapRuntime::new(map.id, 1).render_frame(map, EffectClock { beats, seconds: 0.0 })
    }

    #[test]
    fn rainbow_on_a_virtual_10x10_matrix_is_bpm_synced() {
        let mut map = PixelMap::new(1, "Matrix".into(), 10, 10);
        map.rate = EffectRate::Beats { beats: 4.0 };
        let start = render(&map, 0.0);
        assert_eq!(start.len(), 100);
        // Hue varies across a row, not down a column (direction Right).
        assert_ne!(start[0], start[5]);
        assert_eq!(start[0], start[90]);
        // One full cycle (4 beats) later the pattern is back where it started…
        assert_eq!(render(&map, 4.0), start);
        // …and halfway through it has moved on.
        assert_ne!(render(&map, 2.0), start);
    }

    #[test]
    fn colour_chase_steps_one_cell_along_per_step() {
        let mut map = PixelMap::new(1, "Chase".into(), 3, 1);
        map.generator = Generator::ColourChase;
        map.rate = EffectRate::Beats { beats: 3.0 }; // 1 beat per cell step
        let f0 = render(&map, 0.0);
        assert_eq!(f0, vec![[255.0, 0.0, 0.0], [0.0, 255.0, 0.0], [0.0, 0.0, 255.0]]);
        let f1 = render(&map, 1.0);
        assert_eq!(f1[1], f0[0]); // red moved one cell right
    }

    #[test]
    fn text_scroll_lights_some_pixels_partway_through() {
        let mut map = PixelMap::new(1, "Text".into(), 10, 7);
        map.generator = Generator::TextScroll;
        map.rate = EffectRate::Beats { beats: 1.0 };
        assert!(render(&map, 0.0).iter().all(|c| *c == [0.0; 3])); // still off-screen
        assert!(render(&map, 0.4).iter().any(|c| *c != [0.0; 3]));
    }

    #[test]
    fn fire_heats_up_from_the_bottom() {
        let mut map = PixelMap::new(1, "Fire".into(), 4, 6);
        map.generator = Generator::Fire;
        map.rate = EffectRate::Hz { hz: 1.0 };
        let mut runtime = PixelMapRuntime::new(1, 1);
        let mut frame = Vec::new();
        for i in 1..=40 {
            frame = runtime.render_frame(&map, EffectClock { beats: 0.0, seconds: i as f64 * 0.1 });
        }
        let row_heat = |y: usize| frame[y * 4..y * 4 + 4].iter().map(|c| c[0]).sum::<f32>();
        assert!(row_heat(5) > row_heat(0));
    }

    #[test]
    fn plasma_and_noise_render_within_range() {
        for generator in [Generator::Plasma, Generator::Noise, Generator::Wave] {
            let mut map = PixelMap::new(1, "M".into(), 8, 8);
            map.generator = generator;
            let frame = render(&map, 1.3);
            assert!(frame.iter().flatten().all(|v| (0.0..=255.0).contains(v)), "{generator:?}");
            assert!(frame.iter().any(|c| *c != frame[0]), "{generator:?} is flat");
        }
    }

    #[test]
    fn frame_maps_onto_fixture_rgb_and_raises_their_dimmer() {
        let mut map = PixelMap::new(1, "M".into(), 2, 1);
        map.cells = vec![
            PixelCell { x: 0, y: 0, fixture_number: 1, cell: 0 },
            PixelCell { x: 1, y: 0, fixture_number: 2, cell: 0 },
        ];
        let patch = [rgb_fixture(1, 1), rgb_fixture(2, 11)];
        let index: HashMap<u32, &PatchIndexEntry> = patch.iter().map(|e| (e.fixture_number, e)).collect();
        let frame = vec![[255.0, 0.0, 0.0], [0.0, 0.0, 128.0]];
        let out = frame_to_channels(&map, &frame, &index, 0.5);
        assert_eq!(out[&(1, 1)], 255.0); // dimmer full; the master scales it at merge
        assert_eq!(out[&(1, 2)], 255.0);
        assert_eq!(out[&(1, 14)], 128.0);
        assert_eq!(out[&(1, 12)], 0.0);
    }

    #[test]
    fn multi_cell_fixtures_map_per_cell_and_dimmer_only_cells_get_brightness() {
        let mut bar = rgb_fixture(1, 1);
        bar.cells = vec![
            [("red".to_string(), 5u16), ("green".to_string(), 6), ("blue".to_string(), 7)].into_iter().collect(),
            [("dimmer".to_string(), 8u16)].into_iter().collect(),
        ];
        let mut map = PixelMap::new(1, "M".into(), 2, 1);
        map.cells = vec![
            PixelCell { x: 0, y: 0, fixture_number: 1, cell: 1 },
            PixelCell { x: 1, y: 0, fixture_number: 1, cell: 2 },
        ];
        let index: HashMap<u32, &PatchIndexEntry> = [(1, &bar)].into_iter().collect();
        let out = frame_to_channels(&map, &[[0.0, 255.0, 0.0], [255.0, 255.0, 255.0]], &index, 1.0);
        assert_eq!(out[&(1, 6)], 255.0);
        assert!((out[&(1, 8)] - 255.0).abs() < 0.01);
    }

    #[test]
    fn imports_a_png_scaled_to_the_map() {
        let mut img = image::RgbaImage::new(4, 4);
        for (x, _, p) in img.enumerate_pixels_mut() {
            *p = if x < 2 { image::Rgba([255, 0, 0, 255]) } else { image::Rgba([0, 0, 255, 255]) };
        }
        let mut bytes = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png).unwrap();

        let media = import_media(&bytes, 2, 2).unwrap();
        assert_eq!(media.frames.len(), 1);
        let mut map = PixelMap::new(1, "M".into(), 2, 2);
        map.generator = Generator::Media;
        map.media = Some(media);
        let frame = render(&map, 0.0);
        assert!(frame[0][0] > 200.0 && frame[0][2] < 50.0);
        assert!(frame[1][2] > 200.0 && frame[1][0] < 50.0);
    }

    #[test]
    fn store_update_clamps_size_and_drops_cells_outside_it() {
        let mut store = PixelMapStore::new();
        let id = store.create("M".into(), 4, 4);
        let mut map = store.get(id).unwrap().clone();
        map.width = 2;
        map.cells = vec![PixelCell { x: 3, y: 0, fixture_number: 1, cell: 0 }];
        store.update(map).unwrap();
        assert!(store.get(id).unwrap().cells.is_empty());
    }
}
