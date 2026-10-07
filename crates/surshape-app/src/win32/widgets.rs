//! Controles Win32 clásicos.

use super::icons::{self, Icon};
use super::{audit, bevel, disabled_text, edge, etched, focus_rect, gradient_h, text_width, Bevel, ROW_H};
use crate::theme::{self, body_font, bold_font, ui_font};
use eframe::egui::{
    self, pos2, vec2, Align, Align2, Color32, Id, Layout, PopupCloseBehavior, Rect, Response, RichText, Sense, Ui, UiBuilder,
    WidgetText,
};

// --- Botones -------------------------------------------------------------------------

/// Botón clásico con bisel (ancho mínimo 75, como en los diálogos).
pub fn button(ui: &mut Ui, text: &str) -> Response {
    button_w(ui, text, 75.0, true)
}

/// Botón con ancho mínimo y habilitado o no.
pub fn button_w(ui: &mut Ui, text: &str, min_w: f32, enabled: bool) -> Response {
    let bound = ui.max_rect();
    let font = ui_font();
    let w = (text_width(ui, text, &font) + 16.0).max(min_w);
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(vec2(w, ROW_H + 3.0), sense);
    audit::record_in(ui, bound, text, rect);
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        p.rect_filled(rect, 0.0, theme::FACE);
        let down = enabled && resp.is_pointer_button_down_on();
        bevel(p, rect, if down { Bevel::Pressed } else { Bevel::Raised });
        let c = rect.center() + if down { vec2(1.0, 1.0) } else { vec2(0.0, 0.0) };
        let clip = p.with_clip_rect(rect.shrink(2.0));
        if enabled {
            clip.text(c, Align2::CENTER_CENTER, text, font, theme::TEXT);
        } else {
            disabled_text(&clip, c, Align2::CENTER_CENTER, text, font);
        }
        if resp.has_focus() {
            focus_rect(p, rect.shrink(4.0));
        }
    }
    resp
}

/// Botón principal (Render): relleno de acento y texto en negrita.
pub fn accent_button(ui: &mut Ui, text: &str, min_w: f32, enabled: bool) -> Response {
    let bound = ui.max_rect();
    let font = bold_font(theme::UI_SIZE);
    let w = (text_width(ui, text, &font) + 24.0).max(min_w);
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(vec2(w, ROW_H + 5.0), sense);
    audit::record_in(ui, bound, text, rect);
    let p = ui.painter();
    let down = enabled && resp.is_pointer_button_down_on();
    p.rect_filled(rect, 0.0, if enabled { theme::ACCENT } else { theme::FACE });
    // Borde oscuro exterior: botón por defecto de un diálogo.
    p.rect_stroke(rect, 0.0, egui::Stroke::new(1.0_f32, theme::DARK), egui::StrokeKind::Inside);
    bevel(p, rect.shrink(1.0), if down { Bevel::Pressed } else { Bevel::Raised });
    let c = rect.center() + if down { vec2(1.0, 1.0) } else { vec2(0.0, 0.0) };
    if enabled {
        p.text(c, Align2::CENTER_CENTER, text, font, theme::TEXT);
    } else {
        disabled_text(p, c, Align2::CENTER_CENTER, text, font);
    }
    if resp.has_focus() {
        focus_rect(p, rect.shrink(4.0));
    }
    resp
}

/// Botón plano de toolbar (16x16) con hot-tracking: relieve fino al pasar
/// el puntero, hundido al presionar o si está activo.
pub fn tool_button(ui: &mut Ui, icon: Icon, tooltip: &str, enabled: bool, active: bool) -> Response {
    let bound = ui.max_rect();
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(vec2(23.0, 22.0), sense);
    audit::record_in(ui, bound, tooltip, rect);
    let p = ui.painter();
    let down = enabled && resp.is_pointer_button_down_on();
    if active {
        // Patrón de botón "encendido": cara aclarada.
        p.rect_filled(rect, 0.0, theme::pal::TURQUESA_4);
    }
    if down || active {
        bevel(p, rect, Bevel::ThinSunken);
    } else if enabled && resp.hovered() {
        bevel(p, rect, Bevel::ThinRaised);
    }
    let r = Rect::from_center_size(rect.center() + if down { vec2(1.0, 1.0) } else { vec2(0.0, 0.0) }, vec2(16.0, 16.0));
    icons::paint(p, r, icon, enabled);
    resp.on_hover_text(tooltip)
}

/// Botón cuadrado pequeño con un ícono (spinner, scrollbar, cerrar).
pub fn icon_button(ui: &mut Ui, rect: Rect, id: Id, icon: Icon, enabled: bool) -> Response {
    let resp = ui.interact(rect, id, if enabled { Sense::click_and_drag() } else { Sense::hover() });
    let p = ui.painter();
    p.rect_filled(rect, 0.0, theme::FACE);
    let down = enabled && resp.is_pointer_button_down_on();
    bevel(p, rect, if down { Bevel::Pressed } else { Bevel::Raised });
    let s = (rect.height().min(rect.width()) - 2.0).clamp(6.0, 16.0);
    let r = Rect::from_center_size(rect.center() + if down { vec2(1.0, 1.0) } else { vec2(0.0, 0.0) }, vec2(s, s));
    icons::paint(p, r, icon, enabled);
    resp
}

/// ¿Repetir la acción de un botón mantenido? (como las flechas de Windows:
/// la primera al hacer clic, luego cada 60 ms tras 400 ms).
fn repeat(ui: &Ui, resp: &Response) -> bool {
    if resp.drag_started() || (resp.clicked() && !resp.dragged()) {
        let now = ui.input(|i| i.time);
        ui.ctx().data_mut(|d| d.insert_temp(resp.id.with("rep"), now + 0.4)); // i18n-ok
        return resp.drag_started();
    }
    if resp.is_pointer_button_down_on() {
        ui.ctx().request_repaint();
        let now = ui.input(|i| i.time);
        let next: f64 = ui.ctx().data(|d| d.get_temp(resp.id.with("rep"))).unwrap_or(now + 0.4); // i18n-ok
        if now >= next {
            ui.ctx().data_mut(|d| d.insert_temp(resp.id.with("rep"), now + 0.06)); // i18n-ok
            return true;
        }
    }
    false
}

// --- Texto -------------------------------------------------------------------------------

/// Etiqueta de una línea: si no cabe, se corta con "…" y el texto completo
/// queda en el tooltip.
pub fn label(ui: &mut Ui, text: impl Into<WidgetText>) -> Response {
    let bound = ui.max_rect();
    let r = ui.add(egui::Label::new(text).truncate());
    audit::record_in(ui, bound, "etiqueta", r.rect); // i18n-ok
    r
}

pub fn label_color(ui: &mut Ui, text: &str, color: Color32) -> Response {
    label(ui, RichText::new(text).color(color))
}

/// Texto secundario.
pub fn muted(ui: &mut Ui, text: &str) -> Response {
    label(ui, RichText::new(text).color(theme::TEXT_MUTED))
}

/// Negrita.
pub fn bold(ui: &mut Ui, text: &str) -> Response {
    label(ui, RichText::new(text).font(bold_font(theme::UI_SIZE)))
}

/// Encabezado de columna en una tabla (negrita, sin recorte: la columna se
/// ajusta a él).
pub fn head(ui: &mut Ui, text: &str) -> Response {
    ui.label(RichText::new(text).font(bold_font(theme::UI_SIZE)))
}

/// Texto que se ajusta al ancho (descripciones).
pub fn wrap(ui: &mut Ui, text: impl Into<WidgetText>) -> Response {
    let bound = ui.max_rect();
    let r = ui.add(egui::Label::new(text).wrap());
    audit::record_in(ui, bound, "texto", r.rect); // i18n-ok
    r
}

/// Texto monoespaciado seleccionable (línea de comando, consola).
pub fn mono_selectable(ui: &mut Ui, text: &str) -> Response {
    let bound = ui.max_rect();
    let r = ui.add(egui::Label::new(RichText::new(text).font(theme::mono_font(theme::MONO_SIZE))).selectable(true).wrap());
    audit::record_in(ui, bound, "mono", r.rect); // i18n-ok
    r
}

// --- Casillas y radios ----------------------------------------------------------------

/// Casilla clásica: cuadro blanco hundido de 13 px con tilde.
pub fn checkbox(ui: &mut Ui, on: &mut bool, text: &str) -> Response {
    checkbox_en(ui, on, text, true)
}

pub fn checkbox_en(ui: &mut Ui, on: &mut bool, text: &str, enabled: bool) -> Response {
    let bound = ui.max_rect();
    let font = ui_font();
    let tw = if text.is_empty() { 0.0 } else { text_width(ui, text, &font) + 5.0 };
    let avail = ui.available_width().max(13.0);
    let w = (13.0 + tw).min(avail);
    let (rect, mut resp) = ui.allocate_exact_size(vec2(w, ROW_H), if enabled { Sense::click() } else { Sense::hover() });
    audit::record_in(ui, bound, text, rect);
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let p = ui.painter();
    let b = Rect::from_min_size(pos2(rect.left(), rect.center().y - 6.5), vec2(13.0, 13.0));
    p.rect_filled(b, 0.0, if enabled { theme::FIELD } else { theme::FACE });
    bevel(p, b, Bevel::Sunken);
    if *on {
        icons::paint(p, b.shrink(1.5), Icon::Check, enabled);
    }
    if !text.is_empty() {
        let clip = p.with_clip_rect(rect);
        let pos = pos2(b.right() + 5.0, rect.center().y);
        if enabled {
            clip.text(pos, Align2::LEFT_CENTER, text, font.clone(), theme::TEXT);
        } else {
            disabled_text(&clip, pos, Align2::LEFT_CENTER, text, font.clone());
        }
        if resp.has_focus() {
            focus_rect(p, Rect::from_min_size(pos2(b.right() + 3.0, rect.top() + 2.0), vec2(tw, rect.height() - 4.0)).intersect(rect));
        }
    }
    if tw + 13.0 > w {
        resp = resp.on_hover_text(text);
    }
    resp
}

/// Botón de radio clásico. Devuelve la respuesta (clicked = elegirlo).
pub fn radio(ui: &mut Ui, selected: bool, text: &str) -> Response {
    let bound = ui.max_rect();
    let font = ui_font();
    let tw = text_width(ui, text, &font) + 5.0;
    let w = (13.0 + tw).min(ui.available_width().max(13.0));
    let (rect, resp) = ui.allocate_exact_size(vec2(w, ROW_H), Sense::click());
    audit::record_in(ui, bound, text, rect);
    let p = ui.painter();
    let c = pos2(rect.left() + 6.5, rect.center().y);
    // Círculo hundido: media luna oscura arriba, clara abajo.
    p.circle_filled(c, 6.0, theme::FIELD);
    let arc = |a0: f32, a1: f32, r: f32, col: Color32| {
        let pts: Vec<_> = (0..=12).map(|i| {
            let a = a0 + (a1 - a0) * i as f32 / 12.0;
            c + vec2(a.cos(), a.sin()) * r
        }).collect();
        p.add(egui::Shape::line(pts, egui::Stroke::new(1.0_f32, col)));
    };
    use std::f32::consts::PI;
    arc(PI * 0.75, PI * 1.75, 5.8, theme::SHADOW);
    arc(PI * 0.75, PI * 1.75, 4.8, theme::DARK);
    arc(-PI * 0.25, PI * 0.75, 5.8, theme::LIGHT);
    arc(-PI * 0.25, PI * 0.75, 4.8, theme::FACE);
    if selected {
        p.circle_filled(c, 2.0, theme::TEXT);
    }
    let clip = p.with_clip_rect(rect);
    clip.text(pos2(rect.left() + 18.0, rect.center().y), Align2::LEFT_CENTER, text, font, theme::TEXT);
    if resp.has_focus() {
        focus_rect(p, Rect::from_min_size(pos2(rect.left() + 16.0, rect.top() + 2.0), vec2(tw, rect.height() - 4.0)).intersect(rect));
    }
    resp
}

// --- Campos -----------------------------------------------------------------------------

/// Marco hundido blanco alrededor de lo que dibuja `add`.
pub fn sunken<R>(ui: &mut Ui, fill: Color32, add: impl FnOnce(&mut Ui) -> R) -> R {
    let bound = ui.max_rect();
    let frame = egui::Frame::NONE.fill(fill).inner_margin(egui::Margin::same(3));
    let out = frame.show(ui, add);
    bevel(ui.painter(), out.response.rect, Bevel::Sunken);
    audit::record_in(ui, bound, "marco", out.response.rect); // i18n-ok
    out.inner
}

/// Campo de texto de una línea (hundido, blanco).
pub fn text_field(ui: &mut Ui, text: &mut String, width: f32, hint: &str) -> Response {
    let bound = ui.max_rect();
    let w = width.min(ui.available_width()).max(30.0);
    let (rect, _) = ui.allocate_exact_size(vec2(w, ROW_H + 1.0), Sense::hover());
    audit::record_in(ui, bound, hint, rect);
    ui.painter().rect_filled(rect, 0.0, theme::FIELD);
    bevel(ui.painter(), rect, Bevel::Sunken);
    let inner = rect.shrink2(vec2(4.0, 2.0));
    let mut child = ui.new_child(UiBuilder::new().max_rect(inner).layout(Layout::left_to_right(Align::Center)));
    child.add(egui::TextEdit::singleline(text).frame(false).desired_width(inner.width()).font(ui_font()).hint_text(hint).margin(vec2(0.0, 1.0)))
}

/// Campo de solo lectura hundido (rutas, nombres de archivo).
pub fn readonly_field(ui: &mut Ui, text: &str, width: f32) -> Response {
    let bound = ui.max_rect();
    let w = width.min(ui.available_width()).max(20.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(w, ROW_H + 1.0), Sense::hover());
    audit::record_in(ui, bound, text, rect);
    let p = ui.painter();
    p.rect_filled(rect, 0.0, theme::FACE);
    bevel(p, rect, Bevel::Sunken);
    let font = ui_font();
    let full = text_width(ui, text, &font);
    let clip = p.with_clip_rect(rect.shrink(2.0));
    let shown = elide(ui, text, &font, rect.width() - 8.0);
    clip.text(pos2(rect.left() + 4.0, rect.center().y), Align2::LEFT_CENTER, &shown, font, theme::TEXT);
    if full > rect.width() - 8.0 {
        resp.on_hover_text(text)
    } else {
        resp
    }
}

/// Corta `text` con "…" para que entre en `max_w`.
pub fn elide(ui: &Ui, text: &str, font: &egui::FontId, max_w: f32) -> String {
    if text_width(ui, text, font) <= max_w {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let (mut lo, mut hi) = (0usize, chars.len());
    while lo < hi {
        let mid = (lo + hi + 1) / 2;
        let s: String = chars[..mid].iter().collect::<String>() + "…";
        if text_width(ui, &s, font) <= max_w {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    chars[..lo].iter().collect::<String>() + "…"
}

/// Configuración de un spinner numérico.
#[derive(Clone, Copy, Debug)]
pub struct Spin<'a> {
    pub min: f64,
    pub max: f64,
    /// Paso de las flechas.
    pub step: f64,
    pub decimals: usize,
    pub suffix: &'a str,
    pub width: f32,
}

impl Default for Spin<'_> {
    fn default() -> Self {
        Self { min: f64::MIN, max: f64::MAX, step: 1.0, decimals: 0, suffix: "", width: 70.0 }
    }
}

/// Campo numérico hundido con flechas arriba/abajo. El valor se escribe
/// exacto (clic y tipear) o se arrastra; las flechas suman el paso.
pub fn spin(ui: &mut Ui, v: &mut f64, s: Spin) -> Response {
    let bound = ui.max_rect();
    let w = s.width.min(ui.available_width()).max(40.0);
    let (rect, _) = ui.allocate_exact_size(vec2(w, ROW_H + 1.0), Sense::hover());
    audit::record_in(ui, bound, "spinner", rect); // i18n-ok
    let p = ui.painter();
    p.rect_filled(rect, 0.0, theme::FIELD);
    bevel(p, rect, Bevel::Sunken);
    let arrows = Rect::from_min_max(pos2(rect.right() - 15.0, rect.top() + 2.0), rect.max - vec2(2.0, 2.0));
    let field = Rect::from_min_max(rect.min + vec2(3.0, 2.0), pos2(arrows.left() - 1.0, rect.bottom() - 2.0));
    let speed = if s.step > 0.0 { s.step * 0.25 } else { 0.1 };
    let mut child = ui.new_child(UiBuilder::new().max_rect(field).layout(Layout::left_to_right(Align::Center)));
    child.style_mut().override_font_id = Some(theme::mono_font(theme::MONO_SIZE));
    {
        let wv = &mut child.style_mut().visuals.widgets;
        for w in [&mut wv.inactive, &mut wv.hovered, &mut wv.active] {
            w.bg_fill = theme::FIELD;
            w.weak_bg_fill = theme::FIELD;
            w.bg_stroke = egui::Stroke::NONE;
        }
    }
    child.spacing_mut().interact_size.y = field.height();
    let mut dv = egui::DragValue::new(v).range(s.min..=s.max).speed(speed).fixed_decimals(s.decimals);
    if !s.suffix.is_empty() {
        dv = dv.suffix(format!(" {}", s.suffix));
    }
    let mut resp = child.add_sized(field.size(), dv);
    let half = arrows.height() / 2.0;
    let up = Rect::from_min_size(arrows.min, vec2(arrows.width(), half));
    let down = Rect::from_min_size(arrows.min + vec2(0.0, half), vec2(arrows.width(), half));
    let ru = icon_button(ui, up, resp.id.with("arriba"), Icon::Up, true); // i18n-ok
    let rd = icon_button(ui, down, resp.id.with("abajo"), Icon::Down, true); // i18n-ok
    if repeat(ui, &ru) || (ru.clicked() && !ru.dragged()) {
        *v = (*v + s.step).clamp(s.min, s.max);
        resp.mark_changed();
    }
    if repeat(ui, &rd) || (rd.clicked() && !rd.dragged()) {
        *v = (*v - s.step).clamp(s.min, s.max);
        resp.mark_changed();
    }
    resp
}

/// Trackbar clásico: canal hundido y pulgar en relieve. Escala logarítmica
/// opcional (si el mínimo es > 0).
pub fn slider(ui: &mut Ui, v: &mut f64, min: f64, max: f64, log: bool, width: f32) -> Response {
    let bound = ui.max_rect();
    let w = width.min(ui.available_width()).max(40.0);
    let (rect, mut resp) = ui.allocate_exact_size(vec2(w, ROW_H + 2.0), Sense::click_and_drag());
    audit::record_in(ui, bound, "slider", rect); // i18n-ok
    let log = log && min > 0.0 && max > min;
    let to_t = |x: f64| -> f32 {
        let t = if log { (x / min).ln() / (max / min).ln() } else { (x - min) / (max - min).max(1e-12) };
        t.clamp(0.0, 1.0) as f32
    };
    let from_t = |t: f32| -> f64 {
        let t = t.clamp(0.0, 1.0) as f64;
        if log {
            min * (max / min).powf(t)
        } else {
            min + (max - min) * t
        }
    };
    let track = Rect::from_min_max(pos2(rect.left() + 5.0, rect.center().y - 2.0), pos2(rect.right() - 5.0, rect.center().y + 2.0));
    if let Some(pos) = resp.interact_pointer_pos() {
        if resp.dragged() || resp.clicked() {
            let t = (pos.x - track.left()) / track.width().max(1.0);
            let nv = from_t(t);
            if (nv - *v).abs() > f64::EPSILON {
                *v = nv;
                resp.mark_changed();
            }
        }
    }
    // Teclado: flechas mueven un 1 %.
    if resp.has_focus() {
        let (l, r) = ui.input(|i| (i.key_pressed(egui::Key::ArrowLeft), i.key_pressed(egui::Key::ArrowRight)));
        if l || r {
            let t = to_t(*v) + if r { 0.01 } else { -0.01 };
            *v = from_t(t);
            resp.mark_changed();
        }
    }
    let p = ui.painter();
    bevel(p, track, Bevel::Sunken);
    let x = track.left() + to_t(*v) * track.width();
    let thumb = Rect::from_center_size(pos2(x, rect.center().y), vec2(10.0, rect.height() - 2.0));
    p.rect_filled(thumb, 0.0, theme::FACE);
    bevel(p, thumb, Bevel::Raised);
    if resp.has_focus() {
        focus_rect(p, rect);
    }
    resp
}

// --- Combos ------------------------------------------------------------------------------

/// Combo clásico: campo hundido con el valor y botón de flecha; al hacer
/// clic abre la lista debajo. `add` dibuja los elementos con [`combo_item`].
pub fn combo(ui: &mut Ui, id_salt: impl std::hash::Hash, selected: &str, width: f32, add: impl FnOnce(&mut Ui)) -> Response {
    let bound = ui.max_rect();
    let w = width.min(ui.available_width()).max(40.0);
    let id = ui.make_persistent_id(id_salt);
    let (rect, resp) = ui.allocate_exact_size(vec2(w, ROW_H + 1.0), Sense::click());
    audit::record_in(ui, bound, selected, rect);
    let p = ui.painter();
    p.rect_filled(rect, 0.0, theme::FIELD);
    bevel(p, rect, Bevel::Sunken);
    let arrow = Rect::from_min_max(pos2(rect.right() - 17.0, rect.top() + 2.0), rect.max - vec2(2.0, 2.0));
    let text_r = Rect::from_min_max(rect.min + vec2(3.0, 3.0), pos2(arrow.left() - 2.0, rect.bottom() - 3.0));
    let font = ui_font();
    let shown = elide(ui, selected, &font, text_r.width() - 4.0);
    let open = ui.memory(|m| m.is_popup_open(id));
    if resp.has_focus() || open {
        p.rect_filled(text_r, 0.0, theme::HIGHLIGHT);
        p.text(pos2(text_r.left() + 2.0, text_r.center().y), Align2::LEFT_CENTER, &shown, font, theme::HIGHLIGHT_TEXT);
    } else {
        p.text(pos2(text_r.left() + 2.0, text_r.center().y), Align2::LEFT_CENTER, &shown, font, theme::TEXT);
    }
    let ra = icon_button(ui, arrow, id.with("flecha"), Icon::Down, true); // i18n-ok
    if resp.clicked() || ra.clicked() {
        ui.memory_mut(|m| m.toggle_popup(id));
    }
    let resp = if shown != selected { resp.on_hover_text(selected) } else { resp };
    egui::popup::popup_below_widget(ui, id, &resp, PopupCloseBehavior::CloseOnClick, |ui| {
        ui.set_min_width(w - 4.0);
        ui.spacing_mut().item_spacing.y = 0.0;
        add(ui);
    });
    resp
}

/// Elemento de una lista (combo, menú): fila completa, resaltada con el
/// color de selección al pasar el puntero o si está elegido.
pub fn combo_item(ui: &mut Ui, selected: bool, text: &str) -> Response {
    list_row(ui, selected, text, "", true)
}

/// Fila de lista o de menú: texto a la izquierda, atajo a la derecha.
pub fn list_row(ui: &mut Ui, selected: bool, text: &str, right: &str, enabled: bool) -> Response {
    let font = ui_font();
    let w = ui.available_width().max(text_width(ui, text, &font) + text_width(ui, right, &font) + 40.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(w, ROW_H - 1.0), if enabled { Sense::click() } else { Sense::hover() });
    let p = ui.painter();
    let hot = enabled && (selected || resp.hovered());
    if hot {
        p.rect_filled(rect, 0.0, theme::HIGHLIGHT);
    }
    let color = if hot { theme::HIGHLIGHT_TEXT } else { theme::TEXT };
    let lp = pos2(rect.left() + 18.0, rect.center().y);
    let rp = pos2(rect.right() - 10.0, rect.center().y);
    if enabled {
        p.text(lp, Align2::LEFT_CENTER, text, font.clone(), color);
        if !right.is_empty() {
            p.text(rp, Align2::RIGHT_CENTER, right, font, color);
        }
    } else {
        disabled_text(p, lp, Align2::LEFT_CENTER, text, font.clone());
        if !right.is_empty() {
            disabled_text(p, rp, Align2::RIGHT_CENTER, right, font);
        }
    }
    resp
}

/// Separador de menú (línea grabada).
pub fn menu_sep(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 7.0), Sense::hover());
    let y = rect.center().y;
    ui.painter().rect_filled(Rect::from_min_max(pos2(rect.left() + 2.0, y - 1.0), pos2(rect.right() - 2.0, y)), 0.0, theme::SHADOW);
    ui.painter().rect_filled(Rect::from_min_max(pos2(rect.left() + 2.0, y), pos2(rect.right() - 2.0, y + 1.0)), 0.0, theme::LIGHT);
}

/// Encabezado no clicable dentro de un menú (grupo).
pub fn menu_caption(ui: &mut Ui, text: &str) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H - 2.0), Sense::hover());
    ui.painter().text(pos2(rect.left() + 6.0, rect.center().y), Align2::LEFT_CENTER, text, bold_font(theme::SMALL_SIZE), theme::TEXT_MUTED);
}

// --- Contenedores ------------------------------------------------------------------------

/// Group box: borde grabado con el título sobre el borde superior.
pub fn group<R>(ui: &mut Ui, title: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    let bound = ui.max_rect();
    let font = ui_font();
    let top = ui.cursor().top();
    ui.add_space(7.0);
    let frame = egui::Frame::NONE.inner_margin(egui::Margin { left: 8, right: 8, top: 8, bottom: 6 });
    let out = frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        add(ui)
    });
    let r = out.response.rect;
    let border = Rect::from_min_max(pos2(r.left(), top + 7.0), r.max);
    let p = ui.painter();
    etched(p, border);
    if !title.is_empty() {
        let max_w = (border.width() - 20.0).max(10.0);
        let shown = elide(ui, title, &font, max_w);
        let tw = text_width(ui, &shown, &font);
        let tr = Rect::from_min_size(pos2(border.left() + 7.0, top), vec2(tw + 4.0, 14.0));
        p.rect_filled(tr, 0.0, theme::FACE);
        p.text(pos2(tr.left() + 2.0, tr.center().y), Align2::LEFT_CENTER, shown, font, theme::TEXT);
    }
    audit::record_in(ui, bound, title, border);
    out.inner
}

/// Pestañas clásicas. Devuelve true si cambió la elegida. Después de esto,
/// el contenido va en [`tab_panel`].
pub fn tabs(ui: &mut Ui, sel: &mut usize, labels: &[&str]) -> bool {
    let bound = ui.max_rect();
    let font = ui_font();
    let mut changed = false;
    let h = ROW_H + 2.0;
    let start = ui.cursor().min;
    let avail = ui.available_width();
    let mut x = start.x + 2.0;
    let mut y = start.y + 2.0;
    let mut rows = 1;
    let mut rects = Vec::with_capacity(labels.len());
    for l in labels {
        let w = text_width(ui, l, &font) + 16.0;
        if x + w > start.x + avail && x > start.x + 2.0 {
            // No caben: otra fila (como Windows con muchas pestañas).
            x = start.x + 2.0;
            y += h;
            rows += 1;
        }
        rects.push(Rect::from_min_size(pos2(x, y), vec2(w, h)));
        x += w;
    }
    let total = Rect::from_min_size(start, vec2(avail, rows as f32 * h + 2.0));
    ui.allocate_rect(total, Sense::hover());
    for (i, (l, r)) in labels.iter().zip(&rects).enumerate() {
        let is = i == *sel;
        let r = if is { r.expand2(vec2(2.0, 2.0)) } else { *r };
        let resp = ui.interact(r, ui.id().with(("pestana", i)), Sense::click()); // i18n-ok
        audit::record_in(ui, bound, l, r);
        let p = ui.painter();
        p.rect_filled(r, 0.0, theme::FACE);
        // Borde arriba e izquierda claros, derecha oscura; sin borde abajo.
        p.rect_filled(Rect::from_min_max(r.left_top(), pos2(r.right() - 1.0, r.top() + 1.0)), 0.0, theme::LIGHT);
        p.rect_filled(Rect::from_min_max(r.left_top(), pos2(r.left() + 1.0, r.bottom())), 0.0, theme::LIGHT);
        p.rect_filled(Rect::from_min_max(pos2(r.right() - 1.0, r.top() + 1.0), r.right_bottom()), 0.0, theme::DARK);
        p.rect_filled(Rect::from_min_max(pos2(r.right() - 2.0, r.top() + 2.0), pos2(r.right() - 1.0, r.bottom())), 0.0, theme::SHADOW);
        p.text(r.center(), Align2::CENTER_CENTER, *l, font.clone(), theme::TEXT);
        if is && resp.has_focus() {
            focus_rect(p, r.shrink(3.0));
        }
        if resp.clicked() && !is {
            *sel = i;
            changed = true;
        }
    }
    changed
}

/// Panel en relieve bajo las pestañas.
pub fn tab_panel<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let frame = egui::Frame::NONE.fill(theme::FACE).inner_margin(egui::Margin::same(8));
    let out = frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        add(ui)
    });
    let r = out.response.rect;
    let p = ui.painter();
    p.rect_filled(Rect::from_min_max(r.left_top(), pos2(r.left() + 1.0, r.bottom())), 0.0, theme::LIGHT);
    p.rect_filled(Rect::from_min_max(r.left_top(), pos2(r.right(), r.top() + 1.0)), 0.0, theme::LIGHT);
    edge(p, Rect::from_min_max(pos2(r.left() - 1.0, r.top() - 1.0), r.max), egui::Color32::TRANSPARENT, theme::DARK);
    edge(p, Rect::from_min_max(pos2(r.left() - 1.0, r.top() - 1.0), r.max - vec2(1.0, 1.0)), egui::Color32::TRANSPARENT, theme::SHADOW);
    out.inner
}

/// Barra de título con degradado (#0B4A78 -> #2887AC) y texto blanco en
/// negrita a la izquierda. `right` dibuja botones a la derecha.
pub fn title_bar(ui: &mut Ui, text: &str, right: impl FnOnce(&mut Ui)) -> Rect {
    let bound = ui.max_rect();
    let h = 20.0;
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::hover());
    gradient_h(ui.painter(), rect, theme::TITLE_A, theme::TITLE_B);
    let font = bold_font(theme::UI_SIZE);
    // El texto no pasa del 40 % del ancho: el contraste se verifica ahí.
    let max_w = (rect.width() * 0.4).max(60.0);
    let shown = elide(ui, text, &font, max_w);
    ui.painter().text(pos2(rect.left() + 6.0, rect.center().y), Align2::LEFT_CENTER, &shown, font, theme::TITLE_TEXT);
    let rr = Rect::from_min_max(pos2(rect.left() + rect.width() * 0.45, rect.top() + 1.0), rect.max - vec2(2.0, 1.0));
    let mut child = ui.new_child(UiBuilder::new().max_rect(rr).layout(Layout::right_to_left(Align::Center)));
    child.spacing_mut().item_spacing.x = 2.0;
    right(&mut child);
    audit::record_in(ui, bound, text, rect);
    rect
}

/// Botón pequeño de barra de título ([X]...).
pub fn title_button(ui: &mut Ui, icon: Icon, tooltip: &str) -> Response {
    let (rect, _) = ui.allocate_exact_size(vec2(16.0, 14.0), Sense::hover());
    icon_button(ui, rect, ui.id().with(("titulo", tooltip)), icon, true).on_hover_text(tooltip) // i18n-ok
}

/// Botón de texto pequeño para barras de título (Copiar, Limpiar...).
pub fn title_text_button(ui: &mut Ui, text: &str) -> Response {
    let font = body_font(theme::SMALL_SIZE);
    let w = text_width(ui, text, &font) + 10.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 16.0), Sense::click());
    let p = ui.painter();
    p.rect_filled(rect, 0.0, theme::FACE);
    bevel(p, rect, if resp.is_pointer_button_down_on() { Bevel::Pressed } else { Bevel::Raised });
    p.text(rect.center(), Align2::CENTER_CENTER, text, font, theme::TEXT);
    resp
}

/// Panel de estado hundido con texto (cortado con "…" si no cabe).
pub fn status_panel(ui: &mut Ui, text: &str, width: f32) -> Response {
    let bound = ui.max_rect();
    let w = width.min(ui.available_width()).max(10.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(w, ROW_H), Sense::hover());
    audit::record_in(ui, bound, text, rect);
    bevel(ui.painter(), rect, Bevel::ThinSunken);
    let font = ui_font();
    let shown = elide(ui, text, &font, rect.width() - 8.0);
    ui.painter().with_clip_rect(rect.shrink(1.0)).text(pos2(rect.left() + 4.0, rect.center().y), Align2::LEFT_CENTER, &shown, font, theme::TEXT);
    if shown != text {
        resp.on_hover_text(text)
    } else {
        resp
    }
}

/// Ancho que ocupa un texto en un panel de estado.
pub fn status_width(ui: &Ui, text: &str) -> f32 {
    text_width(ui, text, &ui_font()) + 10.0
}

// --- Scrollbar con flechas ------------------------------------------------------------------

/// Scrollbar clásica (con flechas) en `rect`. `pos` es el desplazamiento
/// (0..content-view). Devuelve true si cambió.
pub fn scrollbar(ui: &mut Ui, rect: Rect, vertical: bool, pos: &mut f32, content: f32, view: f32, line: f32) -> bool {
    let id = ui.id().with(("scroll", vertical, rect.min.x as i32, rect.min.y as i32)); // i18n-ok
    let max = (content - view).max(0.0);
    let before = *pos;
    let p = ui.painter();
    // Canal: trama clara.
    p.rect_filled(rect, 0.0, theme::pal::TURQUESA_4);
    let len = if vertical { rect.height() } else { rect.width() };
    let thick = if vertical { rect.width() } else { rect.height() };
    let (a, b) = if vertical {
        (Rect::from_min_size(rect.min, vec2(thick, thick)), Rect::from_min_size(pos2(rect.left(), rect.bottom() - thick), vec2(thick, thick)))
    } else {
        (Rect::from_min_size(rect.min, vec2(thick, thick)), Rect::from_min_size(pos2(rect.right() - thick, rect.top()), vec2(thick, thick)))
    };
    let enabled = max > 0.0;
    let ra = icon_button(ui, a, id.with("a"), if vertical { Icon::Up } else { Icon::Left }, enabled); // i18n-ok
    let rb = icon_button(ui, b, id.with("b"), if vertical { Icon::Down } else { Icon::Right }, enabled); // i18n-ok
    if enabled && (repeat(ui, &ra) || (ra.clicked() && !ra.dragged())) {
        *pos -= line;
    }
    if enabled && (repeat(ui, &rb) || (rb.clicked() && !rb.dragged())) {
        *pos += line;
    }
    let track_len = (len - 2.0 * thick).max(1.0);
    if enabled {
        let thumb_len = (track_len * view / content.max(1.0)).clamp(12.0, track_len);
        let t = if max > 0.0 { *pos / max } else { 0.0 };
        let off = thick + (track_len - thumb_len) * t.clamp(0.0, 1.0);
        let thumb = if vertical {
            Rect::from_min_size(pos2(rect.left(), rect.top() + off), vec2(thick, thumb_len))
        } else {
            Rect::from_min_size(pos2(rect.left() + off, rect.top()), vec2(thumb_len, thick))
        };
        let track = if vertical { rect.shrink2(vec2(0.0, thick)) } else { rect.shrink2(vec2(thick, 0.0)) };
        let rt = ui.interact(track, id.with("pista"), Sense::click()); // i18n-ok
        let rh = ui.interact(thumb, id.with("pulgar"), Sense::drag()); // i18n-ok
        if rh.dragged() {
            let d = if vertical { rh.drag_delta().y } else { rh.drag_delta().x };
            *pos += d / (track_len - thumb_len).max(1.0) * max;
        } else if rt.clicked() {
            // Clic en la pista: una página hacia ese lado.
            if let Some(pp) = rt.interact_pointer_pos() {
                let at = if vertical { pp.y } else { pp.x };
                let tc = if vertical { thumb.center().y } else { thumb.center().x };
                *pos += if at < tc { -view } else { view };
            }
        }
        let p = ui.painter();
        p.rect_filled(thumb, 0.0, theme::FACE);
        bevel(p, thumb, Bevel::Raised);
    }
    *pos = pos.clamp(0.0, max);
    (*pos - before).abs() > f32::EPSILON
}

// --- Avisos --------------------------------------------------------------------------------

/// Nivel de un mensaje. El color nunca va solo: siempre ícono + palabra.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Info,
    Warning,
    Error,
}

impl Level {
    pub fn color(self) -> Color32 {
        match self {
            Level::Info => theme::TEXT,
            Level::Warning => theme::WARNING,
            Level::Error => theme::ERROR,
        }
    }
    /// Palabra que precede al mensaje ("Aviso:", "Error:").
    pub fn label_key(self) -> &'static str {
        match self {
            Level::Info => "ui.nivel.info",
            Level::Warning => "ui.nivel.aviso",
            Level::Error => "ui.nivel.error",
        }
    }
}

/// Ícono de nivel (no depende de glifos): info = círculo con "i", aviso =
/// triángulo con "!", error = octógono con "×".
pub fn paint_level_icon(p: &egui::Painter, center: egui::Pos2, size: f32, level: Level) {
    let c = level.color();
    let r = size * 0.5;
    let stroke = egui::Stroke::new((size * 0.12).max(1.2), c);
    match level {
        Level::Info => {
            p.circle_stroke(center, r - stroke.width * 0.5, stroke);
            p.circle_filled(center + vec2(0.0, -r * 0.42), size * 0.08, c);
            p.line_segment([center + vec2(0.0, -r * 0.15), center + vec2(0.0, r * 0.5)], stroke);
        }
        Level::Warning => {
            let pts = vec![center + vec2(0.0, -r), center + vec2(r, r * 0.85), center + vec2(-r, r * 0.85)];
            p.add(egui::Shape::convex_polygon(pts, c, egui::Stroke::NONE));
            let ink = theme::FIELD;
            p.line_segment([center + vec2(0.0, -r * 0.35), center + vec2(0.0, r * 0.3)], egui::Stroke::new(stroke.width, ink));
            p.circle_filled(center + vec2(0.0, r * 0.58), size * 0.07, ink);
        }
        Level::Error => {
            let pts: Vec<_> = (0..8)
                .map(|i| {
                    let a = std::f32::consts::PI / 8.0 + i as f32 * std::f32::consts::PI / 4.0;
                    center + vec2(a.cos(), a.sin()) * r
                })
                .collect();
            p.add(egui::Shape::convex_polygon(pts, c, egui::Stroke::NONE));
            let ink = theme::FIELD;
            let k = r * 0.38;
            let s = egui::Stroke::new(stroke.width, ink);
            p.line_segment([center + vec2(-k, -k), center + vec2(k, k)], s);
            p.line_segment([center + vec2(-k, k), center + vec2(k, -k)], s);
        }
    }
}

/// Mensaje con ícono y palabra de nivel; se ajusta al ancho.
pub fn notice(ui: &mut Ui, level: Level, text: &str) -> Response {
    let bound = ui.max_rect();
    let r = ui
        .horizontal_wrapped(|ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
            paint_level_icon(ui.painter(), rect.center(), 13.0, level);
            if level != Level::Info {
                ui.label(RichText::new(surshape_i18n::tr(level.label_key())).font(bold_font(theme::UI_SIZE)).color(level.color()));
            }
            ui.add(egui::Label::new(RichText::new(text).color(level.color())).wrap());
        })
        .response;
    audit::record_in(ui, bound, text, r.rect);
    r
}
