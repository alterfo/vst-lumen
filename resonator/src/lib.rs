#[path = "../../src/filter.rs"]
mod filter;
#[path = "../../src/sympathetic.rs"]
mod sympathetic;
#[path = "../../src/widgets.rs"]
mod widgets;

mod bank;
mod gui;
mod params;
mod presets;

use nih_plug::prelude::*;
use params::ResonatorParams;
use std::num::NonZeroU32;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use sympathetic::{SympFrame, Sympathetic};

const BLOCK: usize = 32;

pub struct Shared {
    pub peak: AtomicF32,
    pub input: AtomicF32,
}

pub struct LumenResonator {
    params: Arc<ResonatorParams>,
    shared: Arc<Shared>,
    sym: Sympathetic,
}

impl Default for LumenResonator {
    fn default() -> Self {
        Self {
            params: Arc::new(ResonatorParams::default()),
            shared: Arc::new(Shared {
                peak: AtomicF32::new(0.0),
                input: AtomicF32::new(0.0),
            }),
            sym: Sympathetic::new(48000.0),
        }
    }
}

impl LumenResonator {
    fn render_block(&mut self, l: &mut [f32], r: &mut [f32]) {
        let steps = l.len() as u32;
        let p = self.params.clone();
        let input_gain = util::db_to_gain(p.input.smoothed.next_step(steps));
        let out_gain = util::db_to_gain(p.output.smoothed.next_step(steps));
        let dry = p.dry.smoothed.next_step(steps);

        let mut dry_l = [0.0f32; BLOCK];
        let mut dry_r = [0.0f32; BLOCK];
        let mut in_peak = 0.0f32;
        for i in 0..l.len() {
            l[i] *= input_gain;
            r[i] *= input_gain;
            dry_l[i] = l[i];
            dry_r[i] = r[i];
            in_peak = in_peak.max(l[i].abs()).max(r[i].abs());
        }

        self.sym.process(
            l,
            r,
            &SympFrame {
                mix: p.mix.smoothed.next_step(steps),
                root: p.root.value(),
                scale: p.scale.value().dsp(),
                register: p.register.value(),
                decay: p.decay.value(),
                damping: p.damping.value(),
                detune_cents: p.detune.value(),
                drift_cents: 0.0,
                sub_mix: p.sub.smoothed.next_step(steps),
                sub_drop: p.drop.value(),
            },
        );

        let mut peak = 0.0f32;
        for i in 0..l.len() {
            l[i] = (l[i] - dry_l[i] * (1.0 - dry)) * out_gain;
            r[i] = (r[i] - dry_r[i] * (1.0 - dry)) * out_gain;
            peak = peak.max(l[i].abs()).max(r[i].abs());
        }
        let shown = self.shared.peak.load(Ordering::Relaxed) * 0.92;
        self.shared.peak.store(peak.max(shown), Ordering::Relaxed);
        let shown_in = self.shared.input.load(Ordering::Relaxed) * 0.92;
        self.shared.input.store(in_peak.max(shown_in), Ordering::Relaxed);
    }
}

impl Plugin for LumenResonator {
    const NAME: &'static str = "Lumen Resonator";
    const VENDOR: &'static str = "Lumen Synth";
    const URL: &'static str = "";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: NonZeroU32::new(2),
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        gui::create(self.params.clone(), self.shared.clone())
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        self.sym = Sympathetic::new(buffer_config.sample_rate);
        true
    }

    fn reset(&mut self) {}

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let total = buffer.samples();
        let channels = buffer.as_slice();
        let (left, right) = channels.split_at_mut(1);
        let left = &mut *left[0];
        let right = &mut *right[0];
        let mut start = 0;
        while start < total {
            let end = (start + BLOCK).min(total);
            self.render_block(&mut left[start..end], &mut right[start..end]);
            start = end;
        }
        ProcessStatus::KeepAlive
    }
}

impl ClapPlugin for LumenResonator {
    const CLAP_ID: &'static str = "org.lumen-synth.lumen-resonator";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("Sympathetic resonator with sub bass");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[ClapFeature::AudioEffect, ClapFeature::Stereo];
}

impl Vst3Plugin for LumenResonator {
    const VST3_CLASS_ID: [u8; 16] = *b"LumenResonatorFx";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx];
}

nih_export_clap!(LumenResonator);
nih_export_vst3!(LumenResonator);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cup_ping_becomes_bounded_bass() {
        let mut s = LumenResonator::default();
        s.sym = Sympathetic::new(48000.0);
        for (_, ptr, _) in s.params.param_map() {
            if let ParamPtr::FloatParam(p) = ptr {
                unsafe { (*p).smoothed.reset((*p).value()) };
            }
        }
        s.params.dry.smoothed.reset(0.0);
        let mut out = Vec::new();
        for blk in 0..(48000 / BLOCK) {
            let mut l = [0.0f32; BLOCK];
            let mut r = [0.0f32; BLOCK];
            if blk < 40 {
                for i in 0..BLOCK {
                    let t = (blk * BLOCK + i) as f32 / 48000.0;
                    l[i] = (std::f32::consts::TAU * 2600.0 * t).sin() * 0.5;
                    r[i] = l[i];
                }
            }
            s.render_block(&mut l, &mut r);
            for v in l.iter() {
                assert!(v.is_finite() && v.abs() < 4.0);
            }
            out.extend_from_slice(&l);
        }
        let tail = &out[24000..];
        let peak = tail.iter().fold(0.0f32, |a, v| a.max(v.abs()));
        assert!(peak > 0.005, "peak {peak}");
    }
}
