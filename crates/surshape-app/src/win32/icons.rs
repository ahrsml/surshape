//! Íconos de 16x16 dibujados con figuras simples (no dependen de glifos de
//! la fuente ni de archivos de imagen).

use crate::theme;
use eframe::egui::{pos2, vec2, Color32, Painter, Pos2, Rect, Shape, Stroke};

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    New,
    Open,
    Save,
    Import,
    Undo,
    Render,
    Cascade,
    Play,
    Pause,
    Stop,
    Home,
    End,
    Loop,
    Console,
    Prefs,
    Up,
    Down,
    Left,
    Right,
    Close,
    Check,
    Wave,
}

fn poly(p: &Painter, pts: Vec<Pos2>, c: Color32) {
    p.add(Shape::convex_polygon(pts, c, Stroke::NONE));
}

fn line(p: &Painter, a: Pos2, b: Pos2, c: Color32, w: f32) {
    p.line_segment([a, b], Stroke::new(w, c));
}

/// Dibuja `icon` centrado en `r` (pensado para 16x16; escala con el alto).
pub fn paint(p: &Painter, r: Rect, icon: Icon, enabled: bool) {
    let s = r.height().min(r.width()) / 16.0;
    let o = r.center() - vec2(8.0, 8.0) * s;
    let at = |x: f32, y: f32| o + vec2(x, y) * s;
    let ink = if enabled { theme::TEXT } else { theme::TEXT_DISABLED };
    let accent = if enabled { theme::pal::ACERO_1 } else { theme::TEXT_DISABLED };
    let paper = theme::FIELD;
    let w = 1.0 * s.max(1.0);
    match icon {
        Icon::New => {
            p.rect_filled(Rect::from_min_max(at(3.0, 1.0), at(13.0, 15.0)), 0.0, paper);
            p.rect_stroke(Rect::from_min_max(at(3.0, 1.0), at(13.0, 15.0)), 0.0, Stroke::new(w, ink), eframe::egui::StrokeKind::Inside);
            poly(p, vec![at(9.0, 1.0), at(13.0, 5.0), at(9.0, 5.0)], accent);
        }
        Icon::Open => {
            poly(p, vec![at(1.0, 4.0), at(6.0, 4.0), at(7.0, 6.0), at(14.0, 6.0), at(14.0, 14.0), at(1.0, 14.0)], theme::pal::CIELO_3);
            p.rect_stroke(Rect::from_min_max(at(1.0, 6.0), at(14.0, 14.0)), 0.0, Stroke::new(w, ink), eframe::egui::StrokeKind::Inside);
            poly(p, vec![at(4.0, 9.0), at(15.0, 9.0), at(13.0, 14.0), at(2.0, 14.0)], accent);
        }
        Icon::Save => {
            p.rect_filled(Rect::from_min_max(at(2.0, 2.0), at(14.0, 14.0)), 0.0, accent);
            p.rect_filled(Rect::from_min_max(at(4.0, 2.0), at(12.0, 7.0)), 0.0, paper);
            p.rect_filled(Rect::from_min_max(at(5.0, 10.0), at(11.0, 14.0)), 0.0, ink);
        }
        Icon::Import => {
            p.rect_stroke(Rect::from_min_max(at(2.0, 8.0), at(14.0, 14.0)), 0.0, Stroke::new(w, ink), eframe::egui::StrokeKind::Inside);
            line(p, at(8.0, 1.0), at(8.0, 10.0), accent, 2.0 * s);
            poly(p, vec![at(4.0, 7.0), at(12.0, 7.0), at(8.0, 11.0)], accent);
        }
        Icon::Undo => {
            let pts: Vec<Pos2> = (0..=10)
                .map(|i| {
                    let a = std::f32::consts::PI * (1.1 - i as f32 * 0.12);
                    at(8.0 + 5.0 * a.cos(), 9.0 - 5.0 * a.sin())
                })
                .collect();
            p.add(Shape::line(pts, Stroke::new(2.0 * s, ink)));
            poly(p, vec![at(0.5, 7.0), at(6.0, 7.0), at(3.0, 12.0)], ink);
        }
        Icon::Render => {
            // Engranaje simplificado: rueda con flecha.
            p.circle_filled(at(8.0, 8.0), 6.0 * s, theme::ACCENT);
            p.circle_stroke(at(8.0, 8.0), 6.0 * s, Stroke::new(w, ink));
            poly(p, vec![at(6.0, 4.5), at(12.0, 8.0), at(6.0, 11.5)], ink);
        }
        Icon::Cascade => {
            for (i, x) in [1.0, 6.0, 11.0].iter().enumerate() {
                let y = 2.0 + i as f32 * 4.0;
                p.rect_filled(Rect::from_min_max(at(*x - 1.0, y), at(*x + 4.0, y + 4.0)), 0.0, theme::ACCENT);
                p.rect_stroke(Rect::from_min_max(at(*x - 1.0, y), at(*x + 4.0, y + 4.0)), 0.0, Stroke::new(w, ink), eframe::egui::StrokeKind::Inside);
            }
        }
        Icon::Play => {
            poly(p, vec![at(4.0, 2.0), at(13.0, 8.0), at(4.0, 14.0)], ink);
        }
        Icon::Pause => {
            p.rect_filled(Rect::from_min_max(at(4.0, 3.0), at(7.0, 13.0)), 0.0, ink);
            p.rect_filled(Rect::from_min_max(at(9.0, 3.0), at(12.0, 13.0)), 0.0, ink);
        }
        Icon::Stop => {
            p.rect_filled(Rect::from_min_max(at(4.0, 4.0), at(12.0, 12.0)), 0.0, ink);
        }
        Icon::Home => {
            p.rect_filled(Rect::from_min_max(at(3.0, 3.0), at(5.0, 13.0)), 0.0, ink);
            poly(p, vec![at(13.0, 3.0), at(13.0, 13.0), at(5.0, 8.0)], ink);
        }
        Icon::End => {
            p.rect_filled(Rect::from_min_max(at(11.0, 3.0), at(13.0, 13.0)), 0.0, ink);
            poly(p, vec![at(3.0, 3.0), at(3.0, 13.0), at(11.0, 8.0)], ink);
        }
        Icon::Loop => {
            let pts: Vec<Pos2> = (0..=16)
                .map(|i| {
                    let a = i as f32 / 16.0 * std::f32::consts::TAU * 0.85;
                    at(8.0 + 5.0 * a.cos(), 8.0 + 4.0 * a.sin())
                })
                .collect();
            p.add(Shape::line(pts, Stroke::new(1.5 * s, ink)));
            poly(p, vec![at(13.0, 4.0), at(13.0, 9.0), at(9.5, 6.5)], ink);
        }
        Icon::Console => {
            p.rect_filled(Rect::from_min_max(at(1.0, 2.0), at(15.0, 14.0)), 0.0, theme::VIEW_BG);
            line(p, at(3.0, 5.0), at(6.0, 8.0), theme::VIEW_TEXT, 1.5 * s);
            line(p, at(6.0, 8.0), at(3.0, 11.0), theme::VIEW_TEXT, 1.5 * s);
            line(p, at(8.0, 11.0), at(12.0, 11.0), theme::VIEW_TEXT, 1.5 * s);
        }
        Icon::Prefs => {
            for (i, y) in [4.0f32, 8.0, 12.0].iter().enumerate() {
                line(p, at(2.0, *y), at(14.0, *y), ink, w);
                let x = [5.0, 10.0, 7.0][i];
                p.rect_filled(Rect::from_min_max(at(x - 1.5, *y - 2.0), at(x + 1.5, *y + 2.0)), 0.0, accent);
            }
        }
        Icon::Up => {
            poly(p, vec![at(4.0, 10.0), at(12.0, 10.0), at(8.0, 6.0)], ink);
        }
        Icon::Down => {
            poly(p, vec![at(4.0, 6.0), at(12.0, 6.0), at(8.0, 10.0)], ink);
        }
        Icon::Left => {
            poly(p, vec![at(10.0, 4.0), at(10.0, 12.0), at(6.0, 8.0)], ink);
        }
        Icon::Right => {
            poly(p, vec![at(6.0, 4.0), at(6.0, 12.0), at(10.0, 8.0)], ink);
        }
        Icon::Close => {
            line(p, at(4.0, 4.0), at(12.0, 12.0), ink, 2.0 * s);
            line(p, at(12.0, 4.0), at(4.0, 12.0), ink, 2.0 * s);
        }
        Icon::Check => {
            line(p, at(3.0, 8.0), at(6.5, 11.5), ink, 2.0 * s);
            line(p, at(6.5, 11.5), at(13.0, 4.5), ink, 2.0 * s);
        }
        Icon::Wave => {
            let pts: Vec<Pos2> = (0..=16)
                .map(|i| {
                    let x = i as f32;
                    pos2(o.x + x * s, o.y + (8.0 - (x * 0.9).sin() * 5.0 * (0.3 + x / 20.0)) * s)
                })
                .collect();
            p.add(Shape::line(pts, Stroke::new(1.5 * s, accent)));
        }
    }
}
