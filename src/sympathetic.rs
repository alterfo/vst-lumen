use crate::filter::soft_clip;
use std::f32::consts::FRAC_PI_2;

pub const STRINGS: usize = 16;
const MAX_FREQ: f32 = 5000.0;
const MIN_FREQ: f32 = 30.0;
const SUB_MIN_FREQ: f32 = 18.0;
const SUB_STRINGS: usize = 8;
const SHIFT_WINDOW: usize = 2048;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Scale {
    Gong,
    Shang,
    Jue,
    Zhi,
    Yu,
    Minor,
    Dorian,
    Phrygian,
    Fifths,
}

impl Scale {
    pub fn degrees(self) -> &'static [u8] {
        match self {
            Scale::Gong => &[0, 2, 4, 7, 9],
            Scale::Shang => &[0, 2, 5, 7, 10],
            Scale::Jue => &[0, 3, 5, 8, 10],
            Scale::Zhi => &[0, 2, 5, 7, 9],
            Scale::Yu => &[0, 3, 5, 7, 10],
            Scale::Minor => &[0, 2, 3, 5, 7, 8, 10],
            Scale::Dorian => &[0, 2, 3, 5, 7, 9, 10],
            Scale::Phrygian => &[0, 1, 3, 5, 7, 8, 10],
            Scale::Fifths => &[0, 7],
        }
    }
}

pub struct SympFrame {
    pub mix: f32,
    pub root: i32,
    pub scale: Scale,
    pub register: i32,
    pub decay: f32,
    pub damping: f32,
    pub detune_cents: f32,
    pub drift_cents: f32,
    pub sub_mix: f32,
    pub sub_drop: i32,
}

struct Str {
    buf: Vec<f32>,
    mask: usize,
    wp: usize,
    delay: usize,
    ap_a: f32,
    ap_x: f32,
    ap_y: f32,
    lp: f32,
    lp_b: f32,
    gain: f32,
    scale: f32,
    active: bool,
    offset: f32,
    gl: f32,
    gr: f32,
    min_freq: f32,
}

impl Str {
    fn new(sr: f32, offset: f32, pan: f32, min_freq: f32) -> Self {
        let len = ((sr / min_freq) as usize + 16).next_power_of_two();
        Self {
            buf: vec![0.0; len],
            mask: len - 1,
            wp: 0,
            delay: 8,
            ap_a: 0.0,
            ap_x: 0.0,
            ap_y: 0.0,
            lp: 0.0,
            lp_b: 0.3,
            gain: 0.0,
            scale: 0.0,
            active: false,
            offset,
            gl: (pan * FRAC_PI_2).cos(),
            gr: (pan * FRAC_PI_2).sin(),
            min_freq,
        }
    }

    fn tune(&mut self, freq: f32, decay: f32, damping: f32, sr: f32) {
        if !(self.min_freq..=MAX_FREQ).contains(&freq) || freq >= 0.45 * sr {
            self.active = false;
            return;
        }
        self.active = true;
        let b = 0.04 + damping.clamp(0.0, 1.0) * 0.55;
        self.lp_b = b;
        let lp_delay = b / (1.0 - b);
        let max = (self.mask - 8) as f32;
        let total = (sr / freq - lp_delay).clamp(4.0, max);
        let int = (total - 0.5).floor().max(2.0);
        let d = total - int;
        self.delay = int as usize;
        self.ap_a = (1.0 - d) / (1.0 + d);
        let period = 1.0 / freq;
        let g = 10f32.powf(-3.0 * period / decay.max(0.05)).min(0.9998);
        self.gain = g;
        self.scale = (1.0 - g) * 2.5;
    }

    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let delayed = self.buf[self.wp.wrapping_sub(self.delay) & self.mask];
        let ap = self.ap_a * delayed + self.ap_x - self.ap_a * self.ap_y;
        self.ap_x = delayed;
        self.ap_y = ap;
        self.lp = (1.0 - self.lp_b) * ap + self.lp_b * self.lp;
        let y = self.lp * self.gain + x * self.scale;
        self.buf[self.wp] = y;
        self.wp = (self.wp + 1) & self.mask;
        y
    }
}

struct PitchDown {
    buf: Vec<f32>,
    mask: usize,
    wp: usize,
    phase: f32,
}

impl PitchDown {
    fn new() -> Self {
        let len = (SHIFT_WINDOW * 2).next_power_of_two();
        Self {
            buf: vec![0.0; len],
            mask: len - 1,
            wp: 0,
            phase: 0.0,
        }
    }

    #[inline]
    fn read(&self, delay: f32) -> f32 {
        let pos = self.wp as f32 - delay;
        let i = pos.floor();
        let frac = pos - i;
        let i = i as isize as usize;
        let a = self.buf[i & self.mask];
        let b = self.buf[i.wrapping_add(1) & self.mask];
        a + (b - a) * frac
    }

    #[inline]
    fn process(&mut self, x: f32, ratio: f32) -> f32 {
        self.buf[self.wp] = x;
        self.wp = (self.wp + 1) & self.mask;
        let w = SHIFT_WINDOW as f32;
        self.phase += (1.0 - ratio) / w;
        self.phase -= self.phase.floor();
        let p2 = (self.phase + 0.5).fract();
        let g1 = (self.phase * std::f32::consts::PI).sin().powi(2);
        let g2 = (p2 * std::f32::consts::PI).sin().powi(2);
        self.read(self.phase * w + 2.0) * g1 + self.read(p2 * w + 2.0) * g2
    }
}

pub struct Sympathetic {
    sr: f32,
    strings: Vec<Str>,
    sub: Vec<Str>,
    shifter: PitchDown,
}

impl Sympathetic {
    pub fn new(sr: f32) -> Self {
        let mut seed = 0x9e37_79b9u32;
        let strings = (0..STRINGS)
            .map(|k| {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                let offset = (seed >> 8) as f32 / 8388608.0 - 1.0;
                let pan = ((k * 5) % STRINGS) as f32 / (STRINGS - 1) as f32;
                Str::new(sr, offset, pan, MIN_FREQ)
            })
            .collect();
        let sub = (0..SUB_STRINGS)
            .map(|k| {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                let offset = (seed >> 8) as f32 / 8388608.0 - 1.0;
                let pan = ((k * 3) % SUB_STRINGS) as f32 / (SUB_STRINGS - 1) as f32;
                Str::new(sr, offset, pan, SUB_MIN_FREQ)
            })
            .collect();
        Self {
            sr,
            strings,
            sub,
            shifter: PitchDown::new(),
        }
    }

    fn retune(&mut self, f: &SympFrame) {
        let degrees = f.scale.degrees();
        let base = 36 + 12 * f.register.clamp(0, 3) + f.root.clamp(0, 11);
        for (k, s) in self.strings.iter_mut().enumerate() {
            let note = base + 12 * (k / degrees.len()) as i32 + degrees[k % degrees.len()] as i32;
            let cents = s.offset * f.detune_cents + f.drift_cents;
            let freq = 440.0 * 2f32.powf((note - 69) as f32 / 12.0 + cents / 1200.0);
            s.tune(freq, f.decay, f.damping, self.sr);
        }
        let drop = f.sub_drop.clamp(1, 3);
        let sub_base = 36 + f.root.clamp(0, 11) - 12 * drop;
        for (k, s) in self.sub.iter_mut().enumerate() {
            let note = sub_base + 12 * (k / degrees.len()) as i32 + degrees[k % degrees.len()] as i32;
            let cents = s.offset * f.detune_cents * 0.5 + f.drift_cents;
            let freq = 440.0 * 2f32.powf((note - 69) as f32 / 12.0 + cents / 1200.0);
            s.tune(freq, f.decay * 1.5, (f.damping + 0.2).min(1.0), self.sr);
        }
    }

    pub fn process(&mut self, l: &mut [f32], r: &mut [f32], f: &SympFrame) {
        let ringing = self.strings.iter().chain(self.sub.iter()).any(|s| s.active && s.lp.abs() > 1e-7);
        if f.mix < 1e-3 && f.sub_mix < 1e-3 && !ringing {
            return;
        }
        self.retune(f);
        let wet = f.mix * 1.5;
        let sub_wet = f.sub_mix * 2.0;
        let ratio = 0.5f32.powi(f.sub_drop.clamp(1, 3));
        for i in 0..l.len() {
            let x = (l[i] + r[i]) * 0.5;
            let mut wl = 0.0;
            let mut wr = 0.0;
            for s in self.strings.iter_mut().filter(|s| s.active) {
                let y = s.process(x);
                wl += y * s.gl;
                wr += y * s.gr;
            }
            let xs = self.shifter.process(x, ratio);
            let mut sl = 0.0;
            let mut sr = 0.0;
            for s in self.sub.iter_mut().filter(|s| s.active) {
                let y = s.process(xs + x * 0.3);
                sl += y * s.gl;
                sr += y * s.gr;
            }
            l[i] += soft_clip(wl) * wet + soft_clip(sl) * sub_wet;
            r[i] += soft_clip(wr) * wet + soft_clip(sr) * sub_wet;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(scale: Scale) -> SympFrame {
        SympFrame {
            mix: 1.0,
            root: 2,
            scale,
            register: 1,
            decay: 6.0,
            damping: 0.4,
            detune_cents: 8.0,
            drift_cents: 0.0,
            sub_mix: 0.0,
            sub_drop: 2,
        }
    }

    #[test]
    fn noise_and_resonant_sine_stay_bounded() {
        for scale in [Scale::Gong, Scale::Minor, Scale::Fifths] {
            let mut s = Sympathetic::new(48000.0);
            let f = frame(scale);
            let mut seed = 7u32;
            let mut peak = 0.0f32;
            for n in 0..(48000 * 3 / 32) {
                let mut l = [0.0f32; 32];
                let mut r = [0.0f32; 32];
                for i in 0..32 {
                    let t = (n * 32 + i) as f32 / 48000.0;
                    seed ^= seed << 13;
                    seed ^= seed >> 17;
                    seed ^= seed << 5;
                    let noise = ((seed >> 8) as f32 / 8388608.0 - 1.0) * 0.2;
                    let sine = (std::f32::consts::TAU * 146.83 * t).sin() * 0.5;
                    l[i] = if n < 1000 { noise + sine } else { 0.0 };
                    r[i] = l[i];
                }
                s.process(&mut l, &mut r, &f);
                for v in l.iter().chain(r.iter()) {
                    assert!(v.is_finite());
                    peak = peak.max(v.abs());
                }
            }
            assert!(peak < 2.3, "peak {peak}");
        }
    }

    #[test]
    fn rings_after_the_input_stops() {
        let mut s = Sympathetic::new(48000.0);
        let f = frame(Scale::Gong);
        for n in 0..300 {
            let mut l = [0.0f32; 32];
            let mut r = [0.0f32; 32];
            for i in 0..32 {
                let t = (n * 32 + i) as f32 / 48000.0;
                l[i] = (std::f32::consts::TAU * 146.83 * t).sin() * 0.4;
                r[i] = l[i];
            }
            s.process(&mut l, &mut r, &f);
        }
        let mut tail = 0.0f32;
        for _ in 0..1500 {
            let mut l = [0.0f32; 32];
            let mut r = [0.0f32; 32];
            s.process(&mut l, &mut r, &f);
            for v in l {
                tail = tail.max(v.abs());
            }
        }
        assert!(tail > 0.01, "tail {tail}");
    }

    #[test]
    fn tuned_string_period_matches_pitch() {
        let mut st = Str::new(48000.0, 0.0, 0.5, MIN_FREQ);
        st.tune(220.0, 8.0, 0.2, 48000.0);
        st.process(1.0);
        let mut out = vec![0.0f32; 24000];
        for v in out.iter_mut() {
            *v = st.process(0.0);
        }
        let corr = |lag: usize| -> f32 { (0..out.len() - lag).map(|i| out[i] * out[i + lag]).sum() };
        let best = (60..600).max_by(|a, b| corr(*a).total_cmp(&corr(*b))).unwrap();
        let (a, b, c) = (corr(best - 1), corr(best), corr(best + 1));
        let frac = 0.5 * (a - c) / (a - 2.0 * b + c);
        let hz = 48000.0 / (best as f32 + frac);
        assert!((hz - 220.0).abs() < 1.0, "hz {hz}");
    }

    #[test]
    fn sub_bank_rings_low_from_a_high_ping() {
        let mut s = Sympathetic::new(48000.0);
        let mut f = frame(Scale::Fifths);
        f.mix = 0.0;
        f.sub_mix = 1.0;
        f.sub_drop = 2;
        let mut out = Vec::new();
        for n in 0..1500 {
            let mut l = [0.0f32; 32];
            let mut r = [0.0f32; 32];
            if n < 40 {
                for i in 0..32 {
                    let t = (n * 32 + i) as f32 / 48000.0;
                    l[i] = (std::f32::consts::TAU * 2600.0 * t).sin() * 0.5;
                    r[i] = l[i];
                }
            }
            s.process(&mut l, &mut r, &f);
            out.extend_from_slice(&l);
        }
        let tail = &out[24000..];
        let peak = tail.iter().fold(0.0f32, |a, v| a.max(v.abs()));
        assert!(peak > 0.005 && peak < 2.3, "peak {peak}");
        let energy: f32 = tail.iter().map(|v| v * v).sum();
        let slope: f32 = tail.windows(2).map(|w| (w[1] - w[0]).powi(2)).sum();
        assert!(slope / energy < 0.05, "ratio {}", slope / energy);
    }
}
