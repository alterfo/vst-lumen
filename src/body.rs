use std::f32::consts::TAU;

const MODES: usize = 10;
const FREQS: [f32; MODES] = [95.0, 190.0, 255.0, 380.0, 480.0, 610.0, 820.0, 1150.0, 1700.0, 2400.0];
const WEIGHTS: [f32; MODES] = [1.0, 0.8, 0.7, 0.6, 0.55, 0.45, 0.4, 0.3, 0.22, 0.15];
const RIGHT_DETUNE: f32 = 1.013;

pub struct BodyFrame {
    pub mix: f32,
    pub size: f32,
    pub decay: f32,
    pub damping: f32,
}

#[derive(Clone, Copy)]
struct Bank {
    y1: [f32; MODES],
    y2: [f32; MODES],
    a1: [f32; MODES],
    a2: [f32; MODES],
    g: [f32; MODES],
}

impl Bank {
    const fn new() -> Self {
        Self {
            y1: [0.0; MODES],
            y2: [0.0; MODES],
            a1: [0.0; MODES],
            a2: [0.0; MODES],
            g: [0.0; MODES],
        }
    }

    fn update(&mut self, f: &BodyFrame, detune: f32, sr: f32) {
        for k in 0..MODES {
            let freq = FREQS[k] / f.size.max(0.1) * detune;
            if freq >= 0.45 * sr {
                self.g[k] = 0.0;
                self.a1[k] = 0.0;
                self.a2[k] = 0.0;
                continue;
            }
            let ratio = FREQS[k] / FREQS[0];
            let t60 = f.decay.max(0.02) / (1.0 + f.damping.clamp(0.0, 1.0) * (ratio - 1.0) * 0.35);
            let r = (-6.9078 / (t60 * sr)).exp();
            let w = TAU * freq / sr;
            self.a1[k] = 2.0 * r * w.cos();
            self.a2[k] = -r * r;
            self.g[k] = WEIGHTS[k] * w.sin() * (2.0 * (1.0 - r * r)).sqrt();
        }
    }

    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let mut out = 0.0;
        for k in 0..MODES {
            let y = self.g[k] * x + self.a1[k] * self.y1[k] + self.a2[k] * self.y2[k];
            self.y2[k] = self.y1[k];
            self.y1[k] = y;
            out += y;
        }
        out
    }
}

pub struct Body {
    sr: f32,
    left: Bank,
    right: Bank,
}

impl Body {
    pub fn new(sr: f32) -> Self {
        Self {
            sr,
            left: Bank::new(),
            right: Bank::new(),
        }
    }

    pub fn process(&mut self, l: &mut [f32], r: &mut [f32], f: &BodyFrame) {
        if f.mix < 1e-3 {
            return;
        }
        self.left.update(f, 1.0, self.sr);
        self.right.update(f, RIGHT_DETUNE, self.sr);
        let wet = f.mix * 0.35;
        for i in 0..l.len() {
            let wl = self.left.process(l[i]);
            let wr = self.right.process(r[i]);
            l[i] += wl * wet;
            r[i] += wr * wet;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_noise_without_blowing_up() {
        let mut body = Body::new(48000.0);
        let f = BodyFrame {
            mix: 1.0,
            size: 1.0,
            decay: 0.5,
            damping: 0.4,
        };
        let mut seed = 1u32;
        let mut peak = 0.0f32;
        let mut in_pow = 0.0f64;
        let mut out_pow = 0.0f64;
        for _ in 0..3000 {
            let mut l = [0.0f32; 32];
            let mut r = [0.0f32; 32];
            for i in 0..32 {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                l[i] = ((seed >> 8) as f32 / 8388608.0 - 1.0) * 0.1;
                r[i] = l[i];
            }
            let dry = l;
            body.process(&mut l, &mut r, &f);
            for i in 0..32 {
                assert!(l[i].is_finite());
                peak = peak.max(l[i].abs());
                in_pow += (dry[i] * dry[i]) as f64;
                out_pow += ((l[i] - dry[i]) * (l[i] - dry[i])) as f64;
            }
        }
        let ratio = (out_pow / in_pow).sqrt();
        assert!(peak < 1.0, "peak {peak}");
        assert!(ratio > 0.1 && ratio < 2.0, "wet/dry rms {ratio}");
    }
}
