//! Barra lateral izquierda: secciones plegables, cada una con su color de
//! encabezado (Archivos, Procesos, Patches, Guardar archivo) y, debajo, los
//! controles de patch/celda (Cargar, Guardar, Ejecutar, Limpiar, Re-editar,
//! Reemplazar, Copiar, Borrar) con sus casillas. Si no cabe, se desplaza.

use crate::app::App;
use crate::shell::{menu_of, Action, MENUS};
use crate::theme::{self, bold_font};
use crate::win32::{self, Bevel, Icon};
use eframe::egui::{self, pos2, vec2, Align2, Rect, Sense, Ui};
use std::sync::Arc;
use surshape_engine::process::{desc_key, name_key};
use surshape_engine::Process;
use surshape_i18n::{t, tr};
use surshape_patch::NodeKind;

const SECTIONS: [&str; 4] = ["ui.lateral.archivos", "ui.lateral.procesos", "ui.lateral.patches", "ui.lateral.guardar"];

impl App {
    pub(crate) fn sidebar(&mut self, ui: &mut Ui) {
        egui::ScrollArea::vertical().id_salt("lateral_scroll").auto_shrink([false, false]).show(ui, |ui| { // i18n-ok
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 3.0;
            for i in 0..SECTIONS.len() {
                if self.section_header(ui, i) {
                    ui.indent(("seccion", i), |ui| match i { // i18n-ok
                        0 => self.side_files(ui),
                        1 => self.side_processes(ui),
                        2 => self.side_patches(ui),
                        _ => self.side_save(ui),
                    });
                }
            }
            ui.add_space(4.0);
            self.side_controls(ui);
        });
    }

    /// Encabezado de color de una sección con su botón de plegar. Devuelve
    /// si está abierta.
    fn section_header(&mut self, ui: &mut Ui, i: usize) -> bool {
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 18.0), Sense::click());
        win32::audit::record(ui, SECTIONS[i], rect);
        let open = self.prefs.secciones[i];
        let p = ui.painter();
        p.rect_filled(rect, 0.0, theme::SIDE_HEADS[i]);
        win32::bevel(p, rect, Bevel::ThinRaised);
        p.rect_filled(Rect::from_min_size(rect.min + vec2(2.0, 2.0), vec2(4.0, rect.height() - 4.0)), 0.0, theme::DARK);
        let max_w = rect.width() - 34.0;
        let txt = win32::elide(ui, &tr(SECTIONS[i]).to_uppercase(), &bold_font(theme::UI_SIZE), max_w);
        p.text(pos2(rect.left() + 10.0, rect.center().y), Align2::LEFT_CENTER, txt, bold_font(theme::UI_SIZE), theme::TEXT);
        let b = Rect::from_min_size(pos2(rect.right() - 18.0, rect.top() + 2.0), vec2(16.0, 14.0));
        let rb = win32::icon_button(ui, b, ui.id().with(("plegar", i)), if open { Icon::Up } else { Icon::Down }, true); // i18n-ok
        self.hint_key(&resp, "ui.ayuda.seccion");
        if resp.clicked() || rb.clicked() {
            self.prefs.secciones[i] = !open;
            self.save_prefs();
        }
        self.prefs.secciones[i]
    }

    /// Archivos: fuentes de la sesión y recientes (arrastrar aquí).
    fn side_files(&mut self, ui: &mut Ui) {
        let sources: Vec<(surshape_patch::NodeId, String, String)> = self
            .session
            .as_ref()
            .map(|s| {
                s.patch
                    .nodos
                    .iter()
                    .filter_map(|n| match &n.tipo {
                        NodeKind::Fuente(src) => Some((
                            n.id,
                            s.patch.cell_label(n.id).unwrap_or_default(),
                            src.ruta.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default(),
                        )),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        win32::muted(ui, tr("ui.lateral.fuentes"));
        if sources.is_empty() {
            win32::muted(ui, tr("ui.lateral.arrastrar"));
        }
        for (id, label, name) in sources {
            let r = win32::list_row(ui, self.selected == Some(id), &format!("{label}  {name}"), "", true);
            self.hint_key(&r, "ui.ayuda.lateral_fuente");
            if r.clicked() {
                self.select(id);
                self.page = crate::app::Page::Main;
            }
        }
        let recent = self.prefs.recientes.clone();
        if !recent.is_empty() {
            win32::muted(ui, tr("ui.menu.recientes"));
            for p in recent.iter().take(8) {
                let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let r = win32::list_row(ui, false, &name, "", p.is_file());
                self.hint(&r, t!("ui.ayuda.reciente", archivo = p.display()));
                if r.double_clicked() {
                    self.import_as(p.clone(), crate::app::ImportTarget::NewRow);
                }
            }
        }
        let b = win32::button_w(ui, tr("ui.menu.importar"), ui.available_width().min(120.0), true);
        self.hint_key(&b, "ui.ayuda.importar");
        if b.clicked() {
            self.do_action(ui.ctx(), Action::Import);
        }
    }

    /// Procesos: catálogo con búsqueda, agrupado por menú.
    fn side_processes(&mut self, ui: &mut Ui) {
        let r = win32::text_field(ui, &mut self.side_search, ui.available_width(), tr("ui.lateral.buscar"));
        self.hint_key(&r, "ui.ayuda.buscar");
        let q = self.side_search.trim().to_lowercase();
        let all: Vec<Arc<dyn Process>> = self.registry.all().to_vec();
        let mut act = None;
        for (mi, mkey) in MENUS.iter().enumerate() {
            let procs: Vec<&Arc<dyn Process>> = all
                .iter()
                .filter(|p| menu_of(p.as_ref()) == mi)
                .filter(|p| q.is_empty() || tr(&name_key(p.id())).to_lowercase().contains(&q) || p.id().contains(&q))
                .collect();
            if procs.is_empty() {
                continue;
            }
            win32::menu_caption(ui, &crate::shell::mnemonic(tr(mkey)).0);
            for p in procs {
                let two = p.inputs().min() >= 2;
                let name = if two { t!("ui.lateral.dos_sonidos", nombre = tr(&name_key(p.id()))) } else { tr(&name_key(p.id())).to_string() };
                let r = win32::list_row(ui, false, &name, p.output_kind().ext(), true);
                self.hint(&r, format!("{} {}", tr(&desc_key(p.id())), tr("ui.ayuda.lateral_proceso")));
                if r.double_clicked() {
                    act = Some(Action::AddProcess(p.id()));
                }
            }
        }
        if let Some(a) = act {
            self.do_action(ui.ctx(), a);
        }
    }

    /// Patches: recetas y patches de texto guardados.
    fn side_patches(&mut self, ui: &mut Ui) {
        let mut act = None;
        win32::muted(ui, tr("ui.lateral.recetas"));
        let list = crate::prefs::recipes_dir().map(|b| surshape_session::recipes::list(&b)).unwrap_or_default();
        if list.is_empty() {
            win32::muted(ui, tr("ui.recetas.ninguna"));
        }
        for name in list {
            let r = win32::list_row(ui, false, &name, "", true);
            self.hint_key(&r, "ui.ayuda.recetas_aplicar");
            if r.double_clicked() {
                act = Some(Action::ApplyRecipe(name.clone()));
            }
        }
        win32::muted(ui, tr("ui.lateral.patches_texto"));
        let patches = crate::ops::list_patches();
        if patches.is_empty() {
            win32::muted(ui, tr("ui.lateral.ninguno"));
        }
        for p in patches {
            let name = p.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let r = win32::list_row(ui, false, &name, "", true);
            self.hint_key(&r, "ui.ayuda.patch_cargar");
            if r.double_clicked() {
                self.load_patch_file(&p);
            }
        }
        if let Some(a) = act {
            self.do_action(ui.ctx(), a);
        }
    }

    /// Guardar archivo: la celda elegida como archivo permanente.
    fn side_save(&mut self, ui: &mut Ui) {
        let sel = self.selected.is_some();
        win32::label(ui, tr("ui.lateral.nombre"));
        let w = ui.available_width();
        let r = win32::text_field(ui, &mut self.save_name, w, tr("ui.lateral.nombre"));
        self.hint_key(&r, "ui.ayuda.guardar_archivo");
        ui.horizontal(|ui| {
            let cur = self.prefs.formato_export;
            let mut chosen = cur;
            let r = win32::combo(ui, "formato_guardar", tr(cur.key()), (w - 82.0).max(60.0), |ui| { // i18n-ok
                for f in crate::prefs::ExportFormat::ALL {
                    if win32::combo_item(ui, f == cur, tr(f.key())).clicked() {
                        chosen = f;
                    }
                }
            });
            self.hint_key(&r, "ui.ayuda.formato");
            if chosen != cur {
                self.prefs.formato_export = chosen;
                self.save_prefs();
            }
            let b = win32::button_w(ui, tr("ui.lateral.guardar_btn"), 74.0, sel);
            self.hint_key(&b, "ui.ayuda.guardar_archivo");
            if b.clicked() {
                self.do_action(ui.ctx(), Action::Export);
            }
        });
    }

    /// Controles de patch/celda (botones y casillas, en dos columnas).
    fn side_controls(&mut self, ui: &mut Ui) {
        let sel = self.selected.is_some();
        let mut act = None;
        win32::group(ui, tr("ui.lateral.controles"), |ui| {
            let half = ((ui.available_width() - 6.0) / 2.0).max(60.0);
            ui.columns(2, |cols| {
                let buttons: [(&str, &str, Action, bool); 8] = [
                    ("ui.control.cargar", "ui.ayuda.patch_cargar", Action::LoadPatch, true),
                    ("ui.control.guardar", "ui.ayuda.patch_guardar", Action::SavePatch, true),
                    ("ui.control.ejecutar", "ui.ayuda.ejecutar", Action::RunCell, sel || self.cell_opts.patch_completo),
                    ("ui.control.limpiar", "ui.ayuda.limpiar", Action::Clear, sel),
                    ("ui.control.reeditar", "ui.ayuda.reeditar", Action::ReEdit, sel),
                    ("ui.control.reemplazar", "ui.ayuda.reemplazar", Action::Replace, sel),
                    ("ui.control.copiar", "ui.ayuda.copiar", Action::Copy, sel),
                    ("ui.control.borrar", "ui.ayuda.quitar", Action::Delete, sel),
                ];
                for (key, help, a, en) in buttons {
                    let w = cols[0].available_width().min(half);
                    let b = win32::button_w(&mut cols[0], tr(key), w, en);
                    self.hint_key(&b, help);
                    if b.clicked() {
                        act = Some(a);
                    }
                }
                let ui = &mut cols[1];
                let o = &mut self.cell_opts;
                let mut hints = Vec::new();
                hints.push((win32::checkbox(ui, &mut o.incluir_fuentes, tr("ui.control.incluir_fuentes")), "ui.ayuda.incluir_fuentes"));
                hints.push((win32::checkbox(ui, &mut o.fila_unica, tr("ui.control.fila_unica")), "ui.ayuda.fila_unica"));
                hints.push((win32::checkbox(ui, &mut o.patch_completo, tr("ui.control.patch_completo")), "ui.ayuda.patch_completo"));
                hints.push((win32::checkbox(ui, &mut o.copiar_fila, tr("ui.control.copiar_fila")), "ui.ayuda.copiar_fila"));
                hints.push((win32::checkbox(ui, &mut o.bulk, tr("ui.control.bulk")), "ui.ayuda.bulk"));
                for (r, k) in hints {
                    self.hint_key(&r, k);
                }
            });
        });
        if let Some(a) = act {
            self.do_action(ui.ctx(), a);
        }
    }
}
