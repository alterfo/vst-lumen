use crate::bank::BANK;
use crate::params::ResonatorParams;
use nih_plug::prelude::*;

pub struct Preset {
    pub name: &'static str,
    pub values: &'static [(&'static str, f32)],
}

const KEEP: [&str; 2] = ["gain", "out"];

pub const FACTORY: &[Preset] = &[Preset {
    name: "Init",
    values: &[("smix", 0.5), ("ssub", 0.0)],
}];

fn deleted_path() -> Option<std::path::PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(std::path::PathBuf::from(home).join("Library/Application Support/Lumen/resonator_deleted_presets.txt"))
}

fn deleted_names() -> Vec<String> {
    deleted_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|t| t.lines().map(str::to_string).collect())
        .unwrap_or_default()
}

pub fn available() -> Vec<&'static Preset> {
    let deleted = deleted_names();
    let list: Vec<&'static Preset> = FACTORY
        .iter()
        .chain(BANK.iter())
        .filter(|p| !deleted.iter().any(|d| d == p.name))
        .collect();
    if list.is_empty() {
        vec![&FACTORY[0]]
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

pub fn apply(setter: &ParamSetter, params: &ResonatorParams, preset: &Preset) {
    for (id, ptr, _) in params.param_map() {
        let listed = preset.values.iter().find(|(k, _)| *k == id);
        if listed.is_none() && KEEP.contains(&id.as_str()) {
            continue;
        }
        let norm = match listed {
            Some((_, plain)) => unsafe { ptr.preview_normalized(*plain) },
            None => unsafe { ptr.default_normalized_value() },
        };
        unsafe {
            setter.raw_context.raw_begin_set_parameter(ptr);
            setter
                .raw_context
                .raw_set_parameter_normalized(ptr, norm.clamp(0.0, 1.0));
            setter.raw_context.raw_end_set_parameter(ptr);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_preset_id_exists_and_value_is_in_range() {
        let params = ResonatorParams::default();
        let map = params.param_map();
        for preset in FACTORY.iter().chain(BANK.iter()) {
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
