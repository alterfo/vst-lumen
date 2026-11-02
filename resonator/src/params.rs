use crate::sympathetic::Scale;
use nih_plug::prelude::*;
use nih_plug_egui::EguiState;
use std::sync::Arc;

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
    pub fn dsp(self) -> Scale {
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

pub fn percent(name: &str, default: f32) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Linear { min: 0.0, max: 1.0 })
        .with_smoother(SmoothingStyle::Linear(10.0))
        .with_unit(" %")
        .with_value_to_string(formatters::v2s_f32_percentage(0))
        .with_string_to_value(formatters::s2v_f32_percentage())
}

fn db(name: &str, default: f32, min: f32, max: f32) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Linear { min, max })
        .with_smoother(SmoothingStyle::Linear(20.0))
        .with_unit(" dB")
        .with_step_size(0.1)
}

#[derive(Params)]
pub struct ResonatorParams {
    #[id = "gain"]
    pub input: FloatParam,
    #[id = "dry"]
    pub dry: FloatParam,
    #[id = "smix"]
    pub mix: FloatParam,
    #[id = "root"]
    pub root: IntParam,
    #[id = "scale"]
    pub scale: EnumParam<StringScale>,
    #[id = "reg"]
    pub register: IntParam,
    #[id = "sdecay"]
    pub decay: FloatParam,
    #[id = "sdamp"]
    pub damping: FloatParam,
    #[id = "sdet"]
    pub detune: FloatParam,
    #[id = "ssub"]
    pub sub: FloatParam,
    #[id = "sdrop"]
    pub drop: IntParam,
    #[id = "out"]
    pub output: FloatParam,
    #[persist = "editor-state"]
    pub editor_state: Arc<EguiState>,
}

impl Default for ResonatorParams {
    fn default() -> Self {
        Self {
            input: db("Input", 0.0, -12.0, 24.0),
            dry: percent("Dry", 1.0),
            mix: percent("Strings Mix", 0.5),
            root: IntParam::new("Root", 2, IntRange::Linear { min: 0, max: 11 })
                .with_value_to_string(Arc::new(|v| NOTE_NAMES[v.clamp(0, 11) as usize].to_string())),
            scale: EnumParam::new("Scale", StringScale::Gong),
            register: IntParam::new("Register", 1, IntRange::Linear { min: 0, max: 3 })
                .with_value_to_string(Arc::new(|v| format!("C{}", v.clamp(0, 3) + 1))),
            decay: FloatParam::new(
                "Strings Decay",
                4.0,
                FloatRange::Skewed {
                    min: 0.3,
                    max: 12.0,
                    factor: FloatRange::skew_factor(-1.5),
                },
            )
            .with_unit(" s"),
            damping: percent("Strings Damping", 0.45),
            detune: FloatParam::new("Strings Detune", 6.0, FloatRange::Linear { min: 0.0, max: 25.0 })
                .with_unit(" ct"),
            sub: percent("Sub Bass", 0.5),
            drop: IntParam::new("Sub Drop", 2, IntRange::Linear { min: 1, max: 3 })
                .with_value_to_string(Arc::new(|v| format!("-{} oct", v.clamp(1, 3)))),
            output: db("Output", 0.0, -24.0, 12.0),
            editor_state: EguiState::from_size(700, 330),
        }
    }
}
