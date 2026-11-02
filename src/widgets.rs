use nih_plug::prelude::*;
use nih_plug_egui::egui::{
    self, pos2, vec2, Align2, Color32, FontId, Pos2, RichText, Sense, Shape, Stroke,
};
use std::f32::consts::PI;

pub const BG: Color32 = Color32::from_rgb(0x1c, 0x19, 0x15);
pub const SURFACE: Color32 = Color32::from_rgb(0x26, 0x22, 0x1d);
pub const SURFACE_HI: Color32 = Color32::from_rgb(0x31, 0x2c, 0x25);
pub const BORDER: Color32 = Color32::from_rgb(0x3d, 0x36, 0x2d);
pub const TRACK: Color32 = Color32::from_rgb(0x45, 0x3d, 0x33);
pub const TEXT: Color32 = Color32::from_rgb(0xe9, 0xe1, 0xd3);
pub const MUTED: Color32 = Color32::from_rgb(0xa0, 0x95, 0x84);
pub const ACCENT: Color32 = Color32::from_rgb(0xd9, 0x94, 0x4f);
pub const SAGE: Color32 = Color32::from_rgb(0x8f, 0xae, 0x7a);

pub const CELL: f32 = 56.0;
pub const GAP: f32 = 8.0;
pub const PAD: f32 = 12.0;
pub const DRAG_RANGE: f32 = 240.0;
pub const PLAY_PX: f32 = 1.6;
pub const OMEGA: f32 = 34.0;
pub const ZETA: f32 = 0.8;

#[derive(Clone, Copy, Default)]
pub struct Knob {
    hand: f32,
    out: f32,
    pos: f32,
    vel: f32,
    coast: f32,
    last_set: f32,
    gesture: bool,
    inited: bool,
}

pub fn angle(v: f32) -> f32 {
    0.75 * PI + v * 1.5 * PI
}

pub fn arc(painter: &egui::Painter, c: Pos2, r: f32, from: f32, to: f32, stroke: Stroke) {
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

pub fn section(
    ui: &mut egui::Ui,
    title: &str,
    cols: usize,
    min_h: f32,
    add: impl FnOnce(&mut egui::Ui),
) {
    egui::Frame::new()
        .fill(SURFACE)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(8.0)
        .inner_margin(PAD)
        .show(ui, |ui| {
            ui.set_width(cols as f32 * CELL);
            ui.vertical(|ui| {
                ui.set_min_height(min_h);
                ui.label(RichText::new(title.to_uppercase()).size(10.5).color(MUTED));
                ui.add_space(4.0);
                add(ui);
            });
        });
}

pub fn grid(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 4.0);
        ui.horizontal_wrapped(add);
    });
}

#[derive(Clone, Copy, Default)]
pub struct Overlay {
    pub midi_value: Option<f32>,
    pub mapped: bool,
    pub learning: bool,
}

pub fn analog_knob(
    ui: &mut egui::Ui,
    setter: &ParamSetter,
    p: &FloatParam,
    label: &str,
    overlay: Overlay,
) -> egui::Response {
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
        setter.begin_set_parameter(p);
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

    let scroll = if resp.hovered() {
        ctx.input(|i| i.smooth_scroll_delta.y)
    } else {
        0.0
    };
    let fine = if mods.alt || mods.command {
        0.02
    } else if mods.shift {
        0.2
    } else {
        1.0
    };
    if scroll != 0.0 {
        if !st.gesture {
            setter.begin_set_parameter(p);
            st.gesture = true;
        }
        let nudge = scroll * 0.0006 * fine;
        st.hand = (st.hand + nudge).clamp(0.0, 1.0);
        st.out = (st.out + nudge).clamp(0.0, 1.0);
    }

    if dragging {
        let d = resp.drag_delta();
        let dn = (-d.y + d.x * 0.4) / DRAG_RANGE * fine;
        st.hand = (st.hand + dn).clamp(0.0, 1.0);
        st.coast = (st.coast * 0.6 + (dn / dt) * 0.4).clamp(-1.2, 1.2);
    } else if st.gesture {
        st.hand = (st.hand + st.coast * dt).clamp(0.0, 1.0);
        st.coast *= (-dt * 14.0).exp();
        if st.coast.abs() < 0.01 || st.hand <= 0.0 || st.hand >= 1.0 {
            st.coast = 0.0;
        }
    }

    if st.gesture {
        let half = PLAY_PX / DRAG_RANGE * fine.max(0.05) * 0.5;
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
            setter.set_parameter_normalized(p, st.pos);
            st.last_set = st.pos;
        }
        let settled = !dragging
            && st.coast == 0.0
            && scroll == 0.0
            && (st.out - st.pos).abs() < 2e-5
            && st.vel.abs() < 2e-4;
        if settled {
            setter.set_parameter_normalized(p, st.out);
            setter.end_set_parameter(p);
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
            setter.begin_set_parameter(p);
        }
        setter.set_parameter_normalized(p, d);
        setter.end_set_parameter(p);
        st = Knob {
            hand: d,
            out: d,
            pos: d,
            last_set: d,
            inited: true,
            ..Knob::default()
        };
    }

    let Overlay {
        midi_value,
        mapped,
        learning,
    } = overlay;
    let painter = ui.painter_at(rect);
    let c = pos2(rect.center().x, rect.top() + 26.0);
    let r = 20.0;
    arc(&painter, c, r, 0.0, 1.0, Stroke::new(3.0, TRACK));
    let def = p.default_normalized_value();
    let bipolar = (0.45..=0.55).contains(&def);
    let origin = if bipolar { def } else { 0.0 };
    arc(
        &painter,
        c,
        r,
        origin.min(st.pos),
        origin.max(st.pos),
        Stroke::new(3.0, ACCENT),
    );
    if let Some(mv) = midi_value {
        arc(
            &painter,
            c,
            r + 4.5,
            0.0,
            mv.clamp(0.0, 1.0),
            Stroke::new(2.0, SAGE),
        );
    }
    painter.circle_filled(
        c,
        r - 5.0,
        if resp.hovered() || st.gesture {
            SURFACE_HI
        } else {
            SURFACE
        },
    );
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
    resp
}

pub fn choice<P: Param>(ui: &mut egui::Ui, setter: &ParamSetter, p: &P, label: &str, width: f32) {
    let steps = p.step_count().unwrap_or(1).max(1);
    let current = (p.unmodulated_normalized_value() * steps as f32).round() as usize;
    let name = |i: usize| p.normalized_value_to_string(i as f32 / steps as f32, false);
    egui::ComboBox::from_id_salt(label)
        .width(width)
        .selected_text(name(current))
        .show_ui(ui, |ui| {
            for i in 0..=steps {
                if ui.selectable_label(i == current, name(i)).clicked() {
                    setter.begin_set_parameter(p);
                    setter.set_parameter_normalized(p, i as f32 / steps as f32);
                    setter.end_set_parameter(p);
                }
            }
        });
}

pub fn toggle(ui: &mut egui::Ui, setter: &ParamSetter, p: &BoolParam, text: &str) {
    let on = p.value();
    let label = RichText::new(text).color(if on { BG } else { TEXT });
    let btn = egui::Button::new(label).fill(if on { ACCENT } else { SURFACE_HI });
    if ui.add(btn).clicked() {
        setter.begin_set_parameter(p);
        setter.set_parameter(p, !on);
        setter.end_set_parameter(p);
    }
}

pub fn meter(ui: &mut egui::Ui, width: f32, peak: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, 8.0), Sense::hover());
    let peak = peak.clamp(0.0, 1.0);
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, TRACK);
    let mut fill = rect;
    fill.set_width(rect.width() * peak.sqrt());
    painter.rect_filled(fill, 4.0, if peak > 0.9 { Color32::from_rgb(0xd1, 0x5a, 0x4a) } else { ACCENT });
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(40));
}

pub fn style(ctx: &egui::Context) {
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
}
