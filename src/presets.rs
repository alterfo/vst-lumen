use crate::params::LumenParams;
use nih_plug::prelude::*;

pub struct Preset {
    pub name: &'static str,
    pub values: &'static [(&'static str, f32)],
}

const MODEL_PLUCK: f32 = 1.0;
const MODEL_STRING: f32 = 2.0;
const MODEL_MEMBRANE: f32 = 3.0;
const MODEL_PLATE: f32 = 4.0;
const MODE_MONO: f32 = 1.0;
const MODE_LEGATO: f32 = 2.0;
const IVAL_FREE: f32 = 0.0;
const IVAL_OCT: f32 = 1.0;
const IVAL_FIFTHS: f32 = 2.0;

pub const PRESETS: &[Preset] = &[
    Preset {
        name: "Init",
        values: &[],
    },
    Preset {
        name: "Warm Kalimba",
        values: &[
            ("o1_wt", 0.05),
            ("o1_uni", 1.0),
            ("o2_lvl", 0.0),
            ("sub", 0.15),
            ("flt_cut", 3200.0),
            ("flt_res", 0.1),
            ("flt_env", 0.2),
            ("ae_a", 0.002),
            ("ae_d", 0.7),
            ("ae_s", 0.0),
            ("ae_r", 0.5),
            ("res_model", MODEL_PLUCK),
            ("res_mix", 0.85),
            ("res_decay", 2.4),
            ("res_damp", 0.55),
            ("res_strike", 0.8),
            ("res_hard", 0.35),
            ("body_mix", 0.35),
            ("body_size", 0.8),
            ("fx_rmix", 0.3),
            ("fx_rdecay", 3.0),
            ("mac_organic", 0.55),
            ("mac_chaos", 0.1),
        ],
    },
    Preset {
        name: "Cedar Pad",
        values: &[
            ("o1_wt", 0.38),
            ("o2_wt", 0.7),
            ("o1_uni", 4.5),
            ("o2_uni", 3.0),
            ("o1_det", 0.3),
            ("flt_cut", 1100.0),
            ("flt_res", 0.2),
            ("flt_env", 0.1),
            ("ae_a", 1.4),
            ("ae_d", 2.0),
            ("ae_s", 0.8),
            ("ae_r", 3.5),
            ("body_mix", 0.3),
            ("body_decay", 0.4),
            ("fx_cmix", 0.5),
            ("fx_rmix", 0.45),
            ("fx_rdecay", 6.0),
            ("evo_rate", 0.03),
            ("evo_tbl", 0.5),
            ("evo_morph", 0.6),
            ("mac_organic", 0.65),
            ("mac_chaos", 0.3),
            ("mac_crate", 0.08),
        ],
    },
    Preset {
        name: "Dusty Marimba",
        values: &[
            ("o1_wt", 0.1),
            ("o1_uni", 1.0),
            ("o2_lvl", 0.0),
            ("sub", 0.3),
            ("flt_cut", 2400.0),
            ("ae_a", 0.003),
            ("ae_d", 0.45),
            ("ae_s", 0.0),
            ("ae_r", 0.4),
            ("res_model", MODEL_PLATE),
            ("res_mix", 0.9),
            ("res_decay", 1.1),
            ("res_damp", 0.7),
            ("res_stiff", 0.25),
            ("res_strike", 0.9),
            ("res_hard", 0.55),
            ("body_mix", 0.45),
            ("body_size", 1.2),
            ("fx_rmix", 0.22),
            ("fx_rdecay", 2.2),
            ("mac_organic", 0.7),
        ],
    },
    Preset {
        name: "Slow String Drift",
        values: &[
            ("o1_wt", 0.25),
            ("o1_uni", 2.0),
            ("o2_lvl", 0.0),
            ("flt_cut", 1700.0),
            ("ae_a", 0.25),
            ("ae_d", 1.0),
            ("ae_s", 0.75),
            ("ae_r", 2.2),
            ("res_model", MODEL_STRING),
            ("res_mix", 0.8),
            ("res_decay", 5.5),
            ("res_damp", 0.35),
            ("res_stiff", 0.12),
            ("res_strike", 0.35),
            ("res_pos", 0.22),
            ("vmode", MODE_LEGATO),
            ("glide", 0.18),
            ("fx_dmix", 0.22),
            ("fx_dtime", 0.46),
            ("fx_dfb", 0.45),
            ("fx_rmix", 0.4),
            ("fx_rdecay", 5.0),
            ("mac_organic", 0.75),
            ("mac_chaos", 0.35),
        ],
    },
    Preset {
        name: "Frozen Moss",
        values: &[
            ("o1_wt", 0.55),
            ("o2_wt", 0.82),
            ("o1_uni", 3.0),
            ("flt_cut", 900.0),
            ("ae_a", 0.9),
            ("ae_r", 2.8),
            ("gran_mix", 0.7),
            ("gran_size", 0.28),
            ("gran_dens", 22.0),
            ("gran_pos", 0.9),
            ("gran_jit", 0.7),
            ("gran_spread", 0.5),
            ("gran_ival", IVAL_FIFTHS),
            ("gran_rev", 0.3),
            ("gran_wide", 0.9),
            ("fx_rmix", 0.55),
            ("fx_rdecay", 9.0),
            ("fx_rdamp", 0.55),
            ("mac_organic", 0.6),
            ("mac_chaos", 0.45),
            ("mac_crate", 0.12),
        ],
    },
    Preset {
        name: "Breathing Sub",
        values: &[
            ("o1_wt", 0.0),
            ("o1_uni", 1.0),
            ("o2_lvl", 0.0),
            ("sub", 0.6),
            ("flt_cut", 420.0),
            ("flt_res", 0.3),
            ("flt_drv", 0.45),
            ("flt_env", 0.35),
            ("ae_a", 0.04),
            ("ae_d", 0.8),
            ("ae_s", 0.85),
            ("ae_r", 0.6),
            ("vmode", MODE_MONO),
            ("glide", 0.09),
            ("fx_rmix", 0.12),
            ("mac_organic", 0.45),
            ("mac_chaos", 0.2),
        ],
    },
    Preset {
        name: "Rain on Hollow Wood",
        values: &[
            ("o1_lvl", 0.35),
            ("o2_lvl", 0.0),
            ("noise", 0.08),
            ("flt_cut", 2600.0),
            ("ae_a", 0.01),
            ("ae_s", 0.5),
            ("ae_r", 1.2),
            ("res_model", MODEL_MEMBRANE),
            ("res_mix", 0.8),
            ("res_decay", 0.9),
            ("res_strike", 1.0),
            ("res_hard", 0.4),
            ("body_mix", 0.8),
            ("body_size", 0.9),
            ("body_decay", 0.5),
            ("gran_mix", 0.5),
            ("gran_size", 0.04),
            ("gran_dens", 48.0),
            ("gran_pos", 0.5),
            ("gran_jit", 0.9),
            ("gran_ival", IVAL_FREE),
            ("gran_spread", 0.35),
            ("fx_rmix", 0.4),
            ("fx_rdecay", 4.0),
            ("mac_organic", 0.8),
            ("mac_chaos", 0.5),
        ],
    },
    Preset {
        name: "Octave Cloud",
        values: &[
            ("o1_wt", 0.3),
            ("o2_wt", 0.6),
            ("o2_tune", 12.0),
            ("o1_uni", 3.0),
            ("flt_cut", 2200.0),
            ("ae_a", 0.5),
            ("ae_r", 2.0),
            ("gran_mix", 0.55),
            ("gran_size", 0.35),
            ("gran_dens", 10.0),
            ("gran_pos", 1.4),
            ("gran_ival", IVAL_OCT),
            ("gran_spread", 0.6),
            ("fx_cmix", 0.4),
            ("fx_rmix", 0.5),
            ("fx_rdecay", 7.0),
            ("mac_organic", 0.5),
            ("mac_chaos", 0.3),
        ],
    },
];

const RANDOM_RANGES: &[(&str, f32, f32)] = &[
    ("o1_wt", 0.0, 1.0),
    ("o2_wt", 0.0, 1.0),
    ("o1_uni", 0.0, 0.5),
    ("o2_uni", 0.0, 0.4),
    ("o1_det", 0.05, 0.5),
    ("o2_det", 0.05, 0.5),
    ("o1_spr", 0.2, 0.9),
    ("o2_spr", 0.2, 0.9),
    ("morph", 0.0, 1.0),
    ("sub", 0.0, 0.4),
    ("noise", 0.0, 0.12),
    ("flt_cut", 0.4, 0.85),
    ("flt_res", 0.0, 0.4),
    ("flt_drv", 0.0, 0.5),
    ("flt_env", 0.4, 0.75),
    ("flt_mode", 0.0, 0.3),
    ("ae_a", 0.05, 0.6),
    ("ae_d", 0.3, 0.7),
    ("ae_s", 0.3, 0.95),
    ("ae_r", 0.35, 0.8),
    ("lfo_rate", 0.1, 0.5),
    ("lfo_wt", 0.0, 0.4),
    ("lfo_cut", 0.0, 0.3),
    ("evo_rate", 0.05, 0.5),
    ("evo_tbl", 0.0, 0.7),
    ("evo_morph", 0.0, 0.7),
    ("evo_cut", 0.0, 0.5),
    ("evo_vox", 0.0, 0.6),
    ("drift", 0.1, 0.8),
    ("fx_cmix", 0.0, 0.6),
    ("fx_dmix", 0.0, 0.35),
    ("fx_dtime", 0.2, 0.8),
    ("fx_dfb", 0.2, 0.7),
    ("fx_rmix", 0.1, 0.6),
    ("fx_rdecay", 0.3, 0.85),
    ("res_mix", 0.3, 0.9),
    ("res_decay", 0.2, 0.8),
    ("res_damp", 0.2, 0.8),
    ("res_stiff", 0.0, 0.6),
    ("res_strike", 0.2, 1.0),
    ("res_hard", 0.2, 0.9),
    ("res_pos", 0.1, 0.8),
    ("body_mix", 0.0, 0.6),
    ("body_size", 0.2, 0.8),
    ("body_decay", 0.2, 0.7),
    ("gran_mix", 0.0, 0.55),
    ("gran_size", 0.2, 0.75),
    ("gran_dens", 0.2, 0.75),
    ("gran_pos", 0.2, 0.7),
    ("gran_jit", 0.1, 0.8),
    ("gran_spread", 0.0, 0.6),
    ("gran_rev", 0.0, 0.5),
    ("mac_organic", 0.25, 0.9),
    ("mac_chaos", 0.0, 0.6),
    ("mac_crate", 0.1, 0.6),
];

const RANDOM_CHOICES: &[(&str, u32)] = &[("res_model", 5), ("gran_ival", 3)];

const KEEP: &[&str] = &["gain", "vmode"];

fn set_norm(setter: &ParamSetter, ptr: ParamPtr, norm: f32) {
    unsafe {
        setter.raw_context.raw_begin_set_parameter(ptr);
        setter.raw_context.raw_set_parameter_normalized(ptr, norm.clamp(0.0, 1.0));
        setter.raw_context.raw_end_set_parameter(ptr);
    }
}

fn is_midi_or_meta(id: &str) -> bool {
    id.starts_with("midi_") || KEEP.contains(&id)
}

pub fn apply_preset(setter: &ParamSetter, params: &LumenParams, preset: &Preset) {
    for (id, ptr, _) in params.param_map() {
        if is_midi_or_meta(&id) && !preset.values.iter().any(|(k, _)| *k == id) {
            continue;
        }
        let norm = match preset.values.iter().find(|(k, _)| *k == id) {
            Some((_, plain)) => unsafe { ptr.preview_normalized(*plain) },
            None => unsafe { ptr.default_normalized_value() },
        };
        set_norm(setter, ptr, norm);
    }
}

struct Rng(u32);

impl Rng {
    fn next(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x >> 8) as f32 / 16777216.0
    }
}

pub fn randomize(setter: &ParamSetter, params: &LumenParams, amount: f32, seed: u32) {
    let mut rng = Rng(seed | 1);
    let amount = amount.clamp(0.0, 1.0);
    for (id, ptr, _) in params.param_map() {
        if let Some((_, lo, hi)) = RANDOM_RANGES.iter().find(|(k, _, _)| *k == id) {
            let r = lo + (hi - lo) * (rng.next() + rng.next()) * 0.5;
            let current = unsafe { ptr.unmodulated_normalized_value() };
            set_norm(setter, ptr, current + (r - current) * amount);
        } else if let Some((_, count)) = RANDOM_CHOICES.iter().find(|(k, _)| *k == id) {
            if rng.next() < amount {
                let pick = (rng.next() * *count as f32) as u32 % count;
                let norm = pick as f32 / (*count - 1) as f32;
                set_norm(setter, ptr, norm);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_preset_id_exists_and_value_is_in_range() {
        let params = LumenParams::default();
        let map = params.param_map();
        for preset in PRESETS {
            for (id, plain) in preset.values {
                let (_, ptr, _) = map
                    .iter()
                    .find(|(pid, _, _)| pid == id)
                    .unwrap_or_else(|| panic!("{}: unknown param {id}", preset.name));
                let norm = unsafe { ptr.preview_normalized(*plain) };
                let back = unsafe { ptr.preview_plain(norm) };
                assert!(
                    (back - plain).abs() <= 0.02 * plain.abs().max(1.0),
                    "{}: {id} = {plain} clamped to {back}",
                    preset.name
                );
            }
        }
    }

    #[test]
    fn random_ranges_reference_real_params() {
        let params = LumenParams::default();
        let map = params.param_map();
        for (id, lo, hi) in RANDOM_RANGES {
            assert!(map.iter().any(|(pid, _, _)| pid == id), "unknown {id}");
            assert!(lo <= hi && *lo >= 0.0 && *hi <= 1.0);
        }
        for (id, _) in RANDOM_CHOICES {
            assert!(map.iter().any(|(pid, _, _)| pid == id), "unknown {id}");
        }
    }
}
