use std::f32::consts::{FRAC_PI_4, PI};

const MAX_GRAINS: usize = 48;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Interval {
    Free,
    Octaves,
    Fifths,
}

pub struct GrainFrame {
    pub mix: f32,
    pub size: f32,
    pub density: f32,
    pub position: f32,
    pub jitter: f32,
    pub pitch: f32,
    pub spread: f32,
    pub interval: Interval,
    pub reverse: f32,
    pub width: f32,
    pub freeze: bool,
}

#[derive(Clone, Copy)]
struct Grain {
    active: bool,
    pos: f32,
    inc: f32,
    age: f32,
    len: f32,
    gl: f32,
    gr: f32,
}

pub struct Granular {
    sr: f32,
    buf_l: Vec<f32>,
    buf_r: Vec<f32>,
    mask: usize,
    wp: usize,
    grains: [Grain; MAX_GRAINS],
    timer: f32,
    rng: u32,
}

impl Granular {
    pub fn new(sr: f32) -> Self {
        let len = ((5.0 * sr) as usize).next_power_of_two();
        Self {
            sr,
            buf_l: vec![0.0; len],
            buf_r: vec![0.0; len],
            mask: len - 1,
            wp: 0,
            grains: [Grain {
                active: false,
                pos: 0.0,
                inc: 1.0,
                age: 0.0,
                len: 1.0,
                gl: 0.0,
                gr: 0.0,
            }; MAX_GRAINS],
            timer: 0.0,
            rng: 0x2545F491,
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

    #[inline]
    fn rand01(&mut self) -> f32 {
        self.rand() * 0.5 + 0.5
    }

    fn pick_pitch(&mut self, f: &GrainFrame) -> f32 {
        let spread = f.spread.clamp(0.0, 1.0);
        match f.interval {
            Interval::Free => f.pitch + spread * 12.0 * (self.rand() + self.rand()) * 0.5,
            Interval::Octaves => {
                if self.rand01() < spread {
                    let k = self.rand01();
                    let st = if k < 0.4 {
                        12.0
                    } else if k < 0.8 {
                        -12.0
                    } else if k < 0.9 {
                        24.0
                    } else {
                        -24.0
                    };
                    f.pitch + st
                } else {
                    f.pitch
                }
            }
            Interval::Fifths => {
                const SET: [f32; 6] = [7.0, 12.0, -12.0, -5.0, 19.0, -17.0];
                if self.rand01() < spread {
                    let i = (self.rand01() * SET.len() as f32) as usize % SET.len();
                    f.pitch + SET[i]
                } else {
                    f.pitch
                }
            }
        }
    }

    fn spawn(&mut self, f: &GrainFrame) {
        let slot = match self.grains.iter().position(|g| !g.active) {
            Some(i) => i,
            None => return,
        };
        let size = (f.size * (1.0 + f.jitter * 0.5 * self.rand())).max(0.004);
        let len = size * self.sr;
        let st = self.pick_pitch(f);
        let mut inc = 2f32.powf(st / 12.0);
        let reverse = self.rand01() < f.reverse;
        let buf_len = (self.mask + 1) as f32;
        let travel = len * inc.abs().max(1.0);
        let min_delay = if reverse {
            travel + 64.0
        } else {
            len * (inc - 1.0).max(0.0) + 64.0
        };
        let max_delay = (buf_len - len * (1.0 + inc.abs()) - 8.0).max(min_delay);
        let jitter = 1.0 + f.jitter * 0.8 * self.rand();
        let delay = (f.position * self.sr * jitter).clamp(min_delay, max_delay);
        let mut start = self.wp as f32 - delay;
        if reverse {
            start += len * inc.abs();
            inc = -inc.abs();
        }
        let pan = self.rand() * f.width.clamp(0.0, 1.0);
        let ang = (pan + 1.0) * FRAC_PI_4;
        self.grains[slot] = Grain {
            active: true,
            pos: start.rem_euclid(buf_len),
            inc,
            age: 0.0,
            len,
            gl: ang.cos() * std::f32::consts::SQRT_2,
            gr: ang.sin() * std::f32::consts::SQRT_2,
        };
    }

    #[inline]
    fn read(&self, pos: f32) -> (f32, f32) {
        let i = pos as usize;
        let fr = pos - i as f32;
        let a = i & self.mask;
        let b = (i + 1) & self.mask;
        (
            self.buf_l[a] + (self.buf_l[b] - self.buf_l[a]) * fr,
            self.buf_r[a] + (self.buf_r[b] - self.buf_r[a]) * fr,
        )
    }

    pub fn process(&mut self, l: &mut [f32], r: &mut [f32], f: &GrainFrame) {
        let active_any = self.grains.iter().any(|g| g.active);
        if f.mix < 1e-3 && !active_any && !f.freeze {
            for i in 0..l.len() {
                self.buf_l[self.wp] = l[i];
                self.buf_r[self.wp] = r[i];
                self.wp = (self.wp + 1) & self.mask;
            }
            return;
        }
        let overlap = f.density * f.size * 0.375;
        let norm = 1.0 / overlap.max(1.0).sqrt();
        let interval = self.sr / f.density.max(0.5);
        let buf_len = (self.mask + 1) as f32;
        for i in 0..l.len() {
            let (dl, dr) = (l[i], r[i]);
            if !f.freeze {
                self.buf_l[self.wp] = dl;
                self.buf_r[self.wp] = dr;
                self.wp = (self.wp + 1) & self.mask;
            }
            self.timer -= 1.0;
            if self.timer <= 0.0 && f.mix > 1e-3 {
                self.spawn(f);
                self.timer += interval * (0.4 + 1.2 * self.rand01());
            }
            let mut wl = 0.0;
            let mut wr = 0.0;
            for k in 0..MAX_GRAINS {
                if !self.grains[k].active {
                    continue;
                }
                let g = self.grains[k];
                let phase = g.age / g.len;
                let s = (PI * phase).sin();
                let w = s * s;
                let (sl, sr) = self.read(g.pos);
                wl += sl * w * g.gl;
                wr += sr * w * g.gr;
                let grain = &mut self.grains[k];
                grain.pos = (grain.pos + grain.inc).rem_euclid(buf_len);
                grain.age += 1.0;
                if grain.age >= grain.len {
                    grain.active = false;
                }
            }
            let m = f.mix;
            l[i] = dl * (1.0 - 0.5 * m) + wl * norm * m;
            r[i] = dr * (1.0 - 0.5 * m) + wr * norm * m;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(pitch: f32, freeze: bool) -> GrainFrame {
        GrainFrame {
            mix: 1.0,
            size: 0.12,
            density: 30.0,
            position: 0.5,
            jitter: 0.2,
            pitch,
            spread: 0.0,
            interval: Interval::Free,
            reverse: 0.0,
            width: 0.5,
            freeze,
        }
    }

    fn run(pitch: f32) -> Vec<f32> {
        let sr = 48000.0;
        let mut g = Granular::new(sr);
        let mut out = Vec::new();
        for block in 0..(sr as usize * 3 / 32) {
            let mut l = [0.0f32; 32];
            let mut r = [0.0f32; 32];
            for i in 0..32 {
                let t = (block * 32 + i) as f32 / sr;
                let s = if t < 1.0 { (t * 220.0 * std::f32::consts::TAU).sin() * 0.5 } else { 0.0 };
                l[i] = s;
                r[i] = s;
            }
            g.process(&mut l, &mut r, &frame(pitch, false));
            out.extend_from_slice(&l);
        }
        out
    }

    #[test]
    fn finite_bounded_and_level_sane() {
        let out = run(0.0);
        assert!(out.iter().all(|v| v.is_finite() && v.abs() < 2.0));
        let seg = &out[48000 + 6000..48000 + 24000];
        let rms = (seg.iter().map(|v| v * v).sum::<f32>() / seg.len() as f32).sqrt();
        assert!(rms > 0.05 && rms < 0.6, "rms {rms}");
    }

    #[test]
    fn pitch_shift_up_an_octave() {
        let out = run(12.0);
        let seg = &out[48000 + 3000..48000 + 3000 + 8192];
        let mut best = (0usize, f32::MIN);
        for lag in 40..400 {
            let c: f32 = seg.iter().zip(&seg[lag..]).map(|(a, b)| a * b).sum();
            if c > best.1 {
                best = (lag, c);
            }
        }
        let freq = 48000.0 / best.0 as f32;
        assert!((freq - 440.0).abs() < 25.0, "dominant {freq}");
    }

    #[test]
    fn freeze_holds_texture_after_input_stops() {
        let sr = 48000.0;
        let mut g = Granular::new(sr);
        let mut tail = 0.0f32;
        for block in 0..(sr as usize * 3 / 32) {
            let mut l = [0.0f32; 32];
            let mut r = [0.0f32; 32];
            let t0 = (block * 32) as f32 / sr;
            if t0 < 1.0 {
                for i in 0..32 {
                    let t = t0 + i as f32 / sr;
                    l[i] = (t * 330.0 * std::f32::consts::TAU).sin() * 0.5;
                    r[i] = l[i];
                }
            }
            g.process(&mut l, &mut r, &frame(0.0, t0 > 1.0));
            if t0 > 2.0 {
                tail = tail.max(l.iter().fold(0.0f32, |m, v| m.max(v.abs())));
            }
        }
        assert!(tail > 0.05, "frozen tail {tail}");
    }
}
