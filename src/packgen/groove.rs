use super::synth::RenderEvent;

pub const BPM: f64 = 128.0;
pub const BARS: usize = 4;
pub const KEY: &str = "A phrygian";
pub const BAR_SECS: f64 = 240.0 / BPM;

pub struct Layer {
    pub name: &'static str,
    pub slug: &'static str,
    pub kit_slug: &'static str,
    pub peak_db: f32,
    pub events: Vec<RenderEvent>,
}

fn step(bar: usize, sixteenth: usize) -> f64 {
    (bar * 16 + sixteenth) as f64 * BAR_SECS / 16.0
}

fn hits(out: &mut Vec<RenderEvent>, bars: &[&[usize]], note: u8, hold: f64, accented: bool) {
    for (bar, steps) in bars.iter().enumerate() {
        for (i, s) in steps.iter().enumerate() {
            let velocity = if accented && i % 2 == 0 { 0.95 } else { 0.72 };
            out.push(RenderEvent {
                time: step(bar, *s),
                note,
                velocity,
                hold,
            });
        }
    }
}

pub fn layers() -> Vec<Layer> {
    let mut kick = Vec::new();
    hits(
        &mut kick,
        &[
            &[0, 10],
            &[0, 6, 10],
            &[0, 10, 14],
            &[0, 6, 10, 13],
        ],
        36,
        0.02,
        true,
    );

    let mut snare = Vec::new();
    hits(
        &mut snare,
        &[&[4, 12], &[4, 12], &[4, 12], &[4, 12, 14, 15]],
        36,
        0.02,
        true,
    );

    let mut hats = Vec::new();
    hits(
        &mut hats,
        &[
            &[0, 2, 8, 10, 12],
            &[0, 2, 8, 10, 12],
            &[0, 2, 8, 10, 12],
            &[0, 2, 8, 10, 12],
        ],
        36,
        0.02,
        true,
    );

    let mut open = Vec::new();
    hits(
        &mut open,
        &[&[6, 14], &[6, 14], &[6, 14], &[6, 14]],
        36,
        0.02,
        false,
    );

    let mut shaker = Vec::new();
    for bar in 0..BARS {
        for s in [1usize, 3, 5, 7, 9, 11, 13, 15] {
            shaker.push(RenderEvent {
                time: step(bar, s),
                note: 36,
                velocity: if s % 4 == 1 { 0.62 } else { 0.4 },
                hold: 0.0,
            });
        }
    }

    let mut perc = Vec::new();
    hits(
        &mut perc,
        &[&[3, 11], &[7, 11, 15], &[3, 11], &[7, 11, 13, 15]],
        43,
        0.02,
        false,
    );

    let roots = [33u8, 34, 41, 40];
    let mut bass = Vec::new();
    for (bar, root) in roots.iter().enumerate() {
        for (s, interval) in [(0usize, 0i32), (3, 0), (6, 7), (10, 0), (12, 12)] {
            bass.push(RenderEvent {
                time: step(bar, s),
                note: (*root as i32 + interval) as u8,
                velocity: if s == 0 { 0.9 } else { 0.7 },
                hold: BAR_SECS * 0.22,
            });
        }
    }

    let chords: [[u8; 3]; BARS] = [
        [57, 60, 64],
        [58, 62, 65],
        [53, 57, 60],
        [52, 56, 59],
    ];
    let mut pad = Vec::new();
    for (bar, chord) in chords.iter().enumerate() {
        for note in chord {
            pad.push(RenderEvent {
                time: step(bar, 0),
                note: *note,
                velocity: 0.6,
                hold: BAR_SECS * 0.98,
            });
        }
    }

    let mut arp = Vec::new();
    for (bar, chord) in chords.iter().enumerate() {
        for s in 0..16usize {
            let idx = (s + bar) % (chord.len() + 1);
            let note = if idx == chord.len() {
                chord[0] + 12
            } else {
                chord[idx]
            };
            arp.push(RenderEvent {
                time: step(bar, s),
                note,
                velocity: if s % 4 == 0 { 0.8 } else { 0.55 },
                hold: BAR_SECS * 0.1,
            });
        }
    }

    vec![
        Layer {
            name: "Kick",
            slug: "kick",
            kit_slug: "kick-deep",
            peak_db: -6.0,
            events: kick,
        },
        Layer {
            name: "Snare",
            slug: "snare",
            kit_slug: "snare-tight",
            peak_db: -8.0,
            events: snare,
        },
        Layer {
            name: "Hats",
            slug: "hats",
            kit_slug: "hat-closed",
            peak_db: -14.0,
            events: hats,
        },
        Layer {
            name: "Open hat",
            slug: "hat-open",
            kit_slug: "hat-open",
            peak_db: -16.0,
            events: open,
        },
        Layer {
            name: "Shaker",
            slug: "shaker",
            kit_slug: "shaker",
            peak_db: -18.0,
            events: shaker,
        },
        Layer {
            name: "Percussion",
            slug: "perc",
            kit_slug: "conga",
            peak_db: -16.0,
            events: perc,
        },
        Layer {
            name: "Bass",
            slug: "bass",
            kit_slug: "bass-pluck",
            peak_db: -8.0,
            events: bass,
        },
        Layer {
            name: "Pad",
            slug: "pad",
            kit_slug: "pad-warm",
            peak_db: -12.0,
            events: pad,
        },
        Layer {
            name: "Arp",
            slug: "arp",
            kit_slug: "arp-pluck",
            peak_db: -14.0,
            events: arp,
        },
    ]
}

pub fn total_secs() -> f64 {
    BARS as f64 * BAR_SECS
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packgen::kit::by_slug;

    #[test]
    fn every_layer_maps_to_a_kit_voice_and_stays_on_grid() {
        let total = total_secs();
        assert!((total - 7.5).abs() < 1e-9, "total {total}");
        for layer in layers() {
            assert!(
                by_slug(layer.kit_slug).is_some(),
                "{}: unknown kit voice {}",
                layer.slug,
                layer.kit_slug
            );
            assert!(!layer.events.is_empty(), "{}: no events", layer.slug);
            for e in &layer.events {
                assert!(e.time >= 0.0 && e.time < total, "{}: off grid", layer.slug);
                let grid = e.time / (BAR_SECS / 16.0);
                assert!(
                    (grid - grid.round()).abs() < 1e-6,
                    "{}: not on a 16th grid ({})",
                    layer.slug,
                    e.time
                );
            }
        }
    }
}
