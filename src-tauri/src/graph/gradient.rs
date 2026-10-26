//! A Gradient node's colour stops: "#rrggbb" or "#rrggbb@0.25" (a place
//! along the gradient, 0-1), comma-separated. A stop without a place gets
//! its even share, so lists written without places still spread out.

use super::compile::Val;
use super::write::parse_hex;

/// Stops with their places, sorted by place.
pub fn parse_stops(text: &str, wrap: bool) -> Vec<(f32, Val)> {
    let tokens: Vec<(Option<f32>, Val)> = text
        .split(',')
        .filter_map(|token| {
            let (color, place) = match token.split_once('@') {
                Some((color, place)) => (color, place.trim().parse::<f32>().ok().filter(|p| p.is_finite()).map(|p| p.clamp(0.0, 1.0))),
                None => (token, None),
            };
            Some((place, parse_hex(color.trim())?))
        })
        .collect();
    let n = tokens.len();
    let mut stops: Vec<(f32, Val)> = tokens
        .into_iter()
        .enumerate()
        .map(|(i, (place, color))| {
            let even = match (wrap, n) {
                (_, 0 | 1) => 0.0,
                (true, _) => i as f32 / n as f32,
                (false, _) => i as f32 / (n - 1) as f32,
            };
            (place.unwrap_or(even), color)
        })
        .collect();
    stops.sort_by(|a, b| a.0.total_cmp(&b.0));
    stops
}

fn lerp(a: Val, b: Val, t: f32) -> Val {
    [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t)
}

/// The colour at `t`: between the stops either side of it. Wrapped, the
/// last stop blends round to the first.
pub fn color_at(stops: &[(f32, Val)], wrap: bool, t: f32) -> Val {
    let (Some(&first), Some(&last)) = (stops.first(), stops.last()) else {
        return [0.0; 3];
    };
    if stops.len() == 1 {
        return first.1;
    }
    let t = if wrap { t.rem_euclid(1.0) } else { t.clamp(0.0, 1.0) };
    if t >= first.0 && t < last.0 {
        let i = stops.iter().rposition(|s| s.0 <= t).unwrap_or(0).min(stops.len() - 2);
        let (a, b) = (stops[i], stops[i + 1]);
        let span = b.0 - a.0;
        return if span <= 0.0 { b.1 } else { lerp(a.1, b.1, (t - a.0) / span) };
    }
    if !wrap {
        return if t < first.0 { first.1 } else { last.1 };
    }
    // Past the last stop or before the first: the way round from last to first.
    let span = first.0 + 1.0 - last.0;
    let at = if t >= last.0 { t } else { t + 1.0 };
    if span <= 0.0 {
        first.1
    } else {
        lerp(last.1, first.1, (at - last.0) / span)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: Val = [1.0, 0.0, 0.0];
    const BLUE: Val = [0.0, 0.0, 1.0];

    #[test]
    fn stops_without_places_spread_evenly_and_places_win() {
        let even = parse_stops("#ff0000, #00ff00, #0000ff", false);
        assert_eq!(even.iter().map(|s| s.0).collect::<Vec<_>>(), vec![0.0, 0.5, 1.0]);
        let wrapped = parse_stops("#ff0000, #00ff00, #0000ff, #ffffff", true);
        assert_eq!(wrapped.iter().map(|s| s.0).collect::<Vec<_>>(), vec![0.0, 0.25, 0.5, 0.75]);
        let placed = parse_stops("#0000ff@0.9, junk, #ff0000@0.2", false);
        assert_eq!(placed, vec![(0.2, RED), (0.9, BLUE)]);
    }

    #[test]
    fn colours_blend_between_stops_and_hold_past_the_ends() {
        let stops = parse_stops("#ff0000@0.25, #0000ff@0.75", false);
        assert_eq!(color_at(&stops, false, 0.0), RED);
        assert_eq!(color_at(&stops, false, 1.0), BLUE);
        assert_eq!(color_at(&stops, false, 0.5), [0.5, 0.0, 0.5]);
    }

    #[test]
    fn wrapped_the_last_stop_blends_round_to_the_first() {
        let stops = parse_stops("#ff0000@0.25, #0000ff@0.75", true);
        assert_eq!(color_at(&stops, true, 0.75), BLUE);
        assert_eq!(color_at(&stops, true, 0.25), RED);
        assert_eq!(color_at(&stops, true, 0.0), [0.5, 0.0, 0.5]); // halfway round, since 0 is midway from 0.75 to 1.25
        assert_eq!(color_at(&stops, true, 1.25), RED); // t wraps
    }

    #[test]
    fn evenly_spaced_stops_play_as_before() {
        let stops = parse_stops("#ff0000, #0000ff", true);
        assert_eq!(color_at(&stops, true, 0.0), RED);
        assert_eq!(color_at(&stops, true, 0.5), BLUE);
        assert_eq!(color_at(&stops, true, 0.75), [0.5, 0.0, 0.5]); // back round to red
    }
}
