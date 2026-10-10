//! GO debounce: ignores a GO that arrives too soon after the previous one on
//! the same playback (a double-tap on a flaky button or a bouncing MIDI pad).

use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct GoDebounce {
    last: HashMap<u32, Instant>,
}

impl GoDebounce {
    /// True if a GO on `playback` at `now` should run. `window_ms == 0`
    /// disables debouncing. An accepted GO restarts the window; a rejected
    /// one does not, so holding a key down can't starve real presses.
    pub fn allow(&mut self, playback: u32, now: Instant, window_ms: u32) -> bool {
        if window_ms == 0 {
            return true;
        }
        let window = Duration::from_millis(window_ms as u64);
        match self.last.get(&playback) {
            Some(previous) if now.saturating_duration_since(*previous) < window => false,
            _ => {
                self.last.insert(playback, now);
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rapid_repeats_are_dropped_per_playback() {
        let mut guard = GoDebounce::default();
        let t0 = Instant::now();
        assert!(guard.allow(1, t0, 200));
        assert!(!guard.allow(1, t0 + Duration::from_millis(50), 200));
        assert!(guard.allow(2, t0 + Duration::from_millis(50), 200));
        assert!(guard.allow(1, t0 + Duration::from_millis(250), 200));
    }

    #[test]
    fn zero_window_never_blocks() {
        let mut guard = GoDebounce::default();
        let t0 = Instant::now();
        assert!(guard.allow(1, t0, 0));
        assert!(guard.allow(1, t0, 0));
    }

    #[test]
    fn a_rejected_go_does_not_extend_the_window() {
        let mut guard = GoDebounce::default();
        let t0 = Instant::now();
        assert!(guard.allow(1, t0, 200));
        assert!(!guard.allow(1, t0 + Duration::from_millis(150), 200));
        assert!(guard.allow(1, t0 + Duration::from_millis(210), 200));
    }
}
