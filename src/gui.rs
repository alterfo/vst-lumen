use crate::midi::{Encoding, SavedMap};
use crate::packgen::{self, Category, KitJob, Mode, KIT};
use crate::params::LumenParams;
use crate::presets::{apply_preset, randomize, PRESETS};
use crate::wavetable::WaveTable;
use crate::widgets::*;
use crate::Shared;
use nih_plug::prelude::*;
use nih_plug_egui::egui::{
    self, pos2, vec2, Color32, Pos2, RichText, Sense, Shape, Stroke, StrokeKind,
};
use nih_plug_egui::{create_egui_editor, EguiState};
use std::cell::RefCell;
use std::sync::atomic::Ordering;
use std::sync::Arc;

enum Action {
    Learn(usize),
    CancelLearn,
    Remove(usize),
    SetEncoding(usize, Encoding),
    SetSensitivity(usize, f32),
    SetInvert(usize, bool),
}

pub struct GuiState {
    maps: Vec<SavedMap>,
    seq: u32,
    preset: usize,
    amount: f32,
    show_midi: bool,
    seed: u32,
    show_kit: bool,
    kit_dir: String,
    kit: KitJob,
    kit_count: usize,
    kit_seed: u64,
    kit_stems: bool,
    kit_mp3: bool,
}

struct Gui<'a> {
    setter: &'a ParamSetter<'a>,
    shared: &'a Shared,
    table: &'a WaveTable,
    maps: &'a [SavedMap],
    actions: RefCell<Vec<Action>>,
}

impl<'a> Gui<'a> {
    fn knob(&self, ui: &mut egui::Ui, p: &FloatParam, label: &str) -> egui::Response {
        let slot = self.shared.midi.slots.slot_of(p);
        let owned = slot
            .map(|s| self.shared.midi.owned[s].load(Ordering::Relaxed))
            .unwrap_or(false);
        let learning = slot
            .map(|s| self.shared.midi.learn.load(Ordering::Relaxed) == s as i32 + 1)
            .unwrap_or(false);
        let mapped = slot
            .map(|s| {
                self.maps
                    .iter()
                    .any(|m| m.id == self.shared.midi.slots.id(s))
            })
            .unwrap_or(false);
        let midi_value = if owned {
            slot.map(|s| self.shared.midi.value[s].load(Ordering::Relaxed))
        } else {
            None
        };
        let resp = analog_knob(
            ui,
            self.setter,
            p,
            label,
            Overlay {
                midi_value,
                mapped,
                learning,
            },
        );
        if let Some(slot) = slot {
            resp.context_menu(|ui| self.knob_menu(ui, p, slot, learning));
        }
        resp
    }

    fn knob_menu(&self, ui: &mut egui::Ui, p: &FloatParam, slot: usize, learning: bool) {
        let id = self.shared.midi.slots.id(slot).to_string();
        let map = self.maps.iter().find(|m| m.id == id);
        if learning {
            if ui.button("Cancel MIDI learn").clicked() {
                self.actions.borrow_mut().push(Action::CancelLearn);
                ui.close_menu();
            }
        } else if ui.button("MIDI learn").clicked() {
            self.actions.borrow_mut().push(Action::Learn(slot));
            ui.close_menu();
        }
        if let Some(m) = map {
            ui.separator();
            let ch = if m.channel == 255 {
                "omni".to_string()
            } else {
                (m.channel + 1).to_string()
            };
            ui.label(RichText::new(format!("CC {} · channel {}", m.cc, ch)).color(MUTED));
            ui.menu_button("Controller type", |ui| {
                for e in Encoding::ALL {
                    if ui.selectable_label(m.encoding == e, e.label()).clicked() {
                        self.actions.borrow_mut().push(Action::SetEncoding(slot, e));
                        ui.close_menu();
                    }
                }
            });
            let mut sens = m.sensitivity;
            if ui
                .add(
                    egui::Slider::new(&mut sens, 0.1..=10.0)
                        .logarithmic(true)
                        .text("Sensitivity"),
                )
                .changed()
            {
                self.actions
                    .borrow_mut()
                    .push(Action::SetSensitivity(slot, sens));
            }
            let mut inv = m.invert;
            if ui.checkbox(&mut inv, "Invert").changed() {
                self.actions.borrow_mut().push(Action::SetInvert(slot, inv));
            }
            if ui.button("Clear mapping").clicked() {
                self.actions.borrow_mut().push(Action::Remove(slot));
                ui.close_menu();
            }
        }
        ui.separator();
        if ui.button("Reset to default").clicked() {
            self.setter.begin_set_parameter(p);
            self.setter
                .set_parameter_normalized(p, p.default_normalized_value());
            self.setter.end_set_parameter(p);
            ui.close_menu();
        }
    }

    fn choice<P: Param>(&self, ui: &mut egui::Ui, p: &P, label: &str, width: f32) {
        choice(ui, self.setter, p, label, width);
    }

    fn toggle(&self, ui: &mut egui::Ui, p: &BoolParam, text: &str) {
        toggle(ui, self.setter, p, text);
    }

    fn wave(&self, ui: &mut egui::Ui, index: usize, width: f32) {
        let (rect, _) = ui.allocate_exact_size(vec2(width, 44.0), Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 4.0, BG);
        painter.rect_stroke(rect, 4.0, Stroke::new(1.0, BORDER), StrokeKind::Inside);
        let pos = self.shared.position[index].load(Ordering::Relaxed);
        let morph = self.shared.morph.load(Ordering::Relaxed) * std::f32::consts::FRAC_PI_2;
        let weight = if index == 0 { morph.cos() } else { morph.sin() };
        let mut buf = [0.0f32; 96];
        self.table.preview(pos, &mut buf);
        let mid = rect.center().y;
        let amp = rect.height() * 0.42;
        let pts: Vec<Pos2> = buf
            .iter()
            .enumerate()
            .map(|(i, v)| {
                pos2(
                    rect.left() + 3.0 + (width - 6.0) * i as f32 / 95.0,
                    mid - v * amp,
                )
            })
            .collect();
        painter.add(Shape::line(
            pts,
            Stroke::new(1.6, ACCENT.gamma_multiply(0.35 + 0.65 * weight)),
        ));
    }

    fn meter(&self, ui: &mut egui::Ui, width: f32) {
        let (rect, _) = ui.allocate_exact_size(vec2(width, 8.0), Sense::hover());
        let peak = self.shared.peak.load(Ordering::Relaxed).clamp(0.0, 1.0);
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 4.0, TRACK);
        let mut fill = rect;
        fill.set_width(rect.width() * peak.sqrt());
        painter.rect_filled(
            fill,
            4.0,
            if peak > 0.9 {
                Color32::from_rgb(0xd1, 0x5a, 0x4a)
            } else {
                ACCENT
            },
        );
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(40));
    }
}

fn apply_actions(
    state: &mut GuiState,
    gui_actions: Vec<Action>,
    params: &LumenParams,
    shared: &Shared,
) {
    if gui_actions.is_empty() {
        return;
    }
    let mut maps = state.maps.clone();
    let mut commit = false;
    for a in gui_actions {
        match a {
            Action::Learn(slot) => shared.midi.request_learn(slot),
            Action::CancelLearn => shared.midi.cancel_learn(),
            Action::Remove(slot) => {
                let id = shared.midi.slots.id(slot);
                maps.retain(|m| m.id != id);
                commit = true;
            }
            Action::SetEncoding(slot, enc) => {
                let id = shared.midi.slots.id(slot);
                if let Some(m) = maps.iter_mut().find(|m| m.id == id) {
                    m.encoding = enc;
                    commit = true;
                }
            }
            Action::SetSensitivity(slot, s) => {
                let id = shared.midi.slots.id(slot);
                if let Some(m) = maps.iter_mut().find(|m| m.id == id) {
                    m.sensitivity = s;
                    commit = true;
                }
            }
            Action::SetInvert(slot, inv) => {
                let id = shared.midi.slots.id(slot);
                if let Some(m) = maps.iter_mut().find(|m| m.id == id) {
                    m.invert = inv;
                    commit = true;
                }
            }
        }
    }
    if commit {
        state.maps = maps.clone();
        shared.midi.commit(params, maps);
    }
}

fn midi_window(ctx: &egui::Context, state: &mut GuiState, params: &LumenParams, shared: &Shared) {
    let mut open = state.show_midi;
    let mut changed: Option<Vec<SavedMap>> = None;
    egui::Window::new("MIDI mappings")
        .open(&mut open)
        .default_width(420.0)
        .show(ctx, |ui| {
            if state.maps.is_empty() {
                ui.label(
                    RichText::new("Right-click a knob, choose MIDI learn, then move a control.")
                        .color(MUTED),
                );
            }
            let mut maps = state.maps.clone();
            let mut remove: Option<usize> = None;
            for (i, m) in maps.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&m.id).color(TEXT));
                    ui.label(RichText::new(format!("CC {}", m.cc)).color(MUTED));
                    egui::ComboBox::from_id_salt(("enc", i))
                        .selected_text(m.encoding.label())
                        .show_ui(ui, |ui| {
                            for e in Encoding::ALL {
                                ui.selectable_value(&mut m.encoding, e, e.label());
                            }
                        });
                    ui.add(egui::Slider::new(&mut m.sensitivity, 0.1..=10.0).logarithmic(true));
                    ui.checkbox(&mut m.invert, "Inv");
                    if ui.button("x").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                maps.remove(i);
            }
            if maps != state.maps {
                changed = Some(maps);
            }
        });
    state.show_midi = open;
    if let Some(maps) = changed {
        state.maps = maps.clone();
        shared.midi.commit(params, maps);
    }
}

fn kit_window(ctx: &egui::Context, state: &mut GuiState) {
    let mut open = state.show_kit;
    egui::Window::new("Sample kit")
        .open(&mut open)
        .default_width(460.0)
        .show(ctx, |ui| {
            ui.label(
                RichText::new("Renders the Lumen kit to disk: 48 kHz / 24-bit stereo WAV.")
                    .color(MUTED),
            );
            ui.horizontal(|ui| {
                ui.label(RichText::new("Folder").color(TEXT));
                ui.add(egui::TextEdit::singleline(&mut state.kit_dir).desired_width(320.0));
            });
            let (running, done, total, status) = state.kit.progress.snapshot();
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!running, egui::Button::new("One-shots"))
                    .clicked()
                {
                    state.kit.start(Mode::OneShots, packgen::default_dir_from(&state.kit_dir));
                }
                if ui
                    .add_enabled(!running, egui::Button::new("Demo groove"))
                    .clicked()
                {
                    state.kit.start(Mode::Groove, packgen::default_dir_from(&state.kit_dir));
                }
                if ui
                    .add_enabled(!running, egui::Button::new("Everything"))
                    .clicked()
                {
                    state.kit.start(Mode::All, packgen::default_dir_from(&state.kit_dir));
                }
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(RichText::new("Mixes").color(TEXT));
                ui.add(egui::DragValue::new(&mut state.kit_count).range(1..=1000));
                ui.label(RichText::new("seed").color(MUTED));
                ui.add(egui::DragValue::new(&mut state.kit_seed).range(0..=9_999_999));
                ui.checkbox(&mut state.kit_stems, "keep stems");
                ui.checkbox(&mut state.kit_mp3, "mp3");
                if ui
                    .add_enabled(!running, egui::Button::new("Generate"))
                    .clicked()
                {
                    state.kit.start_mixes(
                        packgen::MixOptions {
                            count: state.kit_count,
                            seed: state.kit_seed,
                            keep_stems: state.kit_stems,
                            mp3: state.kit_mp3,
                        },
                        packgen::default_dir_from(&state.kit_dir),
                    );
                }
            });
            ui.label(
                RichText::new(
                    "Each mix re-picks voices, tempo, key, swing, patterns, effects and automation from its seed.",
                )
                .size(10.0)
                .color(MUTED),
            );
            if running || total > 0 {
                let fraction = if total > 0 {
                    done as f32 / total as f32
                } else {
                    0.0
                };
                ui.add(
                    egui::ProgressBar::new(fraction)
                        .show_percentage()
                        .desired_width(320.0),
                );
            }
            if !status.is_empty() {
                ui.label(RichText::new(status).color(if running { TEXT } else { ACCENT }));
            }
            ui.separator();
            for category in [
                Category::Drum,
                Category::Perc,
                Category::Bass,
                Category::Synth,
                Category::Texture,
            ] {
                let voices: Vec<&str> = KIT
                    .iter()
                    .filter(|v| v.category == category)
                    .map(|v| v.name)
                    .collect();
                ui.label(
                    RichText::new(format!(
                        "{:?} · {} voices",
                        category,
                        voices.len()
                    ))
                    .color(MUTED),
                );
                ui.label(RichText::new(voices.join(", ")).color(TEXT));
            }
        });
    state.show_kit = open;
}

pub fn create(
    params: Arc<LumenParams>,
    table: Arc<WaveTable>,
    shared: Arc<Shared>,
) -> Option<Box<dyn Editor>> {
    let initial = params
        .midi_maps
        .lock()
        .map(|m| m.clone())
        .unwrap_or_default();
    let state = GuiState {
        maps: initial,
        seq: shared.midi.snapshot_seq.load(Ordering::Relaxed),
        preset: 0,
        amount: 0.6,
        show_midi: false,
        seed: 0x1234_5678,
        show_kit: false,
        kit_dir: packgen::default_dir().to_string_lossy().to_string(),
        kit: KitJob::new(),
        kit_count: 100,
        kit_seed: 1,
        kit_stems: false,
        kit_mp3: true,
    };
    let egui_state: Arc<EguiState> = params.editor_state.clone();
    let build_params = params.clone();
    create_egui_editor(
        egui_state,
        state,
        |ctx, _| {
            let mut v = egui::Visuals::dark();
            v.panel_fill = BG;
            v.window_fill = SURFACE;
            v.extreme_bg_color = BG;
            v.override_text_color = Some(TEXT);
            v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
            v.widgets.inactive.bg_fill = SURFACE_HI;
            v.widgets.inactive.weak_bg_fill = SURFACE_HI;
            v.widgets.hovered.bg_fill = TRACK;
            v.widgets.hovered.weak_bg_fill = TRACK;
            v.widgets.active.bg_fill = ACCENT;
            v.selection.bg_fill = ACCENT.gamma_multiply(0.5);
            ctx.set_visuals(v);
            ctx.style_mut(|s| {
                s.spacing.item_spacing = vec2(GAP, GAP);
            });
        },
        move |ctx, setter, state| {
            let seq = shared.midi.snapshot_seq.load(Ordering::Acquire);
            if seq != state.seq {
                state.seq = seq;
                let snap = shared.midi.snapshot();
                if snap != state.maps {
                    shared.midi.store_only(&build_params, snap.clone());
                    state.maps = snap;
                }
            }

            let p = &*build_params;
            let maps = state.maps.clone();
            let gui = Gui {
                setter,
                shared: &shared,
                table: &table,
                maps: &maps,
                actions: RefCell::new(Vec::new()),
            };
            let mut preset = state.preset;
            let mut amount = state.amount;
            let mut do_random = false;
            let mut show_midi = state.show_midi;
            let mut show_kit = state.show_kit;

            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(BG).inner_margin(16.0))
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.horizontal_top(|ui| {
                                ui.vertical(|ui| {
                                    ui.label(
                                        RichText::new("LUMEN").size(26.0).color(ACCENT).strong(),
                                    );
                                    ui.label(
                                        RichText::new("organic wavetable instrument")
                                            .size(11.0)
                                            .color(MUTED),
                                    );
                                });
                                ui.add_space(16.0);
                                section(ui, "Preset", 4, 134.0, |ui| {
                                    ui.horizontal(|ui| {
                                        if ui.button("<").clicked() {
                                            preset = (preset + PRESETS.len() - 1) % PRESETS.len();
                                            apply_preset(setter, p, &PRESETS[preset]);
                                        }
                                        egui::ComboBox::from_id_salt("preset")
                                            .width(130.0)
                                            .selected_text(PRESETS[preset].name)
                                            .show_ui(ui, |ui| {
                                                for (i, pr) in PRESETS.iter().enumerate() {
                                                    if ui
                                                        .selectable_label(i == preset, pr.name)
                                                        .clicked()
                                                    {
                                                        preset = i;
                                                        apply_preset(setter, p, pr);
                                                    }
                                                }
                                            });
                                        if ui.button(">").clicked() {
                                            preset = (preset + 1) % PRESETS.len();
                                            apply_preset(setter, p, &PRESETS[preset]);
                                        }
                                    });
                                    ui.horizontal(|ui| {
                                        if ui.button("Randomize").clicked() {
                                            do_random = true;
                                        }
                                        ui.add(
                                            egui::Slider::new(&mut amount, 0.0..=1.0)
                                                .show_value(false),
                                        );
                                    });
                                });
                                section(ui, "Macros", 3, 134.0, |ui| {
                                    grid(ui, |ui| {
                                        gui.knob(ui, &p.macros.organic, "Organic");
                                        gui.knob(ui, &p.macros.chaos, "Chaos");
                                        gui.knob(ui, &p.macros.chaos_rate, "Chaos rate");
                                    });
                                });
                                section(ui, "Voice", 4, 134.0, |ui| {
                                    gui.choice(ui, &p.voice_mode, "vmode", 100.0);
                                    grid(ui, |ui| {
                                        gui.knob(ui, &p.glide, "Glide");
                                        gui.knob(ui, &p.drift, "Drift");
                                        gui.knob(ui, &p.gain, "Gain");
                                    });
                                });
                                section(ui, "Output", 2, 134.0, |ui| {
                                    gui.meter(ui, 2.0 * CELL);
                                    ui.add_space(6.0);
                                    if ui.button("MIDI map").clicked() {
                                        show_midi = !show_midi;
                                    }
                                    if ui.button("Sample kit").clicked() {
                                        show_kit = !show_kit;
                                    }
                                    let cc = shared.midi.last_cc.load(Ordering::Relaxed);
                                    ui.label(
                                        RichText::new(if cc >= 0 {
                                            format!("last CC {cc}")
                                        } else {
                                            "no CC yet".to_string()
                                        })
                                        .size(10.0)
                                        .color(MUTED),
                                    );
                                });
                            });

                            ui.horizontal_top(|ui| {
                                for (idx, name, osc) in
                                    [(0usize, "Osc 1", &p.osc1), (1usize, "Osc 2", &p.osc2)]
                                {
                                    section(ui, name, 4, 238.0, |ui| {
                                        gui.wave(ui, idx, 4.0 * CELL);
                                        grid(ui, |ui| {
                                            gui.knob(ui, &osc.position, "Table");
                                            gui.knob(ui, &osc.level, "Level");
                                            gui.knob(ui, &osc.tune, "Tune");
                                            gui.knob(ui, &osc.fine, "Fine");
                                            gui.knob(ui, &osc.unison, "Unison");
                                            gui.knob(ui, &osc.detune, "Detune");
                                            gui.knob(ui, &osc.spread, "Spread");
                                        });
                                    });
                                }
                                section(ui, "Mix", 3, 238.0, |ui| {
                                    grid(ui, |ui| {
                                        gui.knob(ui, &p.morph, "Morph");
                                        gui.knob(ui, &p.sub, "Sub");
                                        gui.knob(ui, &p.noise, "Noise");
                                    });
                                });
                                section(ui, "Filter", 5, 238.0, |ui| {
                                    grid(ui, |ui| {
                                        gui.knob(ui, &p.filter.mode, "LP-BP-HP");
                                        gui.knob(ui, &p.filter.cutoff, "Cutoff");
                                        gui.knob(ui, &p.filter.resonance, "Reson");
                                        gui.knob(ui, &p.filter.drive, "Drive");
                                        gui.knob(ui, &p.filter.env_amount, "Env");
                                    });
                                });
                            });

                            ui.horizontal_top(|ui| {
                                for (name, e) in
                                    [("Amp env", &p.amp_env), ("Filter env", &p.filt_env)]
                                {
                                    section(ui, name, 4, 104.0, |ui| {
                                        grid(ui, |ui| {
                                            gui.knob(ui, &e.attack, "Attack");
                                            gui.knob(ui, &e.decay, "Decay");
                                            gui.knob(ui, &e.sustain, "Sustain");
                                            gui.knob(ui, &e.release, "Release");
                                        });
                                    });
                                }
                                section(ui, "LFO", 5, 104.0, |ui| {
                                    grid(ui, |ui| {
                                        gui.knob(ui, &p.lfo.shape, "Shape");
                                        gui.knob(ui, &p.lfo.rate, "Rate");
                                        gui.knob(ui, &p.lfo.to_position, "> Table");
                                        gui.knob(ui, &p.lfo.to_pitch, "> Pitch");
                                        gui.knob(ui, &p.lfo.to_cutoff, "> Cutoff");
                                    });
                                });
                                section(ui, "Evolve", 5, 104.0, |ui| {
                                    grid(ui, |ui| {
                                        gui.knob(ui, &p.evolve.rate, "Rate");
                                        gui.knob(ui, &p.evolve.to_table, "> Table");
                                        gui.knob(ui, &p.evolve.to_morph, "> Morph");
                                        gui.knob(ui, &p.evolve.to_cutoff, "> Cutoff");
                                        gui.knob(ui, &p.evolve.voice_spread, "Per-voice");
                                    });
                                });
                            });

                            ui.horizontal_top(|ui| {
                                section(ui, "Resonator", 4, 222.0, |ui| {
                                    gui.choice(ui, &p.resonator.model, "model", 4.0 * CELL - 8.0);
                                    grid(ui, |ui| {
                                        gui.knob(ui, &p.resonator.mix, "Mix");
                                        gui.knob(ui, &p.resonator.decay, "Decay");
                                        gui.knob(ui, &p.resonator.damping, "Damping");
                                        gui.knob(ui, &p.resonator.stiffness, "Stiffness");
                                        gui.knob(ui, &p.resonator.strike, "Strike");
                                        gui.knob(ui, &p.resonator.hardness, "Hardness");
                                        gui.knob(ui, &p.resonator.position, "Position");
                                    });
                                });
                                section(ui, "Body", 4, 222.0, |ui| {
                                    grid(ui, |ui| {
                                        gui.knob(ui, &p.body.mix, "Mix");
                                        gui.knob(ui, &p.body.size, "Size");
                                        gui.knob(ui, &p.body.decay, "Decay");
                                        gui.knob(ui, &p.body.damping, "Damping");
                                    });
                                });
                                section(ui, "Granular", 5, 222.0, |ui| {
                                    ui.horizontal(|ui| {
                                        gui.choice(ui, &p.granular.interval, "ival", 90.0);
                                        gui.toggle(ui, &p.granular.freeze, "Freeze");
                                    });
                                    grid(ui, |ui| {
                                        gui.knob(ui, &p.granular.mix, "Mix");
                                        gui.knob(ui, &p.granular.size, "Size");
                                        gui.knob(ui, &p.granular.density, "Density");
                                        gui.knob(ui, &p.granular.position, "Position");
                                        gui.knob(ui, &p.granular.jitter, "Jitter");
                                        gui.knob(ui, &p.granular.pitch, "Pitch");
                                        gui.knob(ui, &p.granular.pitch_spread, "Spread");
                                        gui.knob(ui, &p.granular.reverse, "Reverse");
                                        gui.knob(ui, &p.granular.stereo, "Width");
                                    });
                                });
                                section(ui, "FX", 6, 222.0, |ui| {
                                    grid(ui, |ui| {
                                        gui.knob(ui, &p.fx.chorus_mix, "Chorus");
                                        gui.knob(ui, &p.fx.chorus_rate, "Ch rate");
                                        gui.knob(ui, &p.fx.chorus_depth, "Ch depth");
                                        gui.knob(ui, &p.fx.delay_mix, "Delay");
                                        gui.knob(ui, &p.fx.delay_time, "Dl time");
                                        gui.knob(ui, &p.fx.delay_feedback, "Dl fdbk");
                                        gui.knob(ui, &p.fx.delay_tone, "Dl tone");
                                        gui.knob(ui, &p.fx.reverb_mix, "Reverb");
                                        gui.knob(ui, &p.fx.reverb_decay, "Rv decay");
                                        gui.knob(ui, &p.fx.reverb_damping, "Rv damp");
                                        gui.knob(ui, &p.fx.reverb_predelay, "Pre-dly");
                                    });
                                });
                            });

                            ui.horizontal_top(|ui| {
                                section(ui, "MIDI response", 4, 104.0, |ui| {
                                    grid(ui, |ui| {
                                        gui.knob(ui, &p.midi.fine_exp, "Fine step");
                                        gui.knob(ui, &p.midi.coarse_div, "Coarse");
                                        gui.knob(ui, &p.midi.speed_hi, "Fast at");
                                        gui.knob(ui, &p.midi.inertia, "Inertia");
                                    });
                                });
                            });
                        });
                });

            if do_random {
                state.seed = state.seed.wrapping_mul(1664525).wrapping_add(1013904223)
                    ^ (ctx.input(|i| i.time) * 1000.0) as u32;
                randomize(setter, p, amount, state.seed);
            }
            let actions = gui.actions.into_inner();
            state.preset = preset;
            state.amount = amount;
            state.show_midi = show_midi;
            state.show_kit = show_kit;
            apply_actions(state, actions, p, &shared);
            if state.show_midi {
                midi_window(ctx, state, p, &shared);
            }
            if state.show_kit {
                kit_window(ctx, state);
            }
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use nih_plug::context::gui::GuiContext;
    use nih_plug::context::PluginApi;
    use nih_plug::prelude::PluginState;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Mock {
        log: Mutex<Vec<(char, f32)>>,
    }

    impl GuiContext for Mock {
        fn plugin_api(&self) -> PluginApi {
            PluginApi::Standalone
        }
        fn request_resize(&self) -> bool {
            true
        }
        unsafe fn raw_begin_set_parameter(&self, _: ParamPtr) {
            self.log.lock().unwrap().push(('b', 0.0));
        }
        unsafe fn raw_set_parameter_normalized(&self, _: ParamPtr, v: f32) {
            self.log.lock().unwrap().push(('v', v));
        }
        unsafe fn raw_end_set_parameter(&self, _: ParamPtr) {
            self.log.lock().unwrap().push(('e', 0.0));
        }
        fn get_state(&self) -> PluginState {
            unimplemented!()
        }
        fn set_state(&self, _: PluginState) {}
    }

    struct Rig {
        ctx: egui::Context,
        mock: Arc<Mock>,
        lumen: crate::Lumen,
        t: f64,
    }

    impl Rig {
        fn new() -> Self {
            let ctx = egui::Context::default();
            Self {
                ctx,
                mock: Arc::new(Mock::default()),
                lumen: crate::Lumen::default(),
                t: 0.0,
            }
        }

        fn frame(&mut self, events: Vec<egui::Event>, dt: f64) -> egui::Rect {
            self.t += dt;
            let mut raw = egui::RawInput::default();
            raw.time = Some(self.t);
            raw.predicted_dt = dt as f32;
            raw.screen_rect = Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(400.0, 300.0)));
            raw.events = events;
            let mock = self.mock.clone();
            let setter = ParamSetter {
                raw_context: &*mock,
            };
            let table = self.lumen.table_for_test();
            let shared_arc = self.lumen.shared_for_test();
            let shared_arc = self.lumen.shared_for_test();
            let gui = Gui {
                setter: &setter,
                shared: &*shared_arc,
                table: &table,
                maps: &[],
                actions: RefCell::new(Vec::new()),
            };
            let params = self.lumen.params_for_test();
            let mut rect = egui::Rect::NOTHING;
            self.ctx.run(raw, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    rect = gui.knob(ui, &params.filter.cutoff, "Cutoff").rect;
                });
            });
            rect
        }
    }

    fn run_drag(
        rig: &mut Rig,
        px_per_frame: f32,
        frames: usize,
        modifiers: egui::Modifiers,
    ) -> Vec<f32> {
        let dt = 1.0 / 60.0;
        let rect = rig.frame(vec![], dt);
        let c = rect.center() - vec2(0.0, 20.0);
        rig.frame(vec![ev_move(c)], dt);
        rig.frame(
            vec![egui::Event::PointerButton {
                pos: c,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers,
            }],
            dt,
        );
        let mut y = c.y;
        for _ in 0..frames {
            y -= px_per_frame;
            rig.frame(vec![ev_move(pos2(c.x, y))], dt);
        }
        rig.frame(vec![ev_button(pos2(c.x, y), false)], dt);
        for _ in 0..90 {
            rig.frame(vec![], dt);
        }
        let log = rig.mock.log.lock().unwrap().clone();
        log.iter()
            .filter(|(k, _)| *k == 'v')
            .map(|(_, v)| *v)
            .collect()
    }

    #[test]
    fn slow_precise_drag_is_not_swallowed_by_play() {
        let mut rig = Rig::new();
        let v = run_drag(&mut rig, 0.5, 40, egui::Modifiers::NONE);
        assert!(v.len() > 10, "only {} writes", v.len());
        let moved = v.last().unwrap() - v.first().unwrap();
        assert!(
            moved > 0.06 && moved < 0.12,
            "slow drag moved {moved}, expected about {}",
            20.0 / DRAG_RANGE
        );
    }

    #[test]
    fn fast_flick_overshoots_only_a_little() {
        let mut rig = Rig::new();
        let v = run_drag(&mut rig, 6.0, 10, egui::Modifiers::NONE);
        let moved = v.last().unwrap() - v.first().unwrap();
        let dragged = 60.0 / DRAG_RANGE;
        assert!(
            moved >= dragged - 0.03 && moved < dragged + 0.12,
            "moved {moved}, dragged {dragged}"
        );
    }

    #[test]
    fn scroll_wheel_nudges_the_value() {
        let mut rig = Rig::new();
        let dt = 1.0 / 60.0;
        let rect = rig.frame(vec![], dt);
        let c = rect.center() - vec2(0.0, 20.0);
        rig.frame(vec![ev_move(c)], dt);
        rig.frame(
            vec![egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: vec2(0.0, 24.0),
                modifiers: egui::Modifiers::NONE,
            }],
            dt,
        );
        for _ in 0..90 {
            rig.frame(vec![], dt);
        }
        let log = rig.mock.log.lock().unwrap().clone();
        let v: Vec<f32> = log
            .iter()
            .filter(|(k, _)| *k == 'v')
            .map(|(_, v)| *v)
            .collect();
        assert!(!v.is_empty(), "scroll produced no writes");
        assert!(
            v.last().unwrap() - v.first().unwrap() > 0.004 || v.last().unwrap() > &0.0,
            "no movement"
        );
        assert!(
            log.last().map(|(k, _)| *k) == Some('e'),
            "gesture not closed"
        );
    }

    fn ev_move(p: Pos2) -> egui::Event {
        egui::Event::PointerMoved(p)
    }

    fn ev_button(p: Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos: p,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    #[test]
    fn dragging_a_knob_writes_smooth_monotonic_values() {
        let mut rig = Rig::new();
        let dt = 1.0 / 60.0;
        let rect = rig.frame(vec![], dt);
        let c = rect.center() - vec2(0.0, 20.0);
        rig.frame(vec![ev_move(c)], dt);
        rig.frame(vec![ev_button(c, true)], dt);
        let mut y = c.y;
        for _ in 0..30 {
            y -= 2.0;
            rig.frame(vec![ev_move(pos2(c.x, y))], dt);
        }
        rig.frame(vec![ev_button(pos2(c.x, y), false)], dt);
        for _ in 0..60 {
            rig.frame(vec![], dt);
        }
        let log = rig.mock.log.lock().unwrap().clone();
        let values: Vec<f32> = log
            .iter()
            .filter(|(k, _)| *k == 'v')
            .map(|(_, v)| *v)
            .collect();
        assert!(
            log.first().map(|(k, _)| *k) == Some('b'),
            "no begin: {:?}",
            log.iter().take(3).collect::<Vec<_>>()
        );
        assert!(
            log.last().map(|(k, _)| *k) == Some('e'),
            "gesture never ended"
        );
        assert!(values.len() > 20, "only {} writes", values.len());
        let moved = values.last().unwrap() - values.first().unwrap();
        assert!(
            (moved - 60.0 / DRAG_RANGE).abs() < 0.07,
            "moved {moved}, expected {}",
            60.0 / DRAG_RANGE
        );
        let mut max_step = 0.0f32;
        for w in values.windows(2) {
            assert!(w[1] >= w[0] - 0.002, "not monotonic: {} -> {}", w[0], w[1]);
            max_step = max_step.max(w[1] - w[0]);
        }
        let mean = moved / values.len() as f32;
        assert!(
            max_step < mean * 6.0 + 0.004,
            "jerky: max step {max_step}, mean {mean}"
        );
    }
}
