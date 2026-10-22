use std::f32::consts::TAU;

pub struct FxFrame {
    pub chorus_mix: f32,
    pub chorus_rate: f32,
    pub chorus_depth: f32,
    pub delay_time: f32,
    pub delay_feedback: f32,
    pub delay_tone: f32,
    pub delay_mix: f32,
    pub reverb_mix: f32,
    pub reverb_decay: f32,
    pub reverb_damping: f32,
    pub reverb_predelay: f32,
}

struct Line {
    buf: Vec<f32>,
    mask: usize,
    pos: usize,
}

impl Line {
    fn new(max_samples: usize) -> Self {
        let len = (max_samples + 4).next_power_of_two();
        Self {
            buf: vec![0.0; len],
            mask: len - 1,
            pos: 0,
        }
    }

    #[inline]
    fn write(&mut self, v: f32) {
        self.buf[self.pos] = v;
        self.pos = (self.pos + 1) & self.mask;
    }

    #[inline]
    fn read(&self, delay: f32) -> f32 {
        let d = delay.max(1.0);
        let i = d as usize;
        let fr = d - i as f32;
        let a = self.buf[(self.pos + self.mask + 1 - i) & self.mask];
        let b = self.buf[(self.pos + self.mask + 1 - i - 1) & self.mask];
        a + (b - a) * fr
    }
}

struct Chorus {
    l: Line,
    r: Line,
    phase: f32,
}

impl Chorus {
    fn new(sr: f32) -> Self {
        let max = (0.05 * sr) as usize;
        Self {
            l: Line::new(max),
            r: Line::new(max),
            phase: 0.0,
        }
    }

    #[inline]
    fn process(&mut self, l: f32, r: f32, f: &FxFrame, sr: f32) -> (f32, f32) {
        self.phase += f.chorus_rate / sr;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        let base = 0.014 * sr;
        let depth = f.chorus_depth * 0.006 * sr;
        let ml = (self.phase * TAU).sin();
        let mr = ((self.phase + 0.33) * TAU).sin();
        self.l.write(l);
        self.r.write(r);
        let wl = self.l.read(base + ml * depth);
        let wr = self.r.read(base + mr * depth);
        let m = f.chorus_mix;
        (l + (wl - l) * m * 0.6, r + (wr - r) * m * 0.6)
    }
}

struct Delay {
    l: Line,
    r: Line,
    time: f32,
    lp_l: f32,
    lp_r: f32,
    max: f32,
}

impl Delay {
    fn new(sr: f32) -> Self {
        let max = (2.0 * sr) as usize;
        Self {
            l: Line::new(max),
            r: Line::new(max),
            time: 0.3 * sr,
            lp_l: 0.0,
            lp_r: 0.0,
            max: max as f32,
        }
    }

    #[inline]
    fn process(&mut self, l: f32, r: f32, f: &FxFrame, sr: f32) -> (f32, f32) {
        let target = (f.delay_time * sr).clamp(2.0, self.max);
        self.time += (target - self.time) * 0.0008;
        let dl = self.l.read(self.time);
        let dr = self.r.read(self.time);
        let c = 0.05 + f.delay_tone * 0.9;
        self.lp_l += (dl - self.lp_l) * c;
        self.lp_r += (dr - self.lp_r) * c;
        let mono = (l + r) * 0.5;
        self.l.write(mono + self.lp_r * f.delay_feedback);
        self.r.write(self.lp_l * f.delay_feedback);
        (l + self.lp_l * f.delay_mix, r + self.lp_r * f.delay_mix)
    }
}

struct Allpass {
    line: Line,
    delay: f32,
}

impl Allpass {
    fn new(samples: usize) -> Self {
        Self {
            line: Line::new(samples + 2),
            delay: samples as f32,
        }
    }

    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        const G: f32 = 0.6;
        let d = self.line.read(self.delay);
        let w = x + d * G;
        self.line.write(w);
        d - w * G
    }
}

const FDN: usize = 8;
const BASE_LENGTHS: [f32; FDN] = [
    1031.0, 1327.0, 1523.0, 1801.0, 2053.0, 2311.0, 2579.0, 2837.0,
];
const OUT_L: [f32; FDN] = [1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0];
const OUT_R: [f32; FDN] = [1.0, 1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0];

struct Reverb {
    lines: Vec<Line>,
    lengths: [f32; FDN],
    gains: [f32; FDN],
    damp_state: [f32; FDN],
    mod_phase: [f32; FDN],
    mod_inc: [f32; FDN],
    diffusion: [Allpass; 4],
    predelay: Line,
    last_decay: f32,
    scale: f32,
}

impl Reverb {
    fn new(sr: f32) -> Self {
        let scale = sr / 48000.0;
        let mut lengths = [0.0; FDN];
        let mut lines = Vec::with_capacity(FDN);
        let mut mod_inc = [0.0; FDN];
        let mut mod_phase = [0.0; FDN];
        for i in 0..FDN {
            lengths[i] = BASE_LENGTHS[i] * scale;
            lines.push(Line::new((lengths[i] * 1.2) as usize + 64));
            mod_inc[i] = (0.11 + 0.037 * i as f32) / sr;
            mod_phase[i] = i as f32 / FDN as f32;
        }
        let d = |n: f32| (n * scale) as usize;
        Self {
            lines,
            lengths,
            gains: [0.9; FDN],
            damp_state: [0.0; FDN],
            mod_phase,
            mod_inc,
            diffusion: [
                Allpass::new(d(142.0)),
                Allpass::new(d(107.0)),
                Allpass::new(d(379.0)),
                Allpass::new(d(277.0)),
            ],
            predelay: Line::new((0.25 * sr) as usize),
            last_decay: -1.0,
            scale,
        }
    }

    fn update_gains(&mut self, decay: f32, sr: f32) {
        if (decay - self.last_decay).abs() < 1e-4 {
            return;
        }
        self.last_decay = decay;
        for i in 0..FDN {
            self.gains[i] = 10f32.powf(-3.0 * self.lengths[i] / (sr * decay.max(0.1)));
        }
    }

    #[inline]
    fn process(&mut self, l: f32, r: f32, f: &FxFrame, sr: f32) -> (f32, f32) {
        let mono = (l + r) * 0.5;
        self.predelay.write(mono);
        let mut x = self.predelay.read((f.reverb_predelay * sr).max(1.0));
        for ap in self.diffusion.iter_mut() {
            x = ap.process(x);
        }

        let damp = 0.08 + (1.0 - f.reverb_damping) * 0.85;
        let mut y = [0.0f32; FDN];
        for i in 0..FDN {
            self.mod_phase[i] += self.mod_inc[i];
            if self.mod_phase[i] >= 1.0 {
                self.mod_phase[i] -= 1.0;
            }
            let m = (self.mod_phase[i] * TAU).sin() * 6.0 * self.scale;
            let v = self.lines[i].read(self.lengths[i] + m);
            self.damp_state[i] += (v - self.damp_state[i]) * damp;
            y[i] = self.damp_state[i] * self.gains[i];
        }

        let mut out_l = 0.0;
        let mut out_r = 0.0;
        for i in 0..FDN {
            out_l += y[i] * OUT_L[i];
            out_r += y[i] * OUT_R[i];
        }

        let mut h = y;
        let mut step = 1;
        while step < FDN {
            let mut i = 0;
            while i < FDN {
                for j in i..i + step {
                    let a = h[j];
                    let b = h[j + step];
                    h[j] = a + b;
                    h[j + step] = a - b;
                }
                i += step * 2;
            }
            step *= 2;
        }
        let norm = 1.0 / (FDN as f32).sqrt();
        for i in 0..FDN {
            self.lines[i].write(x * 0.35 + h[i] * norm);
        }

        let wet = 0.35;
        let m = f.reverb_mix;
        (l + out_l * wet * m, r + out_r * wet * m)
    }
}

pub struct Fx {
    sr: f32,
    chorus: Chorus,
    delay: Delay,
    reverb: Reverb,
}

impl Fx {
    pub fn new(sr: f32) -> Self {
        Self {
            sr,
            chorus: Chorus::new(sr),
            delay: Delay::new(sr),
            reverb: Reverb::new(sr),
        }
    }

    pub fn reset(&mut self) {
        let sr = self.sr;
        *self = Self::new(sr);
    }

    pub fn process(&mut self, l: &mut [f32], r: &mut [f32], f: &FxFrame) {
        let sr = self.sr;
        self.reverb.update_gains(f.reverb_decay, sr);
        let chorus_on = f.chorus_mix > 1e-3;
        let delay_on = f.delay_mix > 1e-3;
        let reverb_on = f.reverb_mix > 1e-3;
        for i in 0..l.len() {
            let (mut a, mut b) = (l[i], r[i]);
            if chorus_on {
                (a, b) = self.chorus.process(a, b, f, sr);
            }
            if delay_on {
                (a, b) = self.delay.process(a, b, f, sr);
            }
            if reverb_on {
                (a, b) = self.reverb.process(a, b, f, sr);
            }
            l[i] = a;
            r[i] = b;
        }
    }
}
