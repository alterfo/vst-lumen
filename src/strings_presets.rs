use crate::presets::{set_norm, Preset};
use crate::strings_bank::BANK;
use crate::strings::StringsParams;
use nih_plug::prelude::*;

const GONG: f32 = 0.0;
const ZHI: f32 = 3.0;
const YU: f32 = 4.0;
const MINOR: f32 = 5.0;
const DORIAN: f32 = 6.0;
const PHRYGIAN: f32 = 7.0;
const FIFTHS: f32 = 8.0;
const FREE: f32 = 0.0;
const OCT: f32 = 1.0;
const FIFTH_IV: f32 = 2.0;

const KEEP: [&str; 3] = ["gain", "out", "mix"];

pub const PRESETS: &[Preset] = &[
    Preset {
        name: "Init",
        values: &[("smix", 0.0), ("fx_cmix", 0.0), ("fx_rmix", 0.15), ("bloom", 0.0)],
    },
    Preset {
        name: "Erhu Weeping",
        values: &[
            ("skin", 0.45),
            ("sfreq", 1600.0),
            ("tone", 0.7),
            ("smix", 0.45),
            ("root", 2.0),
            ("scale", YU),
            ("reg", 2.0),
            ("sdecay", 5.0),
            ("sdamp", 0.4),
            ("body_mix", 0.4),
            ("body_size", 1.8),
            ("body_decay", 0.3),
            ("bloom", 0.35),
            ("organic", 0.3),
            ("fx_cmix", 0.0),
            ("fx_dmix", 0.18),
            ("fx_dtime", 0.42),
            ("fx_dfb", 0.4),
            ("fx_rmix", 0.35),
            ("fx_rdecay", 4.5),
        ],
    },
    Preset {
        name: "Erhu Sky Pad",
        values: &[
            ("skin", 0.25),
            ("smix", 0.3),
            ("root", 7.0),
            ("scale", ZHI),
            ("reg", 2.0),
            ("sdecay", 6.0),
            ("body_mix", 0.3),
            ("body_size", 1.7),
            ("gran_mix", 0.5),
            ("gran_size", 0.3),
            ("gran_dens", 12.0),
            ("gran_spread", 0.3),
            ("gran_ival", OCT),
            ("gran_rev", 0.2),
            ("bloom", 0.6),
            ("organic", 0.4),
            ("fx_cmix", 0.15),
            ("fx_rmix", 0.5),
            ("fx_rdecay", 9.0),
        ],
    },
    Preset {
        name: "Gong Garden",
        values: &[
            ("smix", 0.55),
            ("root", 0.0),
            ("scale", GONG),
            ("reg", 1.0),
            ("sdecay", 7.0),
            ("sdet", 10.0),
            ("body_mix", 0.2),
            ("bloom", 0.25),
            ("organic", 0.4),
            ("fx_cmix", 0.2),
            ("fx_rmix", 0.3),
            ("fx_rdecay", 5.0),
        ],
    },
    Preset {
        name: "Clean Halo",
        values: &[
            ("smix", 0.4),
            ("root", 9.0),
            ("scale", MINOR),
            ("reg", 1.0),
            ("sdecay", 5.0),
            ("body_mix", 0.25),
            ("body_size", 1.0),
            ("bloom", 0.4),
            ("fx_cmix", 0.3),
            ("fx_dmix", 0.25),
            ("fx_rmix", 0.35),
            ("fx_rdecay", 5.0),
        ],
    },
    Preset {
        name: "Amber Drive",
        values: &[
            ("drive", 0.45),
            ("tone", 0.5),
            ("touch", 0.4),
            ("tfreq", 700.0),
            ("smix", 0.25),
            ("root", 4.0),
            ("scale", PHRYGIAN),
            ("reg", 1.0),
            ("body_mix", 0.35),
            ("body_size", 0.9),
            ("bloom", 0.2),
            ("fx_cmix", 0.0),
            ("fx_dmix", 0.2),
            ("fx_dfb", 0.45),
            ("fx_rmix", 0.25),
        ],
    },
    Preset {
        name: "Fifths Drone",
        values: &[
            ("smix", 0.7),
            ("root", 2.0),
            ("scale", FIFTHS),
            ("reg", 0.0),
            ("sdecay", 10.0),
            ("sdamp", 0.3),
            ("gran_mix", 0.3),
            ("gran_pitch", -12.0),
            ("gran_ival", FIFTH_IV),
            ("bloom", 0.7),
            ("organic", 0.5),
            ("fx_rmix", 0.5),
            ("fx_rdecay", 12.0),
        ],
    },
    Preset {
        name: "Grain Cloud",
        values: &[
            ("smix", 0.2),
            ("gran_mix", 0.65),
            ("gran_size", 0.4),
            ("gran_dens", 30.0),
            ("gran_pos", 1.2),
            ("gran_jit", 0.6),
            ("gran_pitch", 12.0),
            ("gran_ival", OCT),
            ("gran_rev", 0.35),
            ("gran_wide", 0.9),
            ("bloom", 0.55),
            ("fx_rmix", 0.45),
            ("fx_rdecay", 8.0),
        ],
    },
    Preset {
        name: "Cello Breath",
        values: &[
            ("touch", 0.6),
            ("tfreq", 500.0),
            ("tres", 0.5),
            ("smix", 0.4),
            ("root", 9.0),
            ("scale", DORIAN),
            ("reg", 1.0),
            ("body_mix", 0.5),
            ("body_size", 1.4),
            ("body_decay", 0.5),
            ("bloom", 0.8),
            ("fx_cmix", 0.35),
            ("fx_rmix", 0.35),
        ],
    },
    Preset {
        name: "Wood & Skin",
        values: &[
            ("skin", 0.5),
            ("sfreq", 1100.0),
            ("smix", 0.15),
            ("body_mix", 0.7),
            ("body_size", 1.3),
            ("body_decay", 0.15),
            ("body_damp", 0.6),
            ("bloom", 0.0),
            ("fx_cmix", 0.0),
            ("fx_rmix", 0.15),
            ("gran_ival", FREE),
        ],
    },
];

fn deleted_path() -> Option<std::path::PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(std::path::PathBuf::from(home).join("Library/Application Support/Lumen/strings_deleted_presets.txt"))
}

fn deleted_names() -> Vec<String> {
    deleted_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|t| t.lines().map(str::to_string).collect())
        .unwrap_or_default()
}

pub fn available() -> Vec<&'static Preset> {
    let deleted = deleted_names();
    let list: Vec<&'static Preset> = PRESETS
        .iter()
        .chain(BANK.iter())
        .filter(|p| !deleted.iter().any(|d| d == p.name))
        .collect();
    if list.is_empty() {
        vec![&PRESETS[0]]
    } else {
        list
    }
}

pub fn delete(name: &str) {
    let Some(path) = deleted_path() else { return };
    let mut names = deleted_names();
    if names.iter().any(|n| n == name) {
        return;
    }
    names.push(name.to_string());
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, names.join("\n"));
}

pub fn apply(setter: &ParamSetter, params: &StringsParams, preset: &Preset) {
    for (id, ptr, _) in params.param_map() {
        let listed = preset.values.iter().find(|(k, _)| *k == id);
        if listed.is_none() && KEEP.contains(&id.as_str()) {
            continue;
        }
        let norm = match listed {
            Some((_, plain)) => unsafe { ptr.preview_normalized(*plain) },
            None => unsafe { ptr.default_normalized_value() },
        };
        set_norm(setter, ptr, norm);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_preset_id_exists_and_value_is_in_range() {
        let params = StringsParams::default();
        let map = params.param_map();
        for preset in PRESETS.iter().chain(BANK.iter()) {
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
    fn bank_has_a_hundred_unique_names() {
        assert_eq!(BANK.len(), 100);
        let mut names: Vec<_> = BANK.iter().map(|p| p.name).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), 100);
    }
}
