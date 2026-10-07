//! Página principal: visor de onda a todo el ancho (con lecturas en
//! Courier), barra de transporte, columna de botones rápidos a la derecha y,
//! debajo, la grilla tipo planilla. El alto del visor se cambia arrastrando
//! el divisor y se recuerda.

use crate::app::{App, ViewKind, ViewMode};
use crate::shell::Action;
use crate::theme::{self, mono_font};
use crate::viewer::{self, ViewOpts};
use crate::widgets::{fmt_db, CellState};
use crate::win32::{self, Icon, Spin};
use eframe::egui::{self, pos2, vec2, Align2, Rect, Sense, Ui};
use surshape_i18n::{t, tr};

impl App {
    pub(crate) fn main_page(&mut self, ui: &mut Ui) {
        let face = egui::Frame::NONE.fill(theme::FACE);
        let top = egui::TopBottomPanel::top("zona_visor") // i18n-ok
            .resizable(true)
            .default_height(self.prefs.alto_visor)
            .height_range(190.0..=720.0)
            .frame(face)
            .show_inside(ui, |ui| {
                egui::SidePanel::right("rapidos") // i18n-ok
                    .resizable(false)
                    .exact_width(86.0)
                    .frame(face.inner_margin(egui::Margin { left: 4, right: 0, top: 0, bottom: 0 }))
                    .show_inside(ui, |ui| self.quick_column(ui));
                egui::CentralPanel::default().frame(face).show_inside(ui, |ui| self.viewer_zone(ui));
            });
        let h = top.response.rect.height();
        if (h - self.prefs.alto_visor).abs() > 1.0 && !ui.ctx().input(|i| i.pointer.any_down()) {
            self.prefs.alto_visor = h;
            self.save_prefs();
        }
        egui::CentralPanel::default().frame(face.inner_margin(egui::Margin { left: 0, right: 0, top: 4, bottom: 0 })).show_inside(ui, |ui| {
            self.grid_zone(ui);
        });
    }

    /// Columna derecha de botones rápidos.
    fn quick_column(&mut self, ui: &mut Ui) {
        let sel = self.selected.is_some();
        let w = ui.available_width();
        let mut act = None;
        for (key, help, a, en) in [
            ("ui.rapido.info", "ui.ayuda.info_celda", Action::CellInfo, sel),
            ("ui.rapido.nivel", "ui.ayuda.rapido_nivel", Action::AddProcess("nat.normalize"), sel), // i18n-ok
            ("ui.rapido.editar", "ui.ayuda.editar_externo", Action::EditExternal, sel),
            ("ui.rapido.cortar", "ui.ayuda.rapido_cortar", Action::AddProcess("nat.extraer"), sel && self.view.selection().is_some()), // i18n-ok
            ("ui.rapido.reset", "ui.ayuda.rapido_reset", Action::ZoomAll, true),
        ] {
            let b = win32::button_w(ui, tr(key), w, en);
            self.hint_key(&b, help);
            if b.clicked() {
                act = Some(a);
            }
        }
        ui.add_space(4.0);
        win32::group(ui, tr("ui.procesar.titulo"), |ui| {
            let r = win32::radio(ui, !self.procesar_sel, tr("ui.procesar.todo"));
            self.hint_key(&r, "ui.ayuda.procesar");
            if r.clicked() {
                self.procesar_sel = false;
            }
            let r = win32::radio(ui, self.procesar_sel, tr("ui.procesar.seleccion"));
            self.hint_key(&r, "ui.ayuda.procesar");
            if r.clicked() {
                self.procesar_sel = true;
            }
        });
        if let Some(a) = act {
            if a == Action::ZoomAll {
                self.reset_view();
            } else {
                self.do_action(ui.ctx(), a);
            }
        }
    }

    /// Visor: lecturas, onda y transporte.
    fn viewer_zone(&mut self, ui: &mut Ui) {
        let id = self.selected.filter(|id| self.session.as_ref().is_some_and(|s| s.patch.node(*id).is_some()));
        let target = id.and_then(|_| self.view_target());
        if let Some((k, path)) = &target {
            self.ensure_loaded(k, path);
        }
        let data = target.as_ref().and_then(|(k, _)| self.mem.get(k).cloned());
        let sr = data.as_ref().map_or(48000, |d| d.audio.sr);
        let unit = self.prefs.unidades;
        // Transporte: alto fijo abajo.
        let transport_h = 3.0 * (win32::ROW_H + 6.0) + 4.0;
        let full = ui.available_rect_before_wrap();
        let wave_area = Rect::from_min_max(full.min, pos2(full.right(), (full.bottom() - transport_h).max(full.top() + 60.0)));
        let readout = Rect::from_min_size(wave_area.min, vec2(wave_area.width(), 16.0));
        let wave = Rect::from_min_max(pos2(wave_area.left(), readout.bottom()), wave_area.max);

        // Lecturas (Courier, sobre el fondo del visor).
        let p = ui.painter();
        p.rect_filled(readout, 0.0, theme::VIEW_BG);
        if let Some(d) = &data {
            let frames = d.audio.frames();
            let view_start = self.view.start.max(0.0) / sr as f64;
            let view_end = (self.view.start + self.view.span).min(frames as f64) / sr as f64;
            let pos = if self.player.is_playing_buf(&d.audio) { self.player.position() } else { self.view.cursor };
            let f = mono_font(12.0);
            let rp = p.with_clip_rect(readout);
            rp.text(readout.left_center() + vec2(4.0, 0.0), Align2::LEFT_CENTER, unit.fmt(view_start, sr), f.clone(), theme::VIEW_TEXT);
            rp.text(readout.center(), Align2::CENTER_CENTER, t!("ui.visor.cursor_en", t = unit.fmt(pos as f64 / sr as f64, sr)), f.clone(), theme::VIEW_CURSOR);
            rp.text(readout.right_center() - vec2(4.0, 0.0), Align2::RIGHT_CENTER, unit.fmt(view_end, sr), f, theme::VIEW_TEXT);
        }
        ui.allocate_rect(wave_area, Sense::hover());
        match &data {
            Some(d) => {
                let key = target.as_ref().map(|(k, _)| k.clone()).unwrap_or_default();
                if self.view_kind == ViewKind::Espectrograma {
                    self.ensure_spectro(&key);
                }
                let playhead = self.player.is_playing_buf(&d.audio).then(|| self.player.position());
                let spectro = match self.view_kind {
                    ViewKind::Espectrograma => self.spectros.get(&key).cloned(),
                    ViewKind::Onda => None,
                };
                let loop_region = id
                    .and_then(|i| self.session.as_ref().and_then(|s| s.patch.node(i)))
                    .and_then(|n| n.bucle)
                    .map(|r| (r.inicio as usize, r.fin as usize));
                let markers = self.current_markers();
                let opts = ViewOpts { vzoom: &mut self.vzoom, hidden: self.hidden_ch, loop_region, unit, markers: &markers };
                let r = viewer::show(ui, wave, &mut self.view, d, playhead, spectro.as_deref(), opts);
                self.hint_key(&r, "ui.ayuda.onda");
                if self.view_kind == ViewKind::Espectrograma && self.spectro_pending(&key) {
                    ui.painter().text(wave.center(), Align2::CENTER_CENTER, tr("ui.visor.calculando_espectro"), theme::ui_font(), theme::VIEW_TEXT);
                }
            }
            None => {
                let p = ui.painter();
                p.rect_filled(wave, 0.0, theme::VIEW_BG);
                win32::bevel(p, wave, win32::Bevel::Sunken);
                let msg = match (id, &target) {
                    (None, _) => t!("ui.visor.vacio").to_string(),
                    (Some(i), None) => {
                        let is_proc = self.session.as_ref().and_then(|s| s.patch.node(i)).is_some_and(|n| n.source().is_none());
                        if is_proc && self.view_mode == ViewMode::Resultado { t!("ui.visor.sin_render").to_string() } else { t!("ui.visor.sin_audio").to_string() }
                    }
                    (Some(_), Some((_, path))) => t!("ui.visor.cargando", archivo = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()),
                };
                p.text(wave.center(), Align2::CENTER_CENTER, msg, theme::ui_font(), theme::VIEW_TEXT);
            }
        }
        // --- Transporte ---
        ui.add_space(3.0);
        self.transport(ui, id, data.as_ref(), sr);
    }

    fn transport(&mut self, ui: &mut Ui, id: Option<surshape_patch::NodeId>, data: Option<&crate::Loaded>, sr: u32) {
        let unit = self.prefs.unidades;
        let is_process = id.and_then(|i| self.session.as_ref().and_then(|s| s.patch.node(i))).is_some_and(|n| n.source().is_none());
        // Fila 1: archivo, A/B, duración, canales, picos.
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            let file = id.map(|i| self.cell_file(i, self.view_salida)).unwrap_or_default();
            let path = self.view_target().map(|(_, p)| p.display().to_string()).unwrap_or_default();
            let r = win32::readonly_field(ui, &file, 140.0);
            self.hint(&r, path);
            if is_process {
                for (mode, key, help) in [
                    (ViewMode::Entrada, "ui.visor.modo_a", "ui.ayuda.modo_entrada"),
                    (ViewMode::Resultado, "ui.visor.modo_b", "ui.ayuda.modo_resultado"),
                ] {
                    let r = win32::radio(ui, self.view_mode == mode, tr(key));
                    self.hint_key(&r, help);
                    if r.clicked() && self.view_mode != mode {
                        self.ab_toggle();
                    }
                }
            }
            if let Some(d) = data {
                let a = &d.audio;
                ui.add_space(8.0);
                win32::label(ui, tr("ui.visor.dur"));
                win32::readonly_field(ui, &unit.fmt(a.duration_secs(), a.sr), 96.0);
                win32::label(ui, tr("ui.visor.can"));
                win32::readonly_field(ui, &a.num_channels().to_string(), 26.0);
                win32::label(ui, tr("ui.visor.picos"));
                let peaks: Vec<String> = d.stats.iter().enumerate().map(|(i, s)| format!("{}{} {}", tr("ui.visor.c"), i + 1, fmt_db(s.peak_db()))).collect();
                let r = win32::readonly_field(ui, &peaks.join("  "), 150.0);
                self.hint_key(&r, "ui.ayuda.picos");
                if is_process && id.is_some_and(|i| self.cell_state(i) == CellState::Desactualizado) && self.view_mode == ViewMode::Resultado {
                    win32::notice(ui, win32::Level::Warning, tr("ui.visor.desactualizado_corto"));
                }
            }
        });
        // Fila 2: transporte, bucle, onda/espectro, zoom, canales.
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 1.0;
            let playing = data.is_some_and(|d| self.player.is_playing_buf(&d.audio));
            let has = data.is_some();
            let mut act: Option<u8> = None;
            for (k, icon, tip, en) in [
                (0u8, Icon::Play, "ui.visor.reproducir", has),
                (1, Icon::Pause, "ui.visor.pausa", playing),
                (2, Icon::Stop, "ui.visor.detener", playing || self.player.is_paused()),
                (3, Icon::Home, "ui.visor.inicio", has),
                (4, Icon::End, "ui.visor.fin", has),
            ] {
                let r = win32::tool_button(ui, icon, tr(tip), en, k == 0 && playing);
                self.hint_key(&r, tip);
                if r.clicked() {
                    act = Some(k);
                }
            }
            ui.add_space(4.0);
            let r = win32::checkbox(ui, &mut self.looping, tr("ui.visor.bucle"));
            self.hint_key(&r, "ui.ayuda.bucle");
            ui.add_space(6.0);
            for (kind, key, help) in [(ViewKind::Onda, "ui.visor.onda", "ui.ayuda.onda_modo"), (ViewKind::Espectrograma, "ui.visor.espectrograma", "ui.ayuda.espectrograma")] {
                let r = win32::radio(ui, self.view_kind == kind, tr(key));
                self.hint_key(&r, help);
                if r.clicked() {
                    self.view_kind = kind;
                }
            }
            ui.add_space(6.0);
            for (icon, a, tip) in [(Icon::Up, Action::ZoomIn, "ui.visor.acercar"), (Icon::Down, Action::ZoomOut, "ui.visor.alejar")] {
                let r = win32::tool_button(ui, icon, tr(tip), has, false);
                self.hint_key(&r, "ui.ayuda.zoom");
                if r.clicked() {
                    self.do_action(ui.ctx(), a);
                }
            }
            win32::label(ui, tr("ui.visor.zoom_v"));
            let mut vz = self.vzoom as f64;
            let r = win32::spin(ui, &mut vz, Spin { min: 1.0, max: 64.0, step: 1.0, decimals: 1, suffix: "", width: 56.0 });
            self.hint_key(&r, "ui.ayuda.zoom_vertical");
            if r.changed() {
                self.vzoom = vz as f32;
            }
            if let Some(d) = data {
                let n = d.audio.num_channels();
                if n > 1 {
                    ui.add_space(4.0);
                    for c in 0..n.min(8) {
                        let mut on = self.hidden_ch & (1 << c) == 0;
                        let r = win32::checkbox(ui, &mut on, &t!("ui.visor.canal_corto", n = c + 1));
                        self.hint_key(&r, "ui.ayuda.canales_visibles");
                        if r.changed() {
                            self.hidden_ch ^= 1 << c;
                            // Siempre queda al menos un canal visible.
                            if (0..n).all(|k| self.hidden_ch & (1 << k) != 0) {
                                self.hidden_ch &= !(1 << c);
                            }
                        }
                    }
                }
            }
            match act {
                Some(0) => self.toggle_play(),
                Some(1) => self.player.pause(),
                Some(2) => self.player.stop(),
                Some(3) => {
                    self.view.cursor = 0;
                    self.view.sel = None;
                    self.view.start = 0.0;
                }
                Some(4) => {
                    if let Some(d) = data {
                        self.view.cursor = d.audio.frames();
                        self.view.sel = None;
                    }
                }
                _ => {}
            }
        });
        // Fila 3: inicio / fin de la selección (para procesar) y bucle.
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            let frames = data.map_or(0, |d| d.audio.frames());
            let (mut a, mut b) = self.view.selection().map_or((self.view.cursor as f64, self.view.cursor as f64), |(a, b)| (a as f64, b as f64));
            let srf = sr.max(1) as f64;
            let (mut sa, mut sb) = (a / srf, b / srf);
            let max = frames as f64 / srf;
            let (step, dec, suf) = match unit {
                crate::prefs::TimeUnit::Muestras => (1.0 / srf, 6, tr("ui.unidad.s")),
                _ => (0.01, 6, tr("ui.unidad.s")),
            };
            win32::label(ui, tr("ui.procesar.inicio"));
            let r1 = win32::spin(ui, &mut sa, Spin { min: 0.0, max, step, decimals: dec, suffix: suf, width: 112.0 });
            self.hint_key(&r1, "ui.ayuda.seleccion");
            win32::label(ui, tr("ui.procesar.fin"));
            let r2 = win32::spin(ui, &mut sb, Spin { min: 0.0, max, step, decimals: dec, suffix: suf, width: 112.0 });
            self.hint_key(&r2, "ui.ayuda.seleccion");
            if (r1.changed() || r2.changed()) && data.is_some() {
                a = (sa * srf).round();
                b = (sb * srf).round();
                let (x, y) = (a.min(b) as usize, a.max(b) as usize);
                self.view.sel = if y > x { Some((x.min(frames), y.min(frames))) } else { None };
                self.view.cursor = x.min(frames);
            }
            if let Some((x, y)) = self.view.selection() {
                win32::muted(ui, &t!("ui.visor.largo_sel", largo = unit.fmt((y - x) as f64 / srf, sr)));
            }
            ui.add_space(6.0);
            let sel = self.selected.is_some();
            // Marcadores: anterior, cantidad, siguiente.
            let nm = self.current_markers().len();
            ui.add_space(6.0);
            win32::label(ui, tr("ui.visor.marc"));
            for (icon, a, tip) in [(Icon::Left, Action::MarkPrev, "ui.menu.marc_anterior"), (Icon::Right, Action::MarkNext, "ui.menu.marc_siguiente")] {
                let r = win32::tool_button(ui, icon, tr(tip), nm > 0, false);
                self.hint_key(&r, "ui.ayuda.marc_navegar");
                if r.clicked() {
                    self.do_action(ui.ctx(), a);
                }
                if tip == "ui.menu.marc_anterior" {
                    win32::readonly_field(ui, &nm.to_string(), 30.0);
                }
            }
            ui.add_space(6.0);
            for (key, a, help) in [
                ("ui.menu.bucle_get_corto", Action::LoopGet, "ui.ayuda.bucle_get"),
                ("ui.menu.bucle_set_corto", Action::LoopSet, "ui.ayuda.bucle_set"),
            ] {
                let r = win32::button_w(ui, tr(key), 40.0, sel);
                self.hint_key(&r, help);
                if r.clicked() {
                    self.do_action(ui.ctx(), a);
                }
            }
        });
    }

    /// Reproduce o detiene lo que muestra el visor (selección, bucle de la
    /// celda o desde el cursor).
    pub(crate) fn toggle_play(&mut self) {
        if self.player.is_playing() {
            self.player.stop();
            return;
        }
        if self.player.is_paused() {
            self.player.resume();
            return;
        }
        let Some(l) = self.view_loaded().map(|l| l.audio.clone()) else { return };
        let bucle = self.selected.and_then(|i| self.session.as_ref().and_then(|s| s.patch.node(i))).and_then(|n| n.bucle);
        let (a, b) = match (self.view.selection(), bucle) {
            (Some(s), _) => s,
            (None, Some(r)) if self.looping => (r.inicio as usize, r.fin as usize),
            _ => (self.view.cursor, l.frames()),
        };
        let a = if a >= l.frames() { 0 } else { a };
        self.player.play(l, a, b, self.looping);
        if let Some((key, detail)) = self.player.error.clone() {
            self.status = Some(crate::app::Msg::new(win32::Level::Error, key).arg("detalle", detail));
        }
    }

    /// A/B: alterna el visor (y lo que suena) entre la entrada (A) y el
    /// resultado (B) de la celda, manteniendo el punto de reproducción.
    pub(crate) fn ab_toggle(&mut self) {
        let was_playing = self.player.is_playing();
        let pos_secs = self.view_loaded().map(|l| self.player.position() as f64 / l.audio.sr.max(1) as f64);
        self.view_mode = match self.view_mode {
            ViewMode::Entrada => ViewMode::Resultado,
            ViewMode::Resultado => ViewMode::Entrada,
        };
        self.player.stop();
        let Some((k, path)) = self.view_target() else { return };
        self.ensure_loaded(&k, &path);
        if let (true, Some(secs), Some(l)) = (was_playing, pos_secs, self.mem.get(&k)) {
            let a = l.audio.clone();
            let from = ((secs * a.sr as f64) as usize).min(a.frames());
            let n = a.frames();
            self.player.play(a, from, n, self.looping);
        }
    }

    /// Reset del visor: zoom completo, escala vertical 1, todos los canales.
    pub(crate) fn reset_view(&mut self) {
        let frames = self.view_loaded().map_or(0, |l| l.audio.frames());
        self.view.fit(frames);
        self.view.sel = None;
        self.vzoom = 1.0;
        self.hidden_ch = 0;
    }
}
