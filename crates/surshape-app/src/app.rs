//! Estado de la aplicación, tareas en segundo plano y armado de la ventana.
//!
//! Todo el trabajo pesado (decodificar, calcular hashes, renderizar, escribir
//! WAV, diálogos de archivo) corre en hilos aparte ([`Task`]); aquí solo se
//! consultan en cada cuadro. Los mensajes se guardan como clave i18n +
//! argumentos y se traducen al dibujar, así el cambio de idioma en caliente
//! los alcanza también.
//!
//! La ventana se arma con paneles de egui que llenan el espacio (nada de
//! lienzo fijo): menús, toolbar, barra lateral, visor + transporte + botones
//! rápidos, grilla tipo planilla, línea de ayuda y barra de estado. Cada
//! zona está en su módulo: `shell.rs` (menús, toolbar, estado), `sidebar.rs`,
//! `mainpage.rs` (visor y transporte), `grid.rs` (planilla), `page.rs`
//! (parámetros y mezcla), `bpedit.rs` (Graph-Edit), `console.rs`,
//! `windows.rs` (diálogos) y `ops.rs` (operaciones de celda y de patch).

use crate::credits::{self, Credit};
use crate::player::Player;
use crate::prefs::Prefs;
use crate::theme;
use crate::viewer::ViewState;
use crate::win32::Level;
use crate::Loaded;
use eframe::egui;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use surshape_audio::limiter::LimiterSettings;
use surshape_audio::{decode, export, Analysis, AudioBuf};
use surshape_cdp::CdpInstall;
use surshape_engine::{ProcessError, Registry, RenderOptions, Task};
use surshape_i18n::tr_args;
use surshape_patch::{Keys, NodeError, NodeId, NodeKind, Notice, Patch, SourceInfo};
use surshape_session::runner::{self, port_key, RunResult, RunStatus};
use surshape_session::{file_stamp, hash_file, now_secs, Session};

/// Espera tras el último cambio antes de guardar `sesion.json`.
const AUTOSAVE_DELAY: Duration = Duration::from_millis(800);
/// Pasos de deshacer que se guardan.
const UNDO_MAX: usize = 60;

/// Mensaje traducible: se guarda la clave y se traduce al dibujar.
#[derive(Clone, Debug)]
pub(crate) struct Msg {
    pub level: Level,
    pub key: String,
    pub args: Vec<(String, String)>,
}

impl Msg {
    pub fn new(level: Level, key: &str) -> Self {
        Self { level, key: key.to_string(), args: Vec::new() }
    }
    pub fn arg(mut self, name: &str, v: impl ToString) -> Self {
        self.args.push((name.to_string(), v.to_string()));
        self
    }
    pub fn from_err(e: &ProcessError) -> Self {
        let (key, args) = e.message();
        let level = if matches!(e, ProcessError::Cancelled) { Level::Info } else { Level::Error };
        Self { level, key: key.to_string(), args: args.into_iter().map(|(k, v)| (k.to_string(), v)).collect() }
    }
    pub fn from_notice(level: Level, n: &Notice) -> Self {
        Self { level, key: n.mensaje.clone(), args: n.args.clone() }
    }
    pub fn text(&self) -> String {
        let args: Vec<(&str, String)> = self.args.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
        tr_args(&self.key, &args)
    }
}

pub(crate) enum DialogResult {
    Import(Vec<PathBuf>),
    Export(PathBuf, PathBuf),
    OpenSession(PathBuf),
    SessionsDir(PathBuf),
    CdpDir(PathBuf),
    TempDir(PathBuf),
    Editor(PathBuf),
    /// Breakpoints: importar de un archivo de texto / exportar este texto.
    BpImport(PathBuf),
    BpExport(PathBuf, String),
    /// Otra fuente para una celda fuente (re-ejecutar el patch).
    ReplaceSource(NodeId, PathBuf),
    /// Copiar la cadena de una fila sobre otra fuente.
    DuplicateRow(usize, PathBuf),
    /// Patch de texto: abrir / guardar (con o sin las fuentes).
    LoadPatch(PathBuf),
    SavePatch(PathBuf),
    ExportBat(PathBuf),
    /// Guardar el texto de la consola o de Info.
    SaveText(PathBuf, String),
    /// Etiquetas de Audacity para los marcadores de una fuente.
    ImportLabels(NodeId, PathBuf),
    /// Bulk: la cadena de la fila aplicada a cada archivo.
    Bulk(usize, Vec<PathBuf>),
    /// Guardar el final de cada fila en una carpeta.
    ExportRows(PathBuf),
}

/// Qué hacer con un archivo importado.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ImportTarget {
    NewRow,
    Replace(NodeId),
    DuplicateRow(usize),
}

/// Qué muestra el visor de una celda de proceso.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ViewMode {
    Resultado,
    Entrada,
}

/// Cómo se dibuja el audio en el visor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ViewKind {
    Onda,
    Espectrograma,
}

/// Página del área central.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Page {
    /// Visor, transporte y grilla.
    Main,
    /// Parámetros de una celda.
    Params(NodeId),
    /// Graph-Edit del parámetro que dice `bp_edit`.
    Graph,
    /// Mezcla.
    Mix(NodeId),
    Console,
}

/// Diálogo modal abierto.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Dialog {
    Prefs,
    About,
    Shortcuts,
    CellInfo(NodeId),
    /// Texto (Info de CDP...): título y contenido.
    Text(String, String),
    /// Confirmar una operación.
    Confirm(Confirm),
    /// Nombre para guardar una receta.
    RecipeName(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Confirm {
    /// Borrar las celdas desde esta hasta el final de su fila.
    ClearFrom(NodeId),
    /// Borrar esta celda y las que dependen de ella.
    DeleteWithDependents(NodeId),
}

/// Opciones de la sección "Controles de patch/celda".
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct CellOpts {
    pub incluir_fuentes: bool,
    pub fila_unica: bool,
    pub patch_completo: bool,
    pub copiar_fila: bool,
    /// Bulk: los archivos importados reciben la cadena de la fila elegida.
    pub bulk: bool,
}

/// Columnas y filas del espectrograma (textura por canal).
const SPECTRO_COLS: usize = 2048;
const SPECTRO_ROWS: usize = 256;

/// Plan de render en curso.
pub(crate) struct RunJob {
    pub task: Task<RunResult>,
    pub status: Arc<RunStatus>,
    /// Nodos del plan, en orden.
    pub queue: Vec<NodeId>,
    /// Qué hacer al terminar (desde la página de parámetros).
    pub after: AfterRun,
}

/// Qué hacer cuando termina un render pedido desde la página de parámetros.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AfterRun {
    Nothing,
    /// RENDER: volver a la principal (según preferencias).
    BackToMain,
    /// Previsualizar: escuchar el resultado de la celda.
    Play(NodeId),
}

impl RunJob {
    /// Nodos del plan ya terminados.
    pub fn done_nodes(&self) -> &[NodeId] {
        let i = (self.status.step.load(std::sync::atomic::Ordering::Relaxed) as usize).min(self.queue.len());
        &self.queue[..i]
    }
}

/// Edición en un editor externo en curso.
pub(crate) struct ExtEdit {
    pub file: PathBuf,
    pub stamp: Option<(u64, u64)>,
    pub task: Task<()>,
}

/// Resultado de verificar una fuente al abrir la sesión.
pub(crate) enum SourceCheck {
    Ok,
    Missing,
    Changed { hash: String, bytes: u64, modificado: u64 },
}

pub struct App {
    pub(crate) prefs: Prefs,
    pub(crate) registry: Registry,
    pub(crate) session: Option<Session>,
    /// Audio en memoria por clave de salida (`<clave>_<n>`).
    pub(crate) mem: HashMap<String, Loaded>,
    pub(crate) loads: Vec<(String, Task<Loaded>)>,
    pub(crate) imports: Vec<(PathBuf, ImportTarget, Task<(Loaded, SourceInfo)>)>,
    pub(crate) verify: Option<Task<Vec<(NodeId, SourceCheck)>>>,
    pub(crate) run: Option<RunJob>,
    pub(crate) exports: Vec<Task<(PathBuf, bool)>>,
    pub(crate) dialog: Option<Task<Option<DialogResult>>>,
    /// Celda elegida (con nodo) y posición elegida en la planilla (puede ser
    /// una celda vacía: ahí van los procesos nuevos).
    pub(crate) selected: Option<NodeId>,
    pub(crate) cursor_cell: (usize, usize),
    /// Desplazamiento de la planilla y si hay que llevar la celda elegida a
    /// la vista.
    pub(crate) grid_scroll: (f32, f32),
    pub(crate) grid_follow: bool,
    pub(crate) page: Page,
    pub(crate) view: ViewState,
    pub(crate) view_mode: ViewMode,
    pub(crate) view_kind: ViewKind,
    /// Zoom vertical del visor (1 = normal).
    pub(crate) vzoom: f32,
    /// Canales ocultos en el visor (bit n = canal n).
    pub(crate) hidden_ch: u64,
    /// Espectrogramas listos (una textura por canal), por clave de salida.
    pub(crate) spectros: HashMap<String, Vec<egui::TextureHandle>>,
    spectro_loads: Vec<(String, Task<Vec<egui::ColorImage>>)>,
    pub(crate) looping: bool,
    /// Eligiendo en la grilla la entrada `slot` del nodo (y a qué página
    /// volver después).
    pub(crate) pick: Option<(NodeId, usize)>,
    pub(crate) pick_return: Option<Page>,
    /// Canal que se edita cuando el nodo usa valores por canal.
    pub(crate) chan_tab: usize,
    /// Pestaña de la página de parámetros.
    pub(crate) param_tab: usize,
    pub(crate) bp_edit: Option<crate::bpedit::BpEdit>,
    pub(crate) previews: Option<crate::extras::Previews>,
    pub(crate) variant_cfg: crate::extras::VariantCfg,
    pub(crate) preset_sel: String,
    pub(crate) preset_name: String,
    /// Celdas copiadas (para pegar en otra fila).
    pub(crate) clipboard: Vec<surshape_patch::NodeTemplate>,
    /// Tramo elegido con Mayús+clic: (fila, desde, hasta) en columnas de la
    /// planilla.
    pub(crate) range_sel: Option<(usize, usize, usize)>,
    /// Salida del nodo elegido que muestra el visor (varias salidas).
    pub(crate) view_salida: u16,
    pub(crate) recipe_name: String,
    pub(crate) player: Player,
    pub(crate) status: Option<Msg>,
    pub(crate) help: String,
    pub(crate) modal: Option<Dialog>,
    /// Borrador de las preferencias mientras el diálogo está abierto, y su
    /// pestaña.
    pub(crate) prefs_draft: Option<Prefs>,
    pub(crate) prefs_tab: usize,
    pub(crate) cdp: Option<CdpInstall>,
    pub(crate) exe_dir: Option<PathBuf>,
    pub(crate) overrides: Vec<PathBuf>,
    pub(crate) credits: Vec<Credit>,
    rng: u64,
    dirty_at: Option<Instant>,
    /// Fuentes cuyo archivo no está.
    pub(crate) missing: HashSet<NodeId>,
    /// Claves de cache de todos los nodos (se recalculan cada cuadro).
    pub(crate) keys: Keys,
    /// Menú abierto de la barra de menús y dónde quedó cada título.
    pub(crate) open_menu: Option<usize>,
    pub(crate) menu_rects: Vec<egui::Rect>,
    /// Deshacer: estados anteriores del patch.
    pub(crate) undo: Vec<Patch>,
    /// Nodo cuyos parámetros se están editando (agrupa un deshacer).
    pub(crate) editing: Option<NodeId>,
    /// Reemplazar: el próximo proceso elegido reemplaza esta celda.
    pub(crate) replace: Option<NodeId>,
    pub(crate) side_search: String,
    pub(crate) cell_opts: CellOpts,
    /// "Procesar: selección" (si no, archivo completo).
    pub(crate) procesar_sel: bool,
    pub(crate) save_name: String,
    /// Consola: ejecución elegida.
    pub(crate) console_sel: Option<u64>,
    pub(crate) ext_edits: Vec<ExtEdit>,
    pub(crate) info_task: Option<(String, Task<String>)>,
    /// Firma de los archivos de celda (para regenerarlos solo si cambian).
    pub(crate) cell_files_sig: u64,
    /// Modo captura de pantallas (`--captura`).
    pub(crate) capture: Option<crate::capture::Capture>,
    /// Zoom de interfaz aplicado.
    zoom_applied: f32,
    /// Clave de audio que hay que reproducir en cuanto esté cargada.
    pub(crate) pending_play: Option<String>,
}

/// Fecha y hora UTC "AAAAMMDD_HHMMSS" para nombrar sesiones nuevas.
fn timestamp() -> String {
    let s = now_secs();
    let days = (s / 86_400) as i64;
    let rem = s % 86_400;
    // Días -> fecha civil (algoritmo de H. Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}{m:02}{d:02}_{:02}{:02}{:02}", rem / 3600, (rem % 3600) / 60, rem % 60)
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, prefs: Prefs, exe_dir: Option<PathBuf>, overrides: Vec<PathBuf>) -> Self {
        Self::with_ctx(&cc.egui_ctx, prefs, exe_dir, overrides, None)
    }

    /// Crea la app sobre un contexto de egui. `session`: carpeta de sesión a
    /// abrir (o crear) en lugar de la última (tests y capturas).
    pub fn with_ctx(ctx: &egui::Context, prefs: Prefs, exe_dir: Option<PathBuf>, overrides: Vec<PathBuf>, session: Option<PathBuf>) -> Self {
        theme::apply(ctx);
        let rng = (now_secs() ^ 0x5EED_5EED_1234_5678) | 1;
        if let Some(t) = &prefs.carpeta_temporal {
            // Carpeta de trabajo de los renders (CDP escribe ahí).
            if std::fs::create_dir_all(t).is_ok() {
                std::env::set_var("TMP", t); // i18n-ok
                std::env::set_var("TEMP", t); // i18n-ok
            }
        }
        let mut app = Self {
            prefs,
            registry: Registry::new(),
            session: None,
            mem: HashMap::new(),
            loads: Vec::new(),
            imports: Vec::new(),
            verify: None,
            run: None,
            exports: Vec::new(),
            dialog: None,
            selected: None,
            cursor_cell: (0, 0),
            grid_scroll: (0.0, 0.0),
            grid_follow: false,
            page: Page::Main,
            view: ViewState::default(),
            view_mode: ViewMode::Resultado,
            view_kind: ViewKind::Onda,
            vzoom: 1.0,
            hidden_ch: 0,
            spectros: HashMap::new(),
            spectro_loads: Vec::new(),
            looping: false,
            pick: None,
            pick_return: None,
            chan_tab: 0,
            param_tab: 0,
            bp_edit: None,
            previews: None,
            variant_cfg: Default::default(),
            preset_sel: String::new(),
            preset_name: String::new(),
            clipboard: Vec::new(),
            range_sel: None,
            view_salida: 0,
            recipe_name: String::new(),
            player: Player::default(),
            status: None,
            help: String::new(),
            modal: None,
            prefs_draft: None,
            prefs_tab: 0,
            cdp: None,
            exe_dir,
            overrides,
            credits: credits::all(),
            rng,
            dirty_at: None,
            missing: HashSet::new(),
            keys: Keys::new(),
            open_menu: None,
            menu_rects: Vec::new(),
            undo: Vec::new(),
            editing: None,
            replace: None,
            side_search: String::new(),
            cell_opts: CellOpts::default(),
            procesar_sel: false,
            save_name: String::new(),
            console_sel: None,
            ext_edits: Vec::new(),
            info_task: None,
            cell_files_sig: 0,
            capture: None,
            zoom_applied: 0.0,
            pending_play: None,
        };
        app.player.set_device(app.prefs.dispositivo.clone());
        app.rebuild_registry();
        match session {
            Some(dir) => match Session::open(&dir).or_else(|_| Session::create(&dir)) {
                Ok(s) => app.set_session(s),
                Err(e) => app.status = Some(Msg::new(Level::Error, e.key).arg("detalle", e.detalle)),
            },
            // Reabrir la última sesión; si no hay (o es de un formato viejo),
            // crear una nueva y avisar.
            None => match app.prefs.ultima_sesion.clone().map(|d| Session::open(&d)) {
                Some(Ok(s)) => app.set_session(s),
                Some(Err(e)) => {
                    app.new_session();
                    app.status = Some(Msg::new(Level::Warning, e.key).arg("detalle", e.detalle));
                }
                None => app.new_session(),
            },
        }
        app
    }

    /// Catálogo: procesos nativos + CDP (si está instalado). Se rehace al
    /// cambiar la carpeta de CDP; las celdas de procesos que ya no existan
    /// muestran un error claro.
    pub(crate) fn rebuild_registry(&mut self) {
        let mut reg = Registry::new();
        surshape_native::register(&mut reg);
        self.cdp = CdpInstall::find(self.prefs.carpeta_cdp.as_deref(), self.exe_dir.as_deref()).or_else(|| {
            // Al desarrollar: la compilación propia de CDP.
            let local = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("third_party").join("cdp-bin"); // i18n-ok
            CdpInstall::discover(&local)
        });
        if let Some(c) = &self.cdp {
            surshape_cdp::procs::register(&mut reg, &Arc::new(c.clone()));
        }
        self.registry = reg;
    }

    pub(crate) fn next_seed(&mut self) -> u64 {
        // xorshift64
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng = x;
        x % 1_000_000
    }

    pub(crate) fn save_prefs(&mut self) {
        if let Err(e) = self.prefs.save() {
            self.status = Some(Msg::new(Level::Warning, "err.prefs.guardar").arg("detalle", e));
        }
    }

    pub(crate) fn render_options(&self) -> RenderOptions {
        let limiter =
            self.prefs.limitador.then_some(LimiterSettings { ceiling_db: self.prefs.techo_db, ..Default::default() });
        RenderOptions { limiter, pvoc: self.prefs.pvoc(), ..Default::default() }
    }

    // --- Sesión -------------------------------------------------------------

    pub(crate) fn set_session(&mut self, s: Session) {
        self.player.stop();
        self.mem.clear();
        self.loads.clear();
        self.spectros.clear();
        self.spectro_loads.clear();
        self.imports.clear();
        self.selected = None;
        self.cursor_cell = (0, 0);
        self.page = Page::Main;
        self.view = ViewState::default();
        self.missing.clear();
        self.undo.clear();
        self.cell_files_sig = 0;
        self.prefs.ultima_sesion = Some(s.dir.clone());
        self.start_verify(&s);
        let first = s.patch.filas.first().and_then(|r| r.celdas.first().copied());
        self.session = Some(s);
        self.save_prefs();
        if let Some(id) = first {
            self.select(id);
        }
    }

    /// Verifica en segundo plano que las fuentes sigan iguales.
    pub(crate) fn start_verify(&mut self, s: &Session) {
        let sources: Vec<(NodeId, SourceInfo)> =
            s.patch.nodos.iter().filter_map(|n| n.source().map(|src| (n.id, src.clone()))).collect();
        self.verify = Some(Task::spawn("surshape-verificar", move |_, c| { // i18n-ok
            let mut out = Vec::new();
            for (id, src) in sources {
                let check = match file_stamp(&src.ruta) {
                    None => SourceCheck::Missing,
                    Some((b, m)) if b == src.bytes && m == src.modificado && !src.hash.is_empty() => SourceCheck::Ok,
                    Some((bytes, modificado)) => match hash_file(&src.ruta, None, Some(c)) {
                        Ok(hash) => SourceCheck::Changed { hash, bytes, modificado },
                        Err(_) => SourceCheck::Missing,
                    },
                };
                out.push((id, check));
            }
            Ok(out)
        }));
    }

    pub(crate) fn new_session(&mut self) {
        let dir = self.prefs.sessions_dir().join(format!("sesion_{}", timestamp())); // i18n-ok
        match Session::create(&dir) {
            Ok(s) => {
                self.set_session(s);
                self.status = Some(Msg::new(Level::Info, "ui.estado.sesion_nueva").arg("carpeta", dir.display()));
            }
            Err(e) => self.status = Some(Msg::new(Level::Error, e.key).arg("detalle", e.detalle)),
        }
    }

    /// Guarda ya (cambios estructurales: celdas, renders).
    pub(crate) fn save_now(&mut self) {
        self.dirty_at = None;
        self.editing = None;
        if let Some(Err(e)) = self.session.as_ref().map(|s| s.save()) {
            self.status = Some(Msg::new(Level::Error, e.key).arg("detalle", e.detalle));
        }
        self.sync_cell_files();
    }

    /// Guarda en breve (cambios de parámetros: evita escribir en cada cuadro
    /// mientras se arrastra un control).
    pub(crate) fn mark_dirty(&mut self) {
        self.dirty_at = Some(Instant::now());
    }

    /// Guarda el estado actual del patch para poder deshacer.
    pub(crate) fn push_undo(&mut self) {
        if let Some(s) = &self.session {
            if self.undo.last() != Some(&s.patch) {
                self.undo.push(s.patch.clone());
                if self.undo.len() > UNDO_MAX {
                    self.undo.remove(0);
                }
            }
        }
    }

    /// Antes de cambiar parámetros de `id`: un solo paso de deshacer por
    /// tanda de cambios.
    pub(crate) fn before_param_edit(&mut self, id: NodeId) {
        if self.editing != Some(id) {
            self.push_undo();
            self.editing = Some(id);
        }
    }

    /// Deshace el último cambio del patch (los renders vuelven al instante
    /// desde la cache).
    pub(crate) fn undo_last(&mut self) {
        let Some(prev) = self.undo.pop() else {
            self.status = Some(Msg::new(Level::Info, "ui.estado.nada_que_deshacer"));
            return;
        };
        if let Some(s) = self.session.as_mut() {
            s.patch = prev;
        }
        self.editing = None;
        if self.selected.is_some_and(|id| self.session.as_ref().and_then(|s| s.patch.node(id)).is_none()) {
            self.selected = None;
            self.page = Page::Main;
        }
        self.save_now();
        self.status = Some(Msg::new(Level::Info, "ui.estado.deshecho"));
    }

    pub(crate) fn select(&mut self, id: NodeId) {
        if let Some(pos) = self.session.as_ref().and_then(|s| s.patch.grid_pos(id)) {
            self.cursor_cell = pos;
        }
        if self.selected == Some(id) {
            return;
        }
        // Solo celdas que existen (p. ej. tras quitar una o abrir otra sesión).
        if self.session.as_ref().and_then(|s| s.patch.node(id)).is_none() {
            return;
        }
        self.player.stop();
        self.selected = Some(id);
        self.editing = None;
        self.view_salida = 0;
        self.preset_sel.clear();
        if self.bp_edit.as_ref().is_some_and(|e| e.node != id) {
            self.bp_edit = None;
        }
        self.view = ViewState::default();
        let rendered = self.session.as_ref().and_then(|s| s.patch.node(id)).is_some_and(|n| n.render.is_some());
        self.view_mode = if rendered { ViewMode::Resultado } else { ViewMode::Entrada };
        self.save_name = self.export_name();
    }

    pub fn import(&mut self, paths: Vec<PathBuf>) {
        // Con "Bulk", cada archivo nuevo recibe la cadena de la fila elegida.
        let bulk_row = self.cell_opts.bulk.then_some(self.cursor_cell.0).filter(|r| self.session.as_ref().is_some_and(|s| *r < s.patch.filas.len()));
        for p in paths {
            match bulk_row {
                Some(r) => self.import_as(p, ImportTarget::DuplicateRow(r)),
                None => self.import_as(p, ImportTarget::NewRow),
            }
        }
    }

    pub(crate) fn import_as(&mut self, p: PathBuf, target: ImportTarget) {
        if self.session.is_none() {
            self.status = Some(Msg::new(Level::Error, "err.sesion.ninguna"));
            return;
        }
        if p.is_dir() {
            return;
        }
        if !decode::is_supported(&p) {
            self.status = Some(Msg::new(Level::Error, "err.import.formato").arg("archivo", p.display()));
            return;
        }
        let new_rows = self.imports.iter().filter(|(_, t, _)| !matches!(t, ImportTarget::Replace(_))).count();
        if !matches!(target, ImportTarget::Replace(_)) && self.session.as_ref().is_some_and(|s| s.patch.filas.len() + new_rows >= surshape_patch::MAX_ROWS) {
            self.status = Some(Msg::new(Level::Warning, "err.grilla.filas").arg("n", surshape_patch::MAX_ROWS));
            return;
        }
        if let Some(dir) = p.parent() {
            self.prefs.ultima_carpeta = Some(dir.to_path_buf());
        }
        self.prefs.add_recent(p.clone());
        let path = p.clone();
        let task = Task::spawn("surshape-importar", move |pr, c| { // i18n-ok
            let (bytes, modificado) = file_stamp(&path).unwrap_or_default();
            let hash = hash_file(&path, None, Some(c))?;
            let audio = decode::load_with(&path, Some(pr), Some(c))?;
            let info = SourceInfo {
                ruta: path.clone(),
                hash,
                sr: audio.sr,
                canales: audio.num_channels() as u16,
                frames: audio.frames() as u64,
                bytes,
                modificado,
                marcadores: Vec::new(),
            };
            Ok((Loaded::new(audio), info))
        });
        self.imports.push((p, target, task));
        self.save_prefs();
    }

    // --- Lo que muestra el visor ------------------------------------------------

    /// (clave de salida, archivo) del audio que corresponde mostrar.
    pub(crate) fn view_target(&self) -> Option<(String, PathBuf)> {
        let id = self.selected?;
        let n = self.session.as_ref()?.patch.node(id)?;
        match (&n.tipo, self.view_mode) {
            (NodeKind::Fuente(_), _) => self.port_target(id, 0),
            (_, ViewMode::Resultado) => self.port_target(id, self.view_salida),
            (_, ViewMode::Entrada) => n.entradas.first().and_then(|p| self.port_target(p.nodo, p.salida)),
        }
    }

    /// Audio cargado de lo que muestra el visor.
    pub(crate) fn view_loaded(&self) -> Option<&Loaded> {
        self.view_target().and_then(|(k, _)| self.mem.get(&k))
    }

    /// Pide cargar `key` si no está en memoria ni cargándose.
    pub(crate) fn ensure_loaded(&mut self, key: &str, path: &Path) {
        if self.mem.contains_key(key) || self.loads.iter().any(|(k, _)| k == key) {
            return;
        }
        let path = path.to_path_buf();
        let task = Task::spawn("surshape-carga", move |p, c| Ok(Loaded::new(decode::load_with(&path, Some(p), Some(c))?))); // i18n-ok
        self.loads.push((key.to_string(), task));
    }

    /// Pide calcular el espectrograma de `key` (si el audio ya está en memoria).
    pub(crate) fn ensure_spectro(&mut self, key: &str) {
        if self.spectros.contains_key(key) || self.spectro_loads.iter().any(|(k, _)| k == key) {
            return;
        }
        let Some(audio) = self.mem.get(key).map(|l| l.audio.clone()) else { return };
        let task = Task::spawn("surshape-espectro", move |_, c| { // i18n-ok
            let mut imgs = Vec::new();
            for ch in &audio.channels {
                if c.load(std::sync::atomic::Ordering::Relaxed) {
                    return Err(ProcessError::Cancelled);
                }
                let (cols, data) = surshape_native::spectral::spectrogram(
                    ch,
                    audio.sr,
                    SPECTRO_COLS,
                    SPECTRO_ROWS,
                    crate::viewer::SPECTRO_F_MIN,
                );
                let pixels = data.iter().map(|&v| theme::spectro_color(v)).collect();
                imgs.push(egui::ColorImage { size: [cols, SPECTRO_ROWS], pixels });
            }
            Ok(imgs)
        });
        self.spectro_loads.push((key.to_string(), task));
    }

    /// Sube a la GPU los espectrogramas terminados.
    fn poll_spectros(&mut self, ctx: &egui::Context) {
        let mut i = 0;
        while i < self.spectro_loads.len() {
            match self.spectro_loads[i].1.poll() {
                None => i += 1,
                Some(res) => {
                    let (key, _) = self.spectro_loads.remove(i);
                    if let Ok(imgs) = res {
                        let tex = imgs
                            .into_iter()
                            .enumerate()
                            .map(|(c, img)| ctx.load_texture(format!("espectro_{key}_{c}"), img, egui::TextureOptions::LINEAR)) // i18n-ok
                            .collect();
                        self.spectros.insert(key, tex);
                    }
                }
            }
        }
    }

    pub(crate) fn spectro_pending(&self, key: &str) -> bool {
        self.spectro_loads.iter().any(|(k, _)| k == key)
    }

    // --- Render -----------------------------------------------------------------------

    /// Renderiza lo necesario para `targets` (vacío = todo lo desactualizado).
    pub(crate) fn start_run(&mut self, targets: Vec<NodeId>) {
        self.start_run_then(targets, AfterRun::Nothing);
    }

    pub(crate) fn start_run_then(&mut self, targets: Vec<NodeId>, after: AfterRun) {
        if self.run.is_some() {
            return;
        }
        let Some(s) = self.session.as_ref() else { return };
        let opts = self.render_options();
        let keys = s.patch.keys(&self.registry, &opts);
        let plan = s.patch.render_plan(&targets, &keys);
        if plan.is_empty() {
            self.status = Some(Msg::new(Level::Info, "ui.estado.al_dia"));
            self.after_run(after);
            return;
        }
        let steps = runner::build_steps(s, &plan, &keys, &self.registry);
        let preloaded: HashMap<String, Arc<AudioBuf>> = self.mem.iter().map(|(k, l)| (k.clone(), l.audio.clone())).collect();
        let dir = s.dir.clone();
        let status = Arc::new(RunStatus::default());
        let st = status.clone();
        let queue = steps.iter().map(|s| s.node).collect();
        let task = Task::spawn("surshape-render", move |p, c| Ok(runner::run_steps(&dir, &steps, &opts, preloaded, &st, p, c))); // i18n-ok
        self.run = Some(RunJob { task, status, queue, after });
    }

    fn after_run(&mut self, after: AfterRun) {
        match after {
            AfterRun::Nothing => {}
            AfterRun::BackToMain => {
                if self.prefs.volver_tras_render {
                    self.page = Page::Main;
                }
            }
            AfterRun::Play(id) => {
                let ok = self.session.as_ref().and_then(|s| s.patch.node(id)).is_some_and(|n| n.render.is_some());
                if ok {
                    self.view_mode = ViewMode::Resultado;
                    if let Some((k, path)) = self.port_target(id, 0) {
                        self.ensure_loaded(&k, &path);
                        self.pending_play = Some(k);
                    }
                }
            }
        }
    }

    fn finish_run(&mut self, res: RunResult, after: AfterRun) {
        let (mut computed, mut cached, mut errors) = (0, 0, 0);
        let mut last_error = None;
        if let Some(s) = self.session.as_mut() {
            for st in &res.steps {
                // Varias salidas: cada salida extra abre su fila.
                if let Ok(rec) = &st.result {
                    if rec.salidas.len() > 1 {
                        if let Some(n) = s.patch.node_mut(st.node) {
                            n.render = Some(rec.clone());
                        }
                        s.patch.ensure_output_rows(st.node);
                    }
                }
                let Some(n) = s.patch.node_mut(st.node) else { continue };
                match &st.result {
                    Ok(rec) => {
                        n.render = Some(rec.clone());
                        n.error = None;
                        if st.from_cache {
                            cached += 1;
                        } else {
                            computed += 1;
                        }
                    }
                    Err(ProcessError::Cancelled) => {}
                    Err(e) => {
                        let (key, args) = e.message();
                        n.error = Some(NodeError { clave: st.key.clone(), aviso: Notice::new(key, &args) });
                        errors += 1;
                        last_error = Some(Msg::from_err(e));
                    }
                }
            }
        }
        // El audio nuevo ya está en memoria: solo faltan sus picos.
        for (k, a) in res.audio {
            if !self.mem.contains_key(&k) && !self.loads.iter().any(|(x, _)| *x == k) {
                let task = Task::spawn("surshape-picos", move |_, _| Ok(Loaded::from_arc(a))); // i18n-ok
                self.loads.push((k, task));
            }
        }
        self.status = Some(if res.cancelled {
            Msg::new(Level::Info, "ui.estado.render_cancelado")
        } else if let Some(e) = last_error.filter(|_| errors > 0) {
            e
        } else {
            Msg::new(Level::Info, "ui.estado.render_listo").arg("calculados", computed).arg("cache", cached)
        });
        // Al terminar, el visor pasa a mostrar el resultado.
        if self.selected.is_some() && computed + cached > 0 {
            self.view_mode = ViewMode::Resultado;
        }
        self.save_now();
        if errors == 0 && !res.cancelled {
            self.after_run(after);
        }
    }

    // --- Tareas en segundo plano ----------------------------------------------

    fn poll_tasks(&mut self) {
        // Cargas
        let mut i = 0;
        while i < self.loads.len() {
            match self.loads[i].1.poll() {
                None => i += 1,
                Some(res) => {
                    let (key, _) = self.loads.remove(i);
                    match res {
                        Ok(l) => {
                            self.mem.insert(key, l);
                        }
                        Err(e) => self.status = Some(Msg::from_err(&e)),
                    }
                }
            }
        }
        // Escuchar lo que pidió "Previsualizar" en cuanto esté cargado.
        if let Some(k) = self.pending_play.clone() {
            if let Some(l) = self.mem.get(&k) {
                let a = l.audio.clone();
                let n = a.frames();
                self.player.play(a, 0, n, false);
                self.pending_play = None;
            }
        }
        // Importaciones: fila nueva, reemplazo de fuente o fila duplicada.
        let mut i = 0;
        while i < self.imports.len() {
            match self.imports[i].2.poll() {
                None => i += 1,
                Some(res) => {
                    let (path, target, _) = self.imports.remove(i);
                    match res {
                        Ok((loaded, info)) => self.place_source(path, target, loaded, info),
                        Err(e) => self.status = Some(Msg::from_err(&e)),
                    }
                }
            }
        }
        // Verificación de fuentes
        if let Some(res) = self.verify.as_mut().and_then(|t| t.poll()) {
            self.verify = None;
            if let (Ok(list), Some(s)) = (res, self.session.as_mut()) {
                let mut changed = false;
                for (id, check) in list {
                    match check {
                        SourceCheck::Ok => {}
                        SourceCheck::Missing => {
                            self.missing.insert(id);
                        }
                        SourceCheck::Changed { hash, bytes, modificado } => {
                            if let Some(NodeKind::Fuente(src)) = s.patch.node_mut(id).map(|n| &mut n.tipo) {
                                changed |= !src.hash.is_empty() && src.hash != hash;
                                src.hash = hash;
                                src.bytes = bytes;
                                src.modificado = modificado;
                            }
                        }
                    }
                }
                if !self.missing.is_empty() {
                    self.status = Some(Msg::new(Level::Warning, "ui.estado.fuentes_faltan").arg("n", self.missing.len()));
                } else if changed {
                    self.status = Some(Msg::new(Level::Warning, "ui.estado.fuentes_cambiaron"));
                }
                self.save_now();
            }
        }
        // Render
        if let Some(res) = self.run.as_mut().and_then(|r| r.task.poll()) {
            let after = self.run.as_ref().map_or(AfterRun::Nothing, |r| r.after);
            self.run = None;
            match res {
                Ok(r) => self.finish_run(r, after),
                Err(e) => self.status = Some(Msg::from_err(&e)),
            }
        }
        // Exportaciones
        let mut i = 0;
        while i < self.exports.len() {
            match self.exports[i].poll() {
                None => i += 1,
                Some(res) => {
                    self.exports.remove(i);
                    self.status = Some(match res {
                        Ok((path, true)) => Msg::new(Level::Warning, "ui.estado.exportado_recortado").arg("archivo", path.display()),
                        Ok((path, false)) => Msg::new(Level::Info, "ui.estado.exportado").arg("archivo", path.display()),
                        Err(e) => Msg::from_err(&e),
                    });
                }
            }
        }
        // Info de CDP
        if let Some(res) = self.info_task.as_mut().and_then(|(_, t)| t.poll()) {
            let (title, _) = self.info_task.take().expect("tarea de info"); // i18n-ok
            match res {
                Ok(txt) => self.modal = Some(Dialog::Text(title, txt)),
                Err(e) => self.status = Some(Msg::from_err(&e)),
            }
        }
        self.poll_external_edits();
        // Diálogos de archivo
        if let Some(res) = self.dialog.as_mut().and_then(|d| d.poll()) {
            self.dialog = None;
            match res {
                Ok(Some(r)) => self.dialog_result(r),
                Ok(None) => {}
                Err(e) => self.status = Some(Msg::from_err(&e)),
            }
        }
    }

    fn dialog_result(&mut self, r: DialogResult) {
        match r {
            DialogResult::Import(paths) => self.import(paths),
            DialogResult::Export(src, dst) => self.start_export(src, dst),
            DialogResult::OpenSession(dir) => match Session::open(&dir) {
                Ok(s) => self.set_session(s),
                Err(e) => self.status = Some(Msg::new(Level::Error, e.key).arg("detalle", e.detalle)),
            },
            DialogResult::SessionsDir(dir) => self.edit_prefs(|p| p.carpeta_sesiones = Some(dir)),
            DialogResult::TempDir(dir) => self.edit_prefs(|p| p.carpeta_temporal = Some(dir)),
            DialogResult::Editor(exe) => self.edit_prefs(|p| p.editor_externo = Some(exe)),
            DialogResult::CdpDir(dir) => self.edit_prefs(|p| p.carpeta_cdp = Some(dir)),
            DialogResult::BpImport(path) => self.bp_import(&path),
            DialogResult::ReplaceSource(id, path) => self.import_as(path, ImportTarget::Replace(id)),
            DialogResult::DuplicateRow(row, path) => self.import_as(path, ImportTarget::DuplicateRow(row)),
            DialogResult::BpExport(path, txt) | DialogResult::SaveText(path, txt) => {
                if let Err(e) = std::fs::write(&path, txt) {
                    self.status = Some(Msg::new(Level::Error, "err.audio.escribir").arg("archivo", path.display()).arg("detalle", e));
                }
            }
            DialogResult::LoadPatch(path) => self.load_patch_file(&path),
            DialogResult::SavePatch(path) => self.save_patch_file(&path),
            DialogResult::ExportBat(path) => self.export_bat(&path),
            DialogResult::ImportLabels(src, path) => self.import_labels(src, &path),
            DialogResult::Bulk(row, paths) => {
                for p in paths {
                    self.import_as(p, ImportTarget::DuplicateRow(row));
                }
            }
            DialogResult::ExportRows(dir) => self.export_rows(&dir),
        }
    }

    /// Cambia una preferencia: en el borrador si el diálogo está abierto
    /// (se aplica con Aceptar), si no, directamente.
    fn edit_prefs(&mut self, f: impl FnOnce(&mut Prefs)) {
        match self.prefs_draft.as_mut() {
            Some(d) => f(d),
            None => {
                let mut p = self.prefs.clone();
                f(&mut p);
                self.apply_prefs(p);
            }
        }
    }

    /// Aplica preferencias nuevas (rehace lo que dependa de ellas).
    pub(crate) fn apply_prefs(&mut self, p: Prefs) {
        let cdp_changed = p.carpeta_cdp != self.prefs.carpeta_cdp;
        let lang_changed = p.idioma != self.prefs.idioma;
        let dev_changed = p.dispositivo != self.prefs.dispositivo;
        self.prefs = p;
        if lang_changed {
            surshape_i18n::set_lang(self.prefs.lang());
        }
        if dev_changed {
            self.player.set_device(self.prefs.dispositivo.clone());
        }
        if cdp_changed {
            self.rebuild_registry();
            if self.cdp.is_none() {
                self.status = Some(Msg::new(Level::Warning, "ui.prefs.cdp_no_encontrado"));
            }
        }
        self.save_prefs();
    }

    fn place_source(&mut self, path: PathBuf, target: ImportTarget, loaded: Loaded, info: SourceInfo) {
        match target {
            ImportTarget::NewRow => self.add_source(path, loaded, info),
            ImportTarget::Replace(id) => {
                let opts = self.render_options();
                self.push_undo();
                let Some(s) = self.session.as_mut() else { return };
                if s.patch.set_source(id, info) {
                    self.missing.remove(&id);
                    if let Some(Ok(k)) = s.patch.keys(&self.registry, &opts).get(&id) {
                        self.mem.insert(port_key(k, 0), loaded);
                    }
                    self.save_now();
                    self.status = Some(Msg::new(Level::Info, "ui.estado.fuente_cambiada").arg("archivo", path.display()));
                }
            }
            ImportTarget::DuplicateRow(row) => {
                let chain = self.session.as_ref().and_then(|s| s.patch.recipe_from_row(row, "")).map(|r| r.pasos).unwrap_or_default();
                self.add_source(path, loaded, info);
                if let Some(s) = self.session.as_mut() {
                    let new_row = s.patch.filas.len() - 1;
                    s.patch.paste(new_row, &chain);
                    self.save_now();
                }
            }
        }
    }

    fn add_source(&mut self, path: PathBuf, loaded: Loaded, info: SourceInfo) {
        let opts = self.render_options();
        self.push_undo();
        let Some(s) = self.session.as_mut() else { return };
        let (id, _) = s.patch.add_source(info);
        if let Some(Ok(k)) = s.patch.keys(&self.registry, &opts).get(&id) {
            self.mem.insert(port_key(k, 0), loaded);
        }
        self.save_now();
        self.select(id);
        self.status = Some(Msg::new(Level::Info, "ui.estado.importado").arg("archivo", path.display()));
    }

    pub(crate) fn start_export(&mut self, src: PathBuf, path: PathBuf) {
        let Some(s) = self.session.as_ref() else { return };
        // Nunca se sobrescribe un original ni un archivo de la sesión.
        let same = |a: &Path, b: &Path| match (a.canonicalize(), b.canonicalize()) {
            (Ok(a), Ok(b)) => a == b,
            _ => false,
        };
        let is_source = s.patch.nodos.iter().filter_map(|n| n.source()).any(|x| same(&x.ruta, &path));
        if is_source || path.canonicalize().is_ok_and(|p| p.starts_with(s.dir.canonicalize().unwrap_or_default())) {
            self.status = Some(Msg::new(Level::Error, "err.export.original").arg("archivo", path.display()));
            return;
        }
        let fmt = self.prefs.formato_export;
        let cached = self.view_target().filter(|(_, p)| *p == src).and_then(|(k, _)| self.mem.get(&k).map(|l| l.audio.clone()));
        if let Some(dir) = path.parent() {
            self.prefs.ultima_carpeta = Some(dir.to_path_buf());
            self.save_prefs();
        }
        self.exports.push(Task::spawn("surshape-export", move |_, _| { // i18n-ok
            let audio = match cached {
                Some(a) => a,
                None => Arc::new(decode::load(&src)?),
            };
            let clipped = fmt == crate::prefs::ExportFormat::Int24 && Analysis::of(&audio).clips();
            export::write_wav(&path, &audio, fmt.wav())?;
            Ok((path, clipped))
        }));
    }

    // --- Diálogos de archivo (en un hilo: la interfaz sigue viva) --------------------

    pub(crate) fn open_dialog(&mut self, f: impl FnOnce(rfd::FileDialog) -> Option<DialogResult> + Send + 'static) {
        if self.dialog.is_some() {
            return;
        }
        let dir = self.prefs.ultima_carpeta.clone();
        self.dialog = Some(Task::spawn("surshape-dialogo", move |_, _| { // i18n-ok
            let mut d = rfd::FileDialog::new();
            if let Some(dir) = dir {
                d = d.set_directory(dir);
            }
            Ok(f(d))
        }));
    }

    /// Texto de la línea de ayuda si `resp` está bajo el mouse.
    pub(crate) fn hint(&mut self, resp: &egui::Response, text: impl Into<String>) {
        if resp.hovered() {
            self.help = text.into();
        }
    }

    pub(crate) fn hint_key(&mut self, resp: &egui::Response, key: &str) {
        if resp.hovered() {
            self.help = surshape_i18n::tr(key).to_string();
        }
    }

    // --- Cuadro -----------------------------------------------------------------------

    /// Un cuadro completo de la interfaz (lo llama eframe y los tests).
    pub fn frame(&mut self, ctx: &egui::Context) {
        if (self.prefs.zoom - self.zoom_applied).abs() > 0.001 {
            ctx.set_zoom_factor(self.prefs.zoom.clamp(1.0, 2.0));
            self.zoom_applied = self.prefs.zoom;
        }
        self.poll_tasks();
        self.poll_spectros(ctx);
        self.poll_previews();
        self.help.clear();
        self.keys = match &self.session {
            Some(s) => s.patch.keys(&self.registry, &self.render_options()),
            None => Keys::new(),
        };
        // Autosave diferido (no mientras se arrastra un control).
        if self.dirty_at.is_some_and(|t| t.elapsed() >= AUTOSAVE_DELAY) && !ctx.input(|i| i.pointer.any_down()) {
            self.save_now();
        }

        // Archivos soltados sobre la ventana
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
        if !dropped.is_empty() {
            self.import(dropped);
        }
        self.handle_shortcuts(ctx);

        let face = egui::Frame::NONE.fill(theme::FACE);
        egui::TopBottomPanel::top("menus") // i18n-ok
            .frame(face.inner_margin(egui::Margin::symmetric(2, 1)))
            .show_separator_line(false)
            .show(ctx, |ui| self.menu_bar(ui));
        egui::TopBottomPanel::top("herramientas") // i18n-ok
            .frame(face.inner_margin(egui::Margin::symmetric(4, 2)))
            .show(ctx, |ui| self.toolbar(ui));
        egui::TopBottomPanel::bottom("estado") // i18n-ok
            .frame(face.inner_margin(egui::Margin::symmetric(2, 2)))
            .show_separator_line(false)
            .show(ctx, |ui| self.status_bar(ui));
        egui::TopBottomPanel::bottom("ayuda") // i18n-ok
            .frame(face.inner_margin(egui::Margin::symmetric(4, 2)))
            .show(ctx, |ui| self.help_line(ui));
        let side = egui::SidePanel::left("lateral") // i18n-ok
            .resizable(true)
            .default_width(self.prefs.ancho_lateral)
            .width_range(190.0..=420.0)
            .frame(face.inner_margin(egui::Margin::same(3)))
            .show(ctx, |ui| self.sidebar(ui));
        let w = side.response.rect.width();
        if (w - self.prefs.ancho_lateral).abs() > 1.0 && !ctx.input(|i| i.pointer.any_down()) {
            self.prefs.ancho_lateral = w;
            self.save_prefs();
        }
        egui::CentralPanel::default().frame(face.inner_margin(egui::Margin::same(3))).show(ctx, |ui| match self.page {
            Page::Main => self.main_page(ui),
            Page::Params(id) => self.params_page(ui, id),
            Page::Graph => self.graph_page(ui),
            Page::Mix(id) => self.mix_page(ui, id),
            Page::Console => self.console_page(ui),
        });
        self.menu_popups(ctx);
        self.modal_dialogs(ctx);
        if ctx.input(|i| !i.raw.hovered_files.is_empty()) {
            self.drop_overlay(ctx);
        }
        if let Some(mut cap) = self.capture.take() {
            cap.step(self, ctx);
            if !cap.done {
                self.capture = Some(cap);
            }
        }

        let busy = !self.loads.is_empty()
            || !self.imports.is_empty()
            || self.verify.is_some()
            || !self.spectro_loads.is_empty()
            || self.run.is_some()
            || !self.exports.is_empty()
            || self.dialog.is_some()
            || self.dirty_at.is_some()
            || self.info_task.is_some()
            || !self.ext_edits.is_empty()
            || self.pending_play.is_some()
            || self.capture.is_some()
            || self.previews.as_ref().is_some_and(|p| p.task.is_some())
            || self.player.is_playing();
        if busy {
            ctx.request_repaint_after(Duration::from_millis(33));
        }
    }

    /// Aviso grande al arrastrar archivos sobre la ventana.
    fn drop_overlay(&self, ctx: &egui::Context) {
        let r = ctx.screen_rect();
        let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("soltar"))); // i18n-ok
        p.rect_filled(r, 0.0, egui::Color32::from_rgba_unmultiplied(0x0B, 0x4A, 0x78, 90));
        let font = theme::bold_font(18.0);
        let text = surshape_i18n::tr("ui.sesion.soltar");
        let g = p.layout_no_wrap(text.to_string(), font, theme::TEXT);
        let br = egui::Rect::from_center_size(r.center(), g.size() + egui::vec2(32.0, 20.0));
        p.rect_filled(br, 0.0, theme::FACE);
        crate::win32::bevel(&p, br, crate::win32::Bevel::Raised);
        p.galley(br.center() - g.size() * 0.5, g, theme::TEXT);
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.frame(ctx);
    }
}
