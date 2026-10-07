//! Atajos de teclado: lista única (la usan el diálogo "Atajos", el manual y
//! el manejo de teclas). Ninguno actúa mientras se escribe en un campo. Las
//! letras de acceso de los menús (Alt+letra) están en `shell.rs`.

use crate::app::{App, Page};
use crate::shell::Action;
use eframe::egui::{self, Key, KeyboardShortcut, Modifiers};

/// (clave i18n de las teclas, clave i18n de la descripción).
pub(crate) const ALL: &[(&str, &str)] = &[
    ("ui.atajo.espacio", "ui.atajo.espacio.desc"),
    ("ui.atajo.b", "ui.atajo.b.desc"),
    ("ui.atajo.l", "ui.atajo.l.desc"),
    ("ui.atajo.flechas", "ui.atajo.flechas.desc"),
    ("ui.atajo.intro", "ui.atajo.intro.desc"),
    ("ui.atajo.inicio", "ui.atajo.inicio.desc"),
    ("ui.atajo.zoom", "ui.atajo.zoom.desc"),
    ("ui.atajo.cero", "ui.atajo.cero.desc"),
    ("ui.atajo.ctrl_r", "ui.atajo.ctrl_r.desc"),
    ("ui.atajo.ctrl_alt_r", "ui.atajo.ctrl_alt_r.desc"),
    ("ui.atajo.ctrl_mayus_r", "ui.atajo.ctrl_mayus_r.desc"),
    ("ui.atajo.ctrl_z", "ui.atajo.ctrl_z.desc"),
    ("ui.atajo.ctrl_c", "ui.atajo.ctrl_c.desc"),
    ("ui.atajo.ctrl_v", "ui.atajo.ctrl_v.desc"),
    ("ui.atajo.ctrl_b", "ui.atajo.ctrl_b.desc"),
    ("ui.atajo.supr", "ui.atajo.supr.desc"),
    ("ui.atajo.ctrl_i", "ui.atajo.ctrl_i.desc"),
    ("ui.atajo.ctrl_e", "ui.atajo.ctrl_e.desc"),
    ("ui.atajo.ctrl_o", "ui.atajo.ctrl_o.desc"),
    ("ui.atajo.ctrl_s", "ui.atajo.ctrl_s.desc"),
    ("ui.atajo.ctrl_k", "ui.atajo.ctrl_k.desc"),
    ("ui.atajo.m", "ui.atajo.m.desc"),
    ("ui.atajo.coma_punto", "ui.atajo.coma_punto.desc"),
    ("ui.atajo.alt", "ui.atajo.alt.desc"),
    ("ui.atajo.esc", "ui.atajo.esc.desc"),
    ("ui.atajo.f1", "ui.atajo.f1.desc"),
];

impl App {
    pub(crate) fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let sc = |m: Modifiers, k: Key| ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(m, k)));
        // Esc funciona siempre: cierra el menú, cancela la elección o vuelve
        // a la página principal.
        if self.open_menu.is_none() && self.modal.is_none() && sc(Modifiers::NONE, Key::Escape) {
            if self.pick.is_some() {
                self.pick = None;
                if let Some(p) = self.pick_return.take() {
                    self.page = p;
                }
            } else if self.replace.is_some() {
                self.replace = None;
            } else if self.page != Page::Main {
                self.page = Page::Main;
            }
        }
        if ctx.wants_keyboard_input() || self.modal.is_some() || self.open_menu.is_some() {
            return;
        }
        let sel = self.selected;
        let is_proc = sel.and_then(|id| self.session.as_ref().and_then(|s| s.patch.node(id))).is_some_and(|n| n.source().is_none());
        let cmd = Modifiers::COMMAND;
        let mut act = None;
        if sc(cmd | Modifiers::SHIFT, Key::R) {
            act = Some(Action::RunAll);
        } else if sc(cmd | Modifiers::ALT, Key::R) {
            act = Some(Action::RunFrom);
        } else if sc(cmd, Key::R) {
            act = Some(Action::RunCell);
        }
        for (k, a) in [
            (Key::Z, Action::Undo),
            (Key::C, Action::Copy),
            (Key::V, Action::Paste),
            (Key::B, Action::Branch),
            (Key::I, Action::Import),
            (Key::E, Action::Export),
            (Key::O, Action::LoadPatch),
            (Key::S, Action::SavePatch),
            (Key::K, Action::Console),
        ] {
            if sc(cmd, k) {
                act = Some(a);
            }
        }
        if sc(Modifiers::NONE, Key::F1) {
            act = Some(Action::Shortcuts);
        }
        if let Some(a) = act {
            self.do_action(ctx, a);
            return;
        }
        // Lo que sigue es de la página principal.
        if self.page != Page::Main {
            return;
        }
        if sc(Modifiers::NONE, Key::Delete) && sel.is_some() {
            self.do_action(ctx, Action::Delete);
        }
        if sc(Modifiers::NONE, Key::Enter) && sel.is_some() {
            self.do_action(ctx, Action::ReEdit);
        }
        if sc(Modifiers::NONE, Key::Space) {
            self.toggle_play();
        }
        if is_proc && sc(Modifiers::NONE, Key::B) {
            self.ab_toggle();
        }
        if sel.is_some() && sc(Modifiers::NONE, Key::M) {
            self.marker_add(false);
        }
        if sc(Modifiers::NONE, Key::Comma) {
            self.marker_step(false);
        }
        if sc(Modifiers::NONE, Key::Period) {
            self.marker_step(true);
        }
        if sc(Modifiers::NONE, Key::L) {
            self.looping = !self.looping;
        }
        for (k, dr, dc) in [(Key::ArrowLeft, 0, -1), (Key::ArrowRight, 0, 1), (Key::ArrowUp, -1, 0), (Key::ArrowDown, 1, 0)] {
            if sc(Modifiers::NONE, k) {
                self.navigate(dr, dc);
            }
        }
        // Visor
        let frames = self.view_loaded().map(|l| l.audio.frames());
        if let Some(frames) = frames {
            let center = self.view.start + self.view.span * 0.5;
            if sc(Modifiers::NONE, Key::Home) {
                self.view.cursor = 0;
                self.view.sel = None;
            }
            if sc(Modifiers::NONE, Key::Plus) || sc(Modifiers::NONE, Key::Equals) {
                self.view.zoom(0.5, center, frames);
            }
            if sc(Modifiers::NONE, Key::Minus) {
                self.view.zoom(2.0, center, frames);
            }
            if sc(Modifiers::NONE, Key::Num0) {
                self.reset_view();
            }
        }
    }
}
