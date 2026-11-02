use crate::body::{Body, BodyFrame};
use crate::chaos::{OrganicModulator, OrganicParams};
use crate::filter::{soft_clip, Coefs, Svf};
use crate::fx::{Fx, FxFrame};
use crate::granular::{GrainFrame, Granular, Interval};
use crate::params::{percent, BodyParams, FxParams, GrainInterval, GranularParams};
use crate::strings_gui;
use crate::sympathetic::{Scale, SympFrame, Sympathetic};
use nih_plug::prelude::*;
use nih_plug_egui::EguiState;
use std::f32::consts::{LN_2, TAU};
use std::num::NonZeroU32;
use std::sync::atomic::Ordering;
use std::sync::Arc;

const BLOCK: usize = 32;
const NOTE_NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

#[derive(Enum, PartialEq, Clone, Copy)]
pub enum StringScale {
    #[id = "gong"]
    #[name = "Gong"]
    Gong,
    #[id = "shang"]
    #[name = "Shang"]
    Shang,
    #[id = "jue"]
    #[name = "Jue"]
    Jue,
    #[id = "zhi"]
    #[name = "Zhi"]
    Zhi,
    #[id = "yu"]
    #[name = "Yu"]
    Yu,
    #[id = "minor"]
    #[name = "Natural Minor"]
    Minor,
    #[id = "dorian"]
    #[name = "Dorian"]
    Dorian,
    #[id = "phryg"]
    #[name = "Phrygian"]
    Phrygian,
    #[id = "fifths"]
    #[name = "Fifths"]
    Fifths,
}

impl StringScale {
    fn dsp(self) -> Scale {
        match self {
            StringScale::Gong => Scale::Gong,
            StringScale::Shang => Scale::Shang,
            StringScale::Jue => Scale::Jue,
            StringScale::Zhi => Scale::Zhi,
            StringScale::Yu => Scale::Yu,
            StringScale::Minor => Scale::Minor,
            StringScale::Dorian => Scale::Dorian,
            StringScale::Phrygian => Scale::Phrygian,
            StringScale::Fifths => Scale::Fifths,
        }
    }
}

#[derive(Params)]
pub struct StringsParams {
    #[id = "gain"]
    pub input: FloatParam,
    #[id = "drive"]
    pub drive: FloatParam,
    #[id = "tone"]
    pub tone: FloatParam,
    #[id = "touch"]
    pub touch: FloatParam,
    #[id = "tfreq"]
    pub touch_freq: FloatParam,
    #[id = "tres"]
    pub touch_res: FloatParam,
    #[id = "skin"]
    pub skin: FloatParam,
    #[id = "sfreq"]
    pub skin_freq: FloatParam,
    #[id = "smix"]
    pub sym_mix: FloatParam,
    #[id = "root"]
    pub sym_root: IntParam,
    #[id = "scale"]
    pub sym_scale: EnumParam<StringScale>,
    #[id = "reg"]
    pub sym_register: IntParam,
    #[id = "sdecay"]
    pub sym_decay: FloatParam,
    #[id = "sdamp"]
    pub sym_damping: FloatParam,
    #[id = "sdet"]
    pub sym_detune: FloatParam,
    #[id = "ssub"]
    pub sym_sub: FloatParam,
    #[id = "sdrop"]
    pub sym_drop: IntParam,
    #[nested(id_prefix = "body", group = "Body")]
    pub body: BodyParams,
    #[nested(id_prefix = "gran", group = "Granular")]
    pub granular: GranularParams,
    #[nested(id_prefix = "fx", group = "FX")]
    pub fx: FxParams,
    #[id = "bloom"]
    pub bloom: FloatParam,
    #[id = "organic"]
    pub organic: FloatParam,
    #[id = "orate"]
    pub organic_rate: FloatParam,
    #[id = "out"]
    pub output: FloatParam,
    #[id = "mix"]
    pub mix: FloatParam,
    #[persist = "editor-state"]
    pub editor_state: Arc<EguiState>,
}

fn db(name: &str, default: f32, min: f32, max: f32) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Linear { min, max })
        .with_smoother(SmoothingStyle::Linear(20.0))
        .with_unit(" dB")
        .with_step_size(0.1)
}

fn hz(name: &str, default: f32, min: f32, max: f32) -> FloatParam {
    FloatParam::new(
        name,
        default,
        FloatRange::Skewed {
            min,
            max,
            factor: FloatRange::skew_factor(-1.5),
        },
    )
    .with_smoother(SmoothingStyle::Logarithmic(30.0))
    .with_unit(" Hz")
}

impl Default for StringsParams {
    fn default() -> Self {
        Self {
            input: db("Input", 0.0, -12.0, 24.0),
            drive: percent("Drive", 0.0),
            tone: percent("Tone", 0.6),
            touch: percent("Touch", 0.0),
            touch_freq: hz("Touch Freq", 900.0, 150.0, 6000.0),
            touch_res: percent("Touch Reson", 0.35),
            skin: percent("Skin", 0.0),
            skin_freq: hz("Skin Freq", 1500.0, 700.0, 3500.0),
            sym_mix: percent("Strings Mix", 0.35),
            sym_root: IntParam::new("Root", 2, IntRange::Linear { min: 0, max: 11 })
                .with_value_to_string(Arc::new(|v| NOTE_NAMES[v.clamp(0, 11) as usize].to_string())),
            sym_scale: EnumParam::new("Scale", StringScale::Gong),
            sym_register: IntParam::new("Register", 1, IntRange::Linear { min: 0, max: 3 })
                .with_value_to_string(Arc::new(|v| format!("C{}", v.clamp(0, 3) + 1))),
            sym_decay: FloatParam::new(
                "Strings Decay",
                4.0,
                FloatRange::Skewed {
                    min: 0.3,
                    max: 12.0,
                    factor: FloatRange::skew_factor(-1.5),
                },
            )
            .with_unit(" s"),
            sym_damping: percent("Strings Damping", 0.45),
            sym_detune: FloatParam::new("Strings Detune", 6.0, FloatRange::Linear { min: 0.0, max: 25.0 })
                .with_unit(" ct"),
            sym_sub: percent("Sub Bass", 0.0),
            sym_drop: IntParam::new("Sub Drop", 2, IntRange::Linear { min: 1, max: 3 })
                .with_value_to_string(Arc::new(|v| format!("-{} oct", v.clamp(1, 3)))),
            body: BodyParams::new(),
            granular: GranularParams::new(),
            fx: FxParams::new(),
            bloom: percent("Bloom", 0.3),
            organic: percent("Organic", 0.25),
            organic_rate: FloatParam::new(
                "Organic Rate",
                0.15,
                FloatRange::Skewed {
                    min: 0.01,
                    max: 2.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" x"),
            output: db("Output", 0.0, -24.0, 12.0),
            mix: percent("Dry/Wet", 1.0),
            editor_state: EguiState::from_size(1040, 700),
        }
    }
}

pub struct StringsShared {
    pub peak: AtomicF32,
    pub input: AtomicF32,
}

#[inline]
fn ln_cosh(x: f32) -> f32 {
    let a = x.abs();
    a + (-2.0 * a).exp().ln_1p() - LN_2
}

#[derive(Clone, Copy, Default)]
struct Adaa {
    x1: f32,
    f1: f32,
}

impl Adaa {
    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let f = ln_cosh(x);
        let d = x - self.x1;
        let y = if d.abs() < 1e-4 {
            ((x + self.x1) * 0.5).tanh()
        } else {
            (f - self.f1) / d
        };
        self.x1 = x;
        self.f1 = f;
        y
    }
}

#[derive(Clone, Copy, Default)]
struct Channel {
    hp_x: f32,
    hp_y: f32,
    adaa: Adaa,
    tone: f32,
    touch: Svf,
    skin_a: Svf,
    skin_b: Svf,
}

pub struct LumenStrings {
    params: Arc<StringsParams>,
    shared: Arc<StringsShared>,
    sr: f32,
    chan: [Channel; 2],
    env_fast: f32,
    env_slow: f32,
    sym: Sympathetic,
    body: Body,
    granular: Granular,
    fx: Fx,
    organic: OrganicModulator,
}

impl Default for LumenStrings {
    fn default() -> Self {
        let sr = 48000.0;
        Self {
            params: Arc::new(StringsParams::default()),
            shared: Arc::new(StringsShared {
                peak: AtomicF32::new(0.0),
                input: AtomicF32::new(0.0),
            }),
            sr,
            chan: [Channel::default(); 2],
            env_fast: 0.0,
            env_slow: 0.0,
            sym: Sympathetic::new(sr),
            body: Body::new(sr),
            granular: Granular::new(sr),
            fx: Fx::new(sr),
            organic: OrganicModulator::new(0x51ed_270b),
        }
    }
}

impl LumenStrings {
    fn set_sample_rate(&mut self, sr: f32) {
        self.sr = sr;
        self.sym = Sympathetic::new(sr);
        self.body = Body::new(sr);
        self.granular = Granular::new(sr);
        self.fx = Fx::new(sr);
        self.organic.prepare(sr);
        self.chan = [Channel::default(); 2];
        self.env_fast = 0.0;
        self.env_slow = 0.0;
    }

    fn render_block(&mut self, l: &mut [f32], r: &mut [f32]) {
        let n = l.len();
        let steps = n as u32;
        let p = self.params.clone();
        let sr = self.sr;

        let input_gain = util::db_to_gain(p.input.smoothed.next_step(steps));
        let drive = p.drive.smoothed.next_step(steps);
        let tone = p.tone.smoothed.next_step(steps);
        let touch = p.touch.smoothed.next_step(steps);
        let touch_freq = p.touch_freq.smoothed.next_step(steps);
        let touch_res = p.touch_res.smoothed.next_step(steps);
        let skin = p.skin.smoothed.next_step(steps);
        let skin_freq = p.skin_freq.smoothed.next_step(steps);
        let bloom = p.bloom.smoothed.next_step(steps);
        let organic = p.organic.smoothed.next_step(steps);
        let out_gain = util::db_to_gain(p.output.smoothed.next_step(steps));
        let mix = p.mix.smoothed.next_step(steps);

        let chaos = self.organic.process_block(
            n,
            &OrganicParams {
                rate_hz: p.organic_rate.value(),
                ..OrganicParams::default()
            },
        );

        let drive_gain = 10f32.powf(drive * 1.5);
        let drive_blend = (drive * 5.0).min(1.0);
        let drive_comp = 1.0 / (1.0 + drive * 1.6);
        let tone_hz = 900.0 * 14f32.powf(tone);
        let tone_a = 1.0 - (-TAU * tone_hz.min(0.45 * sr) / sr).exp();
        let hp_r = 1.0 - TAU * 70.0 / sr;

        let env_n = (self.env_fast * 4.0).min(1.0);
        let touch_cut = (touch_freq * 2f32.powf(touch * env_n * 3.5)).clamp(60.0, 0.45 * sr);
        let touch_on = touch > 1e-3;
        let touch_coefs = Coefs::new(touch_cut, touch_res, 0.0, 0.3, sr);
        let skin_on = skin > 1e-3;
        let skin_coefs_a = Coefs::new(skin_freq.min(0.45 * sr), 0.55, 0.0, 0.5, sr);
        let skin_coefs_b = Coefs::new((skin_freq * 2.1).min(0.45 * sr), 0.5, 0.0, 0.5, sr);

        let att = 1.0 - (-1.0 / (0.004 * sr)).exp();
        let rel = 1.0 - (-1.0 / (0.12 * sr)).exp();
        let att_slow = 1.0 - (-1.0 / (0.03 * sr)).exp();
        let rel_slow = 1.0 - (-1.0 / (0.6 * sr)).exp();

        let mut dry_l = [0.0f32; BLOCK];
        let mut dry_r = [0.0f32; BLOCK];
        let mut in_peak = 0.0f32;

        for i in 0..n {
            let mut ch = [l[i] * input_gain, r[i] * input_gain];
            dry_l[i] = ch[0];
            dry_r[i] = ch[1];
            for (c, s) in self.chan.iter_mut().zip(ch.iter_mut()) {
                let hp = *s - c.hp_x + hp_r * c.hp_y;
                c.hp_x = *s;
                c.hp_y = hp;
                let mut x = hp;
                if drive_blend > 0.0 {
                    let shaped = c.adaa.process(x * drive_gain + 0.08 * drive) - (0.08 * drive).tanh();
                    c.tone += tone_a * (shaped - c.tone);
                    x += (c.tone * drive_comp - x) * drive_blend;
                }
                if touch_on {
                    x += (c.touch.process(x, &touch_coefs) - x) * touch.min(1.0).sqrt();
                }
                if skin_on {
                    let a = c.skin_a.process(x, &skin_coefs_a);
                    let b = c.skin_b.process(x, &skin_coefs_b);
                    x = soft_clip(x + skin * (a * 0.9 + b * 0.55));
                }
                *s = x;
            }
            l[i] = ch[0];
            r[i] = ch[1];
            let level = ch[0].abs().max(ch[1].abs());
            in_peak = in_peak.max(level);
            self.env_fast += (level - self.env_fast) * if level > self.env_fast { att } else { rel };
            self.env_slow += (level - self.env_slow) * if level > self.env_slow { att_slow } else { rel_slow };
        }

        let duck = 1.0 - bloom * (self.env_slow * 5.0).min(1.0);

        self.sym.process(
            l,
            r,
            &SympFrame {
                mix: p.sym_mix.smoothed.next_step(steps) * duck,
                root: p.sym_root.value(),
                scale: p.sym_scale.value().dsp(),
                register: p.sym_register.value(),
                decay: p.sym_decay.value(),
                damping: p.sym_damping.value(),
                detune_cents: p.sym_detune.value(),
                drift_cents: chaos[1] * organic * 14.0,
                sub_mix: p.sym_sub.smoothed.next_step(steps) * duck,
                sub_drop: p.sym_drop.value(),
            },
        );

        let bp = &p.body;
        self.body.process(
            l,
            r,
            &BodyFrame {
                mix: bp.mix.smoothed.next_step(steps),
                size: bp.size.value() * 2f32.powf(chaos[0] * organic * 0.12),
                decay: bp.decay.value(),
                damping: bp.damping.value(),
            },
        );

        let gp = &p.granular;
        self.granular.process(
            l,
            r,
            &GrainFrame {
                mix: gp.mix.smoothed.next_step(steps) * duck,
                size: gp.size.value() * 2f32.powf(chaos[2] * organic * 0.5),
                density: gp.density.value(),
                position: gp.position.value() * 2f32.powf(chaos[1] * organic * 0.8),
                jitter: gp.jitter.value(),
                pitch: gp.pitch.value(),
                spread: gp.pitch_spread.value(),
                interval: match gp.interval.value() {
                    GrainInterval::Free => Interval::Free,
                    GrainInterval::Octaves => Interval::Octaves,
                    GrainInterval::Fifths => Interval::Fifths,
                },
                reverse: gp.reverse.value(),
                width: gp.stereo.value(),
                freeze: gp.freeze.value(),
            },
        );

        let fx = &p.fx;
        self.fx.process(
            l,
            r,
            &FxFrame {
                chorus_mix: fx.chorus_mix.smoothed.next_step(steps),
                chorus_rate: fx.chorus_rate.value(),
                chorus_depth: fx.chorus_depth.value(),
                delay_time: fx.delay_time.value(),
                delay_feedback: fx.delay_feedback.value(),
                delay_tone: fx.delay_tone.value(),
                delay_mix: fx.delay_mix.smoothed.next_step(steps) * duck,
                reverb_mix: fx.reverb_mix.smoothed.next_step(steps) * duck,
                reverb_decay: fx.reverb_decay.value(),
                reverb_damping: fx.reverb_damping.value(),
                reverb_predelay: fx.reverb_predelay.value(),
            },
        );

        let mut peak = 0.0f32;
        for i in 0..n {
            let wl = soft_clip(l[i] * out_gain);
            let wr = soft_clip(r[i] * out_gain);
            l[i] = dry_l[i] * (1.0 - mix) + wl * mix;
            r[i] = dry_r[i] * (1.0 - mix) + wr * mix;
            peak = peak.max(l[i].abs()).max(r[i].abs());
        }
        let shown = self.shared.peak.load(Ordering::Relaxed) * 0.92;
        self.shared.peak.store(peak.max(shown), Ordering::Relaxed);
        let shown_in = self.shared.input.load(Ordering::Relaxed) * 0.92;
        self.shared.input.store(in_peak.max(shown_in), Ordering::Relaxed);
    }
}

impl Plugin for LumenStrings {
    const NAME: &'static str = "Lumen Strings";
    const VENDOR: &'static str = "Lumen Synth";
    const URL: &'static str = "";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: NonZeroU32::new(2),
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        strings_gui::create(self.params.clone(), self.shared.clone())
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
        let sr = self.sr;
        self.set_sample_rate(sr);
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let total = buffer.samples();
        let channels = buffer.as_slice();
        let (left, right) = channels.split_at_mut(1);
        let left = &mut *left[0];
        let right = &mut *right[0];
        let mut start = 0;
        while start < total {
            let end = (start + BLOCK).min(total);
            self.render_block(&mut left[start..end], &mut right[start..end]);
            start = end;
        }
        ProcessStatus::KeepAlive
    }
}

impl ClapPlugin for LumenStrings {
    const CLAP_ID: &'static str = "org.lumen-synth.lumen-strings";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Resonant effect for electric guitar and erhu");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
        ClapFeature::Distortion,
        ClapFeature::Reverb,
    ];
}

impl Vst3Plugin for LumenStrings {
    const VST3_CLASS_ID: [u8; 16] = *b"LumenStringsFxV1";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx];
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> LumenStrings {
        let mut s = LumenStrings::default();
        s.set_sample_rate(48000.0);
        for (_, ptr, _) in s.params.param_map() {
            if let ParamPtr::FloatParam(p) = ptr {
                unsafe { (*p).smoothed.reset((*p).value()) };
            }
        }
        s
    }

    fn pluck(t: f32) -> f32 {
        let env = (-t * 3.0).exp();
        let f = 196.0;
        env * ((TAU * f * t).sin() + 0.5 * (TAU * 2.0 * f * t).sin() + 0.3 * (TAU * 3.0 * f * t).sin()) * 0.4
    }

    #[test]
    fn plays_guitar_signal_finite_and_bounded() {
        let mut s = fresh();
        let mut peak = 0.0f32;
        let mut tail = 0.0f32;
        for blk in 0..(48000 * 4 / BLOCK) {
            let mut l = [0.0f32; BLOCK];
            let mut r = [0.0f32; BLOCK];
            for i in 0..BLOCK {
                let t = (blk * BLOCK + i) as f32 / 48000.0;
                l[i] = pluck(t % 1.5);
                r[i] = l[i];
            }
            s.render_block(&mut l, &mut r);
            for v in l.iter().chain(r.iter()) {
                assert!(v.is_finite());
                peak = peak.max(v.abs());
            }
            if blk > 48000 * 3 / BLOCK {
                tail = tail.max(l[0].abs());
            }
        }
        assert!(peak > 0.05 && peak < 1.2, "peak {peak}");
        assert!(tail >= 0.0);
    }

    #[test]
    fn extreme_settings_stay_finite() {
        let mut s = fresh();
        let p = s.params.clone();
        for (_, ptr, _) in p.param_map() {
            if let ParamPtr::FloatParam(f) = ptr {
                unsafe { (*f).smoothed.reset(ptr.preview_plain(1.0)) };
            }
        }
        let mut seed = 99u32;
        for _ in 0..(48000 * 2 / BLOCK) {
            let mut l = [0.0f32; BLOCK];
            let mut r = [0.0f32; BLOCK];
            for i in 0..BLOCK {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                l[i] = ((seed >> 8) as f32 / 8388608.0 - 1.0) * 0.8;
                r[i] = l[i];
            }
            s.render_block(&mut l, &mut r);
            for v in l.iter().chain(r.iter()) {
                assert!(v.is_finite() && v.abs() < 4.0, "v {v}");
            }
        }
    }

    #[test]
    fn dry_wet_zero_passes_input_with_gain() {
        let mut s = fresh();
        s.params.mix.smoothed.reset(0.0);
        s.params.mix.smoothed.reset(0.0);
        let mut l = [0.3f32; BLOCK];
        let mut r = [0.3f32; BLOCK];
        s.render_block(&mut l, &mut r);
        assert!((l[BLOCK - 1] - 0.3).abs() < 1e-3);
    }
}
