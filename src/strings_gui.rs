use crate::strings::{StringsParams, StringsShared};
use crate::strings_presets::{apply, available, delete};
use crate::presets::Preset;
use crate::widgets::*;
use nih_plug::prelude::*;
use nih_plug_egui::egui::{self, RichText};
use nih_plug_egui::create_egui_editor;
use std::sync::atomic::Ordering;
use std::sync::Arc;

struct State {
    preset: usize,
    list: Vec<&'static Preset>,
}

pub fn create(params: Arc<StringsParams>, shared: Arc<StringsShared>) -> Option<Box<dyn Editor>> {
    let egui_state = params.editor_state.clone();
    create_egui_editor(
        egui_state,
        State {
            preset: 0,
            list: available(),
        },
        |ctx, _| style(ctx),
        move |ctx, setter, state| {
            let p = &*params;
            let mut preset = state.preset.min(state.list.len() - 1);
            let mut removed = false;
            let knob = |ui: &mut egui::Ui, prm: &FloatParam, label: &str| {
                analog_knob(ui, setter, prm, label, Overlay::default());
            };
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(BG).inner_margin(16.0))
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                        ui.horizontal_top(|ui| {
                            ui.vertical(|ui| {
                                ui.label(RichText::new("LUMEN STRINGS").size(26.0).color(ACCENT).strong());
                                ui.label(
                                    RichText::new("resonant effect for electric guitar and erhu")
                                        .size(11.0)
                                        .color(MUTED),
                                );
                            });
                            ui.add_space(16.0);
                            section(ui, "Preset", 4, 134.0, |ui| {
                                ui.horizontal(|ui| {
                                    if ui.button("<").clicked() {
                                        preset = (preset + state.list.len() - 1) % state.list.len();
                                        apply(setter, p, state.list[preset]);
                                    }
                                    egui::ComboBox::from_id_salt("preset")
                                        .width(130.0)
                                        .selected_text(state.list[preset].name)
                                        .show_ui(ui, |ui| {
                                            for (i, pr) in state.list.iter().enumerate() {
                                                if ui.selectable_label(i == preset, pr.name).clicked() {
                                                    preset = i;
                                                    apply(setter, p, *pr);
                                                }
                                            }
                                        });
                                    if ui.button(">").clicked() {
                                        preset = (preset + 1) % state.list.len();
                                        apply(setter, p, state.list[preset]);
                                    }
                                    if ui.button("x").on_hover_text("Delete this preset").clicked() {
                                        removed = true;
                                    }
                                });
                                ui.label(RichText::new("input").size(10.0).color(MUTED));
                                meter(ui, 4.0 * CELL - 8.0, shared.input.load(Ordering::Relaxed));
                                ui.label(RichText::new("output").size(10.0).color(MUTED));
                                meter(ui, 4.0 * CELL - 8.0, shared.peak.load(Ordering::Relaxed));
                            });
                            section(ui, "Macros", 3, 134.0, |ui| {
                                grid(ui, |ui| {
                                    knob(ui, &p.bloom, "Bloom");
                                    knob(ui, &p.organic, "Organic");
                                    knob(ui, &p.organic_rate, "Org rate");
                                });
                            });
                            section(ui, "Output", 3, 134.0, |ui| {
                                grid(ui, |ui| {
                                    knob(ui, &p.input, "Input");
                                    knob(ui, &p.output, "Output");
                                    knob(ui, &p.mix, "Dry/Wet");
                                });
                            });
                        });

                        ui.horizontal_top(|ui| {
                            section(ui, "Drive", 2, 104.0, |ui| {
                                grid(ui, |ui| {
                                    knob(ui, &p.drive, "Drive");
                                    knob(ui, &p.tone, "Tone");
                                });
                            });
                            section(ui, "Touch", 3, 104.0, |ui| {
                                grid(ui, |ui| {
                                    knob(ui, &p.touch, "Touch");
                                    knob(ui, &p.touch_freq, "Freq");
                                    knob(ui, &p.touch_res, "Reson");
                                });
                            });
                            section(ui, "Erhu skin", 2, 104.0, |ui| {
                                grid(ui, |ui| {
                                    knob(ui, &p.skin, "Skin");
                                    knob(ui, &p.skin_freq, "Freq");
                                });
                            });
                            section(ui, "Body", 4, 104.0, |ui| {
                                grid(ui, |ui| {
                                    knob(ui, &p.body.mix, "Mix");
                                    knob(ui, &p.body.size, "Size");
                                    knob(ui, &p.body.decay, "Decay");
                                    knob(ui, &p.body.damping, "Damp");
                                });
                            });
                        });

                        ui.horizontal_top(|ui| {
                            section(ui, "Sympathetic strings", 5, 150.0, |ui| {
                                ui.horizontal(|ui| {
                                    choice(ui, setter, &p.sym_root, "root", 48.0);
                                    choice(ui, setter, &p.sym_scale, "scale", 110.0);
                                    choice(ui, setter, &p.sym_register, "reg", 48.0);
                                    choice(ui, setter, &p.sym_drop, "drop", 64.0);
                                });
                                grid(ui, |ui| {
                                    knob(ui, &p.sym_mix, "Mix");
                                    knob(ui, &p.sym_decay, "Decay");
                                    knob(ui, &p.sym_damping, "Damp");
                                    knob(ui, &p.sym_detune, "Detune");
                                    knob(ui, &p.sym_sub, "Sub");
                                });
                            });
                            section(ui, "Granular", 9, 150.0, |ui| {
                                ui.horizontal(|ui| {
                                    choice(ui, setter, &p.granular.interval, "ival", 90.0);
                                    toggle(ui, setter, &p.granular.freeze, "Freeze");
                                });
                                grid(ui, |ui| {
                                    knob(ui, &p.granular.mix, "Mix");
                                    knob(ui, &p.granular.size, "Size");
                                    knob(ui, &p.granular.density, "Density");
                                    knob(ui, &p.granular.position, "Position");
                                    knob(ui, &p.granular.jitter, "Jitter");
                                    knob(ui, &p.granular.pitch, "Pitch");
                                    knob(ui, &p.granular.pitch_spread, "Spread");
                                    knob(ui, &p.granular.reverse, "Reverse");
                                    knob(ui, &p.granular.stereo, "Width");
                                });
                            });
                        });

                        ui.horizontal_top(|ui| {
                            section(ui, "Chorus", 3, 104.0, |ui| {
                                grid(ui, |ui| {
                                    knob(ui, &p.fx.chorus_mix, "Mix");
                                    knob(ui, &p.fx.chorus_rate, "Rate");
                                    knob(ui, &p.fx.chorus_depth, "Depth");
                                });
                            });
                            section(ui, "Delay", 4, 104.0, |ui| {
                                grid(ui, |ui| {
                                    knob(ui, &p.fx.delay_mix, "Mix");
                                    knob(ui, &p.fx.delay_time, "Time");
                                    knob(ui, &p.fx.delay_feedback, "Feedback");
                                    knob(ui, &p.fx.delay_tone, "Tone");
                                });
                            });
                            section(ui, "Reverb", 4, 104.0, |ui| {
                                grid(ui, |ui| {
                                    knob(ui, &p.fx.reverb_mix, "Mix");
                                    knob(ui, &p.fx.reverb_decay, "Decay");
                                    knob(ui, &p.fx.reverb_damping, "Damp");
                                    knob(ui, &p.fx.reverb_predelay, "Pre-delay");
                                });
                            });
                        });
                    });
                });
            if removed {
                delete(state.list[preset].name);
                state.list.remove(preset);
                if state.list.is_empty() {
                    state.list = available();
                }
                preset = preset.min(state.list.len() - 1);
                apply(setter, p, state.list[preset]);
            }
            state.preset = preset;
        },
    )
}
