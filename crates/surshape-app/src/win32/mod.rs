//! Widgets con el look del tema clásico de Windows 2000/XP, dibujados en
//! egui (sin cambiar de toolkit): biseles 3D, campos hundidos blancos,
//! spinners, combos con flecha, casillas y radios clásicos, group boxes con
//! borde grabado, pestañas, scrollbars con flechas, toolbar plana con
//! hot-tracking, paneles de estado hundidos y barras de título con degradado.
//!
//! Sin esquinas redondeadas, sin sombras difusas, sin animaciones.
//!
//! **Auditoría de layout** ([`audit`]): cada widget de este módulo registra
//! su rectángulo y el del contenedor que le tocó; los tests de layout fallan
//! si alguno se sale (regla dura: nada sale de su contenedor).

pub mod icons;
mod widgets;

pub use icons::Icon;
pub use widgets::*;

use crate::theme;
use eframe::egui::{self, pos2, vec2, Color32, Mesh, Painter, Pos2, Rect, Shape, Stroke, Ui};

/// Cómo se ve un borde 3D.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bevel {
    /// Botón en reposo (sobresale).
    Raised,
    /// Botón presionado.
    Pressed,
    /// Campo editable, lista, visor (hundido, 2 px).
    Sunken,
    /// Panel de estado, botón de toolbar presionado (hundido, 1 px).
    ThinSunken,
    /// Botón de toolbar bajo el puntero (sobresale, 1 px).
    ThinRaised,
}

/// Borde de 1 px: arriba/izquierda `tl`, abajo/derecha `br`.
pub fn edge(p: &Painter, r: Rect, tl: Color32, br: Color32) {
    let px = 1.0;
    p.rect_filled(Rect::from_min_max(r.min, pos2(r.max.x - px, r.min.y + px)), 0.0, tl);
    p.rect_filled(Rect::from_min_max(r.min, pos2(r.min.x + px, r.max.y - px)), 0.0, tl);
    p.rect_filled(Rect::from_min_max(pos2(r.min.x, r.max.y - px), r.max), 0.0, br);
    p.rect_filled(Rect::from_min_max(pos2(r.max.x - px, r.min.y), r.max), 0.0, br);
}

/// Dibuja el borde 3D de `r` (sin relleno).
pub fn bevel(p: &Painter, r: Rect, b: Bevel) {
    let th = theme::t();
    #[allow(non_snake_case)]
    let (DARK, FACE, LIGHT, SHADOW, SUNKEN) = (th.DARK, th.FACE, th.LIGHT, th.SHADOW, th.SUNKEN_EDGE);
    match b {
        Bevel::Raised => {
            edge(p, r, LIGHT, DARK);
            edge(p, r.shrink(1.0), FACE, SHADOW);
        }
        Bevel::Pressed => {
            edge(p, r, DARK, LIGHT);
            edge(p, r.shrink(1.0), SHADOW, FACE);
        }
        Bevel::Sunken => {
            edge(p, r, SUNKEN, LIGHT);
            edge(p, r.shrink(1.0), DARK, FACE);
        }
        Bevel::ThinSunken => edge(p, r, SUNKEN, LIGHT),
        Bevel::ThinRaised => edge(p, r, LIGHT, SHADOW),
    }
}

/// Borde grabado (group boxes, separadores).
pub fn etched(p: &Painter, r: Rect) {
    p.rect_stroke(r.translate(vec2(1.0, 1.0)), 0.0, Stroke::new(1.0_f32, theme::t().LIGHT), egui::StrokeKind::Inside);
    p.rect_stroke(r, 0.0, Stroke::new(1.0_f32, theme::t().SHADOW), egui::StrokeKind::Inside);
}

/// Separador vertical grabado (toolbar).
pub fn vsep(p: &Painter, x: f32, y0: f32, y1: f32) {
    p.rect_filled(Rect::from_min_max(pos2(x, y0), pos2(x + 1.0, y1)), 0.0, theme::t().SHADOW);
    p.rect_filled(Rect::from_min_max(pos2(x + 1.0, y0), pos2(x + 2.0, y1)), 0.0, theme::t().LIGHT);
}

/// Rectángulo con degradado horizontal (barras de título).
pub fn gradient_h(p: &Painter, r: Rect, a: Color32, b: Color32) {
    let mut m = Mesh::default();
    m.colored_vertex(r.left_top(), a);
    m.colored_vertex(r.right_top(), b);
    m.colored_vertex(r.right_bottom(), b);
    m.colored_vertex(r.left_bottom(), a);
    m.add_triangle(0, 1, 2);
    m.add_triangle(0, 2, 3);
    p.add(Shape::mesh(m));
}

/// Rectángulo de foco punteado (control con foco del teclado).
pub fn focus_rect(p: &Painter, r: Rect) {
    let s = Stroke::new(1.0_f32, theme::t().TEXT);
    let pts = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
    for w in pts.windows(2) {
        p.extend(Shape::dotted_line(&[w[0], w[1]], theme::t().TEXT, 2.0, 0.5));
    }
    let _ = s;
}

/// Texto con relieve (deshabilitado, como Windows: blanco desplazado y
/// sombra encima).
pub fn disabled_text(p: &Painter, pos: Pos2, anchor: egui::Align2, text: &str, font: egui::FontId) {
    p.text(pos + vec2(1.0, 1.0), anchor, text, font.clone(), theme::t().LIGHT);
    p.text(pos, anchor, text, font, theme::t().TEXT_DISABLED);
}

/// Auditoría de layout: cada widget registra su rectángulo y el disponible
/// en su contenedor. Solo activa en los tests (no cuesta nada si no).
#[cfg_attr(not(test), allow(dead_code))]
pub mod audit {
    use eframe::egui::{Rect, Ui};
    use std::cell::RefCell;

    #[derive(Clone, Debug)]
    pub struct Entry {
        pub what: String,
        pub rect: Rect,
        pub bound: Rect,
    }

    impl Entry {
        /// ¿Se sale de su contenedor a lo ancho? (tolerancia de medio punto)
        /// A lo alto no se mide: las filas crecen para que entre lo que
        /// tienen y lo que no cabe en una página se desplaza.
        pub fn overflows(&self) -> bool {
            let (r, b) = (self.rect, self.bound);
            r.left() < b.left() - 0.5 || r.right() > b.right() + 0.5
        }
    }

    thread_local! {
        static LOG: RefCell<Option<Vec<Entry>>> = const { RefCell::new(None) };
    }

    /// Empieza a registrar (borra lo anterior). Por hilo: los tests corren
    /// en paralelo.
    pub fn start() {
        LOG.with(|l| *l.borrow_mut() = Some(Vec::new()));
    }

    /// Lo registrado hasta ahora (y deja de registrar).
    pub fn take() -> Vec<Entry> {
        LOG.with(|l| l.borrow_mut().take()).unwrap_or_default()
    }

    /// Registra un widget contra el contenedor que tenía antes de ocupar
    /// lugar (egui agranda el contenedor si algo no cabe).
    pub fn record_in(ui: &Ui, bound: Rect, what: &str, rect: Rect) {
        LOG.with(|l| {
            if let Some(v) = l.borrow_mut().as_mut() {
                if ui.is_rect_visible(rect) {
                    v.push(Entry { what: what.to_string(), rect, bound });
                }
            }
        });
    }

    /// Registra un widget. El contenedor es el rectángulo máximo del `ui`
    /// (en un scroll vertical no tiene límite de alto: lo que no cabe se
    /// desplaza, no se sale).
    pub fn record(ui: &Ui, what: &str, rect: Rect) {
        LOG.with(|l| {
            if let Some(v) = l.borrow_mut().as_mut() {
                // Lo que queda fuera de la parte visible no se dibuja: no cuenta.
                if ui.is_rect_visible(rect) {
                    v.push(Entry { what: what.to_string(), rect, bound: ui.max_rect() });
                }
            }
        });
    }
}

/// Alto estándar de un control de una línea.
pub const ROW_H: f32 = 20.0;

pub(crate) fn text_width(ui: &Ui, text: &str, font: &egui::FontId) -> f32 {
    ui.fonts(|f| f.layout_no_wrap(text.to_string(), font.clone(), theme::t().TEXT).size().x)
}
