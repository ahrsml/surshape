//! Operaciones: el despachador de acciones (menús, toolbar, atajos) y las
//! operaciones de celda con la semántica de Soundshaper (agregar, re-editar,
//! reemplazar, copiar, borrar, limpiar, deshacer por celda, ejecutar), los
//! patches de texto (cargar / guardar con o sin fuentes / exportar .bat),
//! el editor externo, los archivos de celda (`~A_3.wav`) y la línea de
//! comando de cada celda.

use crate::app::{App, Confirm, Dialog, DialogResult, ExtEdit, ImportTarget, Msg, Page};
use crate::shell::Action;
use crate::win32::Level;
use eframe::egui;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use surshape_engine::{CmdCtx, FileKind, ParamSet, Process, StableHasher, Task};
use surshape_i18n::t;
use surshape_patch::text::{fix_generators, from_text, to_text};
use surshape_patch::{NodeId, NodeKind, PortRef, Region};
use surshape_session::file_stamp;

/// Carpeta de los archivos de celda dentro de la sesión.
pub(crate) const CELLS_DIR: &str = "celdas"; // i18n-ok
/// Carpeta de las copias para el editor externo.
const EDITED_DIR: &str = "editados"; // i18n-ok
const PATCH_EXT: &str = "sspatch"; // i18n-ok

/// Patches de texto guardados en la carpeta de patches.
pub(crate) fn list_patches() -> Vec<PathBuf> {
    let Some(d) = crate::prefs::patches_dir() else { return Vec::new() };
    let mut v: Vec<PathBuf> = std::fs::read_dir(d)
        .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == PATCH_EXT)).collect())
        .unwrap_or_default();
    v.sort();
    v
}

impl App {
    pub(crate) fn do_action(&mut self, ctx: &egui::Context, a: Action) {
        use Action as A;
        let sel = self.selected;
        match a {
            A::NewSession => self.new_session(),
            A::OpenSession => {
                let base = self.prefs.sessions_dir();
                self.open_dialog(move |d| d.set_directory(base).pick_folder().map(DialogResult::OpenSession));
            }
            A::OpenSessionFolder => {
                let d = self.session.as_ref().map(|s| s.dir.clone());
                self.open_folder(d);
            }
            A::Import => {
                let filter = t!("ui.dialogo.filtro_audio").to_string();
                self.open_dialog(move |d| d.add_filter(filter, surshape_audio::decode::EXTENSIONS).pick_files().map(DialogResult::Import));
            }
            A::ImportRecent(p) => self.import_as(p, ImportTarget::NewRow),
            A::Export => self.dialog_export(),
            A::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            A::Undo => self.undo_last(),
            A::UndoCell => {
                if let Some(id) = sel {
                    self.undo_cell(id);
                }
            }
            A::Copy => {
                if let Some(id) = sel {
                    self.copy_cells_from(id);
                }
            }
            A::Paste => self.paste_into(self.cursor_cell.0),
            A::Delete => {
                if let Some(id) = sel {
                    self.delete_cell(id);
                }
            }
            A::ReEdit => {
                if let Some(id) = sel {
                    self.open_page(id);
                }
            }
            A::Replace => {
                if let Some(id) = sel {
                    self.replace = Some(id);
                    self.page = Page::Main;
                    self.status = Some(Msg::new(Level::Info, "ui.estado.elegir_reemplazo"));
                }
            }
            A::Branch => {
                if let Some(id) = sel {
                    self.branch_from(id);
                }
            }
            A::AddProcess(pid) => self.add_process(pid),
            A::AddMix => self.add_mix(),
            A::EditExternal => self.external_edit(),
            A::LoopGet => self.loop_get(),
            A::LoopSet => self.loop_set(),
            A::LoopStart | A::LoopEnd => self.loop_point(matches!(a, A::LoopStart)),
            A::ZoomIn | A::ZoomOut => {
                if let Some(frames) = self.view_loaded().map(|l| l.audio.frames()) {
                    let c = self.view.start + self.view.span * 0.5;
                    self.view.zoom(if matches!(a, A::ZoomIn) { 0.5 } else { 2.0 }, c, frames);
                }
            }
            A::ZoomAll => self.reset_view(),
            A::CellInfo => {
                if let Some(id) = sel {
                    self.modal = Some(Dialog::CellInfo(id));
                }
            }
            A::CdpInfo(mode) => self.cdp_info(mode),
            A::GraphEdit => {
                if self.bp_edit.is_some() {
                    self.page = Page::Graph;
                }
            }
            A::OpenCellsFolder => {
                let d = self.session.as_ref().map(|s| s.dir.join(CELLS_DIR));
                self.sync_cell_files();
                self.open_folder(d);
            }
            A::RunCell => {
                if self.cell_opts.patch_completo {
                    self.start_run(Vec::new());
                } else if self.cell_opts.fila_unica {
                    self.run_row();
                } else if let Some(id) = sel {
                    self.start_run(vec![id]);
                }
            }
            A::RunRow => self.run_row(),
            A::RunFrom => {
                if let Some(id) = sel {
                    self.render_cascade(id);
                }
            }
            A::RunAll => self.start_run(Vec::new()),
            A::LoadPatch => {
                let dir = crate::prefs::patches_dir();
                let filter = t!("ui.dialogo.filtro_patch").to_string();
                self.open_dialog(move |mut d| {
                    if let Some(dir) = dir {
                        d = d.set_directory(dir);
                    }
                    d.add_filter(filter, &[PATCH_EXT]).pick_file().map(DialogResult::LoadPatch)
                });
            }
            A::SavePatch => {
                let dir = crate::prefs::patches_dir();
                if let Some(d) = &dir {
                    let _ = std::fs::create_dir_all(d);
                }
                let name = self.session.as_ref().and_then(|s| s.dir.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let filter = t!("ui.dialogo.filtro_patch").to_string();
                self.open_dialog(move |mut d| {
                    if let Some(dir) = dir {
                        d = d.set_directory(dir);
                    }
                    d.add_filter(filter, &[PATCH_EXT]).set_file_name(format!("{name}.{PATCH_EXT}")).save_file().map(DialogResult::SavePatch)
                });
            }
            A::ExportBat => {
                let filter = t!("ui.dialogo.filtro_bat").to_string();
                self.open_dialog(move |d| d.add_filter(filter, &["bat"]).set_file_name("patch.bat").save_file().map(DialogResult::ExportBat)); // i18n-ok
            }
            A::ApplyRecipe(name) => {
                let Some(base) = crate::prefs::recipes_dir() else { return };
                match surshape_session::recipes::load(&base, &name) {
                    Ok(r) => {
                        self.clipboard = r.pasos;
                        self.paste_into(self.cursor_cell.0);
                    }
                    Err(e) => self.status = Some(Msg::new(Level::Error, "err.recetas.leer").arg("detalle", e)),
                }
            }
            A::SaveRecipe => {
                self.recipe_name.clear();
                self.modal = Some(Dialog::RecipeName(self.cursor_cell.0));
            }
            A::Clear => {
                if let Some(id) = sel {
                    self.modal = Some(Dialog::Confirm(Confirm::ClearFrom(id)));
                }
            }
            A::Console => self.page = if self.page == Page::Console { Page::Main } else { Page::Console },
            A::OpenPresetsFolder => self.open_folder(crate::prefs::presets_dir()),
            A::OpenRecipesFolder => self.open_folder(crate::prefs::recipes_dir()),
            A::MarkImport => self.dialog_import_labels(),
            A::MarkExport => self.dialog_export_labels(),
            A::MarkAdd => self.marker_add(false),
            A::MarkAddRegion => self.marker_add(true),
            A::MarkClear => self.markers_clear(),
            A::MarkPrev => self.marker_step(false),
            A::MarkNext => self.marker_step(true),
            A::Bulk => {
                let row = self.cursor_cell.0;
                let filter = t!("ui.dialogo.filtro_audio").to_string();
                self.open_dialog(move |d| d.add_filter(filter, surshape_audio::decode::EXTENSIONS).pick_files().map(|p| DialogResult::Bulk(row, p)));
            }
            A::ExportRows => self.open_dialog(|d| d.pick_folder().map(DialogResult::ExportRows)),
            A::InsertSubpatch(name) => self.insert_subpatch(&name),
            A::ExpandSubpatch => {
                if let Some(id) = sel {
                    self.expand_subpatch(id);
                }
            }
            A::Prefs => {
                self.prefs_draft = Some(self.prefs.clone());
                self.modal = Some(Dialog::Prefs);
            }
            A::Lang(l) => {
                let mut p = self.prefs.clone();
                p.idioma = l.code().to_string();
                self.apply_prefs(p);
                ctx.send_viewport_cmd(egui::ViewportCommand::Title(t!("ui.app.titulo").into()));
            }
            A::Units(u) => {
                let mut p = self.prefs.clone();
                p.unidades = u;
                self.apply_prefs(p);
            }
            A::Shortcuts => self.modal = Some(Dialog::Shortcuts),
            A::Manual => self.open_manual(),
            A::About => self.modal = Some(Dialog::About),
            A::Play => self.toggle_play(),
            A::Stop => self.player.stop(),
        }
    }

    // --- Agregar y reemplazar procesos ------------------------------------------------------

    /// Elige un proceso: reemplaza la celda (modo Reemplazar), abre una fila
    /// nueva (generadores) o lo agrega a la fila de la celda elegida (si la
    /// celda no es la última, en una rama nueva desde ella: nada se pierde).
    /// Después abre su página de parámetros.
    pub(crate) fn add_process(&mut self, pid: &str) {
        let Some(p) = self.registry.get(pid).cloned() else { return };
        if let Some(cell) = self.replace.take() {
            self.replace_process(cell, p.as_ref());
            return;
        }
        let seed = self.next_seed();
        if self.session.is_none() {
            return;
        }
        if p.inputs().is_generator() {
            if self.session.as_ref().is_some_and(|s| s.patch.filas.len() >= surshape_patch::MAX_ROWS) {
                self.status = Some(Msg::new(Level::Warning, "err.grilla.filas").arg("n", surshape_patch::MAX_ROWS));
                return;
            }
            self.push_undo();
            let s = self.session.as_mut().expect("sesión"); // i18n-ok
            let (id, _) = s.patch.add_generator(p.as_ref(), seed);
            apply_default_preset(s, id, p.as_ref());
            self.after_add(id);
            return;
        }
        // Fila destino: la de la celda elegida.
        let (row, col) = self.cursor_cell;
        let Some(s) = self.session.as_ref() else { return };
        if row >= s.patch.filas.len() {
            self.status = Some(Msg::new(Level::Warning, "err.grilla.sin_fuente"));
            return;
        }
        let r = &s.patch.filas[row];
        let off = usize::from(r.origen.is_some());
        let last_col = r.celdas.len() + off - 1;
        let from = if col < last_col { s.patch.at(row, col) } else { None };
        if r.celdas.len() + off >= surshape_patch::MAX_COLS && from.is_none() {
            self.status = Some(Msg::new(Level::Warning, "err.grilla.columnas").arg("n", surshape_patch::MAX_COLS));
            return;
        }
        self.push_undo();
        let s = self.session.as_mut().expect("sesión"); // i18n-ok
        let target_row = match from {
            Some(id) => match s.patch.add_branch(PortRef::main(id)) {
                Some(br) => br,
                None => return,
            },
            None => row,
        };
        let Some(id) = s.patch.append(target_row, p.as_ref(), seed) else {
            self.status = Some(Msg::new(Level::Warning, "err.patch.falta_entrada"));
            return;
        };
        apply_default_preset(s, id, p.as_ref());
        // Segunda entrada por defecto: el sonido de la misma fila (nunca
        // queda bloqueado; se cambia en la página de parámetros).
        if p.inputs().min() >= 2 {
            let src = s.patch.filas[target_row].celdas.first().copied().filter(|&c| c != id).or_else(|| s.patch.filas[target_row].origen.map(|o| o.nodo));
            if let Some(src) = src {
                for slot in 1..p.inputs().min() {
                    let _ = s.patch.connect(id, slot, PortRef::main(src));
                }
            }
        }
        // Tiempos desde la selección del visor (edición) o región a procesar.
        let selection = self.view.selection();
        let sr = self.view_loaded().map_or(48000, |l| l.audio.sr) as f64;
        let s = self.session.as_mut().expect("sesión"); // i18n-ok
        if let (Some((a, b)), Some(n)) = (selection, s.patch.node_mut(id)) {
            if p.params().iter().any(|x| x.id == "inicio") && p.params().iter().any(|x| x.id == "fin") { // i18n-ok
                n.params.comun.set("inicio", a as f64 / sr); // i18n-ok
                n.params.comun.set("fin", b as f64 / sr); // i18n-ok
            } else if self.procesar_sel {
                n.region = Some(Region { inicio: a as u64, fin: b as u64 });
            }
        }
        self.after_add(id);
    }

    fn after_add(&mut self, id: NodeId) {
        self.save_now();
        self.select(id);
        self.open_page(id);
    }

    /// Cambia el proceso de una celda manteniendo su lugar y sus entradas.
    pub(crate) fn replace_process(&mut self, cell: NodeId, p: &dyn Process) {
        let seed = self.next_seed();
        self.push_undo();
        let Some(s) = self.session.as_mut() else { return };
        let row_src = s.patch.cell_of(cell).and_then(|(r, _)| s.patch.filas[r].celdas.first().copied());
        let Some(n) = s.patch.node_mut(cell) else { return };
        if n.source().is_some() || matches!(n.tipo, NodeKind::Mezcla { .. }) || p.inputs().is_generator() {
            self.status = Some(Msg::new(Level::Warning, "err.grilla.reemplazo"));
            return;
        }
        n.tipo = NodeKind::Proceso { proceso: p.id().to_string() };
        n.params = ParamSet::defaults(p.params());
        n.seed = seed;
        n.render = None;
        n.error = None;
        let want = p.inputs().min();
        n.entradas.truncate(want.max(1));
        while n.entradas.len() < want {
            match row_src {
                Some(src) if src != cell => n.entradas.push(PortRef::main(src)),
                _ => break,
            }
        }
        apply_default_preset(s, cell, p);
        self.save_now();
        self.select(cell);
        self.open_page(cell);
        self.status = Some(Msg::new(Level::Info, "ui.estado.reemplazada"));
    }

    fn add_mix(&mut self) {
        if self.session.as_ref().is_some_and(|s| s.patch.filas.len() >= surshape_patch::MAX_ROWS) {
            self.status = Some(Msg::new(Level::Warning, "err.grilla.filas").arg("n", surshape_patch::MAX_ROWS));
            return;
        }
        self.push_undo();
        let Some(s) = self.session.as_mut() else { return };
        let (id, _) = s.patch.add_mix();
        self.save_now();
        self.select(id);
        self.page = Page::Mix(id);
    }

    // --- Copiar, pegar, ramificar, borrar ----------------------------------------------------

    /// Celdas a copiar: la fila entera (casilla "Copiar fila"), el tramo
    /// elegido (si contiene `id`) o solo `id`.
    pub(crate) fn copy_cells_from(&mut self, id: NodeId) {
        let Some(s) = self.session.as_ref() else { return };
        let ids: Vec<NodeId> = match (self.cell_opts.copiar_fila, s.patch.cell_of(id)) {
            (true, Some((r, _))) => s.patch.filas[r].celdas.iter().copied().filter(|c| s.patch.node(*c).is_some_and(|n| n.source().is_none())).collect(),
            _ => match (self.range_sel, s.patch.grid_pos(id)) {
                (Some((r, a, b)), Some((row, col))) if r == row && (a.min(b)..=a.max(b)).contains(&col) => {
                    (a.min(b)..=a.max(b)).filter_map(|c| s.patch.at(r, c)).collect()
                }
                _ => vec![id],
            },
        };
        self.clipboard = s.patch.copy_cells(&ids);
        self.status = Some(Msg::new(Level::Info, "ui.grilla.copiadas").arg("n", self.clipboard.len()));
    }

    pub(crate) fn paste_into(&mut self, row: usize) {
        if self.clipboard.is_empty() {
            return;
        }
        let items = self.clipboard.clone();
        if self.session.as_ref().is_none_or(|s| row >= s.patch.filas.len()) {
            self.status = Some(Msg::new(Level::Warning, "err.grilla.sin_fuente"));
            return;
        }
        self.push_undo();
        let Some(s) = self.session.as_mut() else { return };
        let created = s.patch.paste(row, &items);
        if let Some(&last) = created.last() {
            self.save_now();
            self.select(last);
            self.status = Some(Msg::new(Level::Info, "ui.grilla.pegadas").arg("n", created.len()));
        }
    }

    pub(crate) fn branch_from(&mut self, id: NodeId) {
        if self.session.as_ref().is_some_and(|s| s.patch.filas.len() >= surshape_patch::MAX_ROWS) {
            self.status = Some(Msg::new(Level::Warning, "err.grilla.filas").arg("n", surshape_patch::MAX_ROWS));
            return;
        }
        self.push_undo();
        if let Some(s) = self.session.as_mut() {
            if let Some(row) = s.patch.add_branch(PortRef::main(id)) {
                self.cursor_cell = (row, 1);
                self.save_now();
                self.status = Some(Msg::new(Level::Info, "ui.grilla.rama_creada"));
            }
        }
    }

    /// Borra una celda; si otras dependen de ella, pide confirmación.
    pub(crate) fn delete_cell(&mut self, id: NodeId) {
        let Some(s) = self.session.as_ref() else { return };
        if s.patch.direct_dependents(id).next().is_some() {
            self.modal = Some(Dialog::Confirm(Confirm::DeleteWithDependents(id)));
            return;
        }
        self.push_undo();
        let Some(s) = self.session.as_mut() else { return };
        match s.patch.remove(id) {
            Ok(()) => {
                self.player.stop();
                self.selected = None;
                self.page = Page::Main;
                self.save_now();
            }
            Err(e) => self.status = Some(Msg::new(Level::Warning, e.i18n_key())),
        }
    }

    /// Borra varias celdas y todo lo que depende de ellas.
    pub(crate) fn remove_with_dependents(&mut self, ids: &[NodeId]) {
        self.push_undo();
        let Some(s) = self.session.as_mut() else { return };
        let mut all: BTreeSet<NodeId> = ids.iter().copied().collect();
        for &id in ids {
            all.extend(s.patch.dependents(id));
        }
        // Quitar primero las que nadie usa.
        let mut left: Vec<NodeId> = all.into_iter().collect();
        let mut guard = left.len() * left.len() + 1;
        while !left.is_empty() && guard > 0 {
            guard -= 1;
            left.retain(|&id| s.patch.remove(id).is_err() && s.patch.node(id).is_some());
        }
        self.player.stop();
        self.selected = None;
        self.page = Page::Main;
        self.save_now();
    }

    /// Celdas desde `id` hasta el final de su fila.
    pub(crate) fn cells_from(&self, id: NodeId) -> Vec<NodeId> {
        let Some(s) = self.session.as_ref() else { return Vec::new() };
        match s.patch.cell_of(id) {
            Some((r, c)) => s.patch.filas[r].celdas[c..].to_vec(),
            None => Vec::new(),
        }
    }

    /// Deshacer por celda: vuelve los parámetros de la celda a como estaban
    /// en el último estado guardado en que eran distintos.
    pub(crate) fn undo_cell(&mut self, id: NodeId) {
        let Some(cur) = self.session.as_ref().and_then(|s| s.patch.node(id)).cloned() else { return };
        let same = |a: &surshape_patch::Node| a.params == cur.params && a.seed == cur.seed && a.region == cur.region && a.iteraciones == cur.iteraciones && a.tipo == cur.tipo && a.entradas == cur.entradas;
        let prev = self.undo.iter().rev().filter_map(|p| p.node(id)).find(|n| !same(n)).cloned();
        match prev {
            Some(old) => {
                self.push_undo();
                if let Some(n) = self.session.as_mut().and_then(|s| s.patch.node_mut(id)) {
                    n.params = old.params;
                    n.seed = old.seed;
                    n.region = old.region;
                    n.iteraciones = old.iteraciones;
                    n.tipo = old.tipo;
                    n.entradas = old.entradas;
                }
                self.save_now();
                self.status = Some(Msg::new(Level::Info, "ui.estado.deshecho_celda"));
            }
            None => self.status = Some(Msg::new(Level::Info, "ui.estado.nada_que_deshacer")),
        }
    }

    // --- Ejecutar ----------------------------------------------------------------------------

    fn run_row(&mut self) {
        let row = self.cursor_cell.0;
        let targets: Vec<NodeId> = self.session.as_ref().and_then(|s| s.patch.filas.get(row)).map(|r| r.celdas.clone()).unwrap_or_default();
        self.start_run(targets);
    }

    /// Re-render en cascada: la celda y todo lo que depende de ella.
    pub(crate) fn render_cascade(&mut self, id: NodeId) {
        let mut targets = vec![id];
        if let Some(s) = self.session.as_ref() {
            targets.extend(s.patch.dependents(id));
        }
        self.start_run(targets);
    }

    // --- Sub-patches, bulk y exportar filas -----------------------------------------------

    /// Agrega como un solo nodo (sub-patch) la receta `name` al final de la
    /// fila elegida.
    fn insert_subpatch(&mut self, name: &str) {
        let Some(base) = crate::prefs::recipes_dir() else { return };
        let recipe = match surshape_session::recipes::load(&base, name) {
            Ok(r) => r,
            Err(e) => {
                self.status = Some(Msg::new(Level::Error, "err.recetas.leer").arg("detalle", e));
                return;
            }
        };
        let row = self.cursor_cell.0;
        if self.session.as_ref().is_none_or(|s| row >= s.patch.filas.len()) {
            self.status = Some(Msg::new(Level::Warning, "err.grilla.sin_fuente"));
            return;
        }
        self.push_undo();
        let path = surshape_session::recipes::path_of(&base, name);
        let Some(s) = self.session.as_mut() else { return };
        match s.patch.append_subpatch(row, name, path, &recipe.pasos) {
            Some(id) => self.after_add(id),
            None => self.status = Some(Msg::new(Level::Warning, "err.patch.falta_entrada")),
        }
    }

    /// Abre un sub-patch como fila: una rama con sus pasos, para verlos y
    /// editarlos uno por uno.
    fn expand_subpatch(&mut self, id: NodeId) {
        let Some(n) = self.session.as_ref().and_then(|s| s.patch.node(id)).cloned() else { return };
        let (NodeKind::SubPatch { pasos, .. }, Some(&input)) = (&n.tipo, n.entradas.first()) else { return };
        if self.session.as_ref().is_some_and(|s| s.patch.filas.len() >= surshape_patch::MAX_ROWS) {
            self.status = Some(Msg::new(Level::Warning, "err.grilla.filas").arg("n", surshape_patch::MAX_ROWS));
            return;
        }
        self.push_undo();
        let Some(s) = self.session.as_mut() else { return };
        if let Some(row) = s.patch.add_branch(input) {
            let created = s.patch.paste(row, pasos);
            self.save_now();
            if let Some(&last) = created.last() {
                self.select(last);
            }
            self.status = Some(Msg::new(Level::Info, "ui.estado.subpatch_abierto").arg("n", created.len()));
        }
    }

    /// Guarda la última celda calculada de cada fila en `dir`
    /// (`<fuente>_<celda>.wav`).
    pub(crate) fn export_rows(&mut self, dir: &Path) {
        let Some(s) = self.session.as_ref() else { return };
        let mut jobs = Vec::new();
        for r in 0..s.patch.filas.len() {
            let Some(&last) = s.patch.filas[r].celdas.last() else { continue };
            let Some(rec) = s.patch.node(last).and_then(|n| n.render.as_ref()) else { continue };
            let Some(o) = rec.salidas.first() else { continue };
            let src_name = self
                .row_source(last)
                .and_then(|x| s.patch.node(x))
                .and_then(|n| n.source())
                .and_then(|x| x.ruta.file_stem().map(|n| n.to_string_lossy().into_owned()))
                .unwrap_or_default();
            let label = s.patch.cell_label(last).unwrap_or_default();
            jobs.push((s.abs(&o.archivo), dir.join(format!("{src_name}_{label}.wav")))); // i18n-ok
        }
        if jobs.is_empty() {
            self.status = Some(Msg::new(Level::Info, "ui.estado.filas_sin_render"));
            return;
        }
        let n = jobs.len();
        for (src, dst) in jobs {
            self.start_export(src, dst);
        }
        self.status = Some(Msg::new(Level::Info, "ui.estado.exportando_filas").arg("n", n));
    }

    // --- Bucles --------------------------------------------------------------------------------

    fn loop_get(&mut self) {
        let b = self.selected.and_then(|i| self.session.as_ref().and_then(|s| s.patch.node(i))).and_then(|n| n.bucle);
        match b {
            Some(r) => {
                self.view.sel = Some((r.inicio as usize, r.fin as usize));
                self.view.cursor = r.inicio as usize;
            }
            None => self.status = Some(Msg::new(Level::Info, "ui.estado.sin_bucle")),
        }
    }

    fn loop_set(&mut self) {
        let Some(id) = self.selected else { return };
        let sel = self.view.selection();
        if let Some(n) = self.session.as_mut().and_then(|s| s.patch.node_mut(id)) {
            n.bucle = sel.map(|(a, b)| Region { inicio: a as u64, fin: b as u64 });
        }
        self.save_now();
        self.status = Some(Msg::new(Level::Info, if sel.is_some() { "ui.estado.bucle_guardado" } else { "ui.estado.bucle_quitado" }));
    }

    /// LoopPt (inicio) / Endtime (fin) del bucle en el cursor.
    fn loop_point(&mut self, start: bool) {
        let Some(id) = self.selected else { return };
        let frames = self.view_loaded().map_or(0, |l| l.audio.frames()) as u64;
        let cur = self.view.cursor as u64;
        if let Some(n) = self.session.as_mut().and_then(|s| s.patch.node_mut(id)) {
            let mut r = n.bucle.unwrap_or(Region { inicio: 0, fin: frames });
            if start {
                r.inicio = cur.min(r.fin.saturating_sub(1));
            } else {
                r.fin = cur.max(r.inicio + 1);
            }
            n.bucle = Some(r);
        }
        self.save_now();
    }

    // --- Archivos de celda y línea de comando -----------------------------------------------

    /// Nombre del archivo de una salida de celda: `~A_3.wav`, `~B_2.ana`
    /// (salidas extra: `~A_3-2.wav`).
    pub(crate) fn cell_file(&self, id: NodeId, salida: u16) -> String {
        let label = self.session.as_ref().and_then(|s| s.patch.cell_label(id)).unwrap_or_default();
        let ext = self.node_kind(id).ext();
        if salida > 0 {
            format!("~{label}-{}.{ext}", salida + 1)
        } else {
            format!("~{label}.{ext}")
        }
    }

    /// Línea(s) de comando de una celda, como en Soundshaper: los comandos
    /// de CDP exactos (con la conversión automática si hace falta) o el
    /// descriptor del proceso nativo.
    pub(crate) fn cell_command(&self, id: NodeId) -> Vec<String> {
        let Some(n) = self.session.as_ref().and_then(|s| s.patch.node(id)) else { return Vec::new() };
        match &n.tipo {
            NodeKind::Fuente(src) => vec![t!("ui.comando.fuente", ruta = src.ruta.display())],
            NodeKind::Mezcla { canales } => {
                let parts: Vec<String> = n
                    .entradas
                    .iter()
                    .zip(canales)
                    .map(|(p, c)| format!("{}(gan={} pan={} ini={})", self.cell_file(p.nodo, p.salida), c.ganancia_db, c.paneo, c.inicio)) // i18n-ok
                    .collect();
                vec![format!("{} {} -> {}", t!("ui.comando.mezcla"), parts.join(" "), self.cell_file(id, 0))]
            }
            NodeKind::SubPatch { nombre, pasos, .. } => {
                let names: Vec<String> = pasos.iter().filter_map(|t| t.tipo_proceso().map(|p| p.to_string())).collect();
                let input = n.entradas.first().map(|p| self.cell_file(p.nodo, p.salida)).unwrap_or_default();
                vec![format!("subpatch \"{nombre}\" {input}: {} -> {}", names.join(" > "), self.cell_file(id, 0))] // i18n-ok
            }
            NodeKind::Proceso { proceso } => {
                let Some(p) = self.registry.get(proceso) else { return vec![proceso.clone()] };
                let mut lines = Vec::new();
                let mut names = Vec::new();
                for (i, port) in n.entradas.iter().enumerate() {
                    let have = self.node_kind(port.nodo);
                    let want = p.input_kind(i);
                    let base = self.cell_file(port.nodo, port.salida);
                    if want == FileKind::Ana && have != FileKind::Ana {
                        let ana = base.rsplit_once('.').map(|(b, _)| format!("{b}.ana")).unwrap_or_default(); // i18n-ok
                        let pv = self.prefs.pvoc();
                        let args = surshape_cdp::procs::anal_args(0, &base, &ana, pv.puntos, pv.superposicion);
                        lines.push(format!("{}   {}", surshape_engine::console::join_args("pvoc", &args), t!("ui.comando.auto"))); // i18n-ok
                        names.push(ana);
                    } else if want == FileKind::Wav && have == FileKind::Ana {
                        names.push(base.rsplit_once('.').map(|(b, _)| format!("{b}.wav")).unwrap_or_default()); // i18n-ok
                    } else {
                        names.push(base);
                    }
                }
                let out = self.cell_file(id, 0);
                let params = n.params.completed(p.params());
                let dur = n.entradas.first().and_then(|e| self.port_duration(e.nodo, e.salida)).unwrap_or(1.0);
                let dur2 = n.entradas.get(1).and_then(|e| self.port_duration(e.nodo, e.salida)).unwrap_or(0.0);
                let c = CmdCtx { params: &params.comun, seed: n.seed, inputs: &names, output: &out, dur, dur2 };
                lines.extend(p.command(&c));
                lines
            }
        }
    }

    /// Regenera `<sesión>/celdas/` con enlaces duros a los archivos de cada
    /// celda (sin duplicar disco). Solo si cambió algo.
    pub(crate) fn sync_cell_files(&mut self) {
        let Some(s) = self.session.as_ref() else { return };
        let mut want: Vec<(String, PathBuf)> = Vec::new();
        for n in &s.patch.nodos {
            let Some(label) = s.patch.cell_label(n.id) else { continue };
            match (&n.tipo, &n.render) {
                (NodeKind::Fuente(src), _) => want.push((format!("~{label}.wav"), src.ruta.clone())), // i18n-ok
                (_, Some(rec)) => {
                    for (k, o) in rec.salidas.iter().enumerate() {
                        let suf = if k > 0 { format!("-{}", k + 1) } else { String::new() };
                        if o.datos.is_empty() {
                            want.push((format!("~{label}{suf}.wav"), s.abs(&o.archivo))); // i18n-ok
                        } else {
                            let ext = o.kind().ext();
                            if o.datos.len() == 1 {
                                want.push((format!("~{label}{suf}.{ext}"), s.abs(&o.datos[0])));
                            } else {
                                for (ch, d) in o.datos.iter().enumerate() {
                                    want.push((format!("~{label}{suf}_c{}.{ext}", ch + 1), s.abs(d))); // i18n-ok
                                }
                            }
                            // La resíntesis también, para escucharla afuera.
                            want.push((format!("~{label}{suf}.wav"), s.abs(&o.archivo))); // i18n-ok
                        }
                    }
                }
                _ => {}
            }
        }
        let mut h = StableHasher::new();
        for (n, p) in &want {
            h.str(n);
            h.str(&p.display().to_string());
        }
        let sig = u64::from_str_radix(&h.finish_hex()[..16], 16).unwrap_or(1);
        if sig == self.cell_files_sig {
            return;
        }
        self.cell_files_sig = sig;
        let dir = s.dir.join(CELLS_DIR);
        if std::fs::create_dir_all(&dir).is_err() {
            return;
        }
        let names: BTreeSet<&str> = want.iter().map(|(n, _)| n.as_str()).collect();
        // Quitar los enlaces viejos (solo los de celda: empiezan con "~").
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if name.starts_with('~') && !names.contains(name.as_str()) {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }
        for (name, target) in want {
            let link = dir.join(&name);
            if file_stamp(&link) == file_stamp(&target) && link.is_file() {
                continue;
            }
            let _ = std::fs::remove_file(&link);
            // Si está en otro disco no se puede enlazar: la fuente queda
            // donde está (nunca se copia un original).
            let _ = std::fs::hard_link(&target, &link);
        }
    }

    // --- Patches de texto ---------------------------------------------------------------------

    pub(crate) fn save_patch_file(&mut self, path: &Path) {
        let Some(s) = self.session.as_ref() else { return };
        let mut patch = s.patch.clone();
        if self.cell_opts.incluir_fuentes {
            // Copia las fuentes junto al patch y las referencia relativas.
            let stem = path.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let folder = format!("{stem}_fuentes"); // i18n-ok
            let dir = path.parent().unwrap_or(Path::new(".")).join(&folder);
            if let Err(e) = std::fs::create_dir_all(&dir) {
                self.status = Some(Msg::new(Level::Error, "err.patch_texto.guardar").arg("detalle", e));
                return;
            }
            for n in &mut patch.nodos {
                if let NodeKind::Fuente(src) = &mut n.tipo {
                    let name = src.ruta.file_name().map(|f| f.to_os_string()).unwrap_or_default();
                    let dst = dir.join(&name);
                    if !dst.is_file() {
                        if let Err(e) = std::fs::copy(&src.ruta, &dst) {
                            self.status = Some(Msg::new(Level::Error, "err.patch_texto.guardar").arg("detalle", e));
                            return;
                        }
                    }
                    src.ruta = PathBuf::from(&folder).join(name);
                }
            }
        }
        match std::fs::write(path, to_text(&patch)) {
            Ok(()) => self.status = Some(Msg::new(Level::Info, "ui.estado.patch_guardado").arg("archivo", path.display())),
            Err(e) => self.status = Some(Msg::new(Level::Error, "err.patch_texto.guardar").arg("detalle", e)),
        }
    }

    pub(crate) fn load_patch_file(&mut self, path: &Path) {
        let txt = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => {
                self.status = Some(Msg::new(Level::Error, "err.patch_texto.leer").arg("detalle", e));
                return;
            }
        };
        let mut patch = match from_text(&txt) {
            Ok(p) => p,
            Err(e) => {
                self.status = Some(Msg::new(Level::Error, e.key).arg("linea", e.line).arg("detalle", e.detail));
                return;
            }
        };
        let reg = &self.registry;
        fix_generators(&mut patch, |id| reg.get(id).is_some_and(|p| p.inputs().is_generator()));
        // Rutas relativas: al lado del patch (fuentes incluidas).
        let base = path.parent().map(Path::to_path_buf).unwrap_or_default();
        for n in &mut patch.nodos {
            if let NodeKind::Fuente(src) = &mut n.tipo {
                if src.ruta.is_relative() {
                    src.ruta = base.join(&src.ruta);
                }
            }
        }
        self.push_undo();
        let Some(s) = self.session.as_mut() else { return };
        s.patch = patch;
        // Los renders que ya estén en la cache vuelven solos.
        for n in &mut s.patch.nodos {
            n.render = None;
        }
        let snapshot = s.clone();
        self.start_verify(&snapshot);
        self.missing.clear();
        self.selected = None;
        self.page = Page::Main;
        self.save_now();
        self.recover_from_cache();
        self.status = Some(Msg::new(Level::Info, "ui.estado.patch_cargado").arg("archivo", path.display()));
    }

    /// Recupera de la cache los renders de las celdas cuya clave ya existe.
    pub(crate) fn recover_from_cache(&mut self) {
        let opts = self.render_options();
        let Some(s) = self.session.as_mut() else { return };
        let keys = s.patch.keys(&self.registry, &opts);
        let dir = s.dir.clone();
        for n in &mut s.patch.nodos {
            if n.render.is_none() {
                if let Some(Ok(k)) = keys.get(&n.id) {
                    n.render = surshape_session::cache_lookup(&dir, k);
                }
            }
        }
    }

    /// Exporta como script .bat las celdas de CDP (cada canal por separado,
    /// como lo hace SURSHAPE: `housekeep chans` separa y `submix interleave`
    /// vuelve a juntar).
    pub(crate) fn export_bat(&mut self, path: &Path) {
        let Some(s) = self.session.as_ref() else { return };
        let mut native = Vec::new();
        let mut lines = vec![
            "@echo off".to_string(),                                                  // i18n-ok
            format!("rem {}", t!("ui.bat.cabecera")), // i18n-ok
            format!("rem {}", t!("ui.bat.cdp")), // i18n-ok
            format!("set CDP={}", self.cdp.as_ref().map(|c| c.dir.display().to_string()).unwrap_or_default()), // i18n-ok
            "set PATH=%CDP%;%PATH%".to_string(),                                       // i18n-ok
            String::new(),
        ];
        let order = s.patch.topo_order().unwrap_or_default();
        for id in order {
            let Some(n) = s.patch.node(id) else { continue };
            let label = s.patch.cell_label(id).unwrap_or_default();
            let chans = self.node_channels(id).max(1);
            match &n.tipo {
                NodeKind::Fuente(src) => {
                    lines.push(format!("rem {label}")); // i18n-ok
                    lines.push(format!("copy /y \"{}\" ~{label}.wav >nul", src.ruta.display())); // i18n-ok
                    if chans > 1 {
                        lines.push(format!("housekeep chans 2 ~{label}.wav")); // i18n-ok
                    }
                }
                NodeKind::Proceso { proceso } if proceso.starts_with("cdp.") => { // i18n-ok
                    lines.push(format!("rem {label}: {}", self.node_name(id))); // i18n-ok
                    for c in 1..=chans {
                        for l in self.cell_command(id) {
                            let cmd = l.split("   ").next().unwrap_or_default().to_string();
                            lines.push(if chans > 1 { per_channel(&cmd, c) } else { cmd });
                        }
                    }
                    if chans > 1 && self.node_kind(id) == FileKind::Wav {
                        let ins: Vec<String> = (1..=chans).map(|c| format!("~{label}_c{c}.wav")).collect(); // i18n-ok
                        lines.push(format!("submix interleave {} ~{label}.wav", ins.join(" "))); // i18n-ok
                    }
                }
                _ => native.push(label),
            }
        }
        if !native.is_empty() {
            lines.push(String::new());
            lines.push(format!("rem {}", t!("ui.bat.nativas", celdas = native.join(", ")))); // i18n-ok
        }
        match std::fs::write(path, lines.join("\r\n") + "\r\n") { // i18n-ok
            Ok(()) if native.is_empty() => self.status = Some(Msg::new(Level::Info, "ui.estado.bat_exportado").arg("archivo", path.display())),
            Ok(()) => self.status = Some(Msg::new(Level::Warning, "ui.estado.bat_parcial").arg("celdas", native.join(", "))),
            Err(e) => self.status = Some(Msg::new(Level::Error, "err.patch_texto.guardar").arg("detalle", e)),
        }
    }

    /// Canales de la salida de una celda (fuente o render; si no, de su
    /// entrada).
    pub(crate) fn node_channels(&self, id: NodeId) -> usize {
        let Some(n) = self.session.as_ref().and_then(|s| s.patch.node(id)) else { return 1 };
        match (&n.tipo, &n.render) {
            (NodeKind::Fuente(s), _) => s.canales as usize,
            (_, Some(r)) => r.salidas.first().map_or(1, |o| o.canales as usize),
            _ => n.entradas.first().map_or(1, |p| self.node_channels(p.nodo)),
        }
    }

    // --- Editor externo -------------------------------------------------------------------------

    fn external_edit(&mut self) {
        let Some(editor) = self.prefs.editor_externo.clone() else {
            self.status = Some(Msg::new(Level::Warning, "ui.estado.sin_editor"));
            self.prefs_draft = Some(self.prefs.clone());
            self.prefs_tab = 0;
            self.modal = Some(Dialog::Prefs);
            return;
        };
        let (Some(id), Some(s)) = (self.selected, self.session.as_ref()) else { return };
        let Some((_, src)) = self.port_target(id, self.view_salida) else {
            self.status = Some(Msg::new(Level::Info, "ui.visor.sin_render"));
            return;
        };
        let label = s.patch.cell_label(id).unwrap_or_default();
        let dir = s.dir.join(EDITED_DIR);
        let _ = std::fs::create_dir_all(&dir);
        let n = std::fs::read_dir(&dir).map(|r| r.count()).unwrap_or(0) + 1;
        let dst = dir.join(format!("{label}_editado_{n}.wav")); // i18n-ok
        // Una copia (el editor la modifica; el original no se toca).
        let copied = if src.extension().is_some_and(|x| x.eq_ignore_ascii_case("wav")) { // i18n-ok
            std::fs::copy(&src, &dst).map(|_| ()).map_err(|e| e.to_string())
        } else {
            surshape_audio::decode::load(&src)
                .map_err(|e| e.to_string())
                .and_then(|a| surshape_audio::export::write_wav(&dst, &a, surshape_audio::WavFormat::Float32).map_err(|e| e.to_string()))
        };
        if let Err(e) = copied {
            self.status = Some(Msg::new(Level::Error, "err.audio.escribir").arg("archivo", dst.display()).arg("detalle", e));
            return;
        }
        let stamp = file_stamp(&dst);
        let file = dst.clone();
        let task = Task::spawn("surshape-editor", move |_, _| { // i18n-ok
            std::process::Command::new(&editor)
                .arg(&file)
                .status()
                .map(|_| ())
                .map_err(|e| surshape_engine::ProcessError::new("err.editor.lanzar").arg("detalle", e))
        });
        self.ext_edits.push(ExtEdit { file: dst, stamp, task });
        self.status = Some(Msg::new(Level::Info, "ui.estado.editando").arg("celda", label));
    }

    /// Al cerrar el editor: si el archivo cambió, entra como fuente nueva.
    pub(crate) fn poll_external_edits(&mut self) {
        let mut i = 0;
        while i < self.ext_edits.len() {
            match self.ext_edits[i].task.poll() {
                None => i += 1,
                Some(res) => {
                    let e = self.ext_edits.remove(i);
                    match res {
                        Ok(()) if file_stamp(&e.file) != e.stamp => self.import_as(e.file, ImportTarget::NewRow),
                        Ok(()) => self.status = Some(Msg::new(Level::Info, "ui.estado.editor_sin_cambios")),
                        Err(err) => self.status = Some(Msg::from_err(&err)),
                    }
                }
            }
        }
    }

    // --- Info -----------------------------------------------------------------------------------

    /// Corre `sndinfo <modo>` de CDP sobre el archivo de la celda.
    fn cdp_info(&mut self, mode: &'static str) {
        let (Some(id), Some(cdp)) = (self.selected, self.cdp.clone()) else { return };
        let Some((_, file)) = self.port_target(id, self.view_salida) else {
            self.status = Some(Msg::new(Level::Info, "ui.visor.sin_render"));
            return;
        };
        let label = self.session.as_ref().and_then(|s| s.patch.cell_label(id)).unwrap_or_default();
        let title = t!("ui.info.titulo_cdp", modo = mode, celda = label);
        let task = Task::spawn("surshape-info", move |_, c| { // i18n-ok
            let cwd = std::env::temp_dir();
            let out = surshape_cdp::run::run(&cdp.path_of("sndinfo"), &[mode.to_string(), file.display().to_string()], &cwd, c, |_| {})?; // i18n-ok
            Ok(surshape_cdp::run::strip_progress(&out))
        });
        self.info_task = Some((title, task));
    }

    // --- Varios ---------------------------------------------------------------------------------

    /// Exporta lo que muestra el visor (nombre sugerido: el de "Guardar
    /// archivo").
    pub(crate) fn dialog_export(&mut self) {
        let Some((_, src)) = self.view_target() else { return };
        let name = if self.save_name.trim().is_empty() { self.export_name() } else { self.save_name.trim().to_string() };
        let filter = t!("ui.dialogo.filtro_wav").to_string();
        self.open_dialog(move |d| {
            d.add_filter(filter, &["wav"]) // i18n-ok
                .set_file_name(format!("{name}.wav")) // i18n-ok
                .save_file()
                .map(|p| DialogResult::Export(src, p))
        });
    }

    /// Nombre sugerido: fuente + celda + proceso.
    pub(crate) fn export_name(&self) -> String {
        let Some(s) = self.session.as_ref() else { return String::new() };
        let Some(id) = self.selected else { return String::new() };
        let row = s.patch.cell_of(id).map_or(0, |(r, _)| r);
        let src = s
            .patch
            .filas
            .get(row)
            .and_then(|r| r.celdas.first())
            .and_then(|&f| s.patch.node(f))
            .and_then(|n| n.source())
            .and_then(|x| x.ruta.file_stem().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_default();
        let label = s.patch.cell_label(id).unwrap_or_default();
        match s.patch.node(id).and_then(|n| n.process_id()) {
            Some(p) => format!("{src}_{label}_{}", p.split('.').nth(1).unwrap_or(p)), // i18n-ok
            None => src,
        }
    }

    /// Abre el manual en el navegador (junto al exe, o en dist/ al desarrollar).
    pub(crate) fn open_manual(&mut self) {
        let file = format!("manual_{}.html", surshape_i18n::lang().code()); // i18n-ok
        let candidates = [
            self.exe_dir.as_ref().map(|d| d.join("manual").join(&file)), // i18n-ok
            Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("dist").join("manual").join(&file)), // i18n-ok
        ];
        match candidates.into_iter().flatten().find(|p| p.is_file()) {
            Some(p) => {
                let _ = std::process::Command::new("explorer").arg(p).spawn(); // i18n-ok
            }
            None => self.status = Some(Msg::new(Level::Warning, "err.manual.no_encontrado")),
        }
    }

    /// Abre una carpeta en el explorador (la crea si no existe).
    pub(crate) fn open_folder(&mut self, dir: Option<PathBuf>) {
        if let Some(d) = dir {
            let _ = std::fs::create_dir_all(&d);
            let _ = std::process::Command::new("explorer").arg(d).spawn(); // i18n-ok
        }
    }
}

/// Comando de un canal: `~A_1.ana` -> `~A_1_c2.ana` (los nombres de celda).
fn per_channel(cmd: &str, c: usize) -> String {
    cmd.split(' ')
        .map(|w| match (w.starts_with('~'), w.rsplit_once('.')) {
            (true, Some((b, e))) => format!("{b}_c{c}.{e}"), // i18n-ok
            _ => w.to_string(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Aplica el preset por defecto del proceso (si hay) a un nodo recién creado.
pub(crate) fn apply_default_preset(s: &mut surshape_session::Session, id: NodeId, p: &dyn Process) {
    let Some(base) = crate::prefs::presets_dir() else { return };
    if let (Some(v), Some(n)) = (surshape_session::presets::load_default(&base, p.id()), s.patch.node_mut(id)) {
        n.params.comun = v.completed(p.params());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_names_in_bat() {
        assert_eq!(per_channel("blur blur ~A_1.ana ~A_2.ana 12", 2), "blur blur ~A_1_c2.ana ~A_2_c2.ana 12");
    }
}
