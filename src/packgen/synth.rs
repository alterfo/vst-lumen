use super::engine::{patch_from, Engine, BLOCK};
use super::kit::KitVoice;
use crate::wavetable::WaveTable;
use std::sync::Arc;

pub const SAMPLE_RATE: f32 = 48000.0;
const MIN_GATE: f64 = 0.008;

pub fn sample_rate() -> f32 {
    SAMPLE_RATE
}

pub struct RenderEvent {
    pub time: f64,
    pub note: u8,
    pub velocity: f32,
    pub hold: f64,
}

fn mix_into(dst: &mut [f32], src: &[f32], at: usize) {
    let n = src.len().min(dst.len().saturating_sub(at));
    for i in 0..n {
        dst[at + i] += src[i];
    }
}

pub fn render_layer(
    table: &Arc<WaveTable>,
    spec: &super::variant::LayerSpec,
    len_secs: f64,
    span_secs: f32,
) -> (Vec<f32>, Vec<f32>) {
    let sr = sample_rate();
    let total_render = (len_secs * sr as f64).round().max(BLOCK as f64) as usize;
    let mut engine = Engine::new(table.clone(), sr, spec.patch_at(0.0, span_secs));
    let mut left = vec![0.0f32; total_render];
    let mut right = vec![0.0f32; total_render];

    let mut on: Vec<(usize, u8, f32)> = Vec::new();
    let mut off: Vec<(usize, u8)> = Vec::new();
    for e in &spec.events {
        let start = (e.time * sr as f64).round().max(0.0) as usize;
        let stop = ((e.time + e.hold.max(MIN_GATE)) * sr as f64).round().max(0.0) as usize;
        on.push((start, e.note, e.velocity));
        off.push((stop, e.note));
    }
    on.sort_by_key(|(t, _, _)| *t);
    off.sort_by_key(|(t, _)| *t);

    let span = if span_secs > 0.0 {
        span_secs
    } else {
        total_render as f32 / sr
    };
    let mut next_on = 0;
    let mut next_off = 0;
    let mut pos = 0;
    let mut block_l = [0.0f32; BLOCK];
    let mut block_r = [0.0f32; BLOCK];
    while pos < total_render {
        while next_on < on.len() && on[next_on].0 <= pos {
            let (_, note, velocity) = on[next_on];
            engine.note_on(note, velocity);
            next_on += 1;
        }
        while next_off < off.len() && off[next_off].0 <= pos {
            let (_, note) = off[next_off];
            engine.note_off(note);
            next_off += 1;
        }
        let n = BLOCK.min(total_render - pos);
        block_l[..n].fill(0.0);
        block_r[..n].fill(0.0);
        let patch = spec.patch_at(pos as f32 / sr, span);
        engine.render(patch, &mut block_l[..n], &mut block_r[..n]);
        mix_into(&mut left, &block_l[..n], pos);
        mix_into(&mut right, &block_r[..n], pos);
        pos += n;
    }
    (left, right)
}

pub fn render_voice(
    table: &Arc<WaveTable>,
    voice: &KitVoice,
    len_secs: f64,
) -> (Vec<f32>, Vec<f32>) {
    let event = RenderEvent {
        time: 0.0,
        note: voice.note,
        velocity: voice.velocity,
        hold: voice.hold as f64,
    };
    render_events(table, voice.values, &[event], len_secs)
}

pub fn render_events(
    table: &Arc<WaveTable>,
    values: &[(&str, f32)],
    events: &[RenderEvent],
    len_secs: f64,
) -> (Vec<f32>, Vec<f32>) {
    let sr = sample_rate();
    let patch = patch_from(values);
    let mut engine = Engine::new(table.clone(), sr, patch.clone());
    let total = (len_secs * sr as f64).round().max(BLOCK as f64) as usize;
    let mut left = vec![0.0f32; total];
    let mut right = vec![0.0f32; total];

    let mut on: Vec<(usize, u8, f32)> = Vec::new();
    let mut off: Vec<(usize, u8)> = Vec::new();
    for e in events {
        let start = (e.time * sr as f64).round().max(0.0) as usize;
        let stop = ((e.time + e.hold.max(MIN_GATE)) * sr as f64).round().max(0.0) as usize;
        on.push((start, e.note, e.velocity));
        off.push((stop, e.note));
    }
    on.sort_by_key(|(t, _, _)| *t);
    off.sort_by_key(|(t, _)| *t);

    let mut next_on = 0;
    let mut next_off = 0;
    let mut pos = 0;
    let mut block_l = [0.0f32; BLOCK];
    let mut block_r = [0.0f32; BLOCK];
    while pos < total {
        while next_on < on.len() && on[next_on].0 <= pos {
            let (_, note, velocity) = on[next_on];
            engine.note_on(note, velocity);
            next_on += 1;
        }
        while next_off < off.len() && off[next_off].0 <= pos {
            let (_, note) = off[next_off];
            engine.note_off(note);
            next_off += 1;
        }
        let n = BLOCK.min(total - pos);
        block_l[..n].fill(0.0);
        block_r[..n].fill(0.0);
        engine.render(patch.clone(), &mut block_l[..n], &mut block_r[..n]);
        mix_into(&mut left, &block_l[..n], pos);
        mix_into(&mut right, &block_r[..n], pos);
        pos += n;
    }
    (left, right)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packgen::KIT;

    #[test]
    fn every_kit_voice_renders_audible_bounded_audio() {
        let table = Arc::new(WaveTable::new());
        for voice in KIT {
            let len = (voice.hold + voice.tail + 0.2) as f64;
            let (l, r) = render_voice(&table, voice, len);
            let mut peak = 0.0f32;
            let mut energy = 0.0f64;
            for (a, b) in l.iter().zip(r.iter()) {
                assert!(a.is_finite() && b.is_finite(), "{} non-finite", voice.slug);
                peak = peak.max(a.abs()).max(b.abs());
                energy += (a * a + b * b) as f64;
            }
            assert!(peak > 0.002, "{} is silent (peak {peak})", voice.slug);
            assert!(peak <= 1.2, "{} clips (peak {peak})", voice.slug);
            assert!(
                energy / (l.len() as f64 * 2.0) > 1e-7,
                "{} has no energy",
                voice.slug
            );
        }
    }
}

