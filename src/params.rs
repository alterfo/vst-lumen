use crate::midi::SavedMap;
use nih_plug::prelude::*;
use nih_plug_egui::EguiState;
use std::sync::{Arc, Mutex};

fn seconds(name: &str, default: f32) -> FloatParam {
    FloatParam::new(
        name,
        default,
        FloatRange::Skewed {
            min: 0.001,
            max: 10.0,
            factor: FloatRange::skew_factor(-2.0),
        },
    )
    .with_unit(" s")
}

pub(crate) fn percent(name: &str, default: f32) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Linear { min: 0.0, max: 1.0 })
        .with_smoother(SmoothingStyle::Linear(10.0))
        .with_unit(" %")
        .with_value_to_string(formatters::v2s_f32_percentage(0))
        .with_string_to_value(formatters::s2v_f32_percentage())
}

#[derive(Params)]
pub struct OscParams {
    #[id = "wt"]
    pub position: FloatParam,
    #[id = "lvl"]
    pub level: FloatParam,
    #[id = "tune"]
    pub tune: FloatParam,
    #[id = "fine"]
    pub fine: FloatParam,
    #[id = "uni"]
    pub unison: FloatParam,
    #[id = "det"]
    pub detune: FloatParam,
    #[id = "spr"]
    pub spread: FloatParam,
}

impl OscParams {
    fn new(position: f32, level: f32, unison: f32) -> Self {
        Self {
            position: percent("Table Position", position),
            level: percent("Level", level),
            tune: FloatParam::new(
                "Tune",
                0.0,
                FloatRange::Linear {
                    min: -36.0,
                    max: 36.0,
                },
            )
            .with_smoother(SmoothingStyle::Linear(20.0))
            .with_unit(" st"),
            fine: FloatParam::new(
                "Fine",
                0.0,
                FloatRange::Linear {
                    min: -100.0,
                    max: 100.0,
                },
            )
            .with_unit(" ct"),
            unison: FloatParam::new("Unison", unison, FloatRange::Linear { min: 1.0, max: 8.0 })
                .with_smoother(SmoothingStyle::Linear(40.0)),
            detune: percent("Detune", 0.25),
            spread: percent("Stereo Spread", 0.6),
        }
    }
}

#[derive(Params)]
pub struct EnvParams {
    #[id = "a"]
    pub attack: FloatParam,
    #[id = "d"]
    pub decay: FloatParam,
    #[id = "s"]
    pub sustain: FloatParam,
    #[id = "r"]
    pub release: FloatParam,
}

impl EnvParams {
    fn new(a: f32, d: f32, s: f32, r: f32) -> Self {
        Self {
            attack: seconds("Attack", a),
            decay: seconds("Decay", d),
            sustain: percent("Sustain", s),
            release: seconds("Release", r),
        }
    }
}

#[derive(Params)]
pub struct FilterParams {
    #[id = "mode"]
    pub mode: FloatParam,
    #[id = "cut"]
    pub cutoff: FloatParam,
    #[id = "res"]
    pub resonance: FloatParam,
    #[id = "drv"]
    pub drive: FloatParam,
    #[id = "env"]
    pub env_amount: FloatParam,
}

#[derive(Params)]
pub struct LfoParams {
    #[id = "shape"]
    pub shape: FloatParam,
    #[id = "rate"]
    pub rate: FloatParam,
    #[id = "wt"]
    pub to_position: FloatParam,
    #[id = "pitch"]
    pub to_pitch: FloatParam,
    #[id = "cut"]
    pub to_cutoff: FloatParam,
}

#[derive(Params)]
pub struct LumenParams {
    #[nested(id_prefix = "o1", group = "Osc 1")]
    pub osc1: OscParams,
    #[nested(id_prefix = "o2", group = "Osc 2")]
    pub osc2: OscParams,
    #[id = "sub"]
    pub sub: FloatParam,
    #[id = "noise"]
    pub noise: FloatParam,
    #[nested(id_prefix = "flt", group = "Filter")]
    pub filter: FilterParams,
    #[nested(id_prefix = "ae", group = "Amp Env")]
    pub amp_env: EnvParams,
    #[nested(id_prefix = "fe", group = "Filter Env")]
    pub filt_env: EnvParams,
    #[nested(id_prefix = "lfo", group = "LFO")]
    pub lfo: LfoParams,
    #[id = "morph"]
    pub morph: FloatParam,
    #[nested(id_prefix = "evo", group = "Evolve")]
    pub evolve: EvolveParams,
    #[id = "drift"]
    pub drift: FloatParam,
    #[nested(id_prefix = "res", group = "Resonator")]
    pub resonator: ResonatorParams,
    #[nested(id_prefix = "body", group = "Body")]
    pub body: BodyParams,
    #[nested(id_prefix = "gran", group = "Granular")]
    pub granular: GranularParams,
    #[nested(id_prefix = "mac", group = "Macros")]
    pub macros: MacroParams,
    #[id = "vmode"]
    pub voice_mode: EnumParam<VoiceMode>,
    #[id = "glide"]
    pub glide: FloatParam,
    #[nested(id_prefix = "fx", group = "FX")]
    pub fx: FxParams,
    #[nested(id_prefix = "midi", group = "MIDI")]
    pub midi: MidiParams,
    #[persist = "midi-maps"]
    pub midi_maps: Mutex<Vec<SavedMap>>,
    #[persist = "editor-state"]
    pub editor_state: Arc<EguiState>,
    #[id = "gain"]
    pub gain: FloatParam,
}

impl Default for LumenParams {
    fn default() -> Self {
        Self {
            osc1: OscParams::new(0.38, 0.8, 3.0),
            osc2: OscParams::new(0.75, 0.8, 2.0),
            sub: percent("Sub", 0.08),
            noise: percent("Noise", 0.0),
            filter: FilterParams {
                mode: percent("Mode", 0.0),
                cutoff: FloatParam::new(
                    "Cutoff",
                    1400.0,
                    FloatRange::Skewed {
                        min: 20.0,
                        max: 20000.0,
                        factor: FloatRange::skew_factor(-2.5),
                    },
                )
                .with_smoother(SmoothingStyle::Logarithmic(15.0))
                .with_unit(" Hz")
                .with_value_to_string(formatters::v2s_f32_hz_then_khz(0))
                .with_string_to_value(formatters::s2v_f32_hz_then_khz()),
                resonance: percent("Resonance", 0.15),
                drive: percent("Drive", 0.2),
                env_amount: FloatParam::new(
                    "Env Amount",
                    0.25,
                    FloatRange::Linear {
                        min: -1.0,
                        max: 1.0,
                    },
                )
                .with_smoother(SmoothingStyle::Linear(10.0)),
            },
            amp_env: EnvParams::new(0.02, 0.6, 0.7, 0.8),
            filt_env: EnvParams::new(0.01, 0.8, 0.3, 0.8),
            lfo: LfoParams {
                shape: percent("Shape", 0.0),
                rate: FloatParam::new(
                    "Rate",
                    0.4,
                    FloatRange::Skewed {
                        min: 0.02,
                        max: 20.0,
                        factor: FloatRange::skew_factor(-2.0),
                    },
                )
                .with_unit(" Hz"),
                to_position: percent("To Table", 0.0),
                to_pitch: percent("To Pitch", 0.0),
                to_cutoff: percent("To Cutoff", 0.0),
            },
            morph: percent("Osc Morph", 0.5),
            evolve: EvolveParams::new(),
            drift: percent("Drift", 0.35),
            voice_mode: EnumParam::new("Voice Mode", VoiceMode::Poly),
            glide: FloatParam::new(
                "Glide",
                0.0,
                FloatRange::Skewed {
                    min: 0.0,
                    max: 2.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" s"),
            fx: FxParams::new(),
            resonator: ResonatorParams::new(),
            body: BodyParams::new(),
            granular: GranularParams::new(),
            macros: MacroParams::new(),
            midi: MidiParams::new(),
            midi_maps: Mutex::new(Vec::new()),
            editor_state: EguiState::from_size(1260, 900),
            gain: FloatParam::new(
                "Gain",
                util::db_to_gain(-14.0),
                FloatRange::Skewed {
                    min: util::db_to_gain(-36.0),
                    max: util::db_to_gain(6.0),
                    factor: FloatRange::gain_skew_factor(-36.0, 6.0),
                },
            )
            .with_smoother(SmoothingStyle::Logarithmic(10.0))
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_gain_to_db(1))
            .with_string_to_value(formatters::s2v_f32_gain_to_db()),
        }
    }
}

#[derive(Params)]
pub struct EvolveParams {
    #[id = "rate"]
    pub rate: FloatParam,
    #[id = "tbl"]
    pub to_table: FloatParam,
    #[id = "morph"]
    pub to_morph: FloatParam,
    #[id = "cut"]
    pub to_cutoff: FloatParam,
    #[id = "vox"]
    pub voice_spread: FloatParam,
}

impl EvolveParams {
    pub fn new() -> Self {
        Self {
            rate: FloatParam::new(
                "Rate",
                0.05,
                FloatRange::Skewed {
                    min: 0.005,
                    max: 1.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" Hz"),
            to_table: percent("To Table", 0.3),
            to_morph: percent("To Morph", 0.4),
            to_cutoff: percent("To Cutoff", 0.2),
            voice_spread: percent("Per-Voice", 0.3),
        }
    }
}

#[derive(Enum, PartialEq, Eq, Clone, Copy)]
pub enum VoiceMode {
    #[id = "poly"]
    #[name = "Poly"]
    Poly,
    #[id = "mono"]
    #[name = "Mono"]
    Mono,
    #[id = "legato"]
    #[name = "Legato"]
    Legato,
}

#[derive(Params)]
pub struct FxParams {
    #[id = "cmix"]
    pub chorus_mix: FloatParam,
    #[id = "crate"]
    pub chorus_rate: FloatParam,
    #[id = "cdepth"]
    pub chorus_depth: FloatParam,
    #[id = "dtime"]
    pub delay_time: FloatParam,
    #[id = "dfb"]
    pub delay_feedback: FloatParam,
    #[id = "dtone"]
    pub delay_tone: FloatParam,
    #[id = "dmix"]
    pub delay_mix: FloatParam,
    #[id = "rmix"]
    pub reverb_mix: FloatParam,
    #[id = "rdecay"]
    pub reverb_decay: FloatParam,
    #[id = "rdamp"]
    pub reverb_damping: FloatParam,
    #[id = "rpre"]
    pub reverb_predelay: FloatParam,
}

impl FxParams {
    pub fn new() -> Self {
        Self {
            chorus_mix: percent("Chorus Mix", 0.3),
            chorus_rate: FloatParam::new(
                "Chorus Rate",
                0.4,
                FloatRange::Skewed {
                    min: 0.05,
                    max: 4.0,
                    factor: FloatRange::skew_factor(-1.5),
                },
            )
            .with_unit(" Hz"),
            chorus_depth: percent("Chorus Depth", 0.5),
            delay_time: FloatParam::new(
                "Delay Time",
                0.375,
                FloatRange::Skewed {
                    min: 0.02,
                    max: 1.5,
                    factor: FloatRange::skew_factor(-1.5),
                },
            )
            .with_unit(" s"),
            delay_feedback: FloatParam::new(
                "Delay Feedback",
                0.4,
                FloatRange::Linear {
                    min: 0.0,
                    max: 0.95,
                },
            )
            .with_unit(" %")
            .with_value_to_string(formatters::v2s_f32_percentage(0))
            .with_string_to_value(formatters::s2v_f32_percentage()),
            delay_tone: percent("Delay Tone", 0.5),
            delay_mix: percent("Delay Mix", 0.0),
            reverb_mix: percent("Reverb Mix", 0.25),
            reverb_decay: FloatParam::new(
                "Reverb Decay",
                3.5,
                FloatRange::Skewed {
                    min: 0.3,
                    max: 20.0,
                    factor: FloatRange::skew_factor(-1.5),
                },
            )
            .with_unit(" s"),
            reverb_damping: percent("Reverb Damping", 0.4),
            reverb_predelay: FloatParam::new(
                "Reverb Pre-delay",
                0.02,
                FloatRange::Linear { min: 0.0, max: 0.2 },
            )
            .with_unit(" s"),
        }
    }
}

#[derive(Enum, PartialEq, Eq, Clone, Copy)]
pub enum ResModel {
    #[id = "off"]
    #[name = "Off"]
    Off,
    #[id = "pluck"]
    #[name = "Pluck"]
    Pluck,
    #[id = "string"]
    #[name = "String"]
    String,
    #[id = "membrane"]
    #[name = "Membrane"]
    Membrane,
    #[id = "plate"]
    #[name = "Plate"]
    Plate,
}

#[derive(Enum, PartialEq, Eq, Clone, Copy)]
pub enum GrainInterval {
    #[id = "free"]
    #[name = "Free"]
    Free,
    #[id = "oct"]
    #[name = "Octaves"]
    Octaves,
    #[id = "fifth"]
    #[name = "Fifths"]
    Fifths,
}

#[derive(Params)]
pub struct ResonatorParams {
    #[id = "model"]
    pub model: EnumParam<ResModel>,
    #[id = "mix"]
    pub mix: FloatParam,
    #[id = "decay"]
    pub decay: FloatParam,
    #[id = "damp"]
    pub damping: FloatParam,
    #[id = "stiff"]
    pub stiffness: FloatParam,
    #[id = "strike"]
    pub strike: FloatParam,
    #[id = "hard"]
    pub hardness: FloatParam,
    #[id = "pos"]
    pub position: FloatParam,
}

impl ResonatorParams {
    pub fn new() -> Self {
        Self {
            model: EnumParam::new("Model", ResModel::Off),
            mix: percent("Mix", 0.6),
            decay: FloatParam::new(
                "Decay",
                1.8,
                FloatRange::Skewed {
                    min: 0.05,
                    max: 10.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" s"),
            damping: percent("Damping", 0.45),
            stiffness: percent("Stiffness", 0.15),
            strike: percent("Strike", 0.5),
            hardness: percent("Hardness", 0.5),
            position: percent("Position", 0.3),
        }
    }
}

#[derive(Params)]
pub struct BodyParams {
    #[id = "mix"]
    pub mix: FloatParam,
    #[id = "size"]
    pub size: FloatParam,
    #[id = "decay"]
    pub decay: FloatParam,
    #[id = "damp"]
    pub damping: FloatParam,
}

impl BodyParams {
    pub fn new() -> Self {
        Self {
            mix: percent("Mix", 0.0),
            size: FloatParam::new("Size", 1.0, FloatRange::Linear { min: 0.5, max: 2.0 }),
            decay: FloatParam::new(
                "Decay",
                0.25,
                FloatRange::Skewed {
                    min: 0.03,
                    max: 2.0,
                    factor: FloatRange::skew_factor(-1.5),
                },
            )
            .with_unit(" s"),
            damping: percent("Damping", 0.4),
        }
    }
}

#[derive(Params)]
pub struct GranularParams {
    #[id = "mix"]
    pub mix: FloatParam,
    #[id = "size"]
    pub size: FloatParam,
    #[id = "dens"]
    pub density: FloatParam,
    #[id = "pos"]
    pub position: FloatParam,
    #[id = "jit"]
    pub jitter: FloatParam,
    #[id = "pitch"]
    pub pitch: FloatParam,
    #[id = "spread"]
    pub pitch_spread: FloatParam,
    #[id = "ival"]
    pub interval: EnumParam<GrainInterval>,
    #[id = "rev"]
    pub reverse: FloatParam,
    #[id = "wide"]
    pub stereo: FloatParam,
    #[id = "freeze"]
    pub freeze: BoolParam,
}

impl GranularParams {
    pub fn new() -> Self {
        Self {
            mix: percent("Mix", 0.0),
            size: FloatParam::new(
                "Size",
                0.18,
                FloatRange::Skewed {
                    min: 0.01,
                    max: 0.8,
                    factor: FloatRange::skew_factor(-1.5),
                },
            )
            .with_unit(" s"),
            density: FloatParam::new(
                "Density",
                14.0,
                FloatRange::Skewed {
                    min: 1.0,
                    max: 80.0,
                    factor: FloatRange::skew_factor(-1.5),
                },
            )
            .with_unit(" /s"),
            position: FloatParam::new(
                "Position",
                0.3,
                FloatRange::Skewed {
                    min: 0.02,
                    max: 4.0,
                    factor: FloatRange::skew_factor(-1.5),
                },
            )
            .with_unit(" s"),
            jitter: percent("Jitter", 0.4),
            pitch: FloatParam::new(
                "Pitch",
                0.0,
                FloatRange::Linear {
                    min: -24.0,
                    max: 24.0,
                },
            )
            .with_unit(" st"),
            pitch_spread: percent("Pitch Spread", 0.25),
            interval: EnumParam::new("Intervals", GrainInterval::Octaves),
            reverse: percent("Reverse", 0.2),
            stereo: percent("Width", 0.7),
            freeze: BoolParam::new("Freeze", false),
        }
    }
}

#[derive(Params)]
pub struct MacroParams {
    #[id = "organic"]
    pub organic: FloatParam,
    #[id = "chaos"]
    pub chaos: FloatParam,
    #[id = "crate"]
    pub chaos_rate: FloatParam,
}

impl MacroParams {
    pub fn new() -> Self {
        Self {
            organic: percent("Organic", 0.4),
            chaos: percent("Chaos", 0.2),
            chaos_rate: FloatParam::new(
                "Chaos Rate",
                0.15,
                FloatRange::Skewed {
                    min: 0.01,
                    max: 2.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" x"),
        }
    }
}

#[derive(Params)]
pub struct MidiParams {
    #[id = "fine"]
    pub fine_exp: FloatParam,
    #[id = "coarse"]
    pub coarse_div: FloatParam,
    #[id = "fast"]
    pub speed_hi: FloatParam,
    #[id = "inertia"]
    pub inertia: FloatParam,
}

impl MidiParams {
    pub fn new() -> Self {
        Self {
            fine_exp: FloatParam::new(
                "MIDI Fine Step",
                -8.0,
                FloatRange::Linear {
                    min: -10.0,
                    max: -4.0,
                },
            )
            .with_unit(" log10"),
            coarse_div: FloatParam::new(
                "MIDI Coarse Ticks",
                24.0,
                FloatRange::Linear {
                    min: 6.0,
                    max: 256.0,
                },
            )
            .with_unit(" ticks"),
            speed_hi: FloatParam::new(
                "MIDI Fast Speed",
                90.0,
                FloatRange::Linear {
                    min: 20.0,
                    max: 300.0,
                },
            )
            .with_unit(" t/s"),
            inertia: FloatParam::new(
                "MIDI Inertia",
                90.0,
                FloatRange::Skewed {
                    min: 2.0,
                    max: 800.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" ms"),
        }
    }
}
