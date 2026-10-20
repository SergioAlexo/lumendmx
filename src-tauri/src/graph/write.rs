//! Turning an Output node's values (intensity, colour, position ... all
//! 0-1) into a fixture's DMX channels: colour mixing for RGB/RGBW/CMY/colour
//! wheels, a virtual dimmer for fixtures without one, 16-bit pan/tilt with
//! invert/swap, and strobe ranges from the fixture's capabilities.

use std::collections::HashMap;

use super::compile::Val;
use crate::engine::state::ChannelKey;
use crate::show::library::ChannelCapability;
use crate::show::resolve::AttributeChannel;

/// One thing an Output node lights: a fixture, or one cell of it.
#[derive(Debug, Clone, PartialEq)]
pub struct TargetItem {
    /// Fixture number, and cell index when lighting cells separately.
    pub fixture_number: u32,
    pub cell: Option<usize>,
    pub universe: u32,
    /// Where intensity goes. Empty: no dimmer, so intensity scales colour.
    pub dimmers: Vec<AttributeChannel>,
    /// Dimmers held at full while this item is lit through its colour
    /// (a cell of a fixture with one master dimmer).
    pub hold_full: Vec<AttributeChannel>,
    /// Where colour goes: the fixture's colour channels, or each cell's.
    pub color_sets: Vec<HashMap<String, AttributeChannel>>,
    /// Everything else: pan, tilt, strobe, extra attributes.
    pub attributes: HashMap<String, AttributeChannel>,
    pub invert_pan: bool,
    pub invert_tilt: bool,
    pub swap_pan_tilt: bool,
}

/// One item's values for this tick; `None` where the input isn't connected.
#[derive(Debug, Default)]
pub struct ItemValues<'a> {
    pub intensity: Option<f32>,
    pub color: Option<Val>,
    pub position: Option<[f32; 2]>,
    pub strobe: Option<f32>,
    pub extras: Vec<(&'a str, f32)>,
}

/// Writes a 0-1 value to a channel (both bytes of a 16-bit one).
fn set(out: &mut HashMap<ChannelKey, f32>, universe: u32, ch: &AttributeChannel, value: f32) {
    let v = value.clamp(0.0, 1.0);
    match ch.fine {
        Some(fine) => {
            let wide = (v * 65535.0).round() as u32;
            out.insert((universe, ch.coarse), (wide >> 8) as f32);
            out.insert((universe, fine), (wide & 0xff) as f32);
        }
        None => {
            out.insert((universe, ch.coarse), v * 255.0);
        }
    }
}

/// Writes a raw DMX value (0-255) to a channel's coarse byte.
fn set_raw(out: &mut HashMap<ChannelKey, f32>, universe: u32, ch: &AttributeChannel, dmx: u8) {
    out.insert((universe, ch.coarse), dmx as f32);
}

pub fn parse_hex(text: &str) -> Option<Val> {
    let hex = text.trim().trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok().map(|b| b as f32 / 255.0);
    Some([byte(0)?, byte(2)?, byte(4)?])
}

pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> Val {
    let h = h.rem_euclid(1.0) * 6.0;
    let (s, v) = (s.clamp(0.0, 1.0), v.clamp(0.0, 1.0));
    let c = v * s;
    let x = c * (1.0 - ((h % 2.0) - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    [r + m, g + m, b + m]
}

/// The colour-wheel slot nearest `rgb`, as the DMX value in the middle of
/// its range.
fn nearest_wheel_slot(capabilities: &[ChannelCapability], rgb: Val) -> Option<u8> {
    capabilities
        .iter()
        .filter_map(|cap| Some((cap, parse_hex(cap.color.as_deref()?)?)))
        .min_by(|(_, a), (_, b)| {
            let dist = |c: &Val| (0..3).map(|i| (c[i] - rgb[i]).powi(2)).sum::<f32>();
            dist(a).total_cmp(&dist(b))
        })
        .map(|(cap, _)| ((cap.dmx_range[0] as u16 + cap.dmx_range[1] as u16) / 2) as u8)
}

fn write_color(out: &mut HashMap<ChannelKey, f32>, universe: u32, set_channels: &HashMap<String, AttributeChannel>, rgb: Val) {
    let [r, g, b] = rgb.map(|c| c.clamp(0.0, 1.0));
    let ch = |name: &str| set_channels.get(name);
    if ch("red").is_some() || ch("green").is_some() || ch("blue").is_some() {
        // RGBW: the white LED takes the part all three share.
        let w = if ch("white").is_some() { r.min(g).min(b) } else { 0.0 };
        for (name, v) in [("red", r - w), ("green", g - w), ("blue", b - w), ("white", w), ("amber", 0.0), ("uv", 0.0)] {
            if let Some(c) = ch(name) {
                set(out, universe, c, v);
            }
        }
    } else if ch("cyan").is_some() || ch("magenta").is_some() || ch("yellow").is_some() {
        for (name, v) in [("cyan", 1.0 - r), ("magenta", 1.0 - g), ("yellow", 1.0 - b)] {
            if let Some(c) = ch(name) {
                set(out, universe, c, v);
            }
        }
    } else if let Some(wheel) = ch("colorwheel") {
        if let Some(dmx) = nearest_wheel_slot(&wheel.capabilities, rgb) {
            set_raw(out, universe, wheel, dmx);
        }
    } else if let Some(white) = ch("white") {
        set(out, universe, white, r.max(g).max(b));
    }
}

/// Strobe 0 is "no strobe" (the channel's open/no-function range); above
/// that it runs through the strobe range, slow to fast.
fn write_strobe(out: &mut HashMap<ChannelKey, f32>, universe: u32, ch: &AttributeChannel, v: f32) {
    let strobe = ch.capabilities.iter().find(|c| c.attribute == "strobe");
    match strobe {
        None => set(out, universe, ch, v),
        Some(range) if v > 0.001 => {
            let [lo, hi] = range.dmx_range.map(|b| b as f32);
            out.insert((universe, ch.coarse), (lo + v.clamp(0.0, 1.0) * (hi - lo)).round());
        }
        Some(_) => {
            let open = ch.capabilities.iter().find(|c| c.attribute != "strobe").map_or(0, |c| c.dmx_range[0]);
            set_raw(out, universe, ch, open);
        }
    }
}

pub fn write_item(item: &TargetItem, values: &ItemValues, out: &mut HashMap<ChannelKey, f32>) {
    let u = item.universe;
    let virtual_dimmer = item.dimmers.is_empty();
    if let Some(intensity) = values.intensity {
        for d in &item.dimmers {
            set(out, u, d, intensity);
        }
    }
    if values.intensity.is_some() || values.color.is_some() {
        for d in &item.hold_full {
            set(out, u, d, 1.0);
        }
    }
    // A fixture without a dimmer shows intensity through its colour: white
    // when no colour is connected.
    let color = match (values.color, values.intensity) {
        (Some(c), Some(i)) if virtual_dimmer => Some(c.map(|x| x * i)),
        (None, Some(i)) if virtual_dimmer => Some([i, i, i]),
        (c, _) => c,
    };
    if let Some(rgb) = color {
        for set_channels in &item.color_sets {
            write_color(out, u, set_channels, rgb);
        }
    }
    if let Some([pan, tilt]) = values.position {
        let (pan, tilt) = if item.swap_pan_tilt { (tilt, pan) } else { (pan, tilt) };
        let pan = if item.invert_pan { 1.0 - pan } else { pan };
        let tilt = if item.invert_tilt { 1.0 - tilt } else { tilt };
        if let Some(c) = item.attributes.get("pan") {
            set(out, u, c, pan);
        }
        if let Some(c) = item.attributes.get("tilt") {
            set(out, u, c, tilt);
        }
    }
    if let (Some(v), Some(c)) = (values.strobe, item.attributes.get("strobe")) {
        write_strobe(out, u, c, v);
    }
    // Extra attributes last, so e.g. an explicit "white" input overrides
    // what colour mixing put there.
    for (name, v) in &values.extras {
        if let Some(c) = item.attributes.get(*name) {
            set(out, u, c, *v);
        } else {
            for set_channels in &item.color_sets {
                if let Some(c) = set_channels.get(*name) {
                    set(out, u, c, *v);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ch(coarse: u16) -> AttributeChannel {
        AttributeChannel { coarse, fine: None, capabilities: Vec::new() }
    }

    fn item(colors: &[(&str, u16)], dimmer: Option<u16>) -> TargetItem {
        TargetItem {
            fixture_number: 1,
            cell: None,
            universe: 1,
            dimmers: dimmer.map(ch).into_iter().collect(),
            hold_full: Vec::new(),
            color_sets: vec![colors.iter().map(|(n, c)| (n.to_string(), ch(*c))).collect()],
            attributes: HashMap::new(),
            invert_pan: false,
            invert_tilt: false,
            swap_pan_tilt: false,
        }
    }

    fn write(item: &TargetItem, values: ItemValues) -> HashMap<ChannelKey, f32> {
        let mut out = HashMap::new();
        write_item(item, &values, &mut out);
        out
    }

    #[test]
    fn rgbw_takes_the_shared_part_on_white() {
        let target = item(&[("red", 2), ("green", 3), ("blue", 4), ("white", 5)], Some(1));
        let out = write(&target, ItemValues { intensity: Some(1.0), color: Some([1.0, 0.5, 0.5]), ..Default::default() });
        assert_eq!(out[&(1, 1)], 255.0);
        assert_eq!(out[&(1, 2)].round(), 128.0);
        assert_eq!(out[&(1, 3)], 0.0);
        assert_eq!(out[&(1, 5)].round(), 128.0);
    }

    #[test]
    fn without_a_dimmer_intensity_scales_the_colour_or_lights_white() {
        let target = item(&[("red", 1), ("green", 2), ("blue", 3)], None);
        let out = write(&target, ItemValues { intensity: Some(0.5), color: Some([1.0, 0.0, 0.0]), ..Default::default() });
        assert_eq!(out[&(1, 1)].round(), 128.0);
        let out = write(&target, ItemValues { intensity: Some(1.0), ..Default::default() });
        assert_eq!([out[&(1, 1)], out[&(1, 2)], out[&(1, 3)]], [255.0; 3]);
    }

    #[test]
    fn cmy_subtracts_and_wheels_pick_the_nearest_slot() {
        let target = item(&[("cyan", 1), ("magenta", 2), ("yellow", 3)], None);
        let out = write(&target, ItemValues { color: Some([1.0, 0.0, 1.0]), ..Default::default() });
        assert_eq!([out[&(1, 1)], out[&(1, 2)], out[&(1, 3)]], [0.0, 255.0, 0.0]);

        let cap = |lo, hi, color: &str| ChannelCapability { dmx_range: [lo, hi], attribute: "colorwheel".into(), label: None, color: Some(color.into()) };
        let mut wheel = item(&[], None);
        wheel.color_sets[0].insert("colorwheel".into(), AttributeChannel { coarse: 9, fine: None, capabilities: vec![cap(0, 9, "#ffffff"), cap(10, 19, "#ff0000"), cap(20, 29, "#0000ff")] });
        let out = write(&wheel, ItemValues { color: Some([0.9, 0.1, 0.0]), ..Default::default() });
        assert_eq!(out[&(1, 9)], 14.0);
    }

    #[test]
    fn position_is_16_bit_with_invert_and_swap() {
        let mut target = item(&[], Some(1));
        target.attributes.insert("pan".into(), AttributeChannel { coarse: 10, fine: Some(11), capabilities: Vec::new() });
        target.attributes.insert("tilt".into(), ch(12));
        target.invert_tilt = true;
        let out = write(&target, ItemValues { position: Some([0.5, 0.25]), ..Default::default() });
        assert_eq!((out[&(1, 10)], out[&(1, 11)]), (128.0, 0.0)); // 0.5 * 65535 = 32768 (rounded)
        assert_eq!(out[&(1, 12)].round(), 191.0);
        assert!(!out.contains_key(&(1, 1)), "unconnected intensity isn't written");

        target.swap_pan_tilt = true;
        let out = write(&target, ItemValues { position: Some([1.0, 0.0]), ..Default::default() });
        assert_eq!(out[&(1, 10)], 0.0);
        assert_eq!(out[&(1, 12)], 0.0);
    }

    #[test]
    fn strobe_uses_the_capability_ranges() {
        let mut target = item(&[], None);
        let cap = |lo, hi, attribute: &str| ChannelCapability { dmx_range: [lo, hi], attribute: attribute.into(), label: None, color: None };
        target.attributes.insert("strobe".into(), AttributeChannel { coarse: 7, fine: None, capabilities: vec![cap(0, 9, "nofunction"), cap(10, 250, "strobe")] });
        assert_eq!(write(&target, ItemValues { strobe: Some(0.0), ..Default::default() })[&(1, 7)], 0.0);
        assert_eq!(write(&target, ItemValues { strobe: Some(0.5), ..Default::default() })[&(1, 7)], 130.0);
    }

    #[test]
    fn hex_and_hsv() {
        assert_eq!(parse_hex("#ff8000"), Some([1.0, 128.0 / 255.0, 0.0]));
        assert_eq!(parse_hex("nope"), None);
        let blue = hsv_to_rgb(2.0 / 3.0, 1.0, 1.0);
        assert!((blue[2] - 1.0).abs() < 1e-5 && blue[0] < 1e-5);
    }
}
