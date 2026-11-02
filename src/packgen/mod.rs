use crate::wavetable::WaveTable;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

pub mod engine;
pub mod groove;
pub mod kit;
pub mod mp3;
pub mod score;
pub mod synth;
pub mod taste;
pub mod variant;
pub mod wav;

pub use kit::{by_slug, KitVoice, KIT};
pub use synth::sample_rate;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Drum,
    Perc,
    Bass,
    Synth,
    Texture,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    OneShots,
    Groove,
    Mixes,
    All,
}

impl From<&str> for Mode {
    fn from(s: &str) -> Self {
        match s {
            "groove" | "layers" => Mode::Groove,
            "mixes" | "mix" => Mode::Mixes,
            "all" => Mode::All,
            _ => Mode::OneShots,
        }
    }
}

pub fn default_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join("Music Projects/Samples/lumen-kit")
}

pub fn default_dir_from(path: &str) -> PathBuf {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        default_dir()
    } else {
        PathBuf::from(shellexpand_home(trimmed))
    }
}

fn shellexpand_home(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        let home = std::env::var("HOME").unwrap_or_default();
        format!("{home}/{rest}")
    } else {
        path.to_string()
    }
}

pub struct KitProgress {
    pub running: AtomicBool,
    pub done: AtomicUsize,
    pub total: AtomicUsize,
    pub error: Mutex<Option<String>>,
    pub status: Mutex<String>,
}

impl KitProgress {
    pub fn new() -> Self {
        Self {
            running: AtomicBool::new(false),
            done: AtomicUsize::new(0),
            total: AtomicUsize::new(0),
            error: Mutex::new(None),
            status: Mutex::new(String::new()),
        }
    }

    fn say(&self, text: impl Into<String>) {
        if let Ok(mut s) = self.status.lock() {
            *s = text.into();
        }
    }

    fn fail(&self, text: impl Into<String>) {
        let text = text.into();
        if let Ok(mut e) = self.error.lock() {
            *e = Some(text.clone());
        }
        self.say(text);
    }

    pub fn snapshot(&self) -> (bool, usize, usize, String) {
        let status = self.status.lock().map(|s| s.clone()).unwrap_or_default();
        (
            self.running.load(Ordering::Relaxed),
            self.done.load(Ordering::Relaxed),
            self.total.load(Ordering::Relaxed),
            status,
        )
    }
}

impl Default for KitProgress {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
pub struct KitJob {
    pub progress: Arc<KitProgress>,
}

#[derive(Serialize)]
struct VoiceReport {
    file: String,
    name: String,
    category: Category,
    note: u8,
    seconds: f32,
    peak_db: f32,
    rms_db: f32,
    centroid_hz: f32,
    band_low: f32,
    band_mid: f32,
    band_high: f32,
}

#[derive(Serialize)]
struct KitManifest {
    instrument: String,
    sample_rate: u32,
    bit_depth: u16,
    channels: u16,
    voices: Vec<VoiceReport>,
}

#[derive(Serialize)]
struct LayerReport {
    file: String,
    name: String,
    kit_voice: String,
    events: usize,
    peak_db: f32,
    rms_db: f32,
}

#[derive(Serialize)]
struct GrooveManifest {
    bpm: f64,
    key: String,
    bars: usize,
    seconds: f64,
    grid: String,
    mix: String,
    layers: Vec<LayerReport>,
}

fn package_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

impl KitJob {
    pub fn new() -> Self {
        Self {
            progress: Arc::new(KitProgress::new()),
        }
    }

    pub fn start(&self, mode: Mode, dir: PathBuf) {
        if self.progress.running.swap(true, Ordering::SeqCst) {
            return;
        }
        self.progress.done.store(0, Ordering::Relaxed);
        self.progress.total.store(0, Ordering::Relaxed);
        if let Ok(mut e) = self.progress.error.lock() {
            *e = None;
        }
        let progress = self.progress.clone();
        std::thread::spawn(move || {
            let result = run_with_progress(mode, &dir, &progress);
            match result {
                Ok(summary) => progress.say(summary),
                Err(err) => progress.fail(format!("failed: {err}")),
            }
            progress.running.store(false, Ordering::SeqCst);
        });
    }

    pub fn start_mixes(&self, options: MixOptions, dir: PathBuf) {
        if self.progress.running.swap(true, Ordering::SeqCst) {
            return;
        }
        self.progress.done.store(0, Ordering::Relaxed);
        self.progress.total.store(options.count, Ordering::Relaxed);
        if let Ok(mut e) = self.progress.error.lock() {
            *e = None;
        }
        let progress = self.progress.clone();
        std::thread::spawn(move || {
            let result = run_mixes_with_progress(&dir, &options, &progress);
            match result {
                Ok(summary) => progress.say(summary),
                Err(err) => progress.fail(format!("failed: {err}")),
            }
            progress.running.store(false, Ordering::SeqCst);
        });
    }
}

impl Default for KitJob {
    fn default() -> Self {
        Self::new()
    }
}

pub fn run(mode: Mode, dir: &Path) -> std::io::Result<String> {
    run_with_progress(mode, dir, &KitProgress::new())
}

pub struct MixOptions {
    pub count: usize,
    pub seed: u64,
    pub keep_stems: bool,
    pub mp3: bool,
}

impl Default for MixOptions {
    fn default() -> Self {
        Self {
            count: 100,
            seed: 1,
            keep_stems: false,
            mp3: false,
        }
    }
}

pub fn run_mixes(dir: &Path, options: &MixOptions) -> std::io::Result<String> {
    run_mixes_with_progress(dir, options, &KitProgress::new())
}

#[derive(Serialize, Clone)]
struct MixReport {
    index: usize,
    seed: u64,
    folder: String,
    bpm: f64,
    key: String,
    archetype: String,
    form: String,
    swing: f64,
    bars: usize,
    seconds: f32,
    peak_db: f32,
    rms_db: f32,
    automation: Vec<String>,
    voices: Vec<String>,
    character: String,
}

#[derive(Serialize)]
struct MixIndex {
    instrument: String,
    seed: u64,
    count: usize,
    bpm_range: [f64; 2],
    sample_rate: u32,
    bit_depth: u16,
    mixes: Vec<serde_json::Value>,
}

fn render_variant(
    table: &Arc<WaveTable>,
    variant: &variant::Variant,
    out: &Path,
    keep_stems: bool,
    mp3: bool,
    index: usize,
) -> std::io::Result<MixReport> {
    fs::create_dir_all(out)?;
    let span = variant.total_secs() as f32;
    let tail = variant
        .layers
        .iter()
        .filter_map(|l| by_slug(l.kit_slug).map(|v| v.tail))
        .fold(0.0f32, f32::max);
    let len = (span as f64) + tail as f64 + 0.5;
    let mix_len = (len as f32 * sample_rate()) as usize;
    let mut left_sum = vec![0.0f32; mix_len];
    let mut right_sum = vec![0.0f32; mix_len];
    let mut voices = Vec::new();
    for layer in &variant.layers {
        let (mut left, mut right) = synth::render_layer(table, layer, len, span);
        wav::finish(&mut left, &mut right, sample_rate(), layer.peak_db);
        if keep_stems {
            wav::write_wav(
                &out.join(format!("{}.wav", layer.slug)),
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
        voices.push(layer.kit_slug.to_string());
    }
    wav::trim_tail(&mut left_sum, &mut right_sum, sample_rate());
    wav::fade_edges(&mut left_sum, &mut right_sum, sample_rate());
    wav::normalize(&mut left_sum, &mut right_sum, -3.0);
    let stats = wav::stats(&left_sum, &right_sum, sample_rate());
    let mix_path = out.join("mix.wav");
    wav::write_wav(&mix_path, sample_rate() as u32, &left_sum, &right_sum)?;
    if mp3 {
        mp3::alongside(&mix_path, mp3::DEFAULT_BITRATE_KBPS)?;
    }
    let report = MixReport {
        index,
        seed: variant.seed,
        folder: out
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
        bpm: variant.bpm,
        key: variant.key.clone(),
        archetype: variant.archetype.to_string(),
        form: variant.form(),
        swing: variant.swing,
        bars: variant.bars,
        seconds: stats.seconds,
        peak_db: stats.peak_db,
        rms_db: stats.rms_db,
        automation: variant.automation_summary(),
        character: variant.character.to_string(),
        voices,
    };
    fs::write(
        out.join("variant.json"),
        serde_json::to_vec_pretty(&report).unwrap_or_default(),
    )?;
    Ok(report)
}

fn existing_entries(root: &Path) -> (Vec<serde_json::Value>, usize, u64, Vec<Vec<String>>) {
    let mut entries = Vec::new();
    let mut max_index = 0usize;
    let mut max_seed = 0u64;
    let mut voice_sets = Vec::new();
    let Ok(raw) = fs::read_to_string(root.join("index.json")) else {
        return (entries, max_index, max_seed, voice_sets);
    };
    let Ok(doc) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return (entries, max_index, max_seed, voice_sets);
    };
    let Some(mixes) = doc.get("mixes").and_then(|m| m.as_array()) else {
        return (entries, max_index, max_seed, voice_sets);
    };
    for entry in mixes {
        if let Some(index) = entry.get("index").and_then(|v| v.as_u64()) {
            max_index = max_index.max(index as usize);
        }
        if let Some(seed) = entry.get("seed").and_then(|v| v.as_u64()) {
            max_seed = max_seed.max(seed);
        }
        if let Some(voices) = entry.get("voices").and_then(|v| v.as_array()) {
            voice_sets.push(
                voices
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>(),
            );
        }
        entries.push(entry.clone());
    }
    (entries, max_index, max_seed, voice_sets)
}

fn voice_set(variant: &variant::Variant) -> Vec<String> {
    variant
        .layers
        .iter()
        .map(|l| l.kit_slug.to_string())
        .collect()
}

fn voice_distance(a: &[String], b: &[String]) -> usize {
    let mut diff = 0;
    for item in a {
        if !b.contains(item) {
            diff += 1;
        }
    }
    for item in b {
        if !a.contains(item) {
            diff += 1;
        }
    }
    diff
}

fn select_variants(
    taste: &taste::Taste,
    existing: &[Vec<String>],
    start_seed: u64,
    count: usize,
) -> Vec<(u64, variant::Variant)> {
    let mut picked: Vec<(u64, variant::Variant)> = Vec::new();
    let mut picked_sets: Vec<Vec<String>> = Vec::new();
    let mut seed = start_seed;
    let attempts = 6000u64;
    for pass in 0..3 {
        let min_distance = match pass {
            0 => 4,
            1 => 3,
            _ => 2,
        };
        let mut tries = 0u64;
        while picked.len() < count && tries < attempts {
            tries += 1;
            let candidate = variant::Variant::new(seed);
            seed = seed.wrapping_add(1);
            if pass == 0 && taste.enabled() && taste.score(&candidate) < taste.rejection_threshold() {
                continue;
            }
            let set = voice_set(&candidate);
            let near_picked = picked_sets
                .iter()
                .map(|s| voice_distance(&set, s))
                .min()
                .unwrap_or(usize::MAX);
            let near_existing = existing
                .iter()
                .map(|s| voice_distance(&set, s))
                .min()
                .unwrap_or(usize::MAX);
            if near_picked < min_distance || near_existing < min_distance {
                continue;
            }
            picked_sets.push(set);
            picked.push((candidate.seed, candidate));
        }
        if picked.len() >= count {
            break;
        }
    }
    picked
}

fn run_mixes_with_progress(
    dir: &Path,
    options: &MixOptions,
    progress: &KitProgress,
) -> std::io::Result<String> {
    let count = options.count.max(1);
    let root = dir.join("mixes");
    fs::create_dir_all(&root)?;
    let taste = taste::Taste::load(&root);
    let (existing, max_index, max_seed, existing_sets) = existing_entries(&root);
    let start_seed = options.seed.max(max_seed + 1);
    let picked = select_variants(&taste, &existing_sets, start_seed, count);
    if picked.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "no variants passed the taste filter",
        ));
    }
    progress.total.store(picked.len() + 1, Ordering::Relaxed);
    let table = Arc::new(WaveTable::new());
    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .min(8)
        .min(picked.len());
    let picked = Arc::new(picked);

    let results: Vec<Vec<MixReport>> = std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for worker in 0..threads {
            let table = table.clone();
            let root = root.clone();
            let picked = picked.clone();
            let keep_stems = options.keep_stems;
            let mp3 = options.mp3;
            handles.push(scope.spawn(move || {
                let mut local = Vec::new();
                let mut i = worker;
                while i < picked.len() {
                    let (seed, variant) = &picked[i];
                    let index = max_index + 1 + i;
                    let folder = format!("{:03}-seed{}", index, seed);
                    let path = root.join(&folder);
                    if !path.join("mix.wav").is_file() {
                        if let Ok(report) =
                            render_variant(&table, variant, &path, keep_stems, mp3, index)
                        {
                            progress.say(format!("mix {} · {}", index, variant.describe()));
                            local.push(report);
                        }
                    }
                    progress.done.fetch_add(1, Ordering::Relaxed);
                    i += threads;
                }
                local
            }));
        }
        handles
            .into_iter()
            .filter_map(|h| h.join().ok())
            .collect()
    });

    let mut new_reports: Vec<MixReport> = Vec::new();
    for chunk in results {
        new_reports.extend(chunk);
    }
    new_reports.sort_by_key(|r| r.index);
    progress.say("writing mix index");

    let mut merged = existing;
    for report in &new_reports {
        merged.push(serde_json::to_value(report).unwrap_or(serde_json::Value::Null));
    }
    let bpm_values: Vec<f64> = merged
        .iter()
        .filter_map(|e| e.get("bpm").and_then(|v| v.as_f64()))
        .collect();
    let bpm_min = bpm_values.iter().copied().fold(f64::MAX, f64::min);
    let bpm_max = bpm_values.iter().copied().fold(f64::MIN, f64::max);
    let (bpm_min, bpm_max) = if bpm_values.is_empty() {
        (0.0, 0.0)
    } else {
        (bpm_min, bpm_max)
    };
    let index = MixIndex {
        instrument: format!("Lumen {} mixes", package_version()),
        seed: options.seed,
        count: merged.len(),
        bpm_range: [bpm_min, bpm_max],
        sample_rate: sample_rate() as u32,
        bit_depth: 24,
        mixes: merged,
    };
    fs::write(
        root.join("index.json"),
        serde_json::to_vec_pretty(&index).unwrap_or_default(),
    )?;

    let mut table_md = String::from(
        "| # | folder | BPM | key | archetype | form | peak | voices | automation |\n|---|---|---|---|---|---|---|---|---|\n",
    );
    for entry in &index.mixes {
        let field = |name: &str| {
            entry
                .get(name)
                .map(|v| match v {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                })
                .unwrap_or_default()
        };
        let voices = entry
            .get("voices")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        let automation = entry
            .get("automation")
            .and_then(|v| v.as_array())
            .map(|a| {
                let items: Vec<&str> = a.iter().filter_map(|v| v.as_str()).collect();
                if items.is_empty() {
                    "none".to_string()
                } else {
                    items.join(", ")
                }
            })
            .unwrap_or_else(|| "none".to_string());
        let peak = entry
            .get("peak_db")
            .and_then(|v| v.as_f64())
            .unwrap_or(-3.0);
        let bpm = entry
            .get("bpm")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        table_md.push_str(&format!(
            "| {} | `{}` | {:.0} | {} | {} | {} | {:.1} dBFS | {} | {} |\n",
            field("index"),
            field("folder"),
            bpm,
            field("key"),
            field("archetype"),
            field("form"),
            peak,
            voices,
            automation
        ));
    }
    let kept = index
        .mixes
        .iter()
        .filter(|e| {
            e.get("folder")
                .and_then(|f| f.as_str())
                .map(|f| root.join(f).join("mix.wav").is_file())
                .unwrap_or(false)
        })
        .count();
    fs::write(
        root.join("INDEX.md"),
        format!(
            "# Lumen mixes\n\n{} mixes on disk ({} in the index), {:.0}–{:.0} BPM, 48 kHz / 24-bit stereo.\n\nNew batches continue after seed {}; deleted folders are treated as rejected and steer the next selection.\n\n{}",
            kept,
            index.count,
            bpm_min,
            bpm_max,
            max_seed,
            table_md
        ),
    )?;
    progress.done.fetch_add(1, Ordering::Relaxed);
    Ok(format!(
        "{} new mixes ({} in index, {} on disk) -> {}",
        new_reports.len(),
        index.count,
        kept,
        root.display()
    ))
}

fn run_with_progress(mode: Mode, dir: &Path, progress: &KitProgress) -> std::io::Result<String> {
    if mode == Mode::Mixes {
        return run_mixes_with_progress(dir, &MixOptions::default(), progress);
    }
    let one_shots = matches!(mode, Mode::OneShots | Mode::All);
    let groove = matches!(mode, Mode::Groove | Mode::All);
    let total = (if one_shots { KIT.len() } else { 0 })
        + (if groove { groove::layers().len() + 1 } else { 0 })
        + 1;
    progress.total.store(total, Ordering::Relaxed);

    fs::create_dir_all(dir)?;
    let table = Arc::new(WaveTable::new());
    let mut summary = Vec::new();
    let mut written = 0usize;

    if one_shots {
        let out = dir.join("one-shots");
        fs::create_dir_all(&out)?;
        let mut reports = Vec::new();
        for (index, voice) in KIT.iter().enumerate() {
            progress.say(format!("voice {}/{}: {}", index + 1, KIT.len(), voice.name));
            let file = format!("{:02}-{}.wav", index + 1, voice.slug);
            
            let len = (voice.hold + voice.tail + 0.3) as f64;
            let (mut left, mut right) =
                synth::render_voice(&table, voice, len);
            let stats = wav::finish(&mut left, &mut right, sample_rate(), voice.peak_db);
            wav::write_wav(&out.join(&file), sample_rate() as u32, &left, &right)?;
            reports.push(VoiceReport {
                file: file.clone(),
                name: voice.name.to_string(),
                category: voice.category,
                note: voice.note,
                seconds: stats.seconds,
                peak_db: stats.peak_db,
                rms_db: stats.rms_db,
                centroid_hz: stats.centroid_hz,
                band_low: stats.band_low,
                band_mid: stats.band_mid,
                band_high: stats.band_high,
            });
            progress.done.fetch_add(1, Ordering::Relaxed);
            written += 1;
        }
        let manifest = KitManifest {
            instrument: format!("Lumen {} one-shots", package_version()),
            sample_rate: sample_rate() as u32,
            bit_depth: 24,
            channels: 2,
            voices: reports,
        };
        fs::write(
            out.join("kit.json"),
            serde_json::to_vec_pretty(&manifest).unwrap_or_default(),
        )?;
        fs::write(out.join("README.md"), one_shot_readme())?;
        summary.push(format!("{} one-shots", written));
        progress.done.fetch_add(1, Ordering::Relaxed);
    }

    if groove {
        let out = dir.join("groove");
        fs::create_dir_all(&out)?;
        let mix_len =
            (groove::total_secs() as f32 + 4.0) as usize * sample_rate() as usize;
        let mut left_sum = vec![0.0f32; mix_len];
        let mut right_sum = left_sum.clone();
        let mut reports = Vec::new();
        let layers = groove::layers();
        for (index, layer) in layers.iter().enumerate() {
            progress.say(format!(
                "layer {}/{}: {}",
                index + 1,
                layers.len(),
                layer.name
            ));
            let voice = by_slug(layer.kit_slug).expect("groove layer maps to a kit voice");
            
            let len = groove::total_secs() + voice.tail as f64 + 0.3;
            let (mut left, mut right) =
                synth::render_events(&table, voice.values, &layer.events, len);
            let stats = wav::finish(&mut left, &mut right, sample_rate(), layer.peak_db);
            let file = format!("{}.wav", layer.slug);
            wav::write_wav(&out.join(&file), sample_rate() as u32, &left, &right)?;
            for (dst, src) in left_sum.iter_mut().zip(left.iter()) {
                *dst += *src;
            }
            for (dst, src) in right_sum.iter_mut().zip(right.iter()) {
                *dst += *src;
            }
            reports.push(LayerReport {
                file,
                name: layer.name.to_string(),
                kit_voice: layer.kit_slug.to_string(),
                events: layer.events.len(),
                peak_db: stats.peak_db,
                rms_db: stats.rms_db,
            });
            progress.done.fetch_add(1, Ordering::Relaxed);
        }
        progress.say("mixing layers");
        left_sum.truncate(
            (groove::total_secs() as f32 * sample_rate()) as usize
                + (3.0 * sample_rate()) as usize,
        );
        right_sum.truncate(left_sum.len());
        wav::trim_tail(&mut left_sum, &mut right_sum, sample_rate());
        wav::fade_edges(&mut left_sum, &mut right_sum, sample_rate());
        wav::normalize(&mut left_sum, &mut right_sum, -3.0);
        wav::write_wav(&out.join("mix.wav"), sample_rate() as u32, &left_sum, &right_sum)?;
        let manifest = GrooveManifest {
            bpm: groove::BPM,
            key: groove::KEY.to_string(),
            bars: groove::BARS,
            seconds: groove::total_secs(),
            grid: "16th".to_string(),
            mix: "mix.wav".to_string(),
            layers: reports,
        };
        fs::write(
            out.join("groove.json"),
            serde_json::to_vec_pretty(&manifest).unwrap_or_default(),
        )?;
        fs::write(out.join("README.md"), groove_readme())?;
        progress.done.fetch_add(1, Ordering::Relaxed);
        summary.push(format!("{} layered stems + mix", layers.len()));
    }

    let text = if summary.is_empty() {
        "nothing generated".to_string()
    } else {
        format!("{} -> {}", summary.join(", "), dir.display())
    };
    fs::write(dir.join("README.md"), root_readme())?;
    progress.say(text.clone());
    Ok(text)
}

fn root_readme() -> String {
    format!(
        "# Lumen kit

Rendered by the Lumen pack generator v{} — 48 kHz / 24-bit stereo WAV.

- `one-shots/` — {} voices (kicks, snares, hats, percussion, basses, pads, arps, keys, textures),
  one file each, peak-normalized per voice type. `kit.json` has peak/rms/centroid per file.
- `groove/` — the same voices played as a {} BPM {} arrangement: one stem per layer plus `mix.wav`,
  16th grid, every stem starts on bar 1 so they stack at 0.0.

Regenerate everything: `cargo run --release --bin lumen-kit -- --mode all`
or in the plugin: Output → Sample kit → Everything.
",
        package_version(),
        KIT.len(),
        groove::BPM,
        groove::KEY
    )
}

fn one_shot_readme() -> String {
    format!(
        "# Lumen one-shots

Generated by the Lumen pack generator (v{}), {} voices, 48 kHz / 24-bit stereo.
Every file is peak-normalized per voice type so they layer without babysitting levels.

| File | Voice | Category | Note |
|---|---|---|---|
{}
",
        package_version(),
        KIT.len(),
        KIT.iter()
            .enumerate()
            .map(|(i, v)| format!(
                "| `{:02}-{}.wav` | {} | {:?} | {} |",
                i + 1,
                v.slug,
                v.name,
                v.category,
                v.note
            ))
            .collect::<Vec<_>>()
            .join("\n")
    )
}

fn groove_readme() -> String {
    format!(
        "# Lumen demo groove

{} BPM, {}, {} bars (16th grid), every stem starts on bar 1 so they can be dropped
on top of each other at 0.0. `mix.wav` is the same stems summed, peak-normalized to -3 dBFS.

Use the stems to audition the kit against a real arrangement, then replace any layer
with your own one-shots from `../one-shots/`.
",
        groove::BPM,
        groove::KEY,
        groove::BARS
    )
}

pub struct AnalysisRow {
    pub file: String,
    pub stats: wav::SampleStats,
}

pub fn analyze(dir: &Path) -> std::io::Result<Vec<AnalysisRow>> {
    let mut rows = Vec::new();
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|x| x == "wav").unwrap_or(false))
        .collect();
    entries.sort();
    for path in entries {
        let bytes = fs::read(&path)?;
        let (left, right, sr) = read_wav(&bytes)?;
        rows.push(AnalysisRow {
            file: path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
            stats: wav::stats(&left, &right, sr),
        });
    }
    Ok(rows)
}

fn sign_extend_24(bytes: &[u8]) -> i32 {
    let raw = i32::from_le_bytes([bytes[0], bytes[1], bytes[2], 0]);
    if bytes[2] & 0x80 != 0 {
        raw - (1 << 24)
    } else {
        raw
    }
}

fn read_wav(bytes: &[u8]) -> std::io::Result<(Vec<f32>, Vec<f32>, f32)> {
    let bad = || std::io::Error::new(std::io::ErrorKind::InvalidData, "unsupported wav");
    if bytes.len() < 44 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(bad());
    }
    let channels = u16::from_le_bytes([bytes[22], bytes[23]]) as usize;
    if channels != 2 {
        return Err(bad());
    }
    let rate = u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]) as f32;
    let bits = u16::from_le_bytes([bytes[34], bytes[35]]) as usize;
    if bits != 24 {
        return Err(bad());
    }
    let data = &bytes[44..];
    let frames = data.len() / 6;
    let mut left = Vec::with_capacity(frames);
    let mut right = Vec::with_capacity(frames);
    for i in 0..frames {
        let a = &data[i * 6..i * 6 + 3];
        let b = &data[i * 6 + 3..i * 6 + 6];
        let la = sign_extend_24(a);
        let rb = sign_extend_24(b);
        left.push(la as f32 / 8388607.0);
        right.push(rb as f32 / 8388607.0);
    }
    Ok((left, right, rate))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_shot_dir_round_trips_through_the_analyzer() {
        let dir = std::env::temp_dir().join("lumen-packgen-analyze");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let table = Arc::new(WaveTable::new());
        let voice = &KIT[0];
        let (mut l, mut r) = synth::render_voice(&table, voice, 1.0);
        let stats = wav::finish(&mut l, &mut r, sample_rate(), voice.peak_db);
        wav::write_wav(&dir.join("a.wav"), sample_rate() as u32, &l, &r).unwrap();
        let rows = analyze(&dir).unwrap();
        assert_eq!(rows.len(), 1);
        assert!((rows[0].stats.peak_db - stats.peak_db).abs() < 0.5);
        assert!(rows[0].stats.centroid_hz > 20.0);
        let _ = fs::remove_file(dir.join("a.wav"));
    }

    #[test]
    fn mixes_are_reproducible_per_seed_and_differ_between_seeds() {
        let base = std::env::temp_dir().join("lumen-mixes-determinism");
        let _ = fs::remove_dir_all(&base);
        let options = MixOptions {
            count: 3,
            seed: 11,
            keep_stems: false,
            mp3: false,
        };
        run_mixes(&base, &options).unwrap();
        let repeat = base.join("repeat");
        run_mixes(&repeat, &options).unwrap();
        let first = fs::read(base.join("mixes/001-seed11/mix.wav")).unwrap();
        let twin = fs::read(repeat.join("mixes/001-seed11/mix.wav")).unwrap();
        assert_eq!(first, twin, "same seed must render byte-identical audio");
        let other = fs::read(base.join("mixes/002-seed12/mix.wav")).unwrap();
        assert_ne!(first, other, "different seeds must render different audio");
        assert!(first.len() > 44 * 100);
        for name in ["index.json", "INDEX.md"] {
            assert!(base.join("mixes").join(name).is_file(), "{name} missing");
        }
        let _ = fs::remove_dir_all(&base);
    }
}
