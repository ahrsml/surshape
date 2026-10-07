//! Consola: cada ejecución (programas de CDP y procesos nativos) con su
//! hora, celda, programa, código de salida, tiempo y archivos; al elegir una,
//! su comando completo y su salida (Courier, seleccionable).

use crate::app::{App, DialogResult, Msg, Page};
use crate::theme::{self, mono_font};
use crate::win32::{self, Level};
use eframe::egui::{self, Ui};
use surshape_engine::console;
use surshape_i18n::{t, tr};

/// Hora local aproximada "HH:MM:SS" (UTC + la diferencia del sistema no
/// está disponible sin dependencias: se muestra UTC).
fn hms(secs: u64) -> String {
    let s = secs % 86_400;
    format!("{:02}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
}

impl App {
    pub(crate) fn console_page(&mut self, ui: &mut Ui) {
        let entries = console::entries();
        let mut copy = false;
        let mut clear = false;
        let mut save = false;
        let mut back = false;
        win32::title_bar(ui, tr("ui.consola.titulo"), |ui| {
            if win32::title_button(ui, win32::Icon::Close, tr("ui.pagina.volver")).clicked() {
                back = true;
            }
            if win32::title_text_button(ui, tr("ui.consola.guardar")).clicked() {
                save = true;
            }
            if win32::title_text_button(ui, tr("ui.consola.limpiar")).clicked() {
                clear = true;
            }
            if win32::title_text_button(ui, tr("ui.consola.copiar")).clicked() {
                copy = true;
            }
        });
        let all_text = || -> String {
            entries
                .iter()
                .map(|e| format!("#{} {} {} [{}] {} ({:.2} s)\n$ {}\n{}\n", e.n, hms(e.when), e.cell, e.code.map_or("-".into(), |c| c.to_string()), e.files, e.secs, e.command, e.output)) // i18n-ok
                .collect::<Vec<_>>()
                .join("\n")
        };
        if copy {
            ui.ctx().copy_text(all_text());
            self.status = Some(Msg::new(Level::Info, "ui.estado.copiado"));
        }
        if clear {
            console::clear();
            self.console_sel = None;
        }
        if save {
            let txt = all_text();
            self.open_dialog(move |d| d.add_filter("txt", &["txt"]).set_file_name("consola.txt").save_file().map(|p| DialogResult::SaveText(p, txt))); // i18n-ok
        }
        if back {
            self.page = Page::Main;
            return;
        }
        // La última ejecución queda elegida si no se eligió otra.
        let sel = self.console_sel.filter(|n| entries.iter().any(|e| e.n == *n)).or_else(|| entries.last().map(|e| e.n));
        let h = ui.available_height();
        win32::group(ui, tr("ui.consola.ejecuciones"), |ui| {
            if entries.is_empty() {
                win32::muted(ui, tr("ui.consola.vacia"));
                return;
            }
            egui::ScrollArea::both().id_salt("consola_tabla").max_height((h * 0.45).max(90.0)).auto_shrink([false, true]).stick_to_bottom(true).show(ui, |ui| { // i18n-ok
                egui::Grid::new("consola_grid").num_columns(7).spacing([10.0, 1.0]).show(ui, |ui| { // i18n-ok
                    for k in ["ui.consola.col_n", "ui.consola.col_hora", "ui.consola.col_celda", "ui.consola.col_programa", "ui.consola.col_codigo", "ui.consola.col_tiempo", "ui.consola.col_archivos"] {
                        win32::head(ui, tr(k));
                    }
                    ui.end_row();
                    for e in &entries {
                        let selected = sel == Some(e.n);
                        let r = win32::list_row(ui, selected, &format!("{}", e.n), "", true);
                        if r.clicked() {
                            self.console_sel = Some(e.n);
                        }
                        let c = if selected { theme::TEXT } else { theme::TEXT_MUTED };
                        let f = mono_font(theme::MONO_SIZE);
                        ui.label(egui::RichText::new(hms(e.when)).font(f.clone()).color(c));
                        ui.label(egui::RichText::new(&e.cell).font(f.clone()));
                        ui.label(egui::RichText::new(&e.program).font(f.clone()));
                        let code = match (e.code, e.ok) {
                            (Some(c), true) => t!("ui.consola.ok", codigo = c),
                            (Some(c), false) => t!("ui.consola.fallo", codigo = c),
                            (None, true) => t!("ui.consola.nativo").to_string(),
                            (None, false) => t!("ui.consola.no_lanzado").to_string(),
                        };
                        ui.label(egui::RichText::new(code).color(if e.ok { theme::TEXT } else { theme::ERROR }));
                        ui.label(egui::RichText::new(format!("{:.2} {}", e.secs, tr("ui.unidad.s"))).font(f.clone()));
                        ui.label(egui::RichText::new(&e.files).font(f));
                        ui.end_row();
                    }
                });
            });
        });
        if let Some(e) = sel.and_then(|n| entries.iter().find(|e| e.n == n)) {
            win32::group(ui, &t!("ui.consola.salida", n = e.n), |ui| {
                egui::ScrollArea::both().id_salt("consola_salida").auto_shrink([false, false]).show(ui, |ui| { // i18n-ok
                    win32::sunken(ui, theme::FIELD, |ui| {
                        ui.set_width(ui.available_width());
                        let txt = if e.output.is_empty() { format!("$ {}", e.command) } else { format!("$ {}\n{}", e.command, e.output) };
                        win32::mono_selectable(ui, &txt);
                    });
                });
            });
        }
    }
}
