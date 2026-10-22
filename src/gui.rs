use crate::midi::{Encoding, SavedMap};
use crate::params::LumenParams;
use crate::presets::{apply_preset, randomize, PRESETS};
use crate::wavetable::WaveTable;
use crate::Shared;
use nih_plug::prelude::*;
use nih_plug_egui::egui::{
    self, pos2, vec2, Align2, Color32, FontId, Pos2, RichText, Sense, Shape, Stroke, StrokeKind,
};
use nih_plug_egui::{create_egui_editor, EguiState};
use std::cell::RefCell;
use std::f32::consts::PI;
use std::sync::atomic::Ordering;
use std::sync::Arc;

const BG: Color32 = Color32::from_rgb(0x1c, 0x19, 0x15);
const SURFACE: Color32 = Color32::from_rgb(0x26, 0x22, 0x1d);
const SURFACE_HI: Color32 = Color32::from_rgb(0x31, 0x2c, 0x25);
const BORDER: Color32 = Color32::from_rgb(0x3d, 0x36, 0x2d);
const TRACK: Color32 = Color32::from_rgb(0x45, 0x3d, 0x33);
const TEXT: Color32 = Color32::from_rgb(0xe9, 0xe1, 0xd3);
const MUTED: Color32 = Color32::from_rgb(0xa0, 0x95, 0x84);
const ACCENT: Color32 = Color32::from_rgb(0xd9, 0x94, 0x4f);
const SAGE: Color32 = Color32::from_rgb(0x8f, 0xae, 0x7a);

const CELL: f32 = 52.0;
const GAP: f32 = 8.0;
const PAD: f32 = 12.0;
const DRAG_RANGE: f32 = 240.0;
const PLAY: f32 = 0.012;
const OMEGA: f32 = 34.0;
const ZETA: f32 = 0.58;

#[derive(Clone, Copy, Default)]
struct Knob {
    hand: f32,
    out: f32,
    pos: f32,
    vel: f32,
    coast: f32,
    last_set: f32,
    gesture: bool,
    inited: bool,
}

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
}

struct Gui<'a> {
    setter: &'a ParamSetter<'a>,
    shared: &'a Shared,
    table: &'a WaveTable,
    maps: &'a [SavedMap],
    actions: RefCell<Vec<Action>>,
}

fn angle(v: f32) -> f32 {
    0.75 * PI + v * 1.5 * PI
}

fn arc(painter: &egui::Painter, c: Pos2, r: f32, from: f32, to: f32, stroke: Stroke) {
    if (to - from).abs() < 1e-4 {
        return;
    }
    let steps = (((to - from).abs() * 48.0).ceil() as usize).max(2);
    let pts: Vec<Pos2> = (0..=steps)
        .map(|i| {
            let a = angle(from + (to - from) * i as f32 / steps as f32);
            pos2(c.x + r * a.cos(), c.y + r * a.sin())
        })
        .collect();
    painter.add(Shape::line(pts, stroke));
}

fn section(ui: &mut egui::Ui, title: &str, cols: usize, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(SURFACE)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(8.0)
        .inner_margin(PAD)
        .show(ui, |ui| {
            ui.set_width(cols as f32 * CELL);
            ui.vertical(|ui| {
                ui.label(RichText::new(title.to_uppercase()).size(10.5).color(MUTED));
                ui.add_space(4.0);
                add(ui);
            });
        });
}

fn grid(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 4.0);
        ui.horizontal_wrapped(add);
    });
}

impl<'a> Gui<'a> {
    fn knob(&self, ui: &mut egui::Ui, p: &FloatParam, label: &str) {
        let id = ui.make_persistent_id(("knob", p as *const FloatParam as usize));
        let (rect, resp) = ui.allocate_exact_size(vec2(CELL, 78.0), Sense::click_and_drag());
        let ctx = ui.ctx().clone();
        let mut st: Knob = ctx.data_mut(|d| d.get_temp(id)).unwrap_or_default();
        let host = p.unmodulated_normalized_value();
        let dt = ctx.input(|i| i.stable_dt).clamp(0.001, 0.05);
        let mods = ctx.input(|i| i.modifiers);
        let dragging = resp.dragged();

        if !st.inited {
            st = Knob {
                hand: host,
                out: host,
                pos: host,
                last_set: host,
                inited: true,
                ..Knob::default()
            };
        }

        if resp.drag_started() && !st.gesture {
            self.setter.begin_set_parameter(p);
            st.gesture = true;
        }

        if !dragging && !st.gesture {
            st.hand = host;
            st.out = host;
            st.pos = host;
            st.vel = 0.0;
            st.coast = 0.0;
            st.last_set = host;
        }

        let scroll = if resp.hovered() { ctx.input(|i| i.smooth_scroll_delta.y) } else { 0.0 };
        let fine = if mods.alt || mods.command {
            0.02
        } else if mods.shift {
            0.2
        } else {
            1.0
        };
        if scroll != 0.0 {
            if !st.gesture {
                self.setter.begin_set_parameter(p);
                st.gesture = true;
            }
            st.hand = (st.hand + scroll * 0.0006 * fine).clamp(0.0, 1.0);
        }

        if dragging {
            let d = resp.drag_delta();
            let dn = (-d.y + d.x * 0.4) / DRAG_RANGE * fine;
            st.hand = (st.hand + dn).clamp(0.0, 1.0);
            st.coast = st.coast * 0.6 + (dn / dt) * 0.4;
        } else if st.gesture {
            st.hand = (st.hand + st.coast * dt).clamp(0.0, 1.0);
            st.coast *= (-dt * 4.5).exp();
            if st.coast.abs() < 2e-3 || st.hand <= 0.0 || st.hand >= 1.0 {
                st.coast = 0.0;
            }
        }

        if st.gesture {
            let half = PLAY * 0.5;
            if st.hand > st.out + half {
                st.out = st.hand - half;
            } else if st.hand < st.out - half {
                st.out = st.hand + half;
            }
            let h = dt / 4.0;
            for _ in 0..4 {
                let acc = OMEGA * OMEGA * (st.out - st.pos) - 2.0 * ZETA * OMEGA * st.vel;
                st.vel += acc * h;
                st.pos += st.vel * h;
            }
            if st.pos < 0.0 {
                st.pos = 0.0;
                st.vel = 0.0;
            } else if st.pos > 1.0 {
                st.pos = 1.0;
                st.vel = 0.0;
            }
            if (st.pos - st.last_set).abs() > 1e-7 {
                self.setter.set_parameter_normalized(p, st.pos);
                st.last_set = st.pos;
            }
            let settled = !dragging
                && st.coast == 0.0
                && scroll == 0.0
                && (st.out - st.pos).abs() < 2e-5
                && st.vel.abs() < 2e-4;
            if settled {
                self.setter.set_parameter_normalized(p, st.out);
                self.setter.end_set_parameter(p);
                st.gesture = false;
                st.pos = st.out;
                st.last_set = st.out;
            } else {
                ctx.request_repaint();
            }
        }

        if resp.double_clicked() {
            let d = p.default_normalized_value();
            if !st.gesture {
                self.setter.begin_set_parameter(p);
            }
            self.setter.set_parameter_normalized(p, d);
            self.setter.end_set_parameter(p);
            st = Knob {
                hand: d,
                out: d,
                pos: d,
                last_set: d,
                inited: true,
                ..Knob::default()
            };
        }

        let slot = self.shared.midi.slots.slot_of(p);
        let owned = slot.map(|s| self.shared.midi.owned[s].load(Ordering::Relaxed)).unwrap_or(false);
        let learning = slot
            .map(|s| self.shared.midi.learn.load(Ordering::Relaxed) == s as i32 + 1)
            .unwrap_or(false);
        let mapped = slot
            .map(|s| self.maps.iter().any(|m| m.id == self.shared.midi.slots.id(s)))
            .unwrap_or(false);

        let painter = ui.painter_at(rect);
        let c = pos2(rect.center().x, rect.top() + 26.0);
        let r = 20.0;
        arc(&painter, c, r, 0.0, 1.0, Stroke::new(3.0, TRACK));
        let def = p.default_normalized_value();
        let bipolar = (0.45..=0.55).contains(&def);
        let origin = if bipolar { def } else { 0.0 };
        arc(&painter, c, r, origin.min(st.pos), origin.max(st.pos), Stroke::new(3.0, ACCENT));
        if owned {
            let mv = slot.map(|s| self.shared.midi.value[s].load(Ordering::Relaxed)).unwrap_or(0.0);
            arc(&painter, c, r + 4.5, 0.0, mv.clamp(0.0, 1.0), Stroke::new(2.0, SAGE));
        }
        painter.circle_filled(c, r - 5.0, if resp.hovered() || st.gesture { SURFACE_HI } else { SURFACE });
        painter.circle_stroke(c, r - 5.0, Stroke::new(1.0, BORDER));
        let a = angle(st.pos);
        let dir = vec2(a.cos(), a.sin());
        painter.line_segment([c + dir * 5.0, c + dir * (r - 7.0)], Stroke::new(2.5, TEXT));
        if mapped {
            painter.circle_filled(pos2(rect.right() - 7.0, rect.top() + 7.0), 3.0, SAGE);
        }
        if learning {
            let t = ctx.input(|i| i.time) as f32;
            let alpha = (0.5 + 0.5 * (t * 6.0).sin()).clamp(0.0, 1.0);
            painter.circle_stroke(
                c,
                r + 3.0,
                Stroke::new(2.0, ACCENT.gamma_multiply(0.35 + 0.65 * alpha)),
            );
            ctx.request_repaint();
        }
        painter.text(
            pos2(c.x, rect.top() + 52.0),
            Align2::CENTER_TOP,
            label,
            FontId::proportional(11.0),
            TEXT,
        );
        let shown = p.normalized_value_to_string(st.pos, true);
        painter.text(
            pos2(c.x, rect.top() + 65.0),
            Align2::CENTER_TOP,
            shown,
            FontId::proportional(10.0),
            MUTED,
        );

        ctx.data_mut(|d| d.insert_temp(id, st));

        if let Some(slot) = slot {
            resp.context_menu(|ui| self.knob_menu(ui, p, slot, learning));
        }
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
                .add(egui::Slider::new(&mut sens, 0.1..=10.0).logarithmic(true).text("Sensitivity"))
                .changed()
            {
                self.actions.borrow_mut().push(Action::SetSensitivity(slot, sens));
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
            self.setter.set_parameter_normalized(p, p.default_normalized_value());
            self.setter.end_set_parameter(p);
            ui.close_menu();
        }
    }

    fn choice<P: Param>(&self, ui: &mut egui::Ui, p: &P, label: &str, width: f32) {
        let steps = p.step_count().unwrap_or(1).max(1);
        let current = (p.unmodulated_normalized_value() * steps as f32).round() as usize;
        let name = |i: usize| p.normalized_value_to_string(i as f32 / steps as f32, false);
        egui::ComboBox::from_id_salt(label)
            .width(width)
            .selected_text(name(current))
            .show_ui(ui, |ui| {
                for i in 0..=steps {
                    if ui.selectable_label(i == current, name(i)).clicked() {
                        self.setter.begin_set_parameter(p);
                        self.setter.set_parameter_normalized(p, i as f32 / steps as f32);
                        self.setter.end_set_parameter(p);
                    }
                }
            });
    }

    fn toggle(&self, ui: &mut egui::Ui, p: &BoolParam, text: &str) {
        let on = p.value();
        let label = RichText::new(text).color(if on { BG } else { TEXT });
        let btn = egui::Button::new(label).fill(if on { ACCENT } else { SURFACE_HI });
        if ui.add(btn).clicked() {
            self.setter.begin_set_parameter(p);
            self.setter.set_parameter(p, !on);
            self.setter.end_set_parameter(p);
        }
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
            .map(|(i, v)| pos2(rect.left() + 3.0 + (width - 6.0) * i as f32 / 95.0, mid - v * amp))
            .collect();
        painter.add(Shape::line(pts, Stroke::new(1.6, ACCENT.gamma_multiply(0.35 + 0.65 * weight))));
    }

    fn meter(&self, ui: &mut egui::Ui, width: f32) {
        let (rect, _) = ui.allocate_exact_size(vec2(width, 8.0), Sense::hover());
        let peak = self.shared.peak.load(Ordering::Relaxed).clamp(0.0, 1.0);
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 4.0, TRACK);
        let mut fill = rect;
        fill.set_width(rect.width() * peak.sqrt());
        painter.rect_filled(fill, 4.0, if peak > 0.9 { Color32::from_rgb(0xd1, 0x5a, 0x4a) } else { ACCENT });
        ui.ctx().request_repaint();
    }
}

fn apply_actions(state: &mut GuiState, gui_actions: Vec<Action>, params: &LumenParams, shared: &Shared) {
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
    egui::Window::new("MIDI mappings").open(&mut open).default_width(420.0).show(ctx, |ui| {
        if state.maps.is_empty() {
            ui.label(RichText::new("Right-click a knob, choose MIDI learn, then move a control.").color(MUTED));
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

pub fn create(params: Arc<LumenParams>, table: Arc<WaveTable>, shared: Arc<Shared>) -> Option<Box<dyn Editor>> {
    let initial = params.midi_maps.lock().map(|m| m.clone()).unwrap_or_default();
    let state = GuiState {
        maps: initial,
        seq: shared.midi.snapshot_seq.load(Ordering::Relaxed),
        preset: 0,
        amount: 0.6,
        show_midi: false,
        seed: 0x1234_5678,
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

            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(BG).inner_margin(16.0))
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.horizontal_top(|ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new("LUMEN").size(26.0).color(ACCENT).strong());
                            ui.label(RichText::new("organic wavetable instrument").size(11.0).color(MUTED));
                        });
                        ui.add_space(16.0);
                        section(ui, "Preset", 4, |ui| {
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
                                            if ui.selectable_label(i == preset, pr.name).clicked() {
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
                                ui.add(egui::Slider::new(&mut amount, 0.0..=1.0).show_value(false));
                            });
                        });
                        section(ui, "Macros", 3, |ui| {
                            grid(ui, |ui| {
                                gui.knob(ui, &p.macros.organic, "Organic");
                                gui.knob(ui, &p.macros.chaos, "Chaos");
                                gui.knob(ui, &p.macros.chaos_rate, "Chaos rate");
                            });
                        });
                        section(ui, "Voice", 4, |ui| {
                            gui.choice(ui, &p.voice_mode, "vmode", 100.0);
                            grid(ui, |ui| {
                                gui.knob(ui, &p.glide, "Glide");
                                gui.knob(ui, &p.drift, "Drift");
                                gui.knob(ui, &p.gain, "Gain");
                            });
                        });
                        section(ui, "Output", 2, |ui| {
                            gui.meter(ui, 2.0 * CELL);
                            ui.add_space(6.0);
                            if ui.button("MIDI map").clicked() {
                                show_midi = !show_midi;
                            }
                            let cc = shared.midi.last_cc.load(Ordering::Relaxed);
                            ui.label(
                                RichText::new(if cc >= 0 { format!("last CC {cc}") } else { "no CC yet".to_string() })
                                    .size(10.0)
                                    .color(MUTED),
                            );
                        });
                    });

                    ui.horizontal_top(|ui| {
                        for (idx, name, osc) in [(0usize, "Osc 1", &p.osc1), (1usize, "Osc 2", &p.osc2)] {
                            section(ui, name, 4, |ui| {
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
                        section(ui, "Mix", 3, |ui| {
                            grid(ui, |ui| {
                                gui.knob(ui, &p.morph, "Morph");
                                gui.knob(ui, &p.sub, "Sub");
                                gui.knob(ui, &p.noise, "Noise");
                            });
                        });
                        section(ui, "Filter", 5, |ui| {
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
                        for (name, e) in [("Amp env", &p.amp_env), ("Filter env", &p.filt_env)] {
                            section(ui, name, 4, |ui| {
                                grid(ui, |ui| {
                                    gui.knob(ui, &e.attack, "Attack");
                                    gui.knob(ui, &e.decay, "Decay");
                                    gui.knob(ui, &e.sustain, "Sustain");
                                    gui.knob(ui, &e.release, "Release");
                                });
                            });
                        }
                        section(ui, "LFO", 5, |ui| {
                            grid(ui, |ui| {
                                gui.knob(ui, &p.lfo.shape, "Shape");
                                gui.knob(ui, &p.lfo.rate, "Rate");
                                gui.knob(ui, &p.lfo.to_position, "> Table");
                                gui.knob(ui, &p.lfo.to_pitch, "> Pitch");
                                gui.knob(ui, &p.lfo.to_cutoff, "> Cutoff");
                            });
                        });
                        section(ui, "Evolve", 5, |ui| {
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
                        section(ui, "Resonator", 4, |ui| {
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
                        section(ui, "Body", 4, |ui| {
                            grid(ui, |ui| {
                                gui.knob(ui, &p.body.mix, "Mix");
                                gui.knob(ui, &p.body.size, "Size");
                                gui.knob(ui, &p.body.decay, "Decay");
                                gui.knob(ui, &p.body.damping, "Damping");
                            });
                        });
                        section(ui, "Granular", 5, |ui| {
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
                        section(ui, "FX", 6, |ui| {
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
                        section(ui, "MIDI response", 4, |ui| {
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
            apply_actions(state, actions, p, &shared);
            if state.show_midi {
                midi_window(ctx, state, p, &shared);
            }
        },
    )
}
