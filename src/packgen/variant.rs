use super::engine::{patch_from, Patch};
use super::kit::by_slug;
use super::synth::RenderEvent;

pub const BARS: usize = 16;
pub const STEPS_PER_BAR: usize = 16;
pub const CHORD_BARS: usize = 2;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Auto {
    Cutoff,
    Morph,
    Table,
    GranMix,
    ResMix,
    Noise,
    DelayMix,
    FilterMode,
}

impl Auto {
    pub fn label(&self) -> &'static str {
        match self {
            Auto::Cutoff => "cutoff",
            Auto::Morph => "morph",
            Auto::Table => "table",
            Auto::GranMix => "grain",
            Auto::ResMix => "resonator",
            Auto::Noise => "noise",
            Auto::DelayMix => "delay",
            Auto::FilterMode => "filter-mode",
        }
    }
}

pub struct Automation {
    pub target: Auto,
    pub from: f32,
    pub to: f32,
    pub curve: f32,
}

pub struct LayerSpec {
    pub name: String,
    pub slug: String,
    pub kit_slug: &'static str,
    pub peak_db: f32,
    pub events: Vec<RenderEvent>,
    pub patch: Patch,
    pub automation: Vec<Automation>,
    pub duck: bool,
}

impl LayerSpec {
    pub fn patch_at(&self, t: f32, total: f32) -> Patch {
        let mut patch = self.patch.clone();
        if self.automation.is_empty() || total <= 0.0 {
            return patch;
        }
        let x = (t / total).clamp(0.0, 1.0);
        for a in &self.automation {
            let shape = if a.curve.abs() < 1e-3 {
                x
            } else {
                let k = a.curve;
                (x * (1.0 + k)) / (1.0 + k * x)
            };
            let value = a.from + (a.to - a.from) * shape;
            match a.target {
                Auto::Cutoff => patch.cutoff = 60.0 * 2f32.powf(value * 7.0),
                Auto::Morph => patch.morph = value,
                Auto::Table => patch.pos[0] = value,
                Auto::GranMix => patch.gran[0] = value,
                Auto::ResMix => patch.res[1] = value,
                Auto::Noise => patch.noise = value,
                Auto::DelayMix => patch.fx[6] = value,
                Auto::FilterMode => patch.f_mode = value,
            }
        }
        patch
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SectionKind {
    Intro,
    Main,
    Break,
    Drop,
    Outro,
}

impl SectionKind {
    fn label(&self) -> &'static str {
        match self {
            SectionKind::Intro => "intro",
            SectionKind::Main => "main",
            SectionKind::Break => "break",
            SectionKind::Drop => "drop",
            SectionKind::Outro => "outro",
        }
    }

    fn sparse(&self) -> bool {
        matches!(self, SectionKind::Intro | SectionKind::Outro)
    }
}

pub struct Section {
    pub kind: SectionKind,
    pub start_bar: usize,
    pub bars: usize,
}

pub struct Archetype {
    pub name: &'static str,
    pub kick: &'static [&'static str],
    pub backbeat: &'static [&'static str],
    pub hat: &'static [&'static str],
    pub perc: &'static [&'static str],
    pub bass: &'static [&'static str],
    pub pad: &'static [&'static str],
    pub arp: &'static [&'static str],
    pub keys: &'static [&'static str],
    pub texture: &'static [&'static str],
    pub chatty: f32,
    pub fx: [f32; 4],
}

pub struct Variant {
    pub seed: u64,
    pub bpm: f64,
    pub key: String,
    pub mode: &'static str,
    pub root: u8,
    pub swing: f64,
    pub bars: usize,
    pub archetype: &'static str,
    pub character: &'static str,
    pub duck_db: f32,
    pub duck_attack: f32,
    pub duck_release: f32,
    pub glue_db: f32,
    pub sections: Vec<Section>,
    pub layers: Vec<LayerSpec>,
}

pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    pub fn unit(&mut self) -> f32 {
        (self.next() >> 40) as f32 / 16777216.0
    }

    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.unit()
    }

    pub fn int(&mut self, lo: i32, hi: i32) -> i32 {
        if hi <= lo {
            return lo;
        }
        lo + (self.next() % ((hi - lo + 1) as u64)) as i32
    }

    pub fn chance(&mut self, p: f32) -> bool {
        self.unit() < p
    }

    pub fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        items[self.next() as usize % items.len()]
    }
}

const MODES: &[(&str, [i32; 7])] = &[
    ("phrygian", [0, 1, 3, 5, 7, 8, 10]),
    ("dorian", [0, 2, 3, 5, 7, 9, 10]),
    ("minor", [0, 2, 3, 5, 7, 8, 10]),
    ("lydian", [0, 2, 4, 6, 7, 9, 11]),
    ("pentatonic", [0, 3, 5, 7, 10, 12, 15]),
];

const ROOTS: &[(&str, u8)] = &[
    ("A", 33),
    ("Bb", 34),
    ("C", 36),
    ("D", 38),
    ("E", 40),
    ("F", 41),
    ("G", 31),
    ("G#", 32),
];

const ARCHETYPES: &[Archetype] = &[
    Archetype {
        name: "dub techno",
        kick: &["kick-deep", "kick-sub"],
        backbeat: &["rimshot", "clap"],
        hat: &["hat-closed", "hat-pedal"],
        perc: &["shaker", "woodblock"],
        bass: &["bass-sub", "bass-reese"],
        pad: &["pad-dark", "pad-warm", "drone-low"],
        arp: &["arp-stab", "arp-pluck"],
        keys: &["key-felt", "arp-stab"],
        texture: &["texture-drone", "texture-air"],
        chatty: 0.25,
        fx: [0.5, 0.3, 0.35, 0.2],
    },
    Archetype {
        name: "ambient drift",
        kick: &["kick-sub"],
        backbeat: &["rimshot"],
        hat: &["hat-pedal"],
        perc: &["shaker", "bell"],
        bass: &["bass-sub"],
        pad: &["pad-dark", "pad-airy", "pad-warm"],
        arp: &["arp-stab", "music-box"],
        keys: &["key-felt", "arp-stab"],
        texture: &["texture-air", "texture-rain", "texture-drone"],
        chatty: 0.55,
        fx: [0.65, 0.2, 0.5, 0.5],
    },
    Archetype {
        name: "trip hop",
        kick: &["kick-punch", "kick-deep"],
        backbeat: &["snare-body", "snare-tight", "clap"],
        hat: &["hat-closed"],
        perc: &["bongo", "conga", "shaker"],
        bass: &["bass-pluck", "bass-acid"],
        pad: &["pad-dark", "pad-warm"],
        arp: &["arp-stab", "arp-pluck"],
        keys: &["key-felt", "arp-pluck"],
        texture: &["texture-air", "texture-rain"],
        chatty: 0.35,
        fx: [0.35, 0.35, 0.45, 0.25],
    },
    Archetype {
        name: "broken beat",
        kick: &["kick-punch", "kick-deep"],
        backbeat: &["rimshot", "snare-tight"],
        hat: &["hat-closed", "hat-pedal"],
        perc: &["woodblock", "bongo", "conga"],
        bass: &["bass-acid", "bass-reese"],
        pad: &["pad-dark", "pad-warm", "pad-airy"],
        arp: &["arp-stab", "arp-pluck"],
        keys: &["key-felt", "arp-stab"],
        texture: &["texture-air", "texture-downlifter"],
        chatty: 0.4,
        fx: [0.28, 0.4, 0.4, 0.3],
    },
    Archetype {
        name: "cinematic",
        kick: &["kick-sub", "kick-deep"],
        backbeat: &["snare-body", "clap"],
        hat: &["hat-open", "hat-pedal"],
        perc: &["bell", "tom"],
        bass: &["bass-sub", "bass-pluck"],
        pad: &["pad-dark", "pad-airy", "pad-warm"],
        arp: &["arp-stab", "arp-chime"],
        keys: &["key-felt", "arp-stab"],
        texture: &["texture-air", "texture-riser", "texture-downlifter"],
        chatty: 0.5,
        fx: [0.7, 0.25, 0.55, 0.45],
    },
    Archetype {
        name: "lo-fi house",
        kick: &["kick-deep", "kick-punch"],
        backbeat: &["clap", "snare-tight"],
        hat: &["hat-closed", "hat-open"],
        perc: &["shaker", "conga"],
        bass: &["bass-pluck", "bass-sub"],
        pad: &["pad-dark", "pad-warm"],
        arp: &["arp-stab", "arp-pluck"],
        keys: &["key-felt", "arp-stab"],
        texture: &["texture-air", "texture-drone"],
        chatty: 0.4,
        fx: [0.4, 0.35, 0.45, 0.25],
    },
];

const CHARACTERS: &[(&str, [f32; 6])] = &[
    ("bright", [1.7, 0.05, 0.05, -0.05, 0.4, -0.1]),
    ("dark", [0.55, 0.1, 0.1, 0.05, -0.5, 0.0]),
    ("wet", [1.0, 0.0, -0.05, 0.3, 0.0, 0.15]),
    ("dry", [1.05, -0.05, 0.05, -0.2, 0.0, -0.1]),
    ("resonant", [0.85, 0.35, 0.05, 0.05, 0.2, 0.0]),
    ("gritty", [1.2, 0.15, 0.3, -0.05, 0.3, 0.05]),
    ("detuned", [1.0, 0.05, 0.05, 0.05, 1.6, 0.0]),
    ("grainy", [0.9, 0.05, 0.0, 0.1, 0.2, 0.45]),
];

const WIDE_BASS: &[&str] = &["bass-sub", "bass-reese", "bass-acid", "bass-pluck"];
const WIDE_PAD: &[&str] = &["pad-dark", "pad-warm", "pad-airy", "drone-low"];
const WIDE_ARP: &[&str] = &["arp-stab", "arp-pluck", "arp-chime", "music-box"];
const WIDE_KEYS: &[&str] = &["key-felt", "arp-stab", "music-box"];
const WIDE_HAT: &[&str] = &["hat-closed", "hat-pedal"];
const WIDE_PERC: &[&str] = &["shaker", "conga", "bongo", "woodblock", "bell", "tom"];

fn bar_secs(bpm: f64) -> f64 {
    240.0 / bpm
}

fn layer_values(slug: &str) -> &'static [(&'static str, f32)] {
    by_slug(slug).map(|v| v.values).unwrap_or(&[])
}

pub fn jittered_patch(rng: &mut Rng, slug: &str, amount: f32, fx_bias: [f32; 4]) -> Patch {
    jitter(rng, slug, amount, fx_bias, [1.0, 0.0, 0.0, 0.0, 0.0, 0.0])
}

fn jitter(rng: &mut Rng, slug: &str, amount: f32, fx_bias: [f32; 4], character: [f32; 6]) -> Patch {
    let mut patch = patch_from(layer_values(slug));
    let a = amount.clamp(0.0, 1.0);
    patch.cutoff *= 2f32.powf(rng.range(-1.2, 1.2) * a);
    patch.resonance = (patch.resonance + rng.range(-0.25, 0.3) * a).clamp(0.0, 0.85);
    patch.drive = (patch.drive + rng.range(-0.25, 0.25) * a).clamp(0.0, 0.9);
    patch.drift = (patch.drift + rng.range(-0.25, 0.3) * a).clamp(0.0, 0.95);
    patch.organic = (patch.organic + rng.range(-0.3, 0.3) * a).clamp(0.0, 1.0);
    patch.chaos = (patch.chaos + rng.range(-0.25, 0.3) * a).clamp(0.0, 0.9);
    patch.pos[0] = (patch.pos[0] + rng.range(-0.4, 0.4) * a).clamp(0.0, 1.0);
    patch.pos[1] = (patch.pos[1] + rng.range(-0.4, 0.4) * a).clamp(0.0, 1.0);
    patch.unison[0] = (patch.unison[0] + rng.range(-1.5, 1.5) * a).clamp(1.0, 6.0);
    patch.unison[1] = (patch.unison[1] + rng.range(-1.5, 1.5) * a).clamp(1.0, 6.0);
    patch.detune[0] = (patch.detune[0] + rng.range(-0.2, 0.2) * a).clamp(0.0, 0.85);
    patch.spread[0] = (patch.spread[0] + rng.range(-0.3, 0.3) * a).clamp(0.0, 1.0);
    patch.morph = (patch.morph + rng.range(-0.35, 0.35) * a).clamp(0.05, 0.95);
    patch.fx[0] = (patch.fx[0] + (fx_bias[2] - 0.4) * a + rng.range(-0.15, 0.2) * a).clamp(0.0, 0.85);
    patch.fx[7] = (patch.fx[7] + (fx_bias[0] - 0.4) * a + rng.range(-0.12, 0.2) * a).clamp(0.0, 0.85);
    patch.fx[8] = (patch.fx[8] + (fx_bias[0] * 8.0 - 3.0) * a + rng.range(-1.0, 2.0) * a).clamp(0.6, 14.0);
    patch.fx[6] = (patch.fx[6] + (fx_bias[1] - 0.3) * a + rng.range(-0.08, 0.15) * a).clamp(0.0, 0.55);
    patch.fx[3] = (patch.fx[3] + rng.range(-0.08, 0.12) * a).clamp(0.06, 0.95);
    patch.fx[4] = (patch.fx[4] + rng.range(-0.12, 0.15) * a).clamp(0.0, 0.7);
    patch.res[1] = (patch.res[1] + rng.range(-0.25, 0.25) * a).clamp(0.0, 0.95);
    patch.res[2] = (patch.res[2] + rng.range(-0.8, 2.0) * a).clamp(0.1, 6.5);
    patch.body[0] = (patch.body[0] + rng.range(-0.2, 0.25) * a).clamp(0.0, 0.9);
    patch.body[1] = (patch.body[1] + rng.range(-0.25, 0.5) * a).clamp(0.5, 2.0);
    patch.gran[0] = (patch.gran[0] + (fx_bias[3] - 0.25) * a + rng.range(-0.15, 0.2) * a).clamp(0.0, 0.85);
    patch.gran[2] = (patch.gran[2] + rng.range(-6.0, 10.0) * a).clamp(1.0, 80.0);
    patch.sub = (patch.sub + rng.range(-0.15, 0.15) * a).clamp(0.0, 1.0);
    patch.amp[1] = (patch.amp[1] + rng.range(-0.1, 0.5) * a).clamp(0.002, 3.0);
    patch.amp[3] = (patch.amp[3] + rng.range(-0.1, 0.8) * a).clamp(0.002, 6.0);
    patch.lfo[1] = (patch.lfo[1] + rng.range(-0.2, 0.7) * a).clamp(0.02, 8.0);
    patch.evo[0] = (patch.evo[0] + rng.range(-0.03, 0.06) * a).clamp(0.005, 0.9);
    patch.cutoff = (patch.cutoff * character[0]).clamp(40.0, 12000.0);
    patch.resonance = (patch.resonance + character[1]).clamp(0.0, 0.85);
    patch.drive = (patch.drive + character[2]).clamp(0.0, 0.9);
    patch.fx[7] = (patch.fx[7] + character[3]).clamp(0.0, 0.9);
    patch.unison[0] = (patch.unison[0] + character[4]).clamp(1.0, 6.5);
    patch.unison[1] = (patch.unison[1] + character[4] * 0.6).clamp(1.0, 6.5);
    patch.gran[0] = (patch.gran[0] + character[5]).clamp(0.0, 0.88);
    patch
}

fn hit_steps(rng: &mut Rng, kind: SectionKind) -> Vec<(usize, f32)> {
    match kind {
        SectionKind::Intro | SectionKind::Outro => {
            let templates: [&[usize]; 3] = [&[0, 10], &[0, 8], &[0]];
            rng.pick(&templates).iter().map(|s| (*s, 0.85)).collect()
        }
        SectionKind::Break => vec![(0, 0.7)],
        _ => {
            let templates: [&[usize]; 7] = [
                &[0, 10],
                &[0, 6, 10],
                &[0, 7, 10],
                &[0, 6, 10, 14],
                &[0, 3, 8, 11],
                &[0, 10, 14],
                &[0, 4, 10],
            ];
            let mut steps: Vec<(usize, f32)> = rng.pick(&templates).iter().map(|s| (*s, 0.95)).collect();
            if rng.chance(0.45) && !steps.iter().any(|(s, _)| *s == 13) {
                steps.push((13, 0.6));
            }
            steps
        }
    }
}

fn backbeat_steps(rng: &mut Rng, kind: SectionKind) -> Vec<(usize, f32)> {
    if kind == SectionKind::Intro || kind == SectionKind::Break {
        return Vec::new();
    }
    let mut steps = vec![(4usize, 0.9f32), (12, 0.9)];
    if rng.chance(0.5) {
        steps.push((7, 0.32));
    }
    if rng.chance(0.4) {
        steps.push((15, 0.45));
    }
    if kind == SectionKind::Drop && rng.chance(0.5) {
        steps.push((14, 0.6));
    }
    steps
}

fn hat_steps(rng: &mut Rng, kind: SectionKind) -> Vec<(usize, f32)> {
    let stride = match kind {
        SectionKind::Break => 1,
        SectionKind::Intro | SectionKind::Outro => 4,
        _ => {
            if rng.chance(0.5) {
                1
            } else {
                2
            }
        }
    };
    let mut steps = Vec::new();
    let mut i = 0;
    while i < STEPS_PER_BAR {
        if rng.chance(0.88) {
            steps.push((i, if i % 4 == 0 { 0.8 } else { 0.55 }));
        }
        i += stride;
    }
    steps
}

fn perc_steps(rng: &mut Rng, kind: SectionKind) -> Vec<(usize, f32)> {
    let count = match kind {
        SectionKind::Intro => 1,
        SectionKind::Break => 2,
        SectionKind::Drop => 4,
        _ => 3,
    };
    let mut steps = Vec::new();
    let mut guard = 0;
    while steps.len() < count && guard < 64 {
        guard += 1;
        let s = rng.int(0, (STEPS_PER_BAR - 1) as i32) as usize;
        if !steps.iter().any(|(x, _)| *x == s) {
            steps.push((s, rng.range(0.3, 0.85)));
        }
    }
    steps.sort_by(|a, b| a.0.cmp(&b.0));
    steps
}

fn open_hat_steps(rng: &mut Rng, kind: SectionKind) -> Vec<(usize, f32)> {
    match kind {
        SectionKind::Break | SectionKind::Drop => {
            vec![(6, 0.7), (14, 0.7)]
        }
        SectionKind::Main => {
            if rng.chance(0.6) {
                vec![(14, 0.7)]
            } else {
                vec![(6, 0.65), (11, 0.55)]
            }
        }
        _ => Vec::new(),
    }
}

fn chord_slots(rng: &mut Rng) -> [i32; 8] {
    let walks: [[i32; 8]; 7] = [
        [0, 5, 3, 4, 0, 5, 6, 4],
        [0, 0, 5, 5, 3, 3, 4, 4],
        [0, 6, 5, 4, 0, 3, 4, 6],
        [0, 2, 5, 6, 0, 4, 5, 3],
        [0, 4, 3, 6, 0, 5, 1, 4],
        [0, 5, 6, 0, 3, 4, 5, 6],
        [0, 3, 0, 4, 6, 4, 5, 3],
    ];
    let base = rng.pick(&walks);
    let mut out = base;
    if rng.chance(0.4) {
        let i = rng.int(0, 7) as usize;
        out[i] = rng.int(0, 6);
    }
    out
}

fn scale_degree(scale: [i32; 7], root: u8, degree: i32) -> i32 {
    let mut step = degree;
    let mut octave = 0;
    while step >= scale.len() as i32 {
        step -= scale.len() as i32;
        octave += 1;
    }
    while step < 0 {
        step += scale.len() as i32;
        octave -= 1;
    }
    root as i32 + scale[step as usize] + 12 * octave
}

fn chord_tones(rng: &mut Rng, scale: [i32; 7], root: u8, degree: i32) -> [u8; 4] {
    let spread = [0i32, 2, 4, 6];
    let mut out = [0u8; 4];
    for (i, step) in spread.iter().enumerate() {
        out[i] = (scale_degree(scale, root, degree + step) + rng.int(0, 1) * 0).clamp(24, 96) as u8;
    }
    out[3] = (out[0] + 12).min(100) as u8;
    out
}

fn automation_for(rng: &mut Rng, patch: &Patch, sections: &[Section]) -> Vec<Automation> {
    let mut plan = Vec::new();
    let opens_late = sections
        .iter()
        .any(|s| s.kind == SectionKind::Drop);
    let targets: [Auto; 8] = [
        Auto::Cutoff,
        Auto::Morph,
        Auto::GranMix,
        Auto::ResMix,
        Auto::Table,
        Auto::DelayMix,
        Auto::Noise,
        Auto::FilterMode,
    ];
    let count = rng.int(1, 3) as usize;
    for _ in 0..count {
        let target = rng.pick(&targets);
        let (from, to) = match target {
            Auto::Cutoff => {
                let norm = ((patch.cutoff.max(60.0) / 60.0).log2() / 7.0).clamp(0.0, 1.0);
                if opens_late {
                    (norm * 0.35, (norm * 1.6).min(1.0))
                } else if rng.chance(0.5) {
                    (norm, norm * 0.4)
                } else {
                    (norm * 0.8, (norm * 1.5).min(1.0))
                }
            }
            Auto::Morph => (patch.morph, rng.range(0.05, 0.95)),
            Auto::Table => (patch.pos[0], rng.range(0.05, 0.95)),
            Auto::GranMix => (patch.gran[0], rng.range(0.0, 0.75)),
            Auto::ResMix => (patch.res[1], rng.range(0.0, 0.9)),
            Auto::Noise => (patch.noise, rng.range(0.0, 0.2)),
            Auto::DelayMix => (patch.fx[6], rng.range(0.0, 0.4)),
            Auto::FilterMode => (patch.f_mode, rng.range(0.0, 0.9)),
        };
        plan.push(Automation {
            target,
            from,
            to,
            curve: rng.range(-0.7, 0.9),
        });
    }
    plan
}

#[allow(clippy::too_many_arguments)]
fn events_from(
    rng: &mut Rng,
    section: &Section,
    bpm: f64,
    swing: f64,
    steps: &[(usize, f32)],
    note: u8,
    hold_steps: f64,
    humanize: f32,
) -> Vec<RenderEvent> {
    let bar = bar_secs(bpm);
    let step = bar / STEPS_PER_BAR as f64;
    let mut out = Vec::new();
    for b in 0..section.bars {
        let bar_index = section.start_bar + b;
        let fill = b == section.bars - 1 && !section.kind.sparse();
        for (s, vel) in steps {
            let swing_shift = if s % 2 == 1 { swing * 0.5 * step } else { 0.0 };
            let time = bar_index as f64 * bar + *s as f64 * step + swing_shift;
            let velocity = (vel + rng.range(-humanize, humanize)).clamp(0.15, 1.0);
            out.push(RenderEvent {
                time,
                note,
                velocity,
                hold: if hold_steps <= 0.0 {
                    0.0
                } else {
                    hold_steps * step
                },
            });
        }
        if fill && rng.chance(0.7) {
            for s in [13usize, 14, 15] {
                out.push(RenderEvent {
                    time: bar_index as f64 * bar + s as f64 * step,
                    note,
                    velocity: 0.45 + 0.15 * (s - 13) as f32,
                    hold: 0.0,
                });
            }
        }
    }
    out
}

fn make_layer(
    rng: &mut Rng,
    name: &'static str,
    slug: &'static str,
    kit_slug: &'static str,
    peak_db: f32,
    events: Vec<RenderEvent>,
    jitter_amount: f32,
    fx_bias: [f32; 4],
    character: [f32; 6],
    automate: bool,
    sections: &[Section],
) -> LayerSpec {
    let patch = jitter(rng, kit_slug, jitter_amount, fx_bias, character);
    let automation = if automate {
        automation_for(rng, &patch, sections)
    } else {
        Vec::new()
    };
    LayerSpec {
        name: name.to_string(),
        slug: slug.to_string(),
        kit_slug,
        peak_db,
        events,
        patch,
        automation,
        duck: automate,
    }
}

fn section_plan(rng: &mut Rng) -> Vec<Section> {
    let templates: [&[(SectionKind, usize)]; 4] = [
        &[
            (SectionKind::Intro, 2),
            (SectionKind::Main, 6),
            (SectionKind::Break, 4),
            (SectionKind::Main, 2),
            (SectionKind::Outro, 2),
        ],
        &[
            (SectionKind::Intro, 4),
            (SectionKind::Main, 4),
            (SectionKind::Break, 2),
            (SectionKind::Drop, 4),
            (SectionKind::Outro, 2),
        ],
        &[
            (SectionKind::Main, 4),
            (SectionKind::Break, 4),
            (SectionKind::Drop, 4),
            (SectionKind::Outro, 4),
        ],
        &[
            (SectionKind::Intro, 2),
            (SectionKind::Break, 2),
            (SectionKind::Main, 4),
            (SectionKind::Drop, 6),
            (SectionKind::Outro, 2),
        ],
    ];
    let template = rng.pick(&templates);
    let mut out = Vec::new();
    let mut bar = 0;
    for (kind, bars) in template {
        out.push(Section {
            kind: *kind,
            start_bar: bar,
            bars: *bars,
        });
        bar += bars;
    }
    out
}

impl Variant {
    pub fn new(seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        let bpm_options = [88.0f64, 96.0, 100.0, 104.0, 110.0, 116.0, 120.0, 124.0, 128.0, 132.0];
        let bpm = bpm_options[(seed as usize) % bpm_options.len()]
            + (rng.unit() * 4.0 - 2.0).round() as f64;
        let (mode_name, scale) = rng.pick(MODES);
        let (root_name, root) = rng.pick(ROOTS);
        let archetype = &ARCHETYPES[rng.int(0, ARCHETYPES.len() as i32 - 1) as usize];
        let (character_name, character) = rng.pick(CHARACTERS);
        let swing: f64 = if rng.chance(0.45) {
            rng.range(0.08, 0.35) as f64
        } else {
            0.0
        };
        let sections = section_plan(&mut rng);
        let duck_db = rng.range(3.5, 6.5);
        let duck_attack = rng.range(0.001, 0.004);
        let duck_release = rng.range(0.09, 0.2);
        let glue_db = rng.range(0.5, 2.0);
        let bars = sections.iter().map(|s| s.bars).sum::<usize>();
        assert_eq!(bars, BARS);
        let slots = chord_slots(&mut rng);
        let fx_bias = archetype.fx;

        let kick = rng.pick(archetype.kick);
        let backbeat = rng.pick(archetype.backbeat);
        let mut bass_pool: Vec<&'static str> = archetype.bass.to_vec();
        let mut pad_pool: Vec<&'static str> = archetype.pad.to_vec();
        let mut arp_pool: Vec<&'static str> = archetype.arp.to_vec();
        let mut keys_pool: Vec<&'static str> = archetype.keys.to_vec();
        let mut hat_pool: Vec<&'static str> = archetype.hat.to_vec();
        let mut perc_pool: Vec<&'static str> = archetype.perc.to_vec();
        if rng.chance(0.4) {
            bass_pool.push(rng.pick(WIDE_BASS));
        }
        if rng.chance(0.4) {
            pad_pool.push(rng.pick(WIDE_PAD));
        }
        if rng.chance(0.45) {
            arp_pool.push(rng.pick(WIDE_ARP));
        }
        if rng.chance(0.35) {
            keys_pool.push(rng.pick(WIDE_KEYS));
        }
        if rng.chance(0.3) {
            hat_pool.push(rng.pick(WIDE_HAT));
        }
        if rng.chance(0.35) {
            perc_pool.push(rng.pick(WIDE_PERC));
        }
        let bass_voice = rng.pick(&bass_pool);
        let pad = rng.pick(&pad_pool);
        let arp = rng.pick(&arp_pool);
        let keys = rng.pick(&keys_pool);
        let mut hat = rng.pick(&hat_pool);
        if hat == "hat-open" {
            hat = "hat-closed";
        }
        let perc = rng.pick(&perc_pool);

        let mut layers = Vec::new();
        let bar = bar_secs(bpm);
        let step = bar / STEPS_PER_BAR as f64;

        let mut kick_events = Vec::new();
        let mut backbeat_events = Vec::new();
        let mut hat_events = Vec::new();
        let mut open_events = Vec::new();
        let mut perc_events = Vec::new();
        let mut texture_events = Vec::new();
        let mut bass_events = Vec::new();
        let mut pad_events = Vec::new();
        let mut arp_events = Vec::new();
        let mut keys_events = Vec::new();

        for section in &sections {
            let kicks = hit_steps(&mut rng, section.kind);
            kick_events.extend(events_from(
                &mut rng,
                section,
                bpm,
                swing,
                &kicks,
                36,
                0.0,
                0.05,
            ));

            let backbeats = backbeat_steps(&mut rng, section.kind);
            backbeat_events.extend(events_from(
                &mut rng,
                section,
                bpm,
                swing,
                &backbeats,
                36,
                0.0,
                0.1,
            ));

            let hats = hat_steps(&mut rng, section.kind);
            hat_events.extend(events_from(
                &mut rng,
                section,
                bpm,
                swing,
                &hats,
                36,
                0.0,
                0.08,
            ));

            let opens = open_hat_steps(&mut rng, section.kind);
            open_events.extend(events_from(
                &mut rng,
                section,
                bpm,
                swing,
                &opens,
                36,
                0.0,
                0.08,
            ));

            let percs = perc_steps(&mut rng, section.kind);
            perc_events.extend(events_from(
                &mut rng,
                section,
                bpm,
                swing,
                &percs,
                43,
                0.0,
                0.12,
            ));

            for slot in 0..(section.bars / CHORD_BARS).max(1) {
                let slot_index = (section.start_bar + slot * CHORD_BARS) / CHORD_BARS;
                let degree = slots[slot_index % slots.len()];
                let chord = chord_tones(&mut rng, scale, root, degree);
                let start = (section.start_bar + slot * CHORD_BARS) as f64 * bar;

                for note in chord.iter().take(3) {
                    pad_events.push(RenderEvent {
                        time: start,
                        note: *note,
                        velocity: rng.range(0.42, 0.68),
                        hold: bar * CHORD_BARS as f64 * 0.97,
                    });
                }

                let bass_hits: &[(usize, i32, f32)] = match section.kind {
                    SectionKind::Intro | SectionKind::Outro => &[(0, 0, 0.85), (8, 0, 0.6)],
                    SectionKind::Break => &[(0, 0, 0.75), (6, 7, 0.55), (10, 0, 0.5)],
                    _ => &[(0, 0, 0.9), (6, 7, 0.62), (10, 0, 0.72), (12, 12, 0.6)],
                };
                for (s, interval, vel) in bass_hits {
                    let base = scale_degree(scale, root, degree);
                    bass_events.push(RenderEvent {
                        time: start + *s as f64 * step * 2.0,
                        note: (base + interval).clamp(20, 60) as u8,
                        velocity: *vel,
                        hold: step * 3.0,
                    });
                }

                let order: &[usize] = match section.kind {
                    SectionKind::Break => &[0, 2, 1, 3],
                    SectionKind::Drop => &[0, 1, 2, 1, 3, 2, 1, 0],
                    _ => &[0, 1, 2, 3, 2, 1],
                };
                let steps_total = STEPS_PER_BAR * CHORD_BARS;
                for s in 0..steps_total {
                    if rng.chance(archetype.chatty * 0.3) {
                        continue;
                    }
                    let idx = order[(s + slot_index) % order.len()];
                    let octave = if rng.chance(0.2) { 12 } else { 0 };
                    arp_events.push(RenderEvent {
                        time: start + s as f64 * step,
                        note: chord[idx].saturating_add(octave).min(104),
                        velocity: if s % 4 == 0 { 0.8 } else { 0.52 },
                        hold: step * 0.9,
                    });
                }

                if matches!(section.kind, SectionKind::Main | SectionKind::Drop) {
                    let motif: &[(usize, usize, f32)] = match rng.int(0, 2) {
                        0 => &[(0, 0, 0.7), (3, 1, 0.6), (6, 2, 0.65), (10, 1, 0.55), (14, 2, 0.6)],
                        1 => &[(0, 2, 0.7), (4, 0, 0.55), (8, 1, 0.65), (12, 3, 0.6)],
                        _ => &[(0, 0, 0.65), (6, 3, 0.7), (11, 2, 0.55)],
                    };
                    for (s, tone, vel) in motif {
                        if *tone >= chord.len() {
                            continue;
                        }
                        keys_events.push(RenderEvent {
                            time: start + *s as f64 * step,
                            note: chord[*tone].saturating_add(12).min(100),
                            velocity: *vel,
                            hold: step * 4.0,
                        });
                    }
                }
            }

            if section.kind == SectionKind::Break {
                for b in 0..section.bars {
                    texture_events.push(RenderEvent {
                        time: (section.start_bar + b) as f64 * bar,
                        note: (root as i32 + 24) as u8,
                        velocity: rng.range(0.35, 0.6),
                        hold: bar,
                    });
                }
            }
        }

        layers.push(make_layer(
            &mut rng,
            "Kick",
            "kick",
            kick,
            -5.0,
            kick_events,
            0.45,
            fx_bias,
            character,
            false,
            &sections,
        ));
        if !backbeat_events.is_empty() {
            layers.push(make_layer(
                &mut rng,
                "Backbeat",
                "snare",
                backbeat,
                -8.0,
                backbeat_events,
                0.5,
                fx_bias,
                character,
                false,
                &sections,
            ));
        }
        layers.push(make_layer(
            &mut rng,
            "Hats",
            "hats",
            hat,
            -17.0,
            hat_events,
            0.45,
            fx_bias,
            character,
            false,
            &sections,
        ));
        if !open_events.is_empty() {
            layers.push(make_layer(
                &mut rng,
                "Open hat",
                "hat-open",
                "hat-open",
                -19.0,
                open_events,
                0.4,
                fx_bias,
                character,
                false,
                &sections,
            ));
        }
        layers.push(make_layer(
            &mut rng,
            "Percussion",
            "perc",
            perc,
            -18.0,
            perc_events,
            0.55,
            fx_bias,
            character,
            false,
            &sections,
        ));
        layers.push(make_layer(
            &mut rng,
            "Bass",
            "bass",
            bass_voice,
            -7.0,
            bass_events,
            0.6,
            fx_bias,
            character,
            true,
            &sections,
        ));
        layers.push(make_layer(
            &mut rng,
            "Pad",
            "pad",
            pad,
            -11.0,
            pad_events,
            0.55,
            fx_bias,
            character,
            true,
            &sections,
        ));
        layers.push(make_layer(
            &mut rng,
            "Arp",
            "arp",
            arp,
            -13.0,
            arp_events,
            0.6,
            fx_bias,
            character,
            true,
            &sections,
        ));
        if !keys_events.is_empty() {
            layers.push(make_layer(
                &mut rng,
                "Keys",
                "keys",
                keys,
                -12.0,
                keys_events,
                0.6,
                fx_bias,
                character,
                true,
                &sections,
            ));
        }
        if !texture_events.is_empty() {
            let texture = rng.pick(archetype.texture);
            layers.push(make_layer(
                &mut rng,
                "Texture",
                "texture",
                texture,
                -22.0,
                texture_events,
                0.65,
                fx_bias,
                character,
                true,
                &sections,
            ));
        }

        Self {
            seed,
            bpm,
            key: format!("{} {}", root_name, mode_name),
            mode: mode_name,
            root,
            swing,
            bars,
            archetype: archetype.name,
            character: character_name,
            duck_db,
            duck_attack,
            duck_release,
            glue_db,
            sections,
            layers,
        }
    }

    pub fn total_secs(&self) -> f64 {
        self.bars as f64 * bar_secs(self.bpm)
    }

    pub fn form(&self) -> String {
        self.sections
            .iter()
            .map(|s| format!("{}x{}", s.kind.label(), s.bars))
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn describe(&self) -> String {
        let voices: Vec<&str> = self.layers.iter().map(|l| l.kit_slug).collect();
        format!(
            "seed {} · {:.0} BPM · {} · {} · {} · swing {:.2} · {}",
            self.seed,
            self.bpm,
            self.key,
            self.archetype,
            self.form(),
            self.swing,
            voices.join(", ")
        )
    }

    pub fn automation_summary(&self) -> Vec<String> {
        let mut out = Vec::new();
        for layer in &self.layers {
            for a in &layer.automation {
                out.push(format!("{}:{}", layer.slug, a.target.label()));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_is_deterministic_and_different_seeds_differ() {
        let a = Variant::new(7);
        let b = Variant::new(7);
        let c = Variant::new(8);
        assert_eq!(a.describe(), b.describe());
        for (x, y) in a.layers.iter().zip(b.layers.iter()) {
            assert_eq!(x.kit_slug, y.kit_slug);
            assert_eq!(x.events.len(), y.events.len());
            for (e1, e2) in x.events.iter().zip(y.events.iter()) {
                assert!((e1.time - e2.time).abs() < 1e-12);
                assert_eq!(e1.note, e2.note);
            }
        }
        assert_ne!(a.describe(), c.describe());
    }

    #[test]
    fn variants_cross_a_hundred_seeds_with_sane_music() {
        let mut bpms = Vec::new();
        let mut voices = std::collections::HashSet::new();
        let mut forms = std::collections::HashSet::new();
        let mut archetypes = std::collections::HashSet::new();
        for seed in 0..100 {
            let v = Variant::new(seed);
            assert!(v.bpm >= 86.0 && v.bpm <= 134.0, "seed {seed} bpm {}", v.bpm);
            assert_eq!(v.bars, BARS);
            assert_eq!(
                v.sections.iter().map(|s| s.bars).sum::<usize>(),
                BARS,
                "seed {seed} sections do not add up"
            );
            assert!(v.layers.len() >= 7, "seed {seed} layers {}", v.layers.len());
            assert!(
                v.total_secs() > 27.0 && v.total_secs() < 46.0,
                "seed {seed} total {}",
                v.total_secs()
            );
            bpms.push(v.bpm);
            forms.insert(v.form());
            archetypes.insert(v.archetype);
            for l in &v.layers {
                voices.insert(l.kit_slug);
                assert!(!l.events.is_empty(), "seed {seed} {} empty", l.slug);
                let total = v.total_secs();
                let mut last = -1.0f64;
                for e in &l.events {
                    assert!(e.time >= 0.0, "seed {seed} {} negative time", l.slug);
                    assert!(
                        e.time <= total + 1e-6,
                        "seed {seed} {} event past the end ({})",
                        l.slug,
                        e.time
                    );
                    assert!(e.note >= 12 && e.note <= 108, "seed {seed} note {}", e.note);
                    last = last.max(e.time);
                }
                assert!(
                    last > total * 0.4 || l.slug == "texture",
                    "seed {seed} {} stops early",
                    l.slug
                );
            }
        }
        bpms.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert!(bpms.first() != bpms.last(), "bpm never varies");
        assert!(voices.len() >= 18, "only {} distinct voices used", voices.len());
        assert!(forms.len() >= 3, "only {} distinct forms", forms.len());
        assert!(archetypes.len() >= 4, "only {} archetypes", archetypes.len());
    }

    #[test]
    fn automation_stays_inside_parameter_ranges() {
        let v = Variant::new(5);
        let total = v.total_secs() as f32;
        for layer in &v.layers {
            for i in 0..=10 {
                let t = total * i as f32 / 10.0;
                let patch = layer.patch_at(t, total);
                assert!(patch.cutoff >= 20.0 && patch.cutoff <= 20000.0);
                assert!(patch.morph >= 0.0 && patch.morph <= 1.0);
                assert!(patch.gran[0] >= 0.0 && patch.gran[0] <= 1.0);
                assert!(patch.res[1] >= 0.0 && patch.res[1] <= 1.0);
                assert!(patch.pos[0] >= 0.0 && patch.pos[0] <= 1.0);
                assert!(patch.fx[6] >= 0.0 && patch.fx[6] <= 1.0);
                assert!(patch.f_mode >= 0.0 && patch.f_mode <= 1.0);
            }
        }
    }
}
