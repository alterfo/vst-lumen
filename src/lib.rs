mod body;
mod bridge;
mod chaos;
mod envelope;
mod evolver;
mod filter;
mod fx;
mod granular;
mod gui;
mod midi;

mod params;
mod presets;

mod resonator;
mod voice;
mod wavetable;

use body::{Body, BodyFrame};
use bridge::{Bridge, MidiShared};
use chaos::{OrganicModulator, OrganicParams};
use envelope::AdsrSettings;
use evolver::Evolver;
use filter::soft_clip;
use fx::{Fx, FxFrame};
use granular::{GrainFrame, Granular, Interval};
use nih_plug::prelude::*;
use params::{GrainInterval, LumenParams, ResModel, VoiceMode};
use resonator::{Kind, ResFrame};
use std::f32::consts::{FRAC_PI_2, TAU};
use std::num::NonZeroU32;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use voice::{Frame, OscFrame, Voice};
use wavetable::WaveTable;

const VOICES: usize = 16;
const BLOCK: usize = 32;
const HELD: usize = 16;

pub struct Shared {
    pub midi: Arc<MidiShared>,
    pub position: [AtomicF32; 2],
    pub morph: AtomicF32,
    pub peak: AtomicF32,
}

impl Shared {
    fn new(midi: Arc<MidiShared>) -> Self {
        Self {
            midi,
            position: [AtomicF32::new(0.0), AtomicF32::new(0.0)],
            morph: AtomicF32::new(0.0),
            peak: AtomicF32::new(0.0),
        }
    }
}

pub struct Lumen {
    params: Arc<LumenParams>,
    table: Arc<WaveTable>,
    shared: Arc<Shared>,
    bridge: Bridge,
    gain_prev: f32,
    voices: Vec<Voice>,
    fx: Fx,
    body: Body,
    granular: Granular,
    sample_rate: f32,
    counter: u64,
    lfo_phase: f32,
    lfo_smooth: f32,
    bend: f32,
    evo_a: Evolver,
    evo_b: Evolver,
    organic: OrganicModulator,
    held: [(u8, f32); HELD],
    held_len: usize,
    last_pitch: f32,
    has_last: bool,
}

impl Default for Lumen {
    fn default() -> Self {
        let params = Arc::new(LumenParams::default());
        let midi = Arc::new(MidiShared::new(&params));
        Self {
            bridge: Bridge::new(params.clone(), midi.clone()),
            gain_prev: 0.5,
            params,
            table: Arc::new(WaveTable::new()),
            shared: Arc::new(Shared::new(midi)),
            voices: vec![Voice::new(); VOICES],
            fx: Fx::new(44100.0),
            body: Body::new(44100.0),
            granular: Granular::new(44100.0),
            sample_rate: 44100.0,
            counter: 0,
            lfo_phase: 0.0,
            lfo_smooth: 0.0,
            bend: 0.0,
            evo_a: Evolver::new(0x9E3779B9),
            evo_b: Evolver::new(0x85EBCA6B),
            organic: OrganicModulator::new(0xC0FFEE),
            held: [(0, 0.0); HELD],
            held_len: 0,
            last_pitch: 60.0,
            has_last: false,
        }
    }
}

fn lfo_value(shape: f32, ph: f32) -> f32 {
    let sine = (ph * TAU).sin();
    let tri = 1.0 - 4.0 * (ph - 0.5).abs();
    let saw = 1.0 - 2.0 * ph;
    let square = if ph < 0.5 { 1.0 } else { -1.0 };
    let s = shape.clamp(0.0, 1.0) * 3.0;
    if s < 1.0 {
        sine + (tri - sine) * s
    } else if s < 2.0 {
        tri + (saw - tri) * (s - 1.0)
    } else {
        saw + (square - saw) * (s - 2.0)
    }
}

impl Lumen {
    fn set_sample_rate(&mut self, sr: f32) {
        self.sample_rate = sr;
        self.fx = Fx::new(sr);
        self.body = Body::new(sr);
        self.granular = Granular::new(sr);
        self.organic.prepare(sr);
        self.bridge.prepare(sr);
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

    fn note_on(&mut self, note: u8, velocity: f32) {
        let mode = self.params.voice_mode.value();
        let glide_on = self.params.glide.value() > 0.001;
        self.note_on_with(note, velocity, mode, glide_on);
    }

    fn note_on_with(&mut self, note: u8, velocity: f32, mode: VoiceMode, glide_on: bool) {
        self.counter += 1;
        let seed = (self.counter as u32).wrapping_mul(2654435761) ^ (note as u32) << 8;
        let start_pitch = if glide_on && self.has_last {
            self.last_pitch
        } else {
            note as f32
        };
        match mode {
            VoiceMode::Poly => {
                let idx = self.free_voice();
                self.voices[idx].start(note, velocity, self.counter, seed, start_pitch);
            }
            mode => {
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
                if mode == VoiceMode::Legato && v0.active && v0.gate {
                    v0.retarget(note);
                } else {
                    let from = if v0.active { v0.pitch() } else { start_pitch };
                    v0.start(note, velocity, self.counter, seed, from);
                }
            }
        }
        self.has_last = true;
        self.last_pitch = note as f32;
    }

    fn note_off(&mut self, note: u8) {
        let mode = self.params.voice_mode.value();
        self.note_off_with(note, mode);
    }

    fn note_off_with(&mut self, note: u8, mode: VoiceMode) {
        match mode {
            VoiceMode::Poly => {
                for v in self.voices.iter_mut() {
                    if v.active && v.gate && v.note == note {
                        v.release();
                    }
                }
            }
            _ => {
                if let Some(i) = (0..self.held_len).rev().find(|&i| self.held[i].0 == note) {
                    for j in i..self.held_len - 1 {
                        self.held[j] = self.held[j + 1];
                    }
                    self.held_len -= 1;
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
        }
    }

    fn render_block(&mut self, l: &mut [f32], r: &mut [f32]) {
        let n = l.len() as u32;
        let p = self.params.clone();
        self.bridge.pre_block(l.len());
        let sr = self.sample_rate;

        let lfo = lfo_value(self.bridge.eff(&p.lfo.shape, n), self.lfo_phase);
        self.lfo_smooth += (lfo - self.lfo_smooth) * 0.4;
        self.lfo_phase = (self.lfo_phase + self.bridge.val(&p.lfo.rate) * n as f32 / sr).fract();
        let lfo = self.lfo_smooth;

        let to_pos = self.bridge.eff(&p.lfo.to_position, n);
        let to_pitch = self.bridge.eff(&p.lfo.to_pitch, n);
        let to_cut = self.bridge.eff(&p.lfo.to_cutoff, n);

        let evo_inc = self.bridge.val(&p.evolve.rate) * n as f32 / sr;
        let evo_a = self.evo_a.advance(evo_inc);
        let evo_b = self.evo_b.advance(evo_inc * 0.61);
        let evo_table = self.bridge.eff(&p.evolve.to_table, n);
        let evo_morph = self.bridge.eff(&p.evolve.to_morph, n);
        let evo_cut = self.bridge.eff(&p.evolve.to_cutoff, n);
        let evo_vox = self.bridge.eff(&p.evolve.voice_spread, n);
        let chaos_amt = self.bridge.eff(&p.macros.chaos, n);
        let organic = self.bridge.eff(&p.macros.organic, n);
        let chaos = self.organic.process_block(
            n as usize,
            &OrganicParams {
                rate_hz: self.bridge.val(&p.macros.chaos_rate),
                ..OrganicParams::default()
            },
        );
        let morph = (self.bridge.eff(&p.morph, n) + evo_b * evo_morph + chaos[1] * chaos_amt * 0.4)
            .clamp(0.0, 1.0);
        let morph_gain = [(morph * FRAC_PI_2).cos(), (morph * FRAC_PI_2).sin()];

        let osc_frame = |o: &params::OscParams| {
            let semis = self.bridge.eff(&o.tune, n) + self.bridge.val(&o.fine) / 100.0;
            OscFrame {
                position: (self.bridge.eff(&o.position, n) + lfo * to_pos + evo_a * evo_table * 0.5 + chaos[0] * chaos_amt * 0.25)
                    .clamp(0.0, 1.0),
                level: self.bridge.eff(&o.level, n),
                ratio: 2f32.powf(semis / 12.0),
                unison: self.bridge.eff(&o.unison, n),
                detune_cents: self.bridge.eff(&o.detune, n) * 50.0,
                spread: self.bridge.eff(&o.spread, n),
            }
        };
        let mut osc = [osc_frame(&p.osc1), osc_frame(&p.osc2)];
        osc[0].level *= morph_gain[0];
        osc[1].level *= morph_gain[1];

        self.shared.position[0].store(osc[0].position, Ordering::Relaxed);
        self.shared.position[1].store(osc[1].position, Ordering::Relaxed);
        self.shared.morph.store(morph, Ordering::Relaxed);

        let rp = &p.resonator;
        let res_frame = ResFrame {
            kind: match rp.model.value() {
                ResModel::Off => Kind::Off,
                ResModel::Pluck => Kind::Pluck,
                ResModel::String => Kind::String,
                ResModel::Membrane => Kind::Membrane,
                ResModel::Plate => Kind::Plate,
            },
            mix: self.bridge.eff(&rp.mix, n),
            decay: self.bridge.eff(&rp.decay, n),
            damping: self.bridge.eff(&rp.damping, n),
            stiffness: self.bridge.eff(&rp.stiffness, n),
            strike: self.bridge.eff(&rp.strike, n),
            hardness: self.bridge.eff(&rp.hardness, n),
            position: self.bridge.eff(&rp.position, n),
        };

        let glide = self.bridge.val(&p.glide);
        let glide_coef = if glide <= 0.001 {
            1.0
        } else {
            1.0 - (-4.6 * n as f32 / (sr * glide)).exp()
        };

        let frame = Frame {
            sr,
            table: &self.table,
            osc,
            sub: self.bridge.eff(&p.sub, n) * 0.5,
            noise: self.bridge.eff(&p.noise, n) * 0.25,
            filter_morph: self.bridge.eff(&p.filter.mode, n),
            variation: organic,
            res: res_frame,
            cutoff: self.bridge.eff(&p.filter.cutoff, n),
            resonance: self.bridge.eff(&p.filter.resonance, n),
            drive: self.bridge.eff(&p.filter.drive, n),
            env_amount: self.bridge.eff(&p.filter.env_amount, n),
            lfo_cutoff: lfo * to_cut * 3.0 + evo_b * evo_cut * 2.0 + chaos[2] * chaos_amt * 1.5,
            pitch_mul: 2f32.powf((self.bend * 2.0 + lfo * to_pitch * 12.0 + chaos[3] * chaos_amt * 0.06) / 12.0),
            drift_cents: self.bridge.eff(&p.drift, n) * 12.0 * (0.5 + 1.5 * organic),
            pos_spread: evo_vox * 0.2,
            glide_coef,
            amp: AdsrSettings::new(
                self.bridge.val(&p.amp_env.attack),
                self.bridge.val(&p.amp_env.decay),
                self.bridge.eff(&p.amp_env.sustain, n),
                self.bridge.val(&p.amp_env.release),
                sr,
            ),
            filt: AdsrSettings::new(
                self.bridge.val(&p.filt_env.attack),
                self.bridge.val(&p.filt_env.decay),
                self.bridge.eff(&p.filt_env.sustain, n),
                self.bridge.val(&p.filt_env.release),
                sr,
            ),
        };

        for v in self.voices.iter_mut() {
            v.render(&frame, l, r);
        }

        let bp = &p.body;
        self.body.process(
            l,
            r,
            &BodyFrame {
                mix: self.bridge.eff(&bp.mix, n),
                size: self.bridge.eff(&bp.size, n),
                decay: self.bridge.eff(&bp.decay, n),
                damping: self.bridge.eff(&bp.damping, n),
            },
        );

        let gp = &p.granular;
        let grain_frame = GrainFrame {
            mix: self.bridge.eff(&gp.mix, n),
            size: self.bridge.eff(&gp.size, n) * 2f32.powf(chaos[2] * chaos_amt * 0.5),
            density: self.bridge.eff(&gp.density, n),
            position: self.bridge.eff(&gp.position, n) * 2f32.powf(chaos[1] * chaos_amt * 0.8),
            jitter: self.bridge.eff(&gp.jitter, n),
            pitch: self.bridge.eff(&gp.pitch, n),
            spread: self.bridge.eff(&gp.pitch_spread, n),
            interval: match gp.interval.value() {
                GrainInterval::Free => Interval::Free,
                GrainInterval::Octaves => Interval::Octaves,
                GrainInterval::Fifths => Interval::Fifths,
            },
            reverse: self.bridge.eff(&gp.reverse, n),
            width: self.bridge.eff(&gp.stereo, n),
            freeze: gp.freeze.value(),
        };
        self.granular.process(l, r, &grain_frame);

        let fx = &p.fx;
        let fx_frame = FxFrame {
            chorus_mix: self.bridge.eff(&fx.chorus_mix, n),
            chorus_rate: self.bridge.eff(&fx.chorus_rate, n),
            chorus_depth: self.bridge.eff(&fx.chorus_depth, n),
            delay_time: self.bridge.eff(&fx.delay_time, n),
            delay_feedback: self.bridge.eff(&fx.delay_feedback, n),
            delay_tone: self.bridge.eff(&fx.delay_tone, n),
            delay_mix: self.bridge.eff(&fx.delay_mix, n),
            reverb_mix: self.bridge.eff(&fx.reverb_mix, n),
            reverb_decay: self.bridge.eff(&fx.reverb_decay, n),
            reverb_damping: self.bridge.eff(&fx.reverb_damping, n),
            reverb_predelay: self.bridge.eff(&fx.reverb_predelay, n),
        };
        self.fx.process(l, r, &fx_frame);

        let mut peak = 0.0f32;
        let gain_target = self.bridge.eff(&p.gain, n);
        let gain_start = self.gain_prev;
        self.gain_prev = gain_target;
        let len = l.len() as f32;
        for i in 0..l.len() {
            let g = gain_start + (gain_target - gain_start) * (i + 1) as f32 / len;
            l[i] = soft_clip(l[i] * g);
            r[i] = soft_clip(r[i] * g);
            peak = peak.max(l[i].abs()).max(r[i].abs());
        }
        let prev = self.shared.peak.load(Ordering::Relaxed);
        self.shared
            .peak
            .store(if peak > prev { peak } else { prev * 0.98 }, Ordering::Relaxed);
    }
}

impl Plugin for Lumen {
    const NAME: &'static str = "Lumen";
    const VENDOR: &'static str = "Lumen Synth";
    const URL: &'static str = "";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: None,
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];

    const MIDI_INPUT: MidiConfig = MidiConfig::MidiCCs;
    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        gui::create(self.params.clone(), self.table.clone(), self.shared.clone())
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        self.set_sample_rate(buffer_config.sample_rate);
        true
    }

    fn reset(&mut self) {
        for v in self.voices.iter_mut() {
            *v = Voice::new();
        }
        self.held_len = 0;
        self.has_last = false;
        self.lfo_phase = 0.0;
        self.bend = 0.0;
        self.fx = Fx::new(self.sample_rate);
        self.body = Body::new(self.sample_rate);
        self.granular = Granular::new(self.sample_rate);
        self.organic.reset();
        self.bridge.reset();
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let total = buffer.samples();
        let out = buffer.as_slice();
        let (left, right) = out.split_at_mut(1);
        let left = &mut *left[0];
        let right = &mut *right[0];

        let mut next_event = context.next_event();
        let mut start = 0;
        let mut end = BLOCK.min(total);
        while start < total {
            'events: loop {
                match next_event {
                    Some(event) if (event.timing() as usize) <= start => {
                        match event {
                            NoteEvent::NoteOn { note, velocity, .. } => self.note_on(note, velocity),
                            NoteEvent::NoteOff { note, .. } => self.note_off(note),
                            NoteEvent::MidiPitchBend { value, .. } => self.bend = value * 2.0 - 1.0,
                            NoteEvent::MidiCC { timing, channel, cc, value, .. } => {
                                self.bridge.on_cc(timing as usize, channel, cc, value)
                            }
                            _ => {}
                        }
                        next_event = context.next_event();
                    }
                    Some(event) if (event.timing() as usize) < end => {
                        end = event.timing() as usize;
                        break 'events;
                    }
                    _ => break 'events,
                }
            }

            left[start..end].fill(0.0);
            right[start..end].fill(0.0);
            self.render_block(&mut left[start..end], &mut right[start..end]);

            start = end;
            end = (start + BLOCK).min(total);
        }

        self.bridge.advance_clock(total);
        ProcessStatus::KeepAlive
    }
}

impl ClapPlugin for Lumen {
    const CLAP_ID: &'static str = "org.lumen-synth.lumen";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("Warm wavetable synthesizer");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::Instrument,
        ClapFeature::Synthesizer,
        ClapFeature::Stereo,
    ];
}

impl Vst3Plugin for Lumen {
    const VST3_CLASS_ID: [u8; 16] = *b"LumenWavetableSy";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Instrument, Vst3SubCategory::Synth];
}

nih_export_clap!(Lumen);
nih_export_vst3!(Lumen);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_finite_bounded_audio() {
        let mut synth = Lumen::default();
        synth.set_sample_rate(48000.0);
        for (_, ptr, _) in synth.params.param_map() {
            if let ParamPtr::FloatParam(p) = ptr {
                unsafe { (*p).smoothed.reset((*p).value()) };
            }
        }
        synth.note_on(57, 0.9);
        synth.note_on(64, 0.9);
        let mut peak = 0.0f32;
        for block in 0..(48000 / BLOCK) {
            if block == 600 {
                synth.note_off(57);
                synth.note_off(64);
            }
            let mut l = [0.0f32; BLOCK];
            let mut r = [0.0f32; BLOCK];
            synth.render_block(&mut l, &mut r);
            for s in l.iter().chain(r.iter()) {
                assert!(s.is_finite());
                peak = peak.max(s.abs());
            }
        }
        assert!(peak > 0.01 && peak < 1.5, "peak {peak}");
    }

    #[test]
    fn midi_learn_and_turn_moves_cutoff() {
        let mut s = Lumen::default();
        s.set_sample_rate(48000.0);
        for (_, ptr, _) in s.params.param_map() {
            if let ParamPtr::FloatParam(p) = ptr {
                unsafe { (*p).smoothed.reset((*p).value()) };
            }
        }
        let slot = s.shared.midi.slots.slot_of(&s.params.filter.cutoff).unwrap();
        let start = s.shared.midi.slots.host_norm(slot);
        {
            let mut l = [0.0f32; BLOCK];
            let mut r = [0.0f32; BLOCK];
            s.render_block(&mut l, &mut r);
        }
        s.shared.midi.request_learn(slot);
        for _ in 0..40 {
            s.bridge.on_cc(0, 0, 74, 3.0 / 127.0);
            s.bridge.advance_clock(144);
            let mut l = [0.0f32; BLOCK];
            let mut r = [0.0f32; BLOCK];
            s.render_block(&mut l, &mut r);
        }
        for _ in 0..400 {
            let mut l = [0.0f32; BLOCK];
            let mut r = [0.0f32; BLOCK];
            s.render_block(&mut l, &mut r);
            s.bridge.advance_clock(BLOCK);
        }
        use std::sync::atomic::Ordering;
        assert!(s.shared.midi.owned[slot].load(Ordering::Relaxed));
        let now = s.shared.midi.value[slot].load(Ordering::Relaxed);
        assert!(now > start, "start {start} now {now}");
        assert_eq!(s.shared.midi.learn.load(Ordering::Relaxed), 0);
        assert!(s.shared.midi.snapshot_seq.load(Ordering::Relaxed) > 0);
        assert_eq!(s.shared.midi.snapshot().len(), 1);
    }

    #[test]
    fn mip_levels_stay_below_nyquist() {
        let t = WaveTable::new();
        assert_eq!(t.level_for(20.0 / 48000.0), 0);
        assert!(t.level_for(8000.0 / 48000.0) >= 5);
    }
}

#[cfg(test)]
mod render {
    use super::*;

    #[test]
    #[ignore]
    fn render_demo() {
        let path = match std::env::var("LUMEN_WAV") {
            Ok(p) => p,
            Err(_) => return,
        };
        let sr = 48000u32;
        let mut s = Lumen::default();
        s.set_sample_rate(sr as f32);
        for (_, ptr, _) in s.params.param_map() {
            if let ParamPtr::FloatParam(p) = ptr {
                unsafe { (*p).smoothed.reset((*p).value()) };
            }
        }
        let mut pcm: Vec<i16> = Vec::new();
        for b in 0..(sr as usize * 12 / BLOCK) {
            if b == 0 {
                s.note_on(45, 0.8);
                s.note_on(52, 0.8);
                s.note_on(57, 0.8);
                s.note_on(60, 0.8);
            }
            if b == sr as usize * 9 / BLOCK {
                for n in [45, 52, 57, 60] {
                    s.note_off(n);
                }
            }
            let mut l = [0.0f32; BLOCK];
            let mut r = [0.0f32; BLOCK];
            s.render_block(&mut l, &mut r);
            for i in 0..BLOCK {
                pcm.push((l[i].clamp(-1.0, 1.0) * 32767.0) as i16);
                pcm.push((r[i].clamp(-1.0, 1.0) * 32767.0) as i16);
            }
        }
        let mut out = Vec::new();
        let data_len = (pcm.len() * 2) as u32;
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + data_len).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&sr.to_le_bytes());
        out.extend_from_slice(&(sr * 4).to_le_bytes());
        out.extend_from_slice(&4u16.to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&data_len.to_le_bytes());
        for v in pcm {
            out.extend_from_slice(&v.to_le_bytes());
        }
        std::fs::write(path, out).unwrap();
    }
}

#[cfg(test)]
mod stuck {
    use super::*;

    fn fresh() -> Lumen {
        let mut s = Lumen::default();
        s.set_sample_rate(48000.0);
        for (_, ptr, _) in s.params.param_map() {
            if let ParamPtr::FloatParam(p) = ptr {
                unsafe { (*p).smoothed.reset((*p).value()) };
            }
        }
        s
    }

    fn run(s: &mut Lumen, seconds: f32) {
        for _ in 0..(48000.0 * seconds / BLOCK as f32) as usize {
            let mut l = [0.0f32; BLOCK];
            let mut r = [0.0f32; BLOCK];
            s.render_block(&mut l, &mut r);
        }
    }

    fn active(s: &Lumen) -> usize {
        s.voices.iter().filter(|v| v.active).count()
    }

    const MODES: [VoiceMode; 3] = [VoiceMode::Poly, VoiceMode::Mono, VoiceMode::Legato];

    #[test]
    fn every_pairing_order_releases_everything() {
        let orders: [&[(bool, u8)]; 6] = [
            &[(true, 60), (false, 60)],
            &[(true, 60), (true, 67), (false, 60), (false, 67)],
            &[(true, 60), (true, 67), (false, 67), (false, 60)],
            &[(true, 60), (true, 64), (true, 67), (false, 64), (false, 60), (false, 67)],
            &[(true, 60), (false, 60), (true, 67), (false, 67), (true, 72), (false, 72)],
            &[(true, 60), (true, 60), (false, 60), (false, 60)],
        ];
        for mode in MODES {
            for glide_on in [false, true] {
                for (n, order) in orders.iter().enumerate() {
                    let mut s = fresh();
                    for &(on, note) in order.iter() {
                        if on {
                            s.note_on_with(note, 0.8, mode, glide_on);
                        } else {
                            s.note_off_with(note, mode);
                        }
                        run(&mut s, 0.05);
                    }
                    run(&mut s, 6.0);
                    assert_eq!(
                        active(&s),
                        0,
                        "mode {} glide {glide_on} order {n}: voices still active",
                        match mode {
                            VoiceMode::Poly => "poly",
                            VoiceMode::Mono => "mono",
                            VoiceMode::Legato => "legato",
                        }
                    );
                }
            }
        }
    }

    #[test]
    fn pitch_settles_on_the_pressed_note() {
        for mode in [VoiceMode::Mono, VoiceMode::Legato] {
            let mut s = fresh();
            let mut expected = 0u8;
            for note in [48u8, 55, 60, 52, 67, 40] {
                s.note_on_with(note, 0.8, mode, true);
                expected = note;
                run(&mut s, 0.02);
            }
            run(&mut s, 3.0);
            let v = &s.voices[0];
            assert_eq!(v.note, expected);
            assert!((v.pitch() - expected as f32).abs() < 0.3, "pitch {} vs {expected}", v.pitch());
        }
    }
}
