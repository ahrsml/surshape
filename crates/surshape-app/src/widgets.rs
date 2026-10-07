//! Estados de celda y formatos de números. Los controles están en
//! [`crate::win32`].

use crate::theme;
use eframe::egui::{self, vec2, Stroke};
use surshape_i18n::t;

/// Estado de una celda de la grilla, tal como se muestra.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CellState {
    Fuente,
    /// El archivo de la fuente no está.
    FuenteFalta,
    SinRender,
    Renderizado,
    Desactualizado,
    Error,
    EnCola,
    EnProceso(f32),
}

impl CellState {
    pub fn key(self) -> &'static str {
        match self {
            CellState::Fuente => "ui.celda.fuente",
            CellState::FuenteFalta => "ui.celda.falta",
            CellState::SinRender => "ui.celda.sin_render",
            CellState::Renderizado => "ui.celda.renderizada",
            CellState::Desactualizado => "ui.celda.desactualizada",
            CellState::Error => "ui.celda.error",
            CellState::EnCola => "ui.celda.en_cola",
            CellState::EnProceso(_) => "ui.celda.en_proceso",
        }
    }
    pub fn help_key(self) -> &'static str {
        match self {
            CellState::Fuente => "ui.ayuda.celda.fuente",
            CellState::FuenteFalta => "ui.ayuda.celda.falta",
            CellState::SinRender => "ui.ayuda.celda.sin_render",
            CellState::Renderizado => "ui.ayuda.celda.renderizada",
            CellState::Desactualizado => "ui.ayuda.celda.desactualizada",
            CellState::Error => "ui.ayuda.celda.error",
            CellState::EnCola => "ui.ayuda.celda.en_cola",
            CellState::EnProceso(_) => "ui.ayuda.celda.en_proceso",
        }
    }
}

/// Ícono de estado (dibujado, siempre acompañado de texto en la línea de
/// ayuda): renderizada = círculo con tilde · desactualizada = reloj · sin
/// render = círculo vacío · en cola = tres puntos · en proceso = anillo de
/// avance · error / falta = octógono con ×. `ink` es el color sobre el fondo
/// (cambia si la celda está seleccionada).
pub fn paint_state_icon(p: &egui::Painter, c: egui::Pos2, size: f32, st: CellState, ink: egui::Color32) {
    let r = size * 0.5;
    let w = (size * 0.13).max(1.3);
    match st {
        CellState::Fuente => {
            // Onda pequeña: la fuente es audio original.
            let pts: Vec<_> = (0..=12)
                .map(|i| {
                    let x = -r + 2.0 * r * i as f32 / 12.0;
                    c + vec2(x, -(x / r * std::f32::consts::PI * 1.5).sin() * r * 0.6)
                })
                .collect();
            p.add(egui::Shape::line(pts, Stroke::new(w, ink)));
        }
        CellState::Renderizado => {
            p.circle_stroke(c, r - w * 0.5, Stroke::new(w, ink));
            let s = Stroke::new(w, ink);
            p.line_segment([c + vec2(-r * 0.45, 0.0), c + vec2(-r * 0.1, r * 0.38)], s);
            p.line_segment([c + vec2(-r * 0.1, r * 0.38), c + vec2(r * 0.5, -r * 0.4)], s);
        }
        CellState::Desactualizado => {
            let s = Stroke::new(w, ink);
            p.circle_stroke(c, r - w * 0.5, s);
            p.line_segment([c, c + vec2(0.0, -r * 0.6)], s);
            p.line_segment([c, c + vec2(r * 0.45, r * 0.2)], s);
        }
        CellState::SinRender => {
            p.circle_stroke(c, r - w * 0.5, Stroke::new(w * 0.8, ink));
        }
        CellState::EnCola => {
            for k in [-1.0, 0.0, 1.0] {
                p.circle_filled(c + vec2(k * r * 0.65, 0.0), size * 0.1, ink);
            }
        }
        CellState::EnProceso(f) => {
            p.circle_stroke(c, r - w * 0.5, Stroke::new(w, theme::GRID_LINE));
            let n = 24;
            let end = (f.clamp(0.02, 1.0) * n as f32).ceil() as usize;
            let pts: Vec<_> = (0..=end)
                .map(|i| {
                    let a = -std::f32::consts::FRAC_PI_2 + i as f32 / n as f32 * std::f32::consts::TAU;
                    c + vec2(a.cos(), a.sin()) * (r - w * 0.5)
                })
                .collect();
            p.add(egui::Shape::line(pts, Stroke::new(w * 1.4, theme::ACCENT)));
        }
        CellState::Error | CellState::FuenteFalta => {
            let pts: Vec<_> = (0..8)
                .map(|i| {
                    let a = std::f32::consts::PI / 8.0 + i as f32 * std::f32::consts::PI / 4.0;
                    c + vec2(a.cos(), a.sin()) * r
                })
                .collect();
            p.add(egui::Shape::convex_polygon(pts, ink, Stroke::NONE));
            let bg = if ink == theme::HIGHLIGHT_TEXT { theme::HIGHLIGHT } else { theme::FIELD };
            let k = r * 0.38;
            let s = Stroke::new(w, bg);
            p.line_segment([c + vec2(-k, -k), c + vec2(k, k)], s);
            p.line_segment([c + vec2(-k, k), c + vec2(k, -k)], s);
        }
    }
}

/// dBFS con un decimal; "-inf" para silencio.
pub fn fmt_db(db: f64) -> String {
    if db.is_finite() {
        format!("{db:.1}")
    } else {
        t!("ui.unidad.menos_inf").to_string()
    }
}

/// Tamaño de archivo legible (KB, MB).
pub fn fmt_bytes(b: u64) -> String {
    if b >= 1 << 20 {
        format!("{:.1} {}", b as f64 / (1u64 << 20) as f64, t!("ui.unidad.mb"))
    } else {
        format!("{:.1} {}", b as f64 / 1024.0, t!("ui.unidad.kb"))
    }
}
