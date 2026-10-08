//! Diálogos modales clásicos (barra de título con degradado, Aceptar /
//! Cancelar / Aplicar): Preferencias con pestañas, Acerca de, Atajos, Info
//! de celda, texto de Info de CDP, confirmaciones y nombre de receta.

use crate::app::{App, Confirm, Dialog, DialogResult, Msg};
use crate::prefs::{ExportFormat, Prefs, TimeUnit};
use crate::theme::{self, mono_font};
use crate::widgets::{fmt_bytes, fmt_db};
use crate::win32::{self, Icon, Level, Spin};
use eframe::egui::{self, vec2, Id, Ui};
use surshape_i18n::{t, tr, Lang};

/// Botón elegido al pie de un diálogo.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Btn {
    Aceptar,
    Cancelar,
    Aplicar,
}

/// Marco de un diálogo: barra de título (con [X]), contenido y botones.
/// Devuelve el botón elegido (la [X] y Esc cuentan como Cancelar).
fn dialog(ctx: &egui::Context, id: &str, title: &str, width: f32, buttons: &[Btn], add: impl FnOnce(&mut Ui)) -> Option<Btn> {
    let mut out = None;
    let frame = egui::Frame::NONE.fill(theme::t().FACE).inner_margin(egui::Margin::same(3));
    let resp = egui::Modal::new(Id::new(id)).frame(frame).backdrop_color(theme::modal_backdrop()).show(ctx, |ui| {
        let max_w = (ctx.screen_rect().width() - 40.0).max(200.0);
        ui.set_width(width.min(max_w));
        win32::title_bar(ui, title, |ui| {
            if win32::title_button(ui, Icon::Close, tr("ui.dialogo.cancelar")).clicked() {
                out = Some(Btn::Cancelar);
            }
        });
        ui.add_space(4.0);
        let max_h = (ctx.screen_rect().height() - 120.0).max(150.0);
        egui::ScrollArea::vertical().id_salt((id, "contenido")).max_height(max_h).auto_shrink([false, true]).show(ui, |ui| { // i18n-ok
            ui.set_width(ui.available_width());
            add(ui);
        });
        ui.add_space(6.0);
        if !buttons.is_empty() {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                for b in buttons.iter().rev() {
                    let key = match b {
                        Btn::Aceptar => "ui.dialogo.aceptar",
                        Btn::Cancelar => "ui.dialogo.cancelar",
                        Btn::Aplicar => "ui.dialogo.aplicar",
                    };
                    if win32::button(ui, tr(key)).clicked() {
                        out = Some(*b);
                    }
                }
            });
        }
    });
    win32::bevel(&ctx.layer_painter(resp.response.layer_id), resp.response.rect, win32::Bevel::Raised);
    if resp.should_close() && out.is_none() {
        out = Some(Btn::Cancelar);
    }
    out
}

impl App {
    pub(crate) fn modal_dialogs(&mut self, ctx: &egui::Context) {
        let Some(d) = self.modal.clone() else { return };
        match d {
            Dialog::Prefs => self.prefs_dialog(ctx),
            Dialog::About => {
                if dialog(ctx, "acerca", tr("ui.acerca.titulo"), 640.0, &[Btn::Aceptar], |ui| self.about_body(ui)).is_some() { // i18n-ok
                    self.modal = None;
                }
            }
            Dialog::Shortcuts => {
                if dialog(ctx, "atajos", tr("ui.atajos.titulo"), 560.0, &[Btn::Aceptar], |ui| { // i18n-ok
                    egui::Grid::new("atajos").num_columns(2).spacing([18.0, 3.0]).show(ui, |ui| { // i18n-ok
                        for (keys, desc) in crate::shortcuts::ALL {
                            ui.label(egui::RichText::new(tr(keys)).font(theme::mono_bold_font(theme::MONO_SIZE)));
                            win32::label(ui, tr(desc));
                            ui.end_row();
                        }
                    });
                    ui.add_space(4.0);
                    win32::muted(ui, tr("ui.atajos.nota"));
                })
                .is_some()
                {
                    self.modal = None;
                }
            }
            Dialog::CellInfo(id) => {
                let label = self.session.as_ref().and_then(|s| s.patch.cell_label(id)).unwrap_or_default();
                if dialog(ctx, "info_celda", &t!("ui.info.titulo", celda = label), 520.0, &[Btn::Aceptar], |ui| self.cell_info_body(ui, id)).is_some() { // i18n-ok
                    self.modal = None;
                }
            }
            Dialog::Text(title, body) => {
                let mut copy = false;
                let r = dialog(ctx, "texto", &title, 600.0, &[Btn::Aceptar], |ui| { // i18n-ok
                    if win32::button_w(ui, tr("ui.pagina.copiar"), 70.0, true).clicked() {
                        copy = true;
                    }
                    win32::sunken(ui, theme::t().FIELD, |ui| {
                        ui.set_width(ui.available_width());
                        win32::mono_selectable(ui, &body);
                    });
                });
                if copy {
                    ctx.copy_text(body.clone());
                }
                if r.is_some() {
                    self.modal = None;
                }
            }
            Dialog::Confirm(c) => {
                let (title, text) = match c {
                    Confirm::ClearFrom(id) => (tr("ui.confirmar.limpiar_titulo"), t!("ui.confirmar.limpiar", n = self.cells_from(id).len())),
                    Confirm::DeleteWithDependents(id) => {
                        let n = self.session.as_ref().map_or(0, |s| s.patch.dependents(id).len());
                        (tr("ui.confirmar.borrar_titulo"), t!("ui.confirmar.borrar", n = n))
                    }
                };
                match dialog(ctx, "confirmar", title, 420.0, &[Btn::Aceptar, Btn::Cancelar], |ui| { // i18n-ok
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(24.0, 24.0), egui::Sense::hover());
                        win32::paint_level_icon(ui.painter(), r.center(), 20.0, Level::Warning);
                        win32::wrap(ui, &text);
                    });
                    win32::muted(ui, tr("ui.confirmar.deshacer"));
                }) {
                    Some(Btn::Aceptar) => {
                        self.modal = None;
                        match c {
                            Confirm::ClearFrom(id) => {
                                let cells = self.cells_from(id);
                                self.remove_with_dependents(&cells);
                            }
                            Confirm::DeleteWithDependents(id) => self.remove_with_dependents(&[id]),
                        }
                    }
                    Some(_) => self.modal = None,
                    None => {}
                }
            }
            Dialog::RecipeName(row) => {
                let mut name = std::mem::take(&mut self.recipe_name);
                let r = dialog(ctx, "receta", tr("ui.recetas.guardar_titulo"), 380.0, &[Btn::Aceptar, Btn::Cancelar], |ui| { // i18n-ok
                    win32::wrap(ui, tr("ui.recetas.guardar_desc"));
                    win32::text_field(ui, &mut name, ui.available_width(), tr("ui.recetas.nombre"));
                });
                self.recipe_name = name;
                match r {
                    Some(Btn::Aceptar) if !self.recipe_name.trim().is_empty() => {
                        self.modal = None;
                        let recipe = self.session.as_ref().and_then(|s| s.patch.recipe_from_row(row, self.recipe_name.trim()));
                        if let (Some(r), Some(base)) = (recipe, crate::prefs::recipes_dir()) {
                            match surshape_session::recipes::save(&base, &r) {
                                Ok(n) => self.status = Some(Msg::new(Level::Info, "ui.recetas.guardada").arg("nombre", n)),
                                Err(e) => self.status = Some(Msg::new(Level::Error, "err.recetas.guardar").arg("detalle", e)),
                            }
                        }
                    }
                    Some(Btn::Cancelar) => self.modal = None,
                    _ => {}
                }
            }
        }
    }

    // --- Preferencias -----------------------------------------------------------------------

    fn prefs_dialog(&mut self, ctx: &egui::Context) {
        let mut draft = self.prefs_draft.take().unwrap_or_else(|| self.prefs.clone());
        let mut tab = self.prefs_tab;
        let r = dialog(ctx, "preferencias", tr("ui.prefs.titulo"), 640.0, &[Btn::Aceptar, Btn::Cancelar, Btn::Aplicar], |ui| { // i18n-ok
            let tabs = [tr("ui.prefs.tab_rutas"), tr("ui.prefs.tab_pvoc"), tr("ui.prefs.tab_audio"), tr("ui.prefs.tab_unidades"), tr("ui.prefs.tab_comportamiento"), tr("ui.prefs.tab_accesibilidad")];
            win32::tabs(ui, &mut tab, &tabs);
            win32::tab_panel(ui, |ui| match tab {
                0 => self.prefs_paths(ui, &mut draft),
                1 => prefs_pvoc(ui, &mut draft),
                2 => self.prefs_audio(ui, &mut draft),
                3 => prefs_units(ui, &mut draft),
                4 => prefs_behaviour(ui, &mut draft),
                _ => prefs_access(ui, &mut draft),
            });
        });
        self.prefs_tab = tab;
        match r {
            Some(Btn::Aceptar) => {
                self.apply_prefs(draft);
                self.modal = None;
                ctx.send_viewport_cmd(egui::ViewportCommand::Title(t!("ui.app.titulo").into()));
            }
            Some(Btn::Aplicar) => {
                self.apply_prefs(draft.clone());
                self.prefs_draft = Some(draft);
                ctx.send_viewport_cmd(egui::ViewportCommand::Title(t!("ui.app.titulo").into()));
            }
            Some(Btn::Cancelar) => self.modal = None,
            None => self.prefs_draft = Some(draft),
        }
    }

    fn prefs_paths(&mut self, ui: &mut Ui, p: &mut Prefs) {
        win32::group(ui, tr("ui.prefs.cdp"), |ui| {
            let shown = self.cdp.as_ref().map(|c| c.dir.display().to_string()).unwrap_or_default();
            win32::readonly_field(ui, &shown, ui.available_width());
            ui.horizontal_wrapped(|ui| {
                let b = win32::button_w(ui, tr("ui.prefs.examinar"), 80.0, true);
                self.hint_key(&b, "ui.ayuda.cdp_carpeta");
                if b.clicked() {
                    self.open_dialog(|d| d.pick_folder().map(DialogResult::CdpDir));
                }
                let b = win32::button_w(ui, tr("ui.prefs.cdp_restablecer"), 80.0, p.carpeta_cdp.is_some());
                self.hint_key(&b, "ui.ayuda.cdp_restablecer");
                if b.clicked() {
                    p.carpeta_cdp = None;
                }
            });
            match &self.cdp {
                Some(c) => {
                    let v = c.version.clone().unwrap_or_else(|| "?".into()); // i18n-ok
                    let missing = c.missing_core();
                    let faltan = if missing.is_empty() { tr("ui.prefs.ninguno").to_string() } else { missing.join(", ") };
                    win32::notice(ui, Level::Info, &t!("ui.prefs.cdp_estado", version = v, n = c.programs.len(), faltan = faltan));
                }
                None => {
                    win32::notice(ui, Level::Warning, tr("ui.prefs.cdp_no_encontrado"));
                }
            }
        });
        win32::group(ui, tr("ui.prefs.carpetas"), |ui| {
            let tmp = p.carpeta_temporal.clone().unwrap_or_else(std::env::temp_dir).display().to_string();
            let ses = p.sessions_dir().display().to_string();
            let ed = p.editor_externo.as_ref().map(|e| e.display().to_string()).unwrap_or_else(|| t!("ui.prefs.ninguno").to_string());
            for (k, (label, value, help)) in [
                ("ui.prefs.temporal", tmp, "ui.ayuda.carpeta_temporal"),
                ("ui.prefs.carpeta_sesiones", ses, "ui.ayuda.carpeta_sesiones"),
                ("ui.prefs.editor", ed, "ui.ayuda.editor"),
            ]
            .into_iter()
            .enumerate()
            {
                win32::label(ui, tr(label));
                ui.horizontal(|ui| {
                    let w = (ui.available_width() - 86.0).max(60.0);
                    win32::readonly_field(ui, &value, w);
                    let b = win32::button_w(ui, tr("ui.prefs.examinar"), 80.0, true);
                    self.hint_key(&b, help);
                    if b.clicked() {
                        match k {
                            0 => self.open_dialog(|d| d.pick_folder().map(DialogResult::TempDir)),
                            1 => self.open_dialog(|d| d.pick_folder().map(DialogResult::SessionsDir)),
                            _ => {
                                let filter = t!("ui.dialogo.filtro_exe").to_string();
                                self.open_dialog(move |d| d.add_filter(filter, &["exe"]).pick_file().map(DialogResult::Editor)); // i18n-ok
                            }
                        }
                    }
                });
            }
            ui.horizontal_wrapped(|ui| {
                for (key, dir) in [("ui.prefs.carpeta_presets", crate::prefs::presets_dir()), ("ui.prefs.carpeta_recetas", crate::prefs::recipes_dir()), ("ui.prefs.carpeta_patches", crate::prefs::patches_dir())] {
                    let b = win32::button_w(ui, tr(key), 80.0, true);
                    self.hint_key(&b, "ui.ayuda.carpetas");
                    if b.clicked() {
                        self.open_folder(dir);
                    }
                }
            });
        });
        if !self.overrides.is_empty() {
            win32::group(ui, tr("ui.prefs.traducciones"), |ui| {
                for f in &self.overrides {
                    ui.label(egui::RichText::new(f.display().to_string()).font(mono_font(theme::MONO_SIZE)));
                }
            });
        }
    }

    fn prefs_audio(&mut self, ui: &mut Ui, p: &mut Prefs) {
        win32::group(ui, tr("ui.prefs.dispositivo"), |ui| {
            let devices = crate::player::output_devices();
            let shown = p.dispositivo.clone().unwrap_or_else(|| t!("ui.prefs.dispositivo_sistema").to_string());
            let mut chosen: Option<Option<String>> = None;
            let r = win32::combo(ui, "dispositivo", &shown, ui.available_width(), |ui| { // i18n-ok
                if win32::combo_item(ui, p.dispositivo.is_none(), tr("ui.prefs.dispositivo_sistema")).clicked() {
                    chosen = Some(None);
                }
                for d in &devices {
                    if win32::combo_item(ui, p.dispositivo.as_deref() == Some(d.as_str()), d).clicked() {
                        chosen = Some(Some(d.clone()));
                    }
                }
            });
            self.hint_key(&r, "ui.ayuda.dispositivo");
            if let Some(c) = chosen {
                p.dispositivo = c;
            }
            if let Some((key, detail)) = &self.player.error {
                win32::notice(ui, Level::Warning, &Msg::new(Level::Warning, key).arg("detalle", detail).text());
            }
        });
    }

    // --- Acerca de -----------------------------------------------------------------------------

    fn about_body(&mut self, ui: &mut Ui) {
        ui.label(egui::RichText::new(t!("ui.app.titulo")).font(theme::bold_font(18.0)));
        win32::label(ui, t!("creditos.version", version = env!("CARGO_PKG_VERSION"))); // i18n-ok
        win32::label(ui, tr("creditos.autor"));
        win32::muted(ui, &t!("ui.acerca.fuente_ui", fuente = theme::system_font_name()));
        win32::group(ui, tr("creditos.licencia_titulo"), |ui| {
            win32::wrap(ui, tr("creditos.licencia"));
            win32::wrap(ui, tr("creditos.fuente"));
        });
        win32::group(ui, tr("creditos.cdp_titulo"), |ui| {
            win32::wrap(ui, tr("creditos.cdp"));
            match &self.cdp {
                Some(c) => {
                    let v = c.version.clone().unwrap_or_else(|| "?".into()); // i18n-ok
                    win32::wrap(ui, egui::RichText::new(t!("creditos.cdp_en_uso", version = v, carpeta = c.dir.display())).font(mono_font(theme::MONO_SIZE)));
                }
                None => {
                    win32::muted(ui, tr("creditos.cdp_no_en_uso"));
                }
            }
        });
        win32::group(ui, tr("creditos.terceros_titulo"), |ui| {
            win32::wrap(ui, tr("creditos.terceros"));
            egui::Grid::new("creditos_grid").num_columns(3).spacing([12.0, 3.0]).show(ui, |ui| { // i18n-ok
                win32::head(ui, tr("creditos.col_componente"));
                win32::head(ui, tr("creditos.col_licencia"));
                win32::head(ui, tr("creditos.col_uso"));
                ui.end_row();
                for c in &self.credits {
                    match c.url {
                        Some(url) => {
                            ui.hyperlink_to(c.name, url).on_hover_text(url);
                        }
                        None => {
                            ui.label(c.name);
                        }
                    }
                    ui.label(egui::RichText::new(c.license).font(mono_font(theme::MONO_SIZE)));
                    win32::wrap(ui, tr(c.desc_key));
                    ui.end_row();
                }
            });
        });
    }

    // --- Info de celda -------------------------------------------------------------------------

    fn cell_info_body(&mut self, ui: &mut Ui, id: surshape_patch::NodeId) {
        let Some((k, path)) = self.port_target(id, self.view_salida) else {
            win32::muted(ui, tr("ui.visor.sin_render"));
            return;
        };
        self.ensure_loaded(&k, &path);
        let Some(d) = self.mem.get(&k).cloned() else {
            win32::muted(ui, &t!("ui.visor.cargando", archivo = self.cell_file(id, self.view_salida)));
            return;
        };
        let a = &d.audio;
        let unit = self.prefs.unidades;
        let kind = self.node_kind(id);
        let mut size: u64 = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        if let Some(rec) = self.session.as_ref().and_then(|s| s.patch.node(id)).and_then(|n| n.render.as_ref()) {
            if let (Some(o), Some(s)) = (rec.salidas.get(self.view_salida as usize), self.session.as_ref()) {
                size += o.datos.iter().filter_map(|d| std::fs::metadata(s.abs(d)).ok()).map(|m| m.len()).sum::<u64>();
            }
        }
        egui::Grid::new("info_celda").num_columns(2).spacing([16.0, 3.0]).show(ui, |ui| { // i18n-ok
            let rows = [
                (tr("ui.info.archivo"), self.cell_file(id, self.view_salida)),
                (tr("ui.info.tipo"), format!("{} ({})", kind.ext(), tr(kind.key()))),
                (tr("ui.info.sr"), format!("{} {}", a.sr, tr("ui.unidad.hz"))),
                (tr("ui.info.canales"), a.num_channels().to_string()),
                (tr("ui.info.duracion"), format!("{} {}", TimeUnit::Segundos.fmt(a.duration_secs(), a.sr), tr("ui.unidad.s"))),
                (tr("ui.info.muestras"), a.frames().to_string()),
                (tr("ui.info.hms"), unit.fmt(a.duration_secs(), a.sr)),
                (tr("ui.info.tamano"), fmt_bytes(size)),
            ];
            for (k, v) in rows {
                win32::head(ui, k);
                ui.label(egui::RichText::new(v).font(mono_font(theme::MONO_SIZE)));
                ui.end_row();
            }
        });
        win32::group(ui, tr("ui.info.por_canal"), |ui| {
            egui::Grid::new("info_canales").num_columns(4).spacing([16.0, 3.0]).show(ui, |ui| { // i18n-ok
                for k in ["ui.info.col_canal", "ui.info.col_pico", "ui.info.col_rms", "ui.info.col_dc"] {
                    win32::head(ui, tr(k));
                }
                ui.end_row();
                for (i, s) in d.stats.iter().enumerate() {
                    let f = mono_font(theme::MONO_SIZE);
                    ui.label(egui::RichText::new(format!("{}", i + 1)).font(f.clone()));
                    ui.label(egui::RichText::new(format!("{} {}", fmt_db(s.peak_db()), tr("ui.unidad.dbfs"))).font(f.clone()));
                    ui.label(egui::RichText::new(format!("{} {}", fmt_db(s.rms_db()), tr("ui.unidad.dbfs"))).font(f.clone()));
                    ui.label(egui::RichText::new(format!("{:+.6}", s.dc)).font(f));
                    ui.end_row();
                }
            });
        });
        win32::readonly_field(ui, &path.display().to_string(), ui.available_width());
    }
}

fn prefs_pvoc(ui: &mut Ui, p: &mut Prefs) {
    win32::group(ui, tr("ui.prefs.pvoc"), |ui| {
        win32::wrap(ui, tr("ui.prefs.pvoc_desc"));
        egui::Grid::new("prefs_pvoc").num_columns(2).spacing([10.0, 4.0]).show(ui, |ui| { // i18n-ok
            win32::label(ui, tr("ui.prefs.pvoc_puntos"));
            let mut v = p.pvoc_puntos as f64;
            if win32::spin(ui, &mut v, Spin { min: 64.0, max: 8192.0, step: 64.0, decimals: 0, suffix: "", width: 100.0 }).changed() {
                p.pvoc_puntos = v as u32;
            }
            ui.end_row();
            win32::label(ui, tr("ui.prefs.pvoc_superposicion"));
            let mut v = p.pvoc_superposicion as f64;
            if win32::spin(ui, &mut v, Spin { min: 1.0, max: 4.0, step: 1.0, decimals: 0, suffix: "", width: 100.0 }).changed() {
                p.pvoc_superposicion = v as u32;
            }
            ui.end_row();
        });
    });
}

fn prefs_units(ui: &mut Ui, p: &mut Prefs) {
    win32::group(ui, tr("ui.prefs.unidades"), |ui| {
        for u in TimeUnit::ALL {
            if win32::radio(ui, p.unidades == u, &format!("{}   ({})", tr(u.key()), u.fmt(83.456, 48000))).clicked() {
                p.unidades = u;
            }
        }
    });
    win32::group(ui, tr("ui.prefs.idioma"), |ui| {
        for l in Lang::ALL {
            if win32::radio(ui, p.lang() == l, tr(l.name_key())).clicked() {
                p.idioma = l.code().to_string();
            }
        }
    });
}

fn prefs_behaviour(ui: &mut Ui, p: &mut Prefs) {
    win32::group(ui, tr("ui.prefs.render"), |ui| {
        win32::checkbox(ui, &mut p.volver_tras_render, tr("ui.prefs.volver_tras_render"));
    });
    win32::group(ui, tr("ui.prefs.limitador"), |ui| {
        ui.horizontal(|ui| {
            win32::checkbox(ui, &mut p.limitador, tr("ui.prefs.limitador_activo"));
            win32::label(ui, tr("ui.prefs.techo"));
            win32::spin(ui, &mut p.techo_db, Spin { min: -12.0, max: 0.0, step: 0.1, decimals: 1, suffix: tr("ui.unidad.dbfs"), width: 90.0 });
        });
    });
    win32::group(ui, tr("ui.prefs.export"), |ui| {
        for f in ExportFormat::ALL {
            if win32::radio(ui, p.formato_export == f, tr(f.key())).clicked() {
                p.formato_export = f;
            }
        }
    });
}

fn prefs_access(ui: &mut Ui, p: &mut Prefs) {
    win32::group(ui, tr("ui.prefs.tema"), |ui| {
        for k in crate::theme::ThemeKind::ALL {
            if win32::radio(ui, p.tema == k, tr(k.key())).clicked() {
                p.tema = k;
            }
        }
    });
    win32::group(ui, tr("ui.prefs.zoom"), |ui| {
        for z in [1.0f32, 1.25, 1.5] {
            if win32::radio(ui, (p.zoom - z).abs() < 0.01, &format!("{:.0} {}", z * 100.0, tr("ui.unidad.porcentaje"))).clicked() {
                p.zoom = z;
            }
        }
        win32::muted(ui, tr("ui.prefs.zoom_nota"));
    });
}
