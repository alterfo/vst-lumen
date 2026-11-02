use super::variant::Variant;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Default)]
pub struct Taste {
    liked_total: f32,
    disliked_total: f32,
    traits: HashMap<String, (f32, f32)>,
    pub liked: usize,
    pub disliked: usize,
}

impl Taste {
    pub fn load(mixes_dir: &Path) -> Self {
        let mut taste = Taste::default();
        let index = mixes_dir.join("index.json");
        let Ok(raw) = fs::read_to_string(&index) else {
            return taste;
        };
        let Ok(doc) = serde_json::from_str::<Value>(&raw) else {
            return taste;
        };
        let Some(mixes) = doc.get("mixes").and_then(|m| m.as_array()) else {
            return taste;
        };
        for entry in mixes {
            let Some(folder) = entry.get("folder").and_then(|f| f.as_str()) else {
                continue;
            };
            let kept = mixes_dir.join(folder).join("mix.wav").is_file();
            let traits = traits_of(entry);
            if kept {
                taste.liked += 1;
                taste.liked_total += 1.0;
            } else {
                taste.disliked += 1;
                taste.disliked_total += 1.0;
            }
            for trait_name in traits {
                let slot = taste.traits.entry(trait_name).or_insert((0.0, 0.0));
                if kept {
                    slot.0 += 1.0;
                } else {
                    slot.1 += 1.0;
                }
            }
        }
        taste
    }

    pub fn enabled(&self) -> bool {
        self.liked + self.disliked >= 4
    }

    pub fn score(&self, variant: &Variant) -> f32 {
        if !self.enabled() {
            return 0.0;
        }
        let mut total = 0.0;
        for trait_name in variant_traits(variant) {
            let (liked, disliked) = self.traits.get(&trait_name).copied().unwrap_or((0.0, 0.0));
            let p_liked = (liked + 0.7) / (self.liked_total + 1.4);
            let p_disliked = (disliked + 0.7) / (self.disliked_total + 1.4);
            total += (p_liked / p_disliked).ln();
        }
        total
    }

    pub fn rejection_threshold(&self) -> f32 {
        0.0
    }
}

fn traits_of(entry: &Value) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(archetype) = entry.get("archetype").and_then(|v| v.as_str()) {
        out.push(format!("archetype:{archetype}"));
    }
    if let Some(voices) = entry.get("voices").and_then(|v| v.as_array()) {
        for voice in voices {
            if let Some(slug) = voice.as_str() {
                out.push(format!("voice:{slug}"));
            }
        }
    }
    out
}

fn variant_traits(variant: &Variant) -> Vec<String> {
    let mut out = vec![format!("archetype:{}", variant.archetype)];
    for layer in &variant.layers {
        out.push(format!("voice:{}", layer.kit_slug));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_index(dir: &Path, folders: &[(&str, bool, &str, &[&str])]) {
        fs::create_dir_all(dir).unwrap();
        let mut entries = Vec::new();
        for (folder, kept, archetype, voices) in folders {
            if *kept {
                fs::create_dir_all(dir.join(folder)).unwrap();
                let mut f = fs::File::create(dir.join(folder).join("mix.wav")).unwrap();
                f.write_all(b"RIFF").unwrap();
            }
            let voices: Vec<String> = voices.iter().map(|v| v.to_string()).collect();
            entries.push(serde_json::json!({
                "folder": folder,
                "archetype": archetype,
                "voices": voices,
            }));
        }
        fs::write(
            dir.join("index.json"),
            serde_json::to_vec_pretty(&serde_json::json!({ "mixes": entries })).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn deleted_folders_are_learned_as_disliked() {
        let dir = std::env::temp_dir().join("lumen-taste-test");
        let _ = fs::remove_dir_all(&dir);
        write_index(
            &dir,
            &[
                ("001", true, "trip hop", &["kick-punch", "pad-warm"]),
                ("002", true, "trip hop", &["kick-punch", "pad-warm"]),
                ("003", false, "ambient drift", &["kick-sub", "pad-airy"]),
                ("004", false, "ambient drift", &["kick-sub", "pad-airy"]),
                ("005", false, "cinematic", &["bell", "pad-airy"]),
                ("006", false, "cinematic", &["bell", "pad-airy"]),
            ],
        );
        let taste = Taste::load(&dir);
        assert!(taste.enabled());
        assert_eq!(taste.liked, 2);
        assert_eq!(taste.disliked, 4);
        let liked_trait = taste.traits.get("archetype:trip hop").copied().unwrap();
        assert_eq!(liked_trait, (2.0, 0.0));
        let disliked_trait = taste.traits.get("archetype:cinematic").copied().unwrap();
        assert_eq!(disliked_trait, (0.0, 2.0));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_index_means_no_preference() {
        let dir = std::env::temp_dir().join("lumen-taste-empty");
        let _ = fs::remove_dir_all(&dir);
        let taste = Taste::load(&dir);
        assert!(!taste.enabled());
        assert_eq!(taste.liked + taste.disliked, 0);
    }
}
