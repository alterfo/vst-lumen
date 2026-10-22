use crate::envelope::{Adsr, AdsrSettings};
use crate::filter::{Coefs, Svf};
use crate::resonator::{ResFrame, Resonator};
use crate::wavetable::WaveTable;
use std::f32::consts::{PI, SQRT_2, TAU};

pub const MAX_UNISON: usize = 8;

#[derive(Clone, Copy)]
pub struct OscFrame {
    pub position: f32,
    pub level: f32,
    pub ratio: f32,
    pub unison: f32,
    pub detune_cents: f32,
    pub spread: f32,
}

pub struct Frame<'a> {
    pub sr: f32,
    pub table: &'a WaveTable,
    pub osc: [OscFrame; 2],
    pub sub: f32,
    pub noise: f32,
    pub filter_morph: f32,
    pub cutoff: f32,
    pub resonance: f32,
    pub drive: f32,
    pub env_amount: f32,
    pub lfo_cutoff: f32,
    pub pitch_mul: f32,
    pub drift_cents: f32,
    pub pos_spread: f32,
    pub glide_coef: f32,
    pub variation: f32,
    pub res: ResFrame,
    pub amp: AdsrSettings,
    pub filt: AdsrSettings,
}

#[derive(Clone, Copy)]
pub struct Voice {
    pub active: bool,
    pub note: u8,
    pub gate: bool,
    pub age: u64,
    velocity: f32,
    pitch: f32,
    phases: [[f32; MAX_UNISON]; 2],
    sub_phase: f32,
    amp_env: Adsr,
    filt_env: Adsr,
    filter_l: Svf,
    filter_r: Svf,
    resonator: Resonator,
    tail: f32,
    rng: u32,
    drift: f32,
    drift_target: f32,
    var_pitch: f32,
    var_cut: f32,
    var_pos: f32,
    var_amp: f32,
}

fn unison_layout(u: f32) -> (usize, f32) {
    let u = u.clamp(1.0, MAX_UNISON as f32);
    let count = u.ceil() as usize;
    let last = if count > 1 { u - (count - 1) as f32 } else { 1.0 };
    (count, last)
}

impl Voice {
    pub const fn new() -> Self {
        Self {
            active: false,
            note: 0,
            gate: false,
            age: 0,
            velocity: 0.0,
            pitch: 0.0,
            phases: [[0.0; MAX_UNISON]; 2],
            sub_phase: 0.0,
            amp_env: Adsr::new(),
            filt_env: Adsr::new(),
            filter_l: Svf::new(),
            filter_r: Svf::new(),
            resonator: Resonator::new(),
            tail: 0.0,
            rng: 1,
            drift: 0.0,
            drift_target: 0.0,
            var_pitch: 0.0,
            var_cut: 0.0,
            var_pos: 0.0,
            var_amp: 0.0,
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

    pub fn start(&mut self, note: u8, velocity: f32, age: u64, seed: u32, start_pitch: f32) {
        let was_idle = !self.active;
        self.active = true;
        self.gate = true;
        self.note = note;
        self.velocity = velocity;
        self.pitch = start_pitch;
        self.age = age;
        self.rng = seed | 1;
        for o in 0..2 {
            for u in 0..MAX_UNISON {
                self.phases[o][u] = (self.rand() * 0.5 + 0.5).fract();
            }
        }
        self.sub_phase = 0.0;
        if was_idle {
            self.filter_l.reset();
            self.filter_r.reset();
            self.resonator.reset();
            self.amp_env.kill();
            self.filt_env.kill();
            self.tail = 0.0;
        }
        self.drift_target = self.rand();
        self.var_pitch = self.rand();
        self.var_cut = self.rand();
        self.var_pos = self.rand();
        self.var_amp = self.rand();
        let rseed = self.rng.wrapping_mul(2654435761);
        self.resonator.trigger(velocity, rseed);
        self.amp_env.trigger();
        self.filt_env.trigger();
    }

    pub fn release(&mut self) {
        self.gate = false;
        self.amp_env.release();
        self.filt_env.release();
    }

    pub fn retarget(&mut self, note: u8) {
        self.note = note;
    }

    pub fn pitch(&self) -> f32 {
        self.pitch
    }

    pub fn is_releasing(&self) -> bool {
        self.active && self.amp_env.is_releasing()
    }

    pub fn level(&self) -> f32 {
        self.amp_env.value + self.tail
    }

    pub fn render(&mut self, f: &Frame, out_l: &mut [f32], out_r: &mut [f32]) {
        if !self.active {
            return;
        }
        let n = out_l.len();
        if self.rand().abs() > 0.97 {
            self.drift_target = self.rand();
        }
        self.drift += (self.drift_target - self.drift) * 0.002 * n as f32 / 32.0;
        let drift_mul = 2f32.powf((self.drift * f.drift_cents + self.var_pitch * f.variation * 7.0) / 1200.0);
        self.pitch += (self.note as f32 - self.pitch) * f.glide_coef;
        let base = 440.0 * 2f32.powf((self.pitch - 69.0) / 12.0) * f.pitch_mul * drift_mul;

        let mut incs = [[0.0f32; MAX_UNISON]; 2];
        let mut offs = [[(0usize, 0usize, 0.0f32); MAX_UNISON]; 2];
        let mut gains = [[(0.0f32, 0.0f32); MAX_UNISON]; 2];
        let mut counts = [1usize; 2];
        for o in 0..2 {
            let osc = &f.osc[o];
            let (count, last) = unison_layout(osc.unison);
            counts[o] = count;
            let mut power = 0.0;
            for u in 0..count {
                let w = if u == count - 1 { last } else { 1.0 };
                power += w * w;
            }
            let norm = osc.level / power.sqrt().max(1.0);
            let position = (osc.position + self.drift * f.pos_spread + self.var_pos * f.variation * 0.06)
                .clamp(0.0, 1.0);
            for u in 0..count {
                let w = if u == count - 1 { last } else { 1.0 };
                let pos = if count == 1 {
                    0.0
                } else {
                    u as f32 / (count - 1) as f32 * 2.0 - 1.0
                };
                let freq = base * osc.ratio * 2f32.powf(pos * osc.detune_cents / 1200.0);
                let inc = freq / f.sr;
                incs[o][u] = inc;
                let level = f.table.level_for(inc);
                offs[o][u] = f.table.offsets(position, level);
                let pan = pos * osc.spread;
                let ang = (pan + 1.0) * PI * 0.25;
                gains[o][u] = (
                    ang.cos() * SQRT_2 * norm * w,
                    ang.sin() * SQRT_2 * norm * w,
                );
            }
        }

        let fenv = self.filt_env.value;
        let cutoff = (f.cutoff
            * 2f32.powf(f.env_amount * 7.0 * fenv + f.lfo_cutoff + self.var_cut * f.variation * 0.35))
        .clamp(20.0, 0.45 * f.sr);
        let coefs = Coefs::new(cutoff, f.resonance, f.drive, f.filter_morph, f.sr);
        let sub_inc = base * 0.5 / f.sr;
        let res_on = f.res.mix > 1e-3 && f.res.kind != crate::resonator::Kind::Off;
        if res_on {
            self.resonator.update(&f.res, base, f.sr);
        }
        let dry_keep = if res_on { 1.0 - f.res.mix } else { 1.0 };
        let wet_amount = if res_on { f.res.mix } else { 0.0 };
        let amp_trim = 1.0 + self.var_amp * f.variation * 0.15 + self.drift * f.variation * 0.05;

        for i in 0..n {
            let amp = self.amp_env.next(&f.amp);
            self.filt_env.next(&f.filt);
            let mut sl = 0.0;
            let mut sr = 0.0;
            for o in 0..2 {
                if f.osc[o].level <= 0.0 {
                    continue;
                }
                for u in 0..counts[o] {
                    let (off0, off1, wfrac) = offs[o][u];
                    let v = f.table.read(off0, off1, wfrac, self.phases[o][u]);
                    let ph = self.phases[o][u] + incs[o][u];
                    self.phases[o][u] = if ph >= 1.0 { ph - 1.0 } else { ph };
                    sl += v * gains[o][u].0;
                    sr += v * gains[o][u].1;
                }
            }
            if f.sub > 0.0 {
                let s = (self.sub_phase * TAU).sin() * f.sub;
                self.sub_phase += sub_inc;
                if self.sub_phase >= 1.0 {
                    self.sub_phase -= 1.0;
                }
                sl += s;
                sr += s;
            }
            if f.noise > 0.0 {
                let nz = self.rand() * f.noise;
                sl += nz;
                sr += nz;
            }
            let g = amp * self.velocity * amp_trim;
            let yl = self.filter_l.process(sl, &coefs);
            let yr = self.filter_r.process(sr, &coefs);
            let mut ol = yl * g * dry_keep;
            let mut or = yr * g * dry_keep;
            if res_on {
                let wet = self.resonator.process((sl + sr) * 0.5 * g, &f.res) * wet_amount;
                ol += wet;
                or += wet;
                self.tail = self.tail.max(wet.abs()) * 0.9996;
            }
            out_l[i] += ol;
            out_r[i] += or;
        }
        if !res_on {
            self.tail = 0.0;
        }
        if self.amp_env.is_idle() && self.tail < 1e-4 {
            self.active = false;
        }
    }
}

impl Default for Voice {
    fn default() -> Self {
        Self::new()
    }
}
