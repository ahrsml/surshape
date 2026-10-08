//! Barra de menús (16 menús con letra de acceso: Alt+letra), toolbar plana,
//! línea de ayuda y barra de estado con paneles hundidos.
//!
//! La barra de menús es propia (egui no subraya letras de acceso ni abre
//! menús con el teclado): cada título lleva su letra subrayada, Alt+letra lo
//! abre, las flechas izquierda/derecha pasan al vecino, Esc cierra y, con un
//! menú abierto, pasar el puntero sobre otro título lo abre. Si la ventana
//! es angosta, la barra pasa a dos líneas (como Win32).

use crate::app::{App, Page};
use crate::theme::{self, ui_font};
use crate::win32::{self, Bevel, Icon, Level};
use eframe::egui::{self, text::LayoutJob, vec2, Align, Align2, Area, Id, Key, Layout, Order, Rect, Sense, TextFormat, Ui};
use std::sync::Arc;
use surshape_engine::process::{desc_key, name_key};
use surshape_engine::{Family, Process};
use surshape_i18n::{t, tr, Lang};

/// Títulos de los menús (clave i18n; `&` marca la letra de acceso).
pub(crate) const MENUS: [&str; 16] = [
    "ui.menu.sesion",
    "ui.menu.editar",
    "ui.menu.edicion",
    "ui.menu.tiempo",
    "ui.menu.espectral",
    "ui.menu.pitch",
    "ui.menu.synth",
    "ui.menu.destruccion",
    "ui.menu.filtro",
    "ui.menu.info",
    "ui.menu.datos",
    "ui.menu.patch",
    "ui.menu.marcadores",
    "ui.menu.herramientas",
    "ui.menu.preferencias",
    "ui.menu.ayuda",
];

/// Índices de los menús de procesos.
pub(crate) const M_EDICION: usize = 2;
pub(crate) const M_TIEMPO: usize = 3;
pub(crate) const M_ESPECTRAL: usize = 4;
pub(crate) const M_PITCH: usize = 5;
pub(crate) const M_SYNTH: usize = 6;
pub(crate) const M_DESTRUCCION: usize = 7;
pub(crate) const M_FILTRO: usize = 8;

/// Menú donde va un proceso.
pub(crate) fn menu_of(p: &dyn Process) -> usize {
    if p.inputs().is_generator() {
        return M_SYNTH;
    }
    match p.family() {
        Family::Tiempo => M_TIEMPO,
        Family::Espectral => M_ESPECTRAL,
        Family::Tono => M_PITCH,
        Family::Generador => M_SYNTH,
        Family::Destruccion => M_DESTRUCCION,
        Family::FiltroEspacio => M_FILTRO,
        Family::Combinacion if p.id() == "nat.convolve" => M_FILTRO, // i18n-ok
        Family::Combinacion => M_ESPECTRAL,
        Family::Utilidad => M_EDICION,
    }
}

/// Texto sin `&` y la letra de acceso.
pub(crate) fn mnemonic(s: &str) -> (String, Option<(usize, char)>) {
    let mut out = String::new();
    let mut m = None;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '&' {
            if let Some(&n) = chars.peek() {
                if m.is_none() {
                    m = Some((out.len(), n));
                }
                continue;
            }
        }
        out.push(c);
    }
    (out, m)
}

/// Guarda la primera acción elegida.
fn set(a: &mut Option<Action>, chosen: bool, x: Action) {
    if chosen && a.is_none() {
        *a = Some(x);
    }
}

/// Lo que puede pedir un menú, la toolbar o un atajo.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Action {
    NewSession,
    OpenSession,
    OpenSessionFolder,
    Import,
    ImportRecent(std::path::PathBuf),
    Export,
    Quit,
    Undo,
    UndoCell,
    Copy,
    Paste,
    Delete,
    ReEdit,
    Replace,
    Branch,
    AddProcess(&'static str),
    AddMix,
    EditExternal,
    LoopGet,
    LoopSet,
    LoopStart,
    LoopEnd,
    ZoomIn,
    ZoomOut,
    ZoomAll,
    CellInfo,
    CdpInfo(&'static str),
    GraphEdit,
    OpenCellsFolder,
    RunCell,
    RunRow,
    RunFrom,
    RunAll,
    LoadPatch,
    SavePatch,
    ExportBat,
    ApplyRecipe(String),
    SaveRecipe,
    Clear,
    Console,
    OpenPresetsFolder,
    OpenRecipesFolder,
    Prefs,
    MarkImport,
    MarkExport,
    MarkAdd,
    MarkAddRegion,
    MarkClear,
    MarkPrev,
    MarkNext,
    Bulk,
    ExportRows,
    InsertSubpatch(String),
    ExpandSubpatch,
    Lang(Lang),
    Units(crate::prefs::TimeUnit),
    Shortcuts,
    Manual,
    About,
    Play,
    Stop,
}

impl App {
    // --- Barra de menús ------------------------------------------------------------------

    pub(crate) fn menu_bar(&mut self, ui: &mut Ui) {
        // Alt+letra abre el menú; Esc cierra; flechas recorren.
        let alt = ui.input(|i| i.modifiers.alt);
        let titles: Vec<(String, Option<(usize, char)>)> = MENUS.iter().map(|k| mnemonic(tr(k))).collect();
        if alt && !ui.ctx().wants_keyboard_input() {
            let typed: Vec<Key> = ui.input(|i| {
                i.events
                    .iter()
                    .filter_map(|e| match e {
                        egui::Event::Key { key, pressed: true, .. } => Some(*key),
                        _ => None,
                    })
                    .collect()
            });
            for k in typed {
                let name = k.name().to_lowercase();
                if let Some(i) = titles.iter().position(|(_, m)| m.is_some_and(|(_, c)| c.to_lowercase().to_string() == name)) {
                    self.open_menu = Some(i);
                    ui.input_mut(|inp| inp.consume_key(egui::Modifiers::ALT, k));
                }
            }
        }
        if let Some(open) = self.open_menu {
            let (l, r, esc) = ui.input(|i| (i.key_pressed(Key::ArrowLeft), i.key_pressed(Key::ArrowRight), i.key_pressed(Key::Escape)));
            if esc {
                self.open_menu = None;
            } else if l {
                self.open_menu = Some((open + MENUS.len() - 1) % MENUS.len());
            } else if r {
                self.open_menu = Some((open + 1) % MENUS.len());
            }
        }
        let font = ui_font();
        self.menu_rects.clear();
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            for (i, (text, m)) in titles.iter().enumerate() {
                let mut job = LayoutJob::default();
                let fmt = TextFormat { font_id: font.clone(), color: theme::t().TEXT, ..Default::default() };
                match m {
                    Some((pos, c)) => {
                        let end = pos + c.len_utf8();
                        job.append(&text[..*pos], 0.0, fmt.clone());
                        job.append(&text[*pos..end], 0.0, TextFormat { underline: egui::Stroke::new(1.0_f32, theme::t().TEXT), ..fmt.clone() });
                        job.append(&text[end..], 0.0, fmt);
                    }
                    None => job.append(text, 0.0, fmt),
                }
                let galley = ui.fonts(|f| f.layout_job(job));
                let size = galley.size() + vec2(12.0, 6.0);
                let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
                win32::audit::record(ui, text, rect);
                let open = self.open_menu == Some(i);
                if resp.clicked() {
                    self.open_menu = if open { None } else { Some(i) };
                } else if self.open_menu.is_some() && resp.hovered() && !open {
                    self.open_menu = Some(i);
                }
                let p = ui.painter();
                if self.open_menu == Some(i) {
                    win32::bevel(p, rect, Bevel::ThinSunken);
                } else if resp.hovered() {
                    win32::bevel(p, rect, Bevel::ThinRaised);
                }
                let off = if self.open_menu == Some(i) { vec2(1.0, 1.0) } else { vec2(0.0, 0.0) };
                p.galley(rect.center() - galley.size() * 0.5 + off, galley, theme::t().TEXT);
                self.menu_rects.push(rect);
            }
        });
    }

    /// Dibuja el menú abierto (en una capa por encima de todo) y ejecuta la
    /// acción elegida.
    pub(crate) fn menu_popups(&mut self, ctx: &egui::Context) {
        let Some(i) = self.open_menu else { return };
        let Some(&anchor) = self.menu_rects.get(i) else { return };
        let mut action = None;
        let area = Area::new(Id::new(("menu", i))) // i18n-ok
            .order(Order::Foreground)
            .fixed_pos(anchor.left_bottom())
            .constrain(true)
            .show(ctx, |ui| {
                let frame = egui::Frame::NONE.fill(theme::t().FACE).inner_margin(egui::Margin::same(3));
                let r = frame.show(ui, |ui| {
                    ui.set_min_width(180.0);
                    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                    action = self.menu_items(ui, i);
                });
                win32::bevel(ui.painter(), r.response.rect, Bevel::Raised);
            });
        // Clic fuera del menú y de la barra: se cierra.
        let clicked_out = ctx.input(|inp| inp.pointer.any_pressed())
            && ctx.input(|inp| inp.pointer.interact_pos()).is_some_and(|p| !area.response.rect.contains(p) && !self.menu_rects.iter().any(|r| r.contains(p)));
        if clicked_out {
            self.open_menu = None;
        }
        if let Some(a) = action {
            self.open_menu = None;
            self.do_action(ctx, a);
        }
    }

    /// Un elemento de menú. Devuelve true si se eligió.
    fn item(&mut self, ui: &mut Ui, key: &str, shortcut: &str, help: &str, enabled: bool) -> bool {
        let r = win32::list_row(ui, false, tr(key), shortcut, enabled);
        self.hint_key(&r, help);
        r.clicked()
    }

    /// Lista de procesos de un menú: primero los de una entrada; después,
    /// bajo "(2 sonidos)", los que combinan dos.
    fn process_items(&mut self, ui: &mut Ui, menu: usize) -> Option<Action> {
        let procs: Vec<Arc<dyn Process>> = self.registry.all().iter().filter(|p| menu_of(p.as_ref()) == menu).cloned().collect();
        if procs.is_empty() {
            win32::list_row(ui, false, tr("ui.menu.vacio"), "", false);
            return None;
        }
        let mut out = None;
        let (one, two): (Vec<_>, Vec<_>) = procs.into_iter().partition(|p| p.inputs().min() < 2);
        for (k, group) in [one, two].into_iter().enumerate() {
            if group.is_empty() {
                continue;
            }
            if k == 1 {
                win32::menu_sep(ui);
                win32::menu_caption(ui, tr("ui.menu.dos_sonidos"));
            }
            for p in group {
                let r = win32::list_row(ui, false, tr(&name_key(p.id())), p.output_kind().ext(), true);
                self.hint(&r, tr(&desc_key(p.id())));
                if r.clicked() {
                    out = Some(Action::AddProcess(p.id()));
                }
            }
        }
        out
    }

    fn menu_items(&mut self, ui: &mut Ui, menu: usize) -> Option<Action> {
        use Action as A;
        let sel = self.selected.is_some();
        let mut a = None;
        match menu {
            0 => {
                set(&mut a, self.item(ui, "ui.menu.sesion_nueva", "", "ui.ayuda.sesion_nueva", true), A::NewSession);
                set(&mut a, self.item(ui, "ui.menu.sesion_abrir", "", "ui.ayuda.sesion_abrir", true), A::OpenSession);
                set(&mut a, self.item(ui, "ui.menu.sesion_carpeta", "", "ui.ayuda.sesion_carpeta", true), A::OpenSessionFolder);
                win32::menu_sep(ui);
                set(&mut a, self.item(ui, "ui.menu.importar", &t!("ui.atajo.tecla.ctrl_i"), "ui.ayuda.importar", true), A::Import);
                set(&mut a, self.item(ui, "ui.menu.exportar", &t!("ui.atajo.tecla.ctrl_e"), "ui.ayuda.exportar", sel), A::Export);
                let recent = self.prefs.recientes.clone();
                if !recent.is_empty() {
                    win32::menu_sep(ui);
                    win32::menu_caption(ui, tr("ui.menu.recientes"));
                    for p in recent.iter().take(6) {
                        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                        let r = win32::list_row(ui, false, &name, "", p.is_file());
                        self.hint(&r, p.display().to_string());
                        set(&mut a, r.clicked(), A::ImportRecent(p.clone()));
                    }
                }
                win32::menu_sep(ui);
                set(&mut a, self.item(ui, "ui.menu.salir", "", "ui.ayuda.salir", true), A::Quit);
            }
            1 => {
                set(&mut a, self.item(ui, "ui.menu.deshacer", &t!("ui.atajo.tecla.ctrl_z"), "ui.ayuda.deshacer", !self.undo.is_empty()), A::Undo);
                set(&mut a, self.item(ui, "ui.menu.deshacer_celda", "", "ui.ayuda.deshacer_celda", sel && !self.undo.is_empty()), A::UndoCell);
                win32::menu_sep(ui);
                set(&mut a, self.item(ui, "ui.menu.copiar", &t!("ui.atajo.tecla.ctrl_c"), "ui.ayuda.copiar", sel), A::Copy);
                set(&mut a, self.item(ui, "ui.menu.pegar", &t!("ui.atajo.tecla.ctrl_v"), "ui.ayuda.pegar", !self.clipboard.is_empty()), A::Paste);
                set(&mut a, self.item(ui, "ui.menu.borrar", &t!("ui.atajo.tecla.supr"), "ui.ayuda.quitar", sel), A::Delete);
                win32::menu_sep(ui);
                set(&mut a, self.item(ui, "ui.menu.reeditar", &t!("ui.atajo.tecla.intro"), "ui.ayuda.reeditar", sel), A::ReEdit);
                set(&mut a, self.item(ui, "ui.menu.reemplazar", "", "ui.ayuda.reemplazar", sel), A::Replace);
                set(&mut a, self.item(ui, "ui.menu.ramificar", &t!("ui.atajo.tecla.ctrl_b"), "ui.ayuda.ramificar", sel), A::Branch);
            }
            M_EDICION => {
                win32::menu_caption(ui, tr("ui.menu.edicion_onda"));
                if let Some(x) = self.process_items(ui, M_EDICION) {
                    a = Some(x);
                }
                win32::menu_sep(ui);
                set(&mut a, self.item(ui, "ui.menu.mezcla_nueva", "", "ui.ayuda.mezcla_nueva", true), A::AddMix);
                set(&mut a, self.item(ui, "ui.menu.editar_externo", "", "ui.ayuda.editar_externo", sel), A::EditExternal);
                win32::menu_sep(ui);
                win32::menu_caption(ui, tr("ui.menu.bucles"));
                set(&mut a, self.item(ui, "ui.menu.bucle_get", "", "ui.ayuda.bucle_get", sel), A::LoopGet);
                set(&mut a, self.item(ui, "ui.menu.bucle_set", "", "ui.ayuda.bucle_set", sel), A::LoopSet);
                set(&mut a, self.item(ui, "ui.menu.bucle_inicio", "", "ui.ayuda.bucle_inicio", sel), A::LoopStart);
                set(&mut a, self.item(ui, "ui.menu.bucle_fin", "", "ui.ayuda.bucle_fin", sel), A::LoopEnd);
            }
            M_TIEMPO | M_ESPECTRAL | M_PITCH | M_DESTRUCCION | M_FILTRO => a = self.process_items(ui, menu),
            M_SYNTH => a = self.process_items(ui, M_SYNTH),
            9 => {
                set(&mut a, self.item(ui, "ui.menu.info_celda", "", "ui.ayuda.info_celda", sel), A::CellInfo);
                win32::menu_sep(ui);
                win32::menu_caption(ui, tr("ui.menu.info_cdp"));
                let has = self.cdp.as_ref().is_some_and(|c| c.has("sndinfo")); // i18n-ok
                for (key, mode) in [
                    ("ui.menu.info_props", "props"),       // i18n-ok
                    ("ui.menu.info_maxsamp", "maxsamp"),   // i18n-ok
                    ("ui.menu.info_largo", "len"),         // i18n-ok
                    ("ui.menu.info_loudchan", "loudchan"), // i18n-ok
                ] {
                    set(&mut a, self.item(ui, key, "", "ui.ayuda.info_cdp", sel && has), A::CdpInfo(mode));
                }
            }
            10 => {
                set(&mut a, self.item(ui, "ui.menu.graph_edit", "", "ui.ayuda.graph_edit", self.bp_edit.is_some()), A::GraphEdit);
                set(&mut a, self.item(ui, "ui.menu.carpeta_celdas", "", "ui.ayuda.carpeta_celdas", self.session.is_some()), A::OpenCellsFolder);
            }
            11 => {
                set(&mut a, self.item(ui, "ui.menu.ejecutar_celda", &t!("ui.atajo.tecla.ctrl_r"), "ui.ayuda.render", sel), A::RunCell);
                set(&mut a, self.item(ui, "ui.menu.ejecutar_fila", "", "ui.ayuda.ejecutar_fila", sel), A::RunRow);
                set(&mut a, self.item(ui, "ui.menu.ejecutar_desde", &t!("ui.atajo.tecla.ctrl_alt_r"), "ui.ayuda.cascada", sel), A::RunFrom);
                set(&mut a, self.item(ui, "ui.menu.ejecutar_todo", &t!("ui.atajo.tecla.ctrl_mayus_r"), "ui.ayuda.render_todo", true), A::RunAll);
                win32::menu_sep(ui);
                set(&mut a, self.item(ui, "ui.menu.patch_cargar", &t!("ui.atajo.tecla.ctrl_o"), "ui.ayuda.patch_cargar", true), A::LoadPatch);
                set(&mut a, self.item(ui, "ui.menu.patch_guardar", &t!("ui.atajo.tecla.ctrl_s"), "ui.ayuda.patch_guardar", true), A::SavePatch);
                set(&mut a, self.item(ui, "ui.menu.exportar_bat", "", "ui.ayuda.exportar_bat", true), A::ExportBat);
                win32::menu_sep(ui);
                win32::menu_caption(ui, tr("ui.recetas.aplicar"));
                let list = crate::prefs::recipes_dir().map(|b| surshape_session::recipes::list(&b)).unwrap_or_default();
                if list.is_empty() {
                    win32::list_row(ui, false, tr("ui.recetas.ninguna"), "", false);
                }
                for name in list {
                    let r = win32::list_row(ui, false, &name, "", sel);
                    self.hint_key(&r, "ui.ayuda.recetas_aplicar");
                    set(&mut a, r.clicked(), A::ApplyRecipe(name.clone()));
                }
                set(&mut a, self.item(ui, "ui.recetas.guardar_menu", "", "ui.ayuda.recetas_guardar", sel), A::SaveRecipe);
                win32::menu_sep(ui);
                win32::menu_caption(ui, tr("ui.menu.subpatch_insertar"));
                let list = crate::prefs::recipes_dir().map(|b| surshape_session::recipes::list(&b)).unwrap_or_default();
                if list.is_empty() {
                    win32::list_row(ui, false, tr("ui.recetas.ninguna"), "", false);
                }
                for name in list {
                    let r = win32::list_row(ui, false, &name, "", sel);
                    self.hint_key(&r, "ui.ayuda.subpatch_insertar");
                    set(&mut a, r.clicked(), A::InsertSubpatch(name.clone()));
                }
                let is_sub = self.selected.and_then(|i| self.session.as_ref()?.patch.node(i)).is_some_and(|n| matches!(n.tipo, surshape_patch::NodeKind::SubPatch { .. }));
                set(&mut a, self.item(ui, "ui.menu.subpatch_abrir", "", "ui.ayuda.subpatch_abrir", is_sub), A::ExpandSubpatch);
                win32::menu_sep(ui);
                set(&mut a, self.item(ui, "ui.menu.bulk", "", "ui.ayuda.bulk_menu", sel), A::Bulk);
                set(&mut a, self.item(ui, "ui.menu.exportar_filas", "", "ui.ayuda.exportar_filas", true), A::ExportRows);
                win32::menu_sep(ui);
                set(&mut a, self.item(ui, "ui.menu.limpiar", "", "ui.ayuda.limpiar", sel), A::Clear);
            }
            12 => {
                let has = !self.current_markers().is_empty();
                set(&mut a, self.item(ui, "ui.menu.marc_importar", "", "ui.ayuda.marc_importar", sel), A::MarkImport);
                set(&mut a, self.item(ui, "ui.menu.marc_exportar", "", "ui.ayuda.marc_exportar", has), A::MarkExport);
                win32::menu_sep(ui);
                set(&mut a, self.item(ui, "ui.menu.marc_agregar", &t!("ui.atajo.tecla.m"), "ui.ayuda.marc_agregar", sel), A::MarkAdd);
                set(&mut a, self.item(ui, "ui.menu.marc_region", "", "ui.ayuda.marc_region", sel && self.view.selection().is_some()), A::MarkAddRegion);
                set(&mut a, self.item(ui, "ui.menu.marc_anterior", &t!("ui.atajo.tecla.coma"), "ui.ayuda.marc_navegar", has), A::MarkPrev);
                set(&mut a, self.item(ui, "ui.menu.marc_siguiente", &t!("ui.atajo.tecla.punto"), "ui.ayuda.marc_navegar", has), A::MarkNext);
                win32::menu_sep(ui);
                set(&mut a, self.item(ui, "ui.menu.marc_quitar", "", "ui.ayuda.marc_quitar", has), A::MarkClear);
            }
            13 => {
                set(&mut a, self.item(ui, "ui.menu.consola", &t!("ui.atajo.tecla.ctrl_k"), "ui.ayuda.consola", true), A::Console);
                win32::menu_sep(ui);
                set(&mut a, self.item(ui, "ui.menu.carpeta_presets", "", "ui.ayuda.carpetas", true), A::OpenPresetsFolder);
                set(&mut a, self.item(ui, "ui.menu.carpeta_recetas", "", "ui.ayuda.carpetas", true), A::OpenRecipesFolder);
                set(&mut a, self.item(ui, "ui.menu.carpeta_celdas", "", "ui.ayuda.carpeta_celdas", self.session.is_some()), A::OpenCellsFolder);
            }
            14 => {
                set(&mut a, self.item(ui, "ui.menu.preferencias_dialogo", "", "ui.ayuda.preferencias", true), A::Prefs);
                win32::menu_sep(ui);
                win32::menu_caption(ui, tr("ui.prefs.idioma"));
                for l in Lang::ALL {
                    let r = win32::list_row(ui, self.prefs.lang() == l, tr(l.name_key()), "", true);
                    self.hint_key(&r, "ui.ayuda.idioma");
                    set(&mut a, r.clicked(), A::Lang(l));
                }
                win32::menu_sep(ui);
                win32::menu_caption(ui, tr("ui.prefs.unidades"));
                for u in crate::prefs::TimeUnit::ALL {
                    let r = win32::list_row(ui, self.prefs.unidades == u, tr(u.key()), "", true);
                    self.hint_key(&r, "ui.ayuda.unidades");
                    set(&mut a, r.clicked(), A::Units(u));
                }
            }
            _ => {
                set(&mut a, self.item(ui, "ui.menu.atajos", &t!("ui.atajo.tecla.f1"), "ui.ayuda.atajos", true), A::Shortcuts);
                set(&mut a, self.item(ui, "ui.menu.manual", "", "ui.ayuda.manual", true), A::Manual);
                win32::menu_sep(ui);
                set(&mut a, self.item(ui, "ui.menu.acerca", "", "ui.ayuda.acerca", true), A::About);
            }
        }
        a
    }

    // --- Toolbar -------------------------------------------------------------------------

    pub(crate) fn toolbar(&mut self, ui: &mut Ui) {
        use Action as A;
        let sel = self.selected.is_some();
        let mut act = None;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 1.0;
            let groups: [&[(Icon, &str, Action, bool)]; 4] = [
                &[
                    (Icon::New, "ui.barra.nueva", A::NewSession, true),
                    (Icon::Open, "ui.barra.abrir", A::OpenSession, true),
                    (Icon::Save, "ui.barra.guardar", A::SavePatch, true),
                    (Icon::Import, "ui.barra.importar", A::Import, true),
                    (Icon::Undo, "ui.barra.deshacer", A::Undo, !self.undo.is_empty()),
                ],
                &[
                    (Icon::Render, "ui.barra.render", A::RunCell, sel && self.run.is_none()),
                    (Icon::Cascade, "ui.barra.cascada", A::RunAll, self.run.is_none()),
                ],
                &[(Icon::Play, "ui.barra.reproducir", A::Play, sel), (Icon::Stop, "ui.barra.detener", A::Stop, self.player.is_playing())],
                &[(Icon::Console, "ui.barra.consola", A::Console, true), (Icon::Prefs, "ui.barra.preferencias", A::Prefs, true)],
            ];
            for (g, items) in groups.iter().enumerate() {
                if g > 0 {
                    let (r, _) = ui.allocate_exact_size(vec2(8.0, 22.0), Sense::hover());
                    win32::vsep(ui.painter(), r.center().x - 1.0, r.top() + 2.0, r.bottom() - 2.0);
                }
                for (icon, key, a, en) in items.iter() {
                    let active = matches!(a, A::Console) && self.page == Page::Console;
                    let r = win32::tool_button(ui, *icon, tr(key), *en, active);
                    self.hint_key(&r, key);
                    if r.clicked() {
                        act = Some(a.clone());
                    }
                }
            }
            // Progreso del render a la derecha.
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| self.run_progress(ui));
        });
        if let Some(a) = act {
            self.do_action(ui.ctx(), a);
        }
    }

    /// Barra de progreso del render en curso y su botón Cancelar.
    pub(crate) fn run_progress(&mut self, ui: &mut Ui) {
        let Some(run) = &self.run else { return };
        let prog = run.task.progress();
        let step = run.status.step.load(std::sync::atomic::Ordering::Relaxed) as usize + 1;
        let cancelling = run.task.is_cancelling();
        let text = if cancelling {
            t!("ui.proceso.cancelando").to_string()
        } else {
            t!("ui.proceso.progreso_plan", paso = step.min(run.queue.len()), total = run.queue.len(), porcentaje = format!("{:.0}", prog * 100.0))
        };
        let b = win32::button_w(ui, tr("ui.proceso.cancelar"), 70.0, !cancelling);
        self.hint_key(&b, "ui.ayuda.cancelar");
        if b.clicked() {
            if let Some(r) = &self.run {
                r.task.cancel();
            }
        }
        let w = 220.0f32.min(ui.available_width() - 4.0).max(60.0);
        let (rect, _) = ui.allocate_exact_size(vec2(w, 18.0), Sense::hover());
        let p = ui.painter();
        p.rect_filled(rect, 0.0, theme::t().FIELD);
        win32::bevel(p, rect, Bevel::ThinSunken);
        let fill = Rect::from_min_size(rect.min + vec2(2.0, 2.0), vec2((rect.width() - 4.0) * prog, rect.height() - 4.0));
        p.rect_filled(fill, 0.0, theme::t().HIGHLIGHT);
        let clip = p.with_clip_rect(rect.shrink(1.0));
        clip.text(rect.center(), Align2::CENTER_CENTER, &text, ui_font(), theme::t().TEXT);
        // El texto sobre la parte llena va en blanco.
        p.with_clip_rect(fill).text(rect.center(), Align2::CENTER_CENTER, &text, ui_font(), theme::t().HIGHLIGHT_TEXT);
    }

    // --- Línea de ayuda y barra de estado ------------------------------------------------------

    pub(crate) fn help_line(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.set_min_height(win32::ROW_H);
            if !self.help.is_empty() {
                win32::label(ui, self.help.replace('\n', " · "));
                return;
            }
            match self.status.clone() {
                Some(m) => {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let b = win32::title_button(ui, Icon::Close, tr("ui.estado.cerrar"));
                        if b.clicked() {
                            self.status = None;
                        }
                        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                            let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                            win32::paint_level_icon(ui.painter(), r.center(), 13.0, m.level);
                            let word = if m.level == Level::Info { String::new() } else { format!("{} ", tr(m.level.label_key())) };
                            win32::label(ui, m.level.rich(format!("{word}{}", m.text())));
                        });
                    });
                }
                None => {
                    win32::muted(ui, tr("ui.ayuda.defecto"));
                }
            }
        });
    }

    pub(crate) fn status_bar(&mut self, ui: &mut Ui) {
        let none = t!("ui.estado.nada").to_string();
        let (cell, in1, in2, out, cmd) = match self.selected {
            Some(id) => {
                let label = self.session.as_ref().and_then(|s| s.patch.cell_label(id)).unwrap_or_default();
                let ins: Vec<_> = self.session.as_ref().and_then(|s| s.patch.node(id)).map(|n| n.entradas.clone()).unwrap_or_default();
                let f = |i: usize| ins.get(i).map(|p| self.cell_file(p.nodo, p.salida)).unwrap_or_else(|| none.clone());
                let cmd = self.cell_command(id).join(" · ");
                (label, f(0), f(1), self.cell_file(id, 0), cmd)
            }
            None => {
                let (r, c) = self.cursor_cell;
                (format!("{}_{c}", surshape_patch::row_name(r)), none.clone(), none.clone(), none.clone(), String::new())
            }
        };
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let parts = [
                t!("ui.estado.celda", celda = cell),
                t!("ui.estado.entrada", archivo = in1),
                t!("ui.estado.entrada2", archivo = in2),
                t!("ui.estado.salida", archivo = out),
            ];
            for p in &parts {
                let w = win32::status_width(ui, p).min(ui.available_width() * 0.25);
                win32::status_panel(ui, p, w);
            }
            let rest = ui.available_width();
            let r = win32::status_panel(ui, &cmd, rest);
            self.hint_key(&r, "ui.ayuda.comando");
        });
    }

    /// Esquina de una página: la barra de título con [X] para volver.
    pub(crate) fn page_title(&mut self, ui: &mut Ui, text: &str) -> bool {
        let mut back = false;
        win32::title_bar(ui, text, |ui| {
            let b = win32::title_button(ui, Icon::Close, tr("ui.pagina.volver"));
            if b.clicked() {
                back = true;
            }
        });
        back
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mnemonics_are_unique_in_both_languages() {
        for lang in Lang::ALL {
            let mut seen = std::collections::HashSet::new();
            for k in MENUS {
                let (text, m) = mnemonic(surshape_i18n::tr_in(lang, k));
                let (_, c) = m.unwrap_or_else(|| panic!("{k} ({lang:?}) sin letra de acceso: {text}"));
                assert!(seen.insert(c.to_lowercase().to_string()), "letra repetida '{c}' en {k} ({lang:?})");
            }
        }
        assert_eq!(mnemonic("Pa&tch"), ("Patch".to_string(), Some((2, 't'))));
    }
}
