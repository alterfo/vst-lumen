use crate::params::ResonatorParams;
use crate::presets::{apply, available, delete, Preset};
use crate::widgets::*;
use nih_plug::prelude::*;
use nih_plug_egui::create_egui_editor;
use nih_plug_egui::egui::{self, RichText};
use std::sync::atomic::Ordering;
use std::sync::Arc;

use crate::Shared;

struct State {
    preset: usize,
    list: Vec<&'static Preset>,
}

pub fn create(params: Arc<ResonatorParams>, shared: Arc<Shared>) -> Option<Box<dyn Editor>> {
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
                        ui.label(RichText::new("LUMEN RESONATOR").size(26.0).color(ACCENT).strong());
                        ui.label(
                            RichText::new("sympathetic strings with sub bass")
                                .size(11.0)
                                .color(MUTED),
                        );
                        ui.add_space(8.0);
                        ui.horizontal_top(|ui| {
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
                            section(ui, "Levels", 3, 134.0, |ui| {
                                grid(ui, |ui| {
                                    knob(ui, &p.input, "Input");
                                    knob(ui, &p.dry, "Dry");
                                    knob(ui, &p.output, "Output");
                                });
                            });
                        });
                        ui.add_space(8.0);
                        section(ui, "Sympathetic strings", 9, 150.0, |ui| {
                            ui.horizontal(|ui| {
                                choice(ui, setter, &p.root, "root", 48.0);
                                choice(ui, setter, &p.scale, "scale", 110.0);
                                choice(ui, setter, &p.register, "reg", 48.0);
                                choice(ui, setter, &p.drop, "drop", 64.0);
                            });
                            grid(ui, |ui| {
                                knob(ui, &p.mix, "Mix");
                                knob(ui, &p.decay, "Decay");
                                knob(ui, &p.damping, "Damp");
                                knob(ui, &p.detune, "Detune");
                                knob(ui, &p.sub, "Sub");
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
