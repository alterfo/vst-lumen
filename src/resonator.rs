use std::f32::consts::{PI, TAU};

pub const MODES: usize = 12;
const KS_LEN: usize = 4096;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Off,
    Pluck,
    String,
    Membrane,
    Plate,
}

#[derive(Clone, Copy)]
pub struct ResFrame {
    pub kind: Kind,
    pub mix: f32,
    pub decay: f32,
    pub damping: f32,
    pub stiffness: f32,
    pub strike: f32,
    pub hardness: f32,
    pub position: f32,
}

const MEMBRANE_RATIOS: [f32; MODES] = [
    1.000, 1.593, 2.136, 2.296, 2.653, 2.918, 3.156, 3.501, 3.600, 3.652, 4.060, 4.231,
];
const MEMBRANE_WEIGHTS: [f32; MODES] = [
    1.0, 0.85, 0.75, 0.6, 0.55, 0.45, 0.4, 0.33, 0.3, 0.28, 0.22, 0.2,
];
const PLATE_RATIOS: [f32; MODES] = [
    1.0, 1.56, 2.31, 2.80, 3.66, 4.17, 5.12, 5.69, 6.45, 7.31, 8.18, 9.20,
];
const PLATE_WEIGHTS: [f32; MODES] = [
    1.0, 0.9, 0.8, 0.7, 0.6, 0.5, 0.45, 0.4, 0.35, 0.3, 0.25, 0.2,
];

#[derive(Clone, Copy)]
pub struct Resonator {
    y1: [f32; MODES],
    y2: [f32; MODES],
    a1: [f32; MODES],
    a2: [f32; MODES],
    gain: [f32; MODES],
    gain_burst: [f32; MODES],
    out_scale: f32,
    jitter: [f32; MODES],
    ks: [f32; KS_LEN],
    ks_pos: usize,
    ks_int: usize,
    ks_ap_a: f32,
    ks_ap_x: f32,
    ks_ap_y: f32,
    ks_lp: f32,
    ks_lp_b: f32,
    ks_gain: f32,
    burst: f32,
    burst_coef: f32,
    burst_lp: f32,
    hard_coef: f32,
    rng: u32,
}

impl Resonator {
    pub const fn new() -> Self {
        Self {
            y1: [0.0; MODES],
            y2: [0.0; MODES],
            a1: [0.0; MODES],
            a2: [0.0; MODES],
            gain: [0.0; MODES],
            gain_burst: [0.0; MODES],
            out_scale: 0.02,
            jitter: [0.0; MODES],
            ks: [0.0; KS_LEN],
            ks_pos: 0,
            ks_int: 100,
            ks_ap_a: 0.0,
            ks_ap_x: 0.0,
            ks_ap_y: 0.0,
            ks_lp: 0.0,
            ks_lp_b: 0.3,
            ks_gain: 0.99,
            burst: 0.0,
            burst_coef: 0.99,
            burst_lp: 0.0,
            hard_coef: 0.5,
            rng: 12345,
        }
    }

    #[inline]
    fn rand(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x >> 8) as f32 / 8388608.0 - 1.0
    }

    pub fn reset(&mut self) {
        let rng = self.rng;
        *self = Self::new();
        self.rng = rng;
    }

    pub fn trigger(&mut self, velocity: f32, seed: u32) {
        self.rng = seed | 1;
        self.burst = velocity;
        for k in 0..MODES {
            self.jitter[k] = self.rand();
        }
    }

    pub fn update(&mut self, f: &ResFrame, freq: f32, sr: f32) {
        let hard = f.hardness.clamp(0.0, 1.0);
        self.hard_coef = 0.04 + hard * hard * 0.96;
        match f.kind {
            Kind::Off => {}
            Kind::Pluck => self.update_pluck(f, freq, sr),
            _ => self.update_modal(f, freq, sr),
        }
    }

    fn update_pluck(&mut self, f: &ResFrame, freq: f32, sr: f32) {
        let b = 0.05 + f.damping.clamp(0.0, 1.0) * 0.6;
        self.ks_lp_b = b;
        let lp_delay = b / (1.0 - b);
        let total = (sr / freq.max(20.0) - lp_delay).clamp(4.0, (KS_LEN - 8) as f32);
        let int = (total - 0.5).floor().max(2.0);
        let d = total - int;
        self.ks_int = int as usize;
        self.ks_ap_a = (1.0 - d) / (1.0 + d);
        let period = 1.0 / freq.max(20.0);
        self.ks_gain = 10f32.powf(-3.0 * period / f.decay.max(0.05)).min(0.9995);
        self.burst_coef = (-1.0 / (sr * (0.4 * period).max(0.0005))).exp();
    }

    fn update_modal(&mut self, f: &ResFrame, freq: f32, sr: f32) {
        let stiff = f.stiffness.clamp(0.0, 1.0);
        let b_inharm = stiff * stiff * 0.02;
        let pick = 0.05 + f.position.clamp(0.0, 1.0) * 0.45;
        self.burst_coef = (-1.0 / (sr * 0.004)).exp();
        let nyquist = 0.45 * sr;
        let mut weight_sum = 0.0;
        for k in 0..MODES {
            let (ratio, weight) = match f.kind {
                Kind::String => {
                    let n = (k + 1) as f32;
                    let ratio = n * (1.0 + b_inharm * n * n).sqrt();
                    let comb = (n * PI * pick).sin().abs();
                    (ratio, n.powf(-0.9) * (0.25 + 0.75 * comb))
                }
                Kind::Membrane => (
                    MEMBRANE_RATIOS[k].powf(1.0 + stiff * 0.12),
                    MEMBRANE_WEIGHTS[k],
                ),
                _ => (PLATE_RATIOS[k].powf(1.0 + stiff * 0.12), PLATE_WEIGHTS[k]),
            };
            let fk = freq * ratio * (1.0 + self.jitter[k] * 0.003 * (0.3 + stiff));
            if fk >= nyquist || fk < 10.0 {
                self.gain[k] = 0.0;
                self.gain_burst[k] = 0.0;
                self.a1[k] = 0.0;
                self.a2[k] = 0.0;
                continue;
            }
            let t60 = f.decay.max(0.05) / (1.0 + f.damping.clamp(0.0, 1.0) * (ratio - 1.0) * 1.6);
            let r = (-6.9078 / (t60 * sr)).exp();
            let w = TAU * fk / sr;
            self.a1[k] = 2.0 * r * w.cos();
            self.a2[k] = -r * r;
            let peak = (1.0 - r) * (1.0 + r * r - 2.0 * r * (2.0 * w).cos()).max(1e-9).sqrt();
            self.gain[k] = weight * peak;
            self.gain_burst[k] = weight * w.sin();
            weight_sum += weight;
        }
        self.out_scale = 0.036 / weight_sum.max(0.1);
    }

    #[inline]
    pub fn process(&mut self, input: f32, f: &ResFrame) -> f32 {
        if f.kind == Kind::Off {
            return 0.0;
        }
        let noise = self.rand();
        self.burst_lp += (noise * self.burst - self.burst_lp) * self.hard_coef;
        self.burst *= self.burst_coef;
        let strike = f.strike.clamp(0.0, 1.0);
        let burst = self.burst_lp * strike * 6.0;
        let drive = input * (0.35 * (1.0 - 0.7 * strike));
        let exc = burst + drive;

        if f.kind == Kind::Pluck {
            let idx = (self.ks_pos + KS_LEN - self.ks_int) & (KS_LEN - 1);
            let delayed = self.ks[idx];
            let ap = self.ks_ap_a * delayed + self.ks_ap_x - self.ks_ap_a * self.ks_ap_y;
            self.ks_ap_x = delayed;
            self.ks_ap_y = ap;
            self.ks_lp = (1.0 - self.ks_lp_b) * ap + self.ks_lp_b * self.ks_lp;
            let fb = self.ks_lp * self.ks_gain;
            self.ks[self.ks_pos] = (exc + fb).clamp(-4.0, 4.0);
            self.ks_pos = (self.ks_pos + 1) & (KS_LEN - 1);
            return self.ks_lp * 0.4;
        }

        let mut out = 0.0;
        for k in 0..MODES {
            let y = self.gain[k] * drive + self.gain_burst[k] * burst + self.a1[k] * self.y1[k] + self.a2[k] * self.y2[k];
            self.y2[k] = self.y1[k];
            self.y1[k] = y;
            out += y;
        }
        out * self.out_scale
    }
}

impl Default for Resonator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(kind: Kind, decay: f32, damping: f32, stiffness: f32) -> ResFrame {
        ResFrame {
            kind,
            mix: 1.0,
            decay,
            damping,
            stiffness,
            strike: 1.0,
            hardness: 0.6,
            position: 0.3,
        }
    }

    fn render(kind: Kind, freq: f32, decay: f32, seconds: f32) -> Vec<f32> {
        let sr = 48000.0;
        let f = frame(kind, decay, 0.4, 0.2);
        let mut r = Resonator::new();
        r.trigger(1.0, 99);
        let mut out = Vec::new();
        for block in 0..(sr * seconds) as usize / 32 {
            let _ = block;
            r.update(&f, freq, sr);
            for _ in 0..32 {
                out.push(r.process(0.0, &f));
            }
        }
        out
    }

    fn estimate_freq(x: &[f32], sr: f32, lo: f32, hi: f32) -> f32 {
        let start = x.len() / 8;
        let seg = &x[start..start + 8192.min(x.len() - start)];
        let min_lag = (sr / hi) as usize;
        let max_lag = (sr / lo) as usize;
        let mut best = (0usize, f32::MIN);
        for lag in min_lag..max_lag {
            let c: f32 = seg.iter().zip(&seg[lag..]).map(|(a, b)| a * b).sum();
            if c > best.1 {
                best = (lag, c);
            }
        }
        let l = best.0 as f32;
        let c0: f32 = seg.iter().zip(&seg[best.0 - 1..]).map(|(a, b)| a * b).sum();
        let c2: f32 = seg.iter().zip(&seg[best.0 + 1..]).map(|(a, b)| a * b).sum();
        let denom = c0 - 2.0 * best.1 + c2;
        let shift = if denom.abs() > 1e-9 { 0.5 * (c0 - c2) / denom } else { 0.0 };
        sr / (l + shift)
    }

    #[test]
    fn pluck_is_in_tune() {
        for freq in [82.41f32, 220.0, 440.0, 880.0] {
            let out = render(Kind::Pluck, freq, 3.0, 1.2);
            let est = estimate_freq(&out, 48000.0, freq * 0.8, freq * 1.25);
            let cents = 1200.0 * (est / freq).log2();
            assert!(cents.abs() < 8.0, "{freq} Hz estimated {est} ({cents} ct)");
        }
    }

    #[test]
    fn all_models_stable_and_audible() {
        for kind in [Kind::Pluck, Kind::String, Kind::Membrane, Kind::Plate] {
            for (decay, damp, stiff) in [(0.05, 0.0, 0.0), (10.0, 1.0, 1.0), (10.0, 0.0, 0.0)] {
                let sr = 48000.0;
                let f = frame(kind, decay, damp, stiff);
                let mut r = Resonator::new();
                r.trigger(1.0, 7);
                let mut peak = 0.0f32;
                for _ in 0..(sr as usize * 4 / 32) {
                    r.update(&f, 110.0, sr);
                    for _ in 0..32 {
                        let y = r.process(0.0, &f);
                        assert!(y.is_finite());
                        peak = peak.max(y.abs());
                    }
                }
                assert!(peak > 0.1 && peak < 1.6, "peak {peak}");
            }
        }
    }

    #[test]
    fn decay_follows_t60() {
        let out = render(Kind::String, 220.0, 2.0, 3.0);
        let rms = |a: usize, b: usize| {
            (out[a..b].iter().map(|v| v * v).sum::<f32>() / (b - a) as f32).sqrt()
        };
        let early = rms(4800, 4800 + 4800);
        let late = rms(4800 + 96000, 4800 + 96000 + 4800);
        let db = 20.0 * (late / early).log10();
        assert!(db < -25.0 && db > -90.0, "decay over 2 s: {db} dB");
    }
}
