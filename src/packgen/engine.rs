use crate::body::{Body, BodyFrame};
use crate::chaos::{OrganicModulator, OrganicParams};
use crate::envelope::AdsrSettings;
use crate::evolver::Evolver;
use crate::filter::soft_clip;
use crate::fx::{Fx, FxFrame};
use crate::granular::{GrainFrame, Granular, Interval};
use crate::lfo_value;
use crate::resonator::{Kind, ResFrame};
use crate::voice::{Frame, OscFrame, Voice};
use crate::wavetable::WaveTable;
use nih_plug::prelude::util;
use std::sync::Arc;

pub const BLOCK: usize = 32;
pub const VOICES: usize = 16;
const HELD: usize = 16;

#[derive(Clone)]
pub struct Patch {
    pub(crate) pos: [f32; 2],
    pub(crate) level: [f32; 2],
    pub(crate) tune: [f32; 2],
    pub(crate) fine: [f32; 2],
    pub(crate) unison: [f32; 2],
    pub(crate) detune: [f32; 2],
    pub(crate) spread: [f32; 2],
    pub(crate) sub: f32,
    pub(crate) noise: f32,
    pub(crate) f_mode: f32,
    pub(crate) cutoff: f32,
    pub(crate) resonance: f32,
    pub(crate) drive: f32,
    pub(crate) env_amount: f32,
    pub(crate) amp: [f32; 4],
    pub(crate) filt: [f32; 4],
    pub(crate) lfo: [f32; 5],
    pub(crate) morph: f32,
    pub(crate) evo: [f32; 5],
    pub(crate) drift: f32,
    pub(crate) res: [f32; 8],
    pub(crate) body: [f32; 4],
    pub(crate) gran: [f32; 11],
    pub(crate) organic: f32,
    pub(crate) chaos: f32,
    pub(crate) chaos_rate: f32,
    pub(crate) glide: f32,
    pub(crate) fx: [f32; 12],
    pub(crate) gain: f32,
    pub(crate) mode: f32,
}

impl Default for Patch {
    fn default() -> Self {
        Self {
            pos: [0.38, 0.75],
            level: [0.8, 0.8],
            tune: [0.0, 0.0],
            fine: [0.0, 0.0],
            unison: [3.0, 2.0],
            detune: [0.25, 0.25],
            spread: [0.6, 0.6],
            sub: 0.08,
            noise: 0.0,
            f_mode: 0.0,
            cutoff: 1400.0,
            resonance: 0.15,
            drive: 0.2,
            env_amount: 0.25,
            amp: [0.02, 0.6, 0.7, 0.8],
            filt: [0.01, 0.8, 0.3, 0.8],
            lfo: [0.0, 0.4, 0.0, 0.0, 0.0],
            morph: 0.5,
            evo: [0.05, 0.3, 0.4, 0.2, 0.3],
            drift: 0.35,
            res: [0.0, 0.6, 1.8, 0.45, 0.15, 0.5, 0.5, 0.3],
            body: [0.0, 1.0, 0.25, 0.4],
            gran: [0.0, 0.18, 14.0, 0.3, 0.4, 0.0, 0.25, 1.0, 0.2, 0.7, 0.0],
            organic: 0.4,
            chaos: 0.2,
            chaos_rate: 0.15,
            glide: 0.0,
            fx: [0.3, 0.4, 0.5, 0.375, 0.4, 0.5, 0.0, 0.25, 3.5, 0.4, 0.02, 0.0],
            gain: -6.0,
            mode: 0.0,
        }
    }
}

fn osc_slot(id: &str) -> Option<(usize, u8)> {
    let bytes = id.as_bytes();
    if bytes.len() < 5 || bytes[0] != b'o' || !bytes[1].is_ascii_digit() || bytes[2] != b'_' {
        return None;
    }
    let index = (bytes[1] - b'1') as usize;
    if index > 1 {
        return None;
    }
    let field = match &id[3..] {
        "wt" => 0,
        "lvl" => 1,
        "tune" => 2,
        "fine" => 3,
        "uni" => 4,
        "det" => 5,
        "spr" => 6,
        _ => return None,
    };
    Some((index, field))
}

pub fn patch_from(values: &[(&str, f32)]) -> Patch {
    let mut p = Patch::default();
    for (id, v) in values {
        let v = *v;
        if let Some((o, field)) = osc_slot(id) {
            match field {
                0 => p.pos[o] = v,
                1 => p.level[o] = v,
                2 => p.tune[o] = v,
                3 => p.fine[o] = v,
                4 => p.unison[o] = v,
                5 => p.detune[o] = v,
                _ => p.spread[o] = v,
            }
            continue;
        }
        match *id {
            "sub" => p.sub = v,
            "noise" => p.noise = v,
            "flt_mode" => p.f_mode = v,
            "flt_cut" => p.cutoff = v,
            "flt_res" => p.resonance = v,
            "flt_drv" => p.drive = v,
            "flt_env" => p.env_amount = v,
            "ae_a" => p.amp[0] = v,
            "ae_d" => p.amp[1] = v,
            "ae_s" => p.amp[2] = v,
            "ae_r" => p.amp[3] = v,
            "fe_a" => p.filt[0] = v,
            "fe_d" => p.filt[1] = v,
            "fe_s" => p.filt[2] = v,
            "fe_r" => p.filt[3] = v,
            "lfo_shape" => p.lfo[0] = v,
            "lfo_rate" => p.lfo[1] = v,
            "lfo_wt" => p.lfo[2] = v,
            "lfo_pitch" => p.lfo[3] = v,
            "lfo_cut" => p.lfo[4] = v,
            "morph" => p.morph = v,
            "evo_rate" => p.evo[0] = v,
            "evo_tbl" => p.evo[1] = v,
            "evo_morph" => p.evo[2] = v,
            "evo_cut" => p.evo[3] = v,
            "evo_vox" => p.evo[4] = v,
            "drift" => p.drift = v,
            "res_model" => p.res[0] = v,
            "res_mix" => p.res[1] = v,
            "res_decay" => p.res[2] = v,
            "res_damp" => p.res[3] = v,
            "res_stiff" => p.res[4] = v,
            "res_strike" => p.res[5] = v,
            "res_hard" => p.res[6] = v,
            "res_pos" => p.res[7] = v,
            "body_mix" => p.body[0] = v,
            "body_size" => p.body[1] = v,
            "body_decay" => p.body[2] = v,
            "body_damp" => p.body[3] = v,
            "gran_mix" => p.gran[0] = v,
            "gran_size" => p.gran[1] = v,
            "gran_dens" => p.gran[2] = v,
            "gran_pos" => p.gran[3] = v,
            "gran_jit" => p.gran[4] = v,
            "gran_pitch" => p.gran[5] = v,
            "gran_spread" => p.gran[6] = v,
            "gran_ival" => p.gran[7] = v,
            "gran_rev" => p.gran[8] = v,
            "gran_wide" => p.gran[9] = v,
            "gran_freeze" => p.gran[10] = v,
            "mac_organic" => p.organic = v,
            "mac_chaos" => p.chaos = v,
            "mac_crate" => p.chaos_rate = v,
            "glide" => p.glide = v,
            "fx_cmix" => p.fx[0] = v,
            "fx_crate" => p.fx[1] = v,
            "fx_cdepth" => p.fx[2] = v,
            "fx_dtime" => p.fx[3] = v,
            "fx_dfb" => p.fx[4] = v,
            "fx_dtone" => p.fx[5] = v,
            "fx_dmix" => p.fx[6] = v,
            "fx_rmix" => p.fx[7] = v,
            "fx_rdecay" => p.fx[8] = v,
            "fx_rdamp" => p.fx[9] = v,
            "fx_rpre" => p.fx[10] = v,
            "vmode" => p.mode = v,
            "gain" => p.gain = v,
            _ => {}
        }
    }
    p
}

pub struct Engine {
    voices: Vec<Voice>,
    fx: Fx,
    body: Body,
    granular: Granular,
    organic: OrganicModulator,
    evo_a: Evolver,
    evo_b: Evolver,
    lfo_phase: f32,
    lfo_smooth: f32,
    sr: f32,
    counter: u64,
    has_last: bool,
    last_pitch: f32,
    held: [(u8, f32); HELD],
    held_len: usize,
    legato: bool,
    mono: bool,
    gain_prev: f32,
    table: Arc<WaveTable>,
    patch: Patch,
}

impl Engine {
    pub fn new(table: Arc<WaveTable>, sr: f32, patch: Patch) -> Self {
        Self {
            voices: vec![Voice::new(); VOICES],
            fx: Fx::new(sr),
            body: Body::new(sr),
            granular: Granular::new(sr),
            organic: OrganicModulator::new(0xC0FFEE),
            evo_a: Evolver::new(0x9E3779B9),
            evo_b: Evolver::new(0x85EBCA6B),
            lfo_phase: 0.0,
            lfo_smooth: 0.0,
            sr,
            counter: 0,
            has_last: false,
            last_pitch: 60.0,
            held: [(0, 0.0); HELD],
            held_len: 0,
            legato: false,
            mono: false,
            gain_prev: util::db_to_gain(patch.gain),
            table,
            patch,
        }
    }

    fn free_voice(&self) -> usize {
        self.voices
            .iter()
            .position(|v| !v.active)
            .or_else(|| {
                self.voices
                    .iter()
                    .enumerate()
                    .filter(|(_, v)| v.is_releasing())
                    .min_by(|a, b| a.1.level().total_cmp(&b.1.level()))
                    .map(|(i, _)| i)
            })
            .unwrap_or_else(|| {
                self.voices
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, v)| v.age)
                    .map(|(i, _)| i)
                    .unwrap_or(0)
            })
    }

    pub fn note_on(&mut self, note: u8, velocity: f32) {
        self.counter += 1;
        let seed = (self.counter as u32).wrapping_mul(2654435761) ^ (note as u32) << 8;
        let glide_on = self.patch.glide > 0.001;
        let start_pitch = if glide_on && self.has_last {
            self.last_pitch
        } else {
            note as f32
        };
        if !self.mono {
            let idx = self.free_voice();
            self.voices[idx].start(note, velocity, self.counter, seed, start_pitch);
        } else {
            if self.held_len < HELD {
                self.held[self.held_len] = (note, velocity);
                self.held_len += 1;
            }
            for v in self.voices.iter_mut().skip(1) {
                if v.active && v.gate {
                    v.release();
                }
            }
            let v0 = &mut self.voices[0];
            if self.legato && v0.active && v0.gate {
                v0.retarget(note);
            } else {
                let from = if v0.active { v0.pitch() } else { start_pitch };
                v0.start(note, velocity, self.counter, seed, from);
            }
        }
        self.has_last = true;
        self.last_pitch = note as f32;
    }

    pub fn note_off(&mut self, note: u8) {
        if !self.mono {
            for v in self.voices.iter_mut() {
                if v.active && v.gate && v.note == note {
                    v.release();
                }
            }
            return;
        }
        if let Some(i) = (0..self.held_len).rev().find(|&i| self.held[i].0 == note) {
            for j in i..self.held_len.saturating_sub(1) {
                self.held[j] = self.held[j + 1];
            }
            self.held_len = self.held_len.saturating_sub(1);
        }
        let v0 = &mut self.voices[0];
        if self.held_len > 0 {
            if v0.note == note {
                let top = self.held[self.held_len - 1].0;
                v0.retarget(top);
                self.last_pitch = top as f32;
            }
        } else if v0.active && v0.gate {
            v0.release();
        }
    }

    fn settle(&mut self, patch: Patch) {
        self.mono = patch.mode >= 0.5;
        self.legato = patch.mode >= 1.5;
        self.patch = patch;
    }

    pub fn render(&mut self, patch: Patch, l: &mut [f32], r: &mut [f32]) {
        self.settle(patch.clone());
        let p = &self.patch;
        let n = l.len() as u32;
        let sr = self.sr;

        let lfo = lfo_value(p.lfo[0], self.lfo_phase);
        self.lfo_smooth += (lfo - self.lfo_smooth) * 0.4;
        self.lfo_phase = (self.lfo_phase + p.lfo[1] * n as f32 / sr).fract();
        let lfo = self.lfo_smooth;

        let to_pos = p.lfo[2];
        let to_pitch = p.lfo[3];
        let to_cut = p.lfo[4];

        let evo_inc = p.evo[0] * n as f32 / sr;
        let evo_a = self.evo_a.advance(evo_inc);
        let evo_b = self.evo_b.advance(evo_inc * 0.61);
        let evo_table = p.evo[1];
        let evo_morph = p.evo[2];
        let evo_cut = p.evo[3];
        let evo_vox = p.evo[4];
        let chaos_amt = p.chaos;
        let organic = p.organic;
        let chaos = self.organic.process_block(
            n as usize,
            &OrganicParams {
                rate_hz: p.chaos_rate,
                ..OrganicParams::default()
            },
        );
        let morph = (p.morph + evo_b * evo_morph + chaos[1] * chaos_amt * 0.4).clamp(0.0, 1.0);
        let morph_gain = [
            (morph * std::f32::consts::FRAC_PI_2).cos(),
            (morph * std::f32::consts::FRAC_PI_2).sin(),
        ];

        let mut osc = [OscFrame {
            position: 0.0,
            level: 0.0,
            ratio: 1.0,
            unison: 1.0,
            detune_cents: 0.0,
            spread: 0.0,
        }; 2];
        for (o, frame) in osc.iter_mut().enumerate() {
            let semis = p.tune[o] + p.fine[o] / 100.0;
            *frame = OscFrame {
                position: (p.pos[o] + lfo * to_pos + evo_a * evo_table * 0.5 + chaos[0] * chaos_amt * 0.25)
                    .clamp(0.0, 1.0),
                level: p.level[o] * morph_gain[o],
                ratio: 2f32.powf(semis / 12.0),
                unison: p.unison[o],
                detune_cents: p.detune[o] * 50.0,
                spread: p.spread[o],
            };
        }

        let res_frame = ResFrame {
            kind: match p.res[0] as i32 {
                1 => Kind::Pluck,
                2 => Kind::String,
                3 => Kind::Membrane,
                4 => Kind::Plate,
                _ => Kind::Off,
            },
            mix: p.res[1],
            decay: p.res[2],
            damping: p.res[3],
            stiffness: p.res[4],
            strike: p.res[5],
            hardness: p.res[6],
            position: p.res[7],
        };

        let glide = p.glide;
        let glide_coef = if glide <= 0.001 {
            1.0
        } else {
            1.0 - (-4.6 * n as f32 / (sr * glide)).exp()
        };

        let frame = Frame {
            sr,
            table: &self.table,
            osc,
            sub: p.sub * 0.5,
            noise: p.noise * 0.25,
            filter_morph: p.f_mode,
            variation: organic,
            res: res_frame,
            cutoff: p.cutoff,
            resonance: p.resonance,
            drive: p.drive,
            env_amount: p.env_amount,
            lfo_cutoff: lfo * to_cut * 3.0 + evo_b * evo_cut * 2.0 + chaos[2] * chaos_amt * 1.5,
            pitch_mul: 2f32.powf(
                (lfo * to_pitch * 12.0 + chaos[3] * chaos_amt * 0.06) / 12.0,
            ),
            drift_cents: p.drift * 12.0 * (0.5 + 1.5 * organic),
            pos_spread: evo_vox * 0.2,
            glide_coef,
            amp: AdsrSettings::new(p.amp[0], p.amp[1], p.amp[2], p.amp[3], sr),
            filt: AdsrSettings::new(p.filt[0], p.filt[1], p.filt[2], p.filt[3], sr),
        };

        for v in self.voices.iter_mut() {
            v.render(&frame, l, r);
        }

        self.body.process(
            l,
            r,
            &BodyFrame {
                mix: p.body[0],
                size: p.body[1],
                decay: p.body[2],
                damping: p.body[3],
            },
        );

        self.granular.process(
            l,
            r,
            &GrainFrame {
                mix: p.gran[0],
                size: p.gran[1],
                density: p.gran[2],
                position: p.gran[3],
                jitter: p.gran[4],
                pitch: p.gran[5],
                spread: p.gran[6],
                interval: match p.gran[7] as i32 {
                    0 => Interval::Free,
                    2 => Interval::Fifths,
                    _ => Interval::Octaves,
                },
                reverse: p.gran[8],
                width: p.gran[9],
                freeze: p.gran[10] > 0.5,
            },
        );

        self.fx.process(
            l,
            r,
            &FxFrame {
                chorus_mix: p.fx[0],
                chorus_rate: p.fx[1],
                chorus_depth: p.fx[2],
                delay_time: p.fx[3],
                delay_feedback: p.fx[4],
                delay_tone: p.fx[5],
                delay_mix: p.fx[6],
                reverb_mix: p.fx[7],
                reverb_decay: p.fx[8],
                reverb_damping: p.fx[9],
                reverb_predelay: p.fx[10],
            },
        );

        let gain_target = util::db_to_gain(p.gain);
        let gain_start = self.gain_prev;
        self.gain_prev = gain_target;
        let len = l.len() as f32;
        for i in 0..l.len() {
            let g = gain_start + (gain_target - gain_start) * (i + 1) as f32 / len;
            l[i] = soft_clip(l[i] * g);
            r[i] = soft_clip(r[i] * g);
        }
    }
}
