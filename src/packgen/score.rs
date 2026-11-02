use super::kit::{by_slug, KIT};
use super::variant::{jittered_patch, Automation, Auto, LayerSpec, Rng};
use super::{sample_rate, synth, wav};
use crate::wavetable::WaveTable;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::sync::Arc;

#[derive(Deserialize)]
struct ScoreEvent {
    time: f64,
    note: u8,
    #[serde(default = "default_velocity")]
    velocity: f32,
    #[serde(default = "default_duration")]
    duration: f64,
}

fn default_velocity() -> f32 {
    0.8
}

fn default_duration() -> f64 {
    0.25
}

#[derive(Deserialize)]
struct ScoreAuto {
    target: String,
    from: f32,
    to: f32,
    #[serde(default)]
    curve: f32,
}

#[derive(Deserialize)]
struct ScoreLayer {
    name: String,
    kit_voice: String,
    #[serde(default)]
    peak_db: Option<f32>,
    #[serde(default)]
    jitter: Option<f32>,
    #[serde(default)]
    events: Vec<ScoreEvent>,
    #[serde(default)]
    automation: Vec<ScoreAuto>,
}

#[derive(Deserialize)]
struct ScoreDoc {
    #[serde(default)]
    title: Option<String>,
    bpm: f64,
    #[serde(default)]
    bars: Option<usize>,
    #[serde(default)]
    seed: Option<u64>,
    #[serde(default)]
    jitter: Option<f32>,
    layers: Vec<ScoreLayer>,
}

#[derive(Serialize)]
struct LayerReport {
    name: String,
    kit_voice: String,
    events: usize,
    peak_db: f32,
    rms_db: f32,
    file: String,
}

#[derive(Serialize, Debug)]
pub struct ScoreReport {
    pub title: String,
    pub bpm: f64,
    pub bars: usize,
    pub seconds: f32,
    pub peak_db: f32,
    pub rms_db: f32,
    pub mix: String,
    pub layers: Vec<LayerSummary>,
}

#[derive(Serialize, Debug)]
pub struct LayerSummary {
    pub name: String,
    pub kit_voice: String,
    pub events: usize,
}

fn slugify(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

fn parse_target(name: &str) -> Option<Auto> {
    match name {
        "cutoff" => Some(Auto::Cutoff),
        "morph" => Some(Auto::Morph),
        "table" => Some(Auto::Table),
        "grain" | "granular" => Some(Auto::GranMix),
        "resonator" => Some(Auto::ResMix),
        "noise" => Some(Auto::Noise),
        "delay" => Some(Auto::DelayMix),
        "filter-mode" => Some(Auto::FilterMode),
        _ => None,
    }
}

fn unknown_voice_error(slug: &str) -> std::io::Error {
    let names: Vec<&str> = KIT.iter().map(|v| v.slug).collect();
    std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        format!(
            "unknown kit voice '{slug}'. Available: {}",
            names.join(", ")
        ),
    )
}

fn velocity_of(v: f32) -> f32 {
    let scaled = if v > 1.5 { v / 127.0 } else { v };
    scaled.clamp(0.05, 1.0)
}

pub fn render_file(
    score_path: &Path,
    out: &Path,
    keep_stems: bool,
    mp3: bool,
) -> std::io::Result<ScoreReport> {
    let raw = fs::read_to_string(score_path)?;
    let doc: ScoreDoc = serde_json::from_str(&raw).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("invalid score json: {e}"),
        )
    })?;
    if doc.layers.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "score has no layers",
        ));
    }
    if !(20.0..=400.0).contains(&doc.bpm) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("bpm {} out of range", doc.bpm),
        ));
    }

    let table = Arc::new(WaveTable::new());
    let seed = doc.seed.unwrap_or(1);
    let bar = 240.0 / doc.bpm;
    let mut latest = 0.0f64;
    for layer in &doc.layers {
        for e in &layer.events {
            latest = latest.max(e.time + e.duration);
        }
    }
    let bars = doc
        .bars
        .unwrap_or_else(|| (latest / bar).ceil().max(1.0) as usize);
    let span = (bars as f64 * bar).max(latest);
    let tail = doc
        .layers
        .iter()
        .filter_map(|l| by_slug(&l.kit_voice).map(|v| v.tail))
        .fold(0.0f32, f32::max);
    let len = span + tail as f64 + 0.5;
    let total = (len as f32 * sample_rate()) as usize;
    let mut left_sum = vec![0.0f32; total];
    let mut right_sum = vec![0.0f32; total];

    fs::create_dir_all(out)?;
    let stems_dir = out.join("stems");
    if keep_stems {
        fs::create_dir_all(&stems_dir)?;
    }

    let mut summaries = Vec::new();
    for (index, layer) in doc.layers.iter().enumerate() {
        if by_slug(&layer.kit_voice).is_none() {
            return Err(unknown_voice_error(&layer.kit_voice));
        }
        let mut rng = Rng::new(seed.wrapping_mul(1000).wrapping_add(index as u64));
        let amount = layer.jitter.unwrap_or(doc.jitter.unwrap_or(0.45));
        let patch = jittered_patch(&mut rng, &layer.kit_voice, amount, [0.4, 0.35, 0.4, 0.3]);
        let automation: Vec<Automation> = layer
            .automation
            .iter()
            .filter_map(|a| {
                parse_target(&a.target).map(|target| Automation {
                    target,
                    from: a.from.clamp(0.0, 1.0),
                    to: a.to.clamp(0.0, 1.0),
                    curve: a.curve,
                })
            })
            .collect();
        let events: Vec<synth::RenderEvent> = layer
            .events
            .iter()
            .map(|e| synth::RenderEvent {
                time: e.time.max(0.0),
                note: e.note.clamp(12, 108),
                velocity: velocity_of(e.velocity),
                hold: e.duration.max(0.0),
            })
            .collect();
        let spec = LayerSpec {
            name: layer.name.clone(),
            slug: slugify(&layer.name),
            kit_slug: by_slug(&layer.kit_voice).expect("checked").slug,
            peak_db: layer.peak_db.unwrap_or(-10.0),
            events,
            patch,
            automation,
        };
        let (mut left, mut right) = synth::render_layer(&table, &spec, len, span as f32);
        wav::finish(&mut left, &mut right, sample_rate(), spec.peak_db);
        if keep_stems {
            wav::write_wav(
                &stems_dir.join(format!("{:02}-{}.wav", index + 1, spec.slug)),
                sample_rate() as u32,
                &left,
                &right,
            )?;
        }
        for (dst, src) in left_sum.iter_mut().zip(left.iter()) {
            *dst += *src;
        }
        for (dst, src) in right_sum.iter_mut().zip(right.iter()) {
            *dst += *src;
        }
        summaries.push(LayerSummary {
            name: layer.name.clone(),
            kit_voice: layer.kit_voice.clone(),
            events: layer.events.len(),
        });
    }

    wav::trim_tail(&mut left_sum, &mut right_sum, sample_rate());
    wav::fade_edges(&mut left_sum, &mut right_sum, sample_rate());
    wav::normalize(&mut left_sum, &mut right_sum, -3.0);
    let stats = wav::stats(&left_sum, &right_sum, sample_rate());
    let mix_path = out.join("mix.wav");
    wav::write_wav(&mix_path, sample_rate() as u32, &left_sum, &right_sum)?;
    if mp3 {
        super::mp3::alongside(&mix_path, super::mp3::DEFAULT_BITRATE_KBPS)?;
    }

    let report = ScoreReport {
        title: doc
            .title
            .clone()
            .unwrap_or_else(|| score_path.display().to_string()),
        bpm: doc.bpm,
        bars,
        seconds: stats.seconds,
        peak_db: stats.peak_db,
        rms_db: stats.rms_db,
        mix: out.join("mix.wav").display().to_string(),
        layers: summaries,
    };
    fs::write(
        out.join("report.json"),
        serde_json::to_vec_pretty(&report).unwrap_or_default(),
    )?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_score(dir: &Path, body: &str) -> std::path::PathBuf {
        let path = dir.join("score.json");
        fs::write(&path, body).unwrap();
        path
    }

    #[test]
    fn renders_a_score_and_rejects_unknown_voices() {
        let dir = std::env::temp_dir().join("lumen-score-test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = write_score(
            &dir,
            r#"{
              "title": "t",
              "bpm": 120,
              "bars": 2,
              "seed": 3,
              "layers": [
                {"name": "Kick", "kit_voice": "kick-deep", "peak_db": -6,
                 "events": [{"time": 0.0, "note": 36, "velocity": 0.9, "duration": 0.02},
                            {"time": 1.0, "note": 36, "velocity": 0.8, "duration": 0.02}]},
                {"name": "Pad", "kit_voice": "pad-warm", "events": [
                    {"time": 0.0, "note": 57, "velocity": 90, "duration": 3.5}],
                 "automation": [{"target": "cutoff", "from": 0.3, "to": 0.8, "curve": 0.4}]}
              ]
            }"#,
        );
        let out = dir.join("out");
        let report = render_file(&path, &out, true, false).unwrap();
        assert_eq!(report.layers.len(), 2);
        assert!(report.seconds > 3.5);
        assert!(out.join("mix.wav").is_file());
        assert!(out.join("report.json").is_file());
        assert_eq!(fs::read_dir(out.join("stems")).unwrap().count(), 2);
        let rows = super::super::analyze(&out).unwrap();
        assert_eq!(rows.len(), 1);
        assert!(rows[0].stats.peak_db > -4.0);

        let bad = write_score(
            &dir,
            r#"{"bpm": 120, "layers": [{"name": "x", "kit_voice": "nope", "events": []}]}"#,
        );
        let err = render_file(&bad, &out, false, false).unwrap_err();
        assert!(err.to_string().contains("unknown kit voice"));
        let _ = fs::remove_dir_all(&dir);
    }
}
