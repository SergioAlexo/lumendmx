//! Audio analysis for the tempo and the graph's Audio node: a short-time
//! FFT feeds (a) low/mid/high band levels and (b) a spectral-flux onset
//! signal, from which a BPM is tracked by autocorrelation.
//!
//! Pure DSP with no I/O, so it's tested against synthesized audio. The cpal
//! capture thread (`audio_input.rs`) just calls `push`.

use std::f32::consts::PI;

use crate::engine::audio::BandLevels;

const FFT_SIZE: usize = 1024;
const HOP: usize = 256;
/// Seconds of onset history the tempo estimate looks at.
const HISTORY_SECS: f32 = 8.0;
/// Don't estimate until this much history exists.
const MIN_HISTORY_SECS: f32 = 4.0;
/// How often the tempo estimate is refreshed.
const ESTIMATE_EVERY_SECS: f32 = 0.5;
/// The BPM range a beat is searched in. Estimates outside it fold by octave.
const BPM_MIN: f32 = 70.0;
const BPM_MAX: f32 = 180.0;
/// Estimates this close (BPM) over the last few refreshes count as locked.
const LOCK_SPREAD_BPM: f32 = 1.5;
const LOCK_COUNT: usize = 4;

const LOW_HZ: (f32, f32) = (30.0, 250.0);
const MID_HZ: (f32, f32) = (250.0, 2500.0);
const HIGH_HZ: (f32, f32) = (2500.0, 12000.0);

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TempoEstimate {
    pub bpm: Option<f32>,
    /// The last few estimates agree: safe to drive the show tempo.
    pub locked: bool,
    /// 0.0-1.0: how periodic the onsets are.
    pub confidence: f32,
}

pub struct Analyzer {
    sample_rate: f32,
    window: Vec<f32>,
    twiddles: Vec<(f32, f32)>,
    /// The most recent `FFT_SIZE` samples (ring, written at `write`).
    ring: Vec<f32>,
    write: usize,
    since_hop: usize,
    prev_mag: Vec<f32>,
    flux: Vec<f32>,
    flux_cap: usize,
    frames_since_estimate: usize,
    recent: Vec<f32>,
    estimate: TempoEstimate,
    levels: BandLevels,
    /// Slowly decaying peak per band, so levels read 0-1 whatever the input
    /// gain is.
    peaks: [f32; 3],
    rms_peak: f32,
    band_bins: [(usize, usize); 3],
}

impl Analyzer {
    pub fn new(sample_rate: f32) -> Self {
        let window = (0..FFT_SIZE).map(|i| 0.5 - 0.5 * (2.0 * PI * i as f32 / FFT_SIZE as f32).cos()).collect();
        let twiddles =
            (0..FFT_SIZE / 2).map(|k| { let a = -2.0 * PI * k as f32 / FFT_SIZE as f32; (a.cos(), a.sin()) }).collect();
        let bin = |hz: f32| ((hz * FFT_SIZE as f32 / sample_rate) as usize).clamp(1, FFT_SIZE / 2 - 1);
        let band_bins = [LOW_HZ, MID_HZ, HIGH_HZ].map(|(a, b)| (bin(a), bin(b).max(bin(a) + 1)));
        Self {
            sample_rate,
            window,
            twiddles,
            ring: vec![0.0; FFT_SIZE],
            write: 0,
            since_hop: 0,
            prev_mag: vec![0.0; FFT_SIZE / 2],
            flux: Vec::new(),
            flux_cap: (HISTORY_SECS * sample_rate / HOP as f32) as usize,
            frames_since_estimate: 0,
            recent: Vec::new(),
            estimate: TempoEstimate::default(),
            levels: BandLevels::default(),
            peaks: [1e-4; 3],
            rms_peak: 1e-4,
            band_bins,
        }
    }

    fn frames_per_sec(&self) -> f32 {
        self.sample_rate / HOP as f32
    }

    pub fn levels(&self) -> BandLevels {
        self.levels
    }

    pub fn tempo(&self) -> TempoEstimate {
        self.estimate
    }

    pub fn push(&mut self, samples: &[f32]) {
        for &s in samples {
            self.ring[self.write] = s;
            self.write = (self.write + 1) % FFT_SIZE;
            self.since_hop += 1;
            if self.since_hop == HOP {
                self.since_hop = 0;
                self.process_frame();
            }
        }
    }

    fn process_frame(&mut self) {
        let mut re: Vec<f32> = (0..FFT_SIZE).map(|i| self.ring[(self.write + i) % FFT_SIZE] * self.window[i]).collect();
        let mut im = vec![0.0f32; FFT_SIZE];
        fft(&mut re, &mut im, &self.twiddles);
        let mag: Vec<f32> = (0..FFT_SIZE / 2).map(|k| (re[k] * re[k] + im[k] * im[k]).sqrt() / FFT_SIZE as f32).collect();

        // Band levels: RMS magnitude in each band, normalized by a decaying
        // peak (about 10s half-life at the hop rate).
        let decay = 0.5f32.powf(1.0 / (10.0 * self.frames_per_sec()));
        let mut raw = [0.0f32; 3];
        for (i, &(lo, hi)) in self.band_bins.iter().enumerate() {
            let energy: f32 = mag[lo..hi].iter().map(|m| m * m).sum::<f32>() / (hi - lo) as f32;
            raw[i] = energy.sqrt();
            self.peaks[i] = (self.peaks[i] * decay).max(raw[i]).max(1e-4);
        }
        let total: f32 = (mag.iter().map(|m| m * m).sum::<f32>() / mag.len() as f32).sqrt();
        self.rms_peak = (self.rms_peak * decay).max(total).max(1e-4);
        // Below an absolute floor the input is silence, however quiet the
        // decayed peak has become.
        let floor = 2e-5;
        let level = |v: f32, peak: f32| if v < floor { 0.0 } else { (v / peak).clamp(0.0, 1.0) };
        self.levels = BandLevels {
            low: level(raw[0], self.peaks[0]),
            mid: level(raw[1], self.peaks[1]),
            high: level(raw[2], self.peaks[2]),
            rms: level(total, self.rms_peak),
            loudness: loudness(&self.ring),
        };

        // Onset strength: positive change in log-compressed magnitude.
        let mut flux = 0.0;
        for (prev, &m) in self.prev_mag.iter_mut().zip(&mag).skip(1) {
            flux += ((1.0 + 1000.0 * m).ln() - (1.0 + 1000.0 * *prev).ln()).max(0.0);
            *prev = m;
        }
        self.flux.push(flux);
        if self.flux.len() > self.flux_cap {
            self.flux.remove(0);
        }

        self.frames_since_estimate += 1;
        if self.frames_since_estimate as f32 >= ESTIMATE_EVERY_SECS * self.frames_per_sec() {
            self.frames_since_estimate = 0;
            self.refresh_tempo();
        }
    }

    fn refresh_tempo(&mut self) {
        let fps = self.frames_per_sec();
        if (self.flux.len() as f32) < MIN_HISTORY_SECS * fps {
            return;
        }
        // Remove the slow baseline and keep only the peaks.
        let mean = self.flux.iter().sum::<f32>() / self.flux.len() as f32;
        let x: Vec<f32> = self.flux.iter().map(|v| (v - mean).max(0.0)).collect();
        let n = x.len();

        let lag_lo = (fps * 60.0 / BPM_MAX) as usize;
        let lag_hi = (fps * 60.0 / BPM_MIN).ceil() as usize;
        let max_lag = (lag_hi * 4 + 2).min(n / 2);
        if lag_lo < 2 || lag_hi + 2 >= max_lag {
            return;
        }
        let energy: f32 = x.iter().map(|v| v * v).sum();
        if energy <= 1e-6 {
            self.estimate = TempoEstimate::default();
            self.recent.clear();
            return;
        }
        let ac: Vec<f32> = (0..=max_lag)
            .map(|l| x[..n - l].iter().zip(&x[l..]).map(|(a, b)| a * b).sum::<f32>() / (n - l) as f32)
            .collect();
        let ac0 = ac[0].max(1e-9);

        // Coarse beat period: strongest autocorrelation peak in range, with
        // a mild preference for tempos near 120 BPM to settle octave ties.
        let prior = |lag: usize| {
            let bpm = 60.0 * fps / lag as f32;
            let octaves = (bpm / 120.0).log2();
            (-0.5 * octaves * octaves / 0.8).exp() * 0.25 + 0.75
        };
        let (mut best_lag, mut best_score) = (0usize, 0.0f32);
        for lag in lag_lo..=lag_hi {
            if ac[lag] >= ac[lag - 1] && ac[lag] >= ac[lag + 1] {
                let score = ac[lag] * prior(lag);
                if score > best_score {
                    (best_lag, best_score) = (lag, score);
                }
            }
        }
        if best_lag == 0 {
            return;
        }

        // Refine on the multiples of the period: the error of a peak
        // position is the same whatever the lag, so k periods away it's k
        // times smaller relative to the beat.
        let mut period = interpolated_peak(&ac, best_lag, 1);
        for k in 2..=4usize {
            let centre = (period * k as f32).round() as usize;
            let radius = (best_lag as f32 * 0.04 * k as f32).ceil() as usize + 1;
            if centre + radius >= ac.len() {
                break;
            }
            let (lo, hi) = (centre - radius, centre + radius);
            let peak = (lo..=hi).max_by(|&a, &b| ac[a].partial_cmp(&ac[b]).unwrap()).unwrap();
            if ac[peak] < 0.3 * ac[best_lag] || peak == lo || peak == hi {
                break;
            }
            period = interpolated_peak(&ac, peak, k);
        }
        let bpm = 60.0 * fps / period;

        self.recent.push(bpm);
        if self.recent.len() > LOCK_COUNT {
            self.recent.remove(0);
        }
        let spread = self.recent.iter().cloned().fold(f32::MIN, f32::max) - self.recent.iter().cloned().fold(f32::MAX, f32::min);
        let locked = self.recent.len() == LOCK_COUNT && spread <= LOCK_SPREAD_BPM;
        let mut sorted = self.recent.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        self.estimate = TempoEstimate {
            bpm: Some(sorted[sorted.len() / 2]),
            locked,
            confidence: (ac[best_lag] / ac0).clamp(0.0, 1.0),
        };
    }
}

/// The period (in frames per beat) of the autocorrelation peak at `peak`,
/// refined by a parabola through its neighbours and divided by `k` (the
/// number of beats it spans).
/// Absolute loudness of a block of samples: its RMS in dBFS, mapped
/// -60 dB → 0.0 and 0 dB (full scale) → 1.0.
fn loudness(samples: &[f32]) -> f32 {
    let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len().max(1) as f32).sqrt();
    let db = 20.0 * rms.max(1e-9).log10();
    ((db + 60.0) / 60.0).clamp(0.0, 1.0)
}

fn interpolated_peak(ac: &[f32], peak: usize, k: usize) -> f32 {
    let (a, b, c) = (ac[peak - 1], ac[peak], ac[peak + 1]);
    let denom = a - 2.0 * b + c;
    let offset = if denom.abs() < 1e-12 { 0.0 } else { (0.5 * (a - c) / denom).clamp(-1.0, 1.0) };
    (peak as f32 + offset) / k as f32
}

/// In-place iterative radix-2 FFT. `twiddles[k] = e^(-2πik/N)`.
fn fft(re: &mut [f32], im: &mut [f32], twiddles: &[(f32, f32)]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let step = n / len;
        for start in (0..n).step_by(len) {
            for k in 0..len / 2 {
                let (wr, wi) = twiddles[k * step];
                let (a, b) = (start + k, start + k + len / 2);
                let tr = re[b] * wr - im[b] * wi;
                let ti = re[b] * wi + im[b] * wr;
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
            }
        }
        len <<= 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 44_100.0;

    /// A tiny deterministic noise source (no `rand` dependency).
    fn noise(state: &mut u32) -> f32 {
        *state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        (*state >> 8) as f32 / (1u32 << 23) as f32 - 1.0
    }

    /// A click track: a 12ms decaying noise burst on every beat, plus an
    /// optional weaker one on the off-beat.
    fn click_track(bpm: f32, secs: f32, offbeats: bool) -> Vec<f32> {
        let mut out = vec![0.0f32; (secs * SR) as usize];
        let mut rng = 12345u32;
        let beat = 60.0 / bpm;
        let mut i = 0;
        loop {
            for (offset, gain) in [(0.0, 0.8), (beat / 2.0, if offbeats { 0.3 } else { 0.0 })] {
                if gain == 0.0 {
                    continue;
                }
                let start = ((i as f32 * beat + offset) * SR) as usize;
                for n in 0..(0.012 * SR) as usize {
                    if let Some(s) = out.get_mut(start + n) {
                        *s += noise(&mut rng) * gain * (-(n as f32) / (0.003 * SR)).exp();
                    }
                }
            }
            i += 1;
            if i as f32 * beat > secs {
                break;
            }
        }
        out
    }

    fn detect(samples: &[f32]) -> TempoEstimate {
        let mut analyzer = Analyzer::new(SR);
        // In small chunks, like a device callback.
        for chunk in samples.chunks(480) {
            analyzer.push(chunk);
        }
        analyzer.tempo()
    }

    #[test]
    fn locks_to_click_tracks_within_one_bpm() {
        for bpm in [90.0, 100.0, 120.0, 128.0, 140.0, 150.0] {
            let est = detect(&click_track(bpm, 14.0, false));
            let got = est.bpm.unwrap_or(0.0);
            assert!((got - bpm).abs() <= 1.0, "{bpm} BPM detected as {got}");
            assert!(est.locked, "{bpm} BPM: not locked");
        }
    }

    #[test]
    fn locks_through_off_beat_hats_and_a_bass_hum() {
        let mut track = click_track(124.0, 14.0, true);
        for (i, s) in track.iter_mut().enumerate() {
            *s += 0.05 * (2.0 * PI * 55.0 * i as f32 / SR).sin();
        }
        let est = detect(&track);
        assert!((est.bpm.unwrap() - 124.0).abs() <= 1.0, "{:?}", est);
    }

    #[test]
    fn silence_and_steady_tones_do_not_produce_a_tempo() {
        assert_eq!(detect(&vec![0.0; (10.0 * SR) as usize]).bpm, None);
        let tone: Vec<f32> = (0..(10.0 * SR) as usize).map(|i| 0.3 * (2.0 * PI * 440.0 * i as f32 / SR).sin()).collect();
        assert!(!detect(&tone).locked);
    }

    #[test]
    fn band_levels_separate_bass_from_treble() {
        let mut analyzer = Analyzer::new(SR);
        let bass: Vec<f32> = (0..8192).map(|i| 0.5 * (2.0 * PI * 80.0 * i as f32 / SR).sin()).collect();
        analyzer.push(&bass);
        let l = analyzer.levels();
        assert!(l.low > 0.5 && l.high < 0.1, "{l:?}");

        let mut analyzer = Analyzer::new(SR);
        let treble: Vec<f32> = (0..8192).map(|i| 0.5 * (2.0 * PI * 6000.0 * i as f32 / SR).sin()).collect();
        analyzer.push(&treble);
        let l = analyzer.levels();
        assert!(l.high > 0.5 && l.low < 0.1, "{l:?}");
    }

    #[test]
    fn loudness_is_absolute_not_normalized_to_recent_peaks() {
        // Quiet room tone stays low however long it runs (the relative
        // bands creep up to ~1.0 on it as their peak decays).
        let mut analyzer = Analyzer::new(SR);
        let hiss: Vec<f32> = (0..(12.0 * SR) as usize).map(|i| 0.001 * (2.0 * PI * 1000.0 * i as f32 / SR).sin()).collect();
        analyzer.push(&hiss);
        assert!(analyzer.levels().loudness < 0.1, "{:?}", analyzer.levels());

        // A loud tone (about -9 dBFS) reads high straight away.
        let loud: Vec<f32> = (0..8192).map(|i| 0.5 * (2.0 * PI * 1000.0 * i as f32 / SR).sin()).collect();
        analyzer.push(&loud);
        let l = analyzer.levels().loudness;
        assert!((l - 0.85).abs() < 0.02, "{l}");
        assert_eq!(loudness(&[0.0; 64]), 0.0);
    }

    #[test]
    fn fft_finds_a_pure_tone_in_the_right_bin() {
        let analyzer = Analyzer::new(SR);
        let k = 40;
        let mut re: Vec<f32> = (0..FFT_SIZE).map(|i| (2.0 * PI * k as f32 * i as f32 / FFT_SIZE as f32).cos()).collect();
        let mut im = vec![0.0; FFT_SIZE];
        fft(&mut re, &mut im, &analyzer.twiddles);
        let peak = (0..FFT_SIZE / 2).max_by(|&a, &b| (re[a].hypot(im[a])).partial_cmp(&re[b].hypot(im[b])).unwrap()).unwrap();
        assert_eq!(peak, k);
    }
}
