//! El trait [`Process`] y su contexto de render.

use crate::data::{DataSet, FileKind};
use crate::params::{Curve, ParamKind, ParamSpec, ParamValue, ParamValues};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use surshape_audio::{AudioBuf, AudioError};

/// Familia del catálogo. Cada una tiene su clave i18n.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Family {
    Tiempo,
    Espectral,
    Tono,
    Destruccion,
    FiltroEspacio,
    Generador,
    Combinacion,
    Utilidad,
}

impl Family {
    pub const ALL: [Family; 8] = [
        Family::Tiempo,
        Family::Espectral,
        Family::Tono,
        Family::Destruccion,
        Family::FiltroEspacio,
        Family::Generador,
        Family::Combinacion,
        Family::Utilidad,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Family::Tiempo => "ui.familia.tiempo",
            Family::Espectral => "ui.familia.espectral",
            Family::Tono => "ui.familia.tono",
            Family::Destruccion => "ui.familia.destruccion",
            Family::FiltroEspacio => "ui.familia.filtro_espacio",
            Family::Generador => "ui.familia.generador",
            Family::Combinacion => "ui.familia.combinacion",
            Family::Utilidad => "ui.familia.utilidad",
        }
    }
}

/// Motor que ejecuta el proceso.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Engine {
    Nativo,
    Cdp,
}

impl Engine {
    /// Identificador estable (patch).
    pub fn id(self) -> &'static str {
        match self {
            Engine::Nativo => "nativo",
            Engine::Cdp => "cdp",
        }
    }
    /// Nombre completo.
    pub fn key(self) -> &'static str {
        match self {
            Engine::Nativo => "ui.motor.nativo",
            Engine::Cdp => "ui.motor.cdp",
        }
    }
    /// Distintivo corto para el catálogo.
    pub fn badge_key(self) -> &'static str {
        match self {
            Engine::Nativo => "ui.motor.nativo.corto",
            Engine::Cdp => "ui.motor.cdp.corto",
        }
    }
}

/// Una entrada de un proceso. La primera es la "principal" (la celda de la
/// izquierda en la grilla); las demás se eligen con clic en cualquier celda.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InputSpec {
    /// Clave i18n del nombre de la entrada.
    pub key: &'static str,
}

impl InputSpec {
    pub const PRINCIPAL: InputSpec = InputSpec { key: "ui.entrada.principal" };
    /// Segunda entrada genérica.
    pub const SEGUNDA: InputSpec = InputSpec { key: "ui.entrada.segunda" };
    /// Respuesta al impulso (convolución).
    pub const IR: InputSpec = InputSpec { key: "ui.entrada.ir" };
    /// Fuente de espectro / modulador (síntesis cruzada).
    pub const MODULADOR: InputSpec = InputSpec { key: "ui.entrada.modulador" };
}

/// Cuántas entradas acepta un proceso.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Inputs {
    /// Exactamente estas.
    Fixed(&'static [InputSpec]),
    /// Cualquier cantidad >= 1 (mezcla).
    Variadic,
}

impl Inputs {
    pub fn accepts(self, n: usize) -> bool {
        match self {
            Inputs::Fixed(s) => s.len() == n,
            Inputs::Variadic => n >= 1,
        }
    }
    pub fn min(self) -> usize {
        match self {
            Inputs::Fixed(s) => s.len(),
            Inputs::Variadic => 1,
        }
    }
    /// ¿Es un generador (sin entradas)?
    pub fn is_generator(self) -> bool {
        matches!(self, Inputs::Fixed(s) if s.is_empty())
    }
    /// Especificación de la entrada `i` (si es fija).
    pub fn spec(self, i: usize) -> Option<InputSpec> {
        match self {
            Inputs::Fixed(s) => s.get(i).copied(),
            Inputs::Variadic => Some(if i == 0 { InputSpec::PRINCIPAL } else { InputSpec::SEGUNDA }),
        }
    }
}

/// Cuántas salidas produce. Las salidas extra van a filas nuevas de la grilla.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outputs {
    Fixed(usize),
    /// Se sabe al terminar (p. ej. "slice" espectral de CDP).
    Dynamic,
}

/// Duración esperada de la salida (la verifican los tests y la validación).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LenRule {
    /// Igual a la primera entrada.
    Same,
    /// Cantidad exacta de muestras.
    Frames(usize),
    /// Entrada x factor (redondeado; tolerancia de 1 muestra).
    Scaled(f64),
    /// No se puede saber de antemano.
    Unknown,
}

impl LenRule {
    pub fn expected(self, in_frames: usize) -> Option<usize> {
        match self {
            LenRule::Same => Some(in_frames),
            LenRule::Frames(n) => Some(n),
            LenRule::Scaled(f) => Some((in_frames as f64 * f).round() as usize),
            LenRule::Unknown => None,
        }
    }
    /// ¿`actual` cumple la regla?
    pub fn matches(self, in_frames: usize, actual: usize) -> bool {
        match self.expected(in_frames) {
            Some(e) => e.abs_diff(actual) <= 1,
            None => true,
        }
    }
}

/// Error de un proceso: cancelado, o clave i18n + argumentos.
#[derive(Clone, Debug, PartialEq)]
pub enum ProcessError {
    Cancelled,
    Failed { key: &'static str, args: Vec<(&'static str, String)> },
}

impl ProcessError {
    pub fn new(key: &'static str) -> Self {
        ProcessError::Failed { key, args: Vec::new() }
    }
    pub fn arg(self, name: &'static str, v: impl ToString) -> Self {
        match self {
            ProcessError::Failed { key, mut args } => {
                args.push((name, v.to_string()));
                ProcessError::Failed { key, args }
            }
            c => c,
        }
    }
    /// Clave i18n y argumentos, para mostrar.
    pub fn message(&self) -> (&'static str, Vec<(&'static str, String)>) {
        match self {
            ProcessError::Cancelled => ("err.cancelado", Vec::new()),
            ProcessError::Failed { key, args } => (key, args.clone()),
        }
    }
}

impl From<AudioError> for ProcessError {
    fn from(e: AudioError) -> Self {
        if e.key == "err.cancelado" {
            ProcessError::Cancelled
        } else {
            ProcessError::Failed { key: e.key, args: e.args }
        }
    }
}

impl std::fmt::Display for ProcessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (k, a) = self.message();
        write!(f, "{k} {a:?}")
    }
}

impl std::error::Error for ProcessError {}

/// Todo lo que un proceso necesita durante el render.
pub struct RenderCtx<'a> {
    pub inputs: &'a [Arc<AudioBuf>],
    /// Valores completos y dentro de rango (de un canal, si el render va
    /// canal por canal).
    pub params: &'a ParamValues,
    pub seed: u64,
    /// 0..=1_000_000.
    pub progress: &'a AtomicU32,
    pub cancel: &'a AtomicBool,
    /// Carpeta temporal propia de este render (se borra al terminar).
    pub workdir: &'a Path,
}

impl RenderCtx<'_> {
    pub fn input(&self, i: usize) -> &AudioBuf {
        &self.inputs[i]
    }
    /// Duración de referencia para los breakpoints normalizados: la entrada
    /// principal o, en un generador, el parámetro "duracion".
    pub fn ref_duration(&self) -> f64 {
        match self.inputs.first() {
            Some(b) => b.duration_secs(),
            None => self.params.get("duracion"),
        }
    }
    /// Valor inicial de `id` (para parámetros fijos).
    pub fn p(&self, id: &str) -> f64 {
        self.params.get(id)
    }
    pub fn flag(&self, id: &str) -> bool {
        self.params.get(id) >= 0.5
    }
    /// Lector de `id` a lo largo del tiempo de la entrada principal (los
    /// breakpoints normalizados se escalan a su duración).
    pub fn curve(&self, id: &str) -> Curve {
        let dur = self.ref_duration();
        Curve::new(self.params.value(id).unwrap_or(&ParamValue::Fixed(0.0)), dur)
    }
    /// Progreso 0..1.
    pub fn set_progress(&self, f: f64) {
        self.progress.store((f.clamp(0.0, 1.0) * 1_000_000.0) as u32, Ordering::Relaxed);
    }
    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
    /// `Err(Cancelled)` si el usuario canceló. Llamar seguido en los bucles.
    pub fn check(&self) -> Result<(), ProcessError> {
        if self.cancelled() {
            Err(ProcessError::Cancelled)
        } else {
            Ok(())
        }
    }
}

/// Contexto de un proceso que trabaja con archivos (CDP): entradas como
/// archivos por canal, salidas escritas en `workdir`.
pub struct FileCtx<'a> {
    pub inputs: &'a [DataSet],
    pub params: &'a ParamValues,
    pub seed: u64,
    pub progress: &'a AtomicU32,
    pub cancel: &'a AtomicBool,
    pub workdir: &'a Path,
}

impl FileCtx<'_> {
    pub fn set_progress(&self, f: f64) {
        self.progress.store((f.clamp(0.0, 1.0) * 1_000_000.0) as u32, Ordering::Relaxed);
    }
    pub fn check(&self) -> Result<(), ProcessError> {
        if self.cancel.load(Ordering::Relaxed) {
            Err(ProcessError::Cancelled)
        } else {
            Ok(())
        }
    }
}

/// Lo necesario para mostrar la línea de comando de una celda sin
/// ejecutarla: nombres de archivo de las entradas y la salida.
pub struct CmdCtx<'a> {
    pub params: &'a ParamValues,
    pub seed: u64,
    /// Nombres de las entradas (p. ej. `~A_1.ana`).
    pub inputs: &'a [String],
    /// Nombre de la salida (p. ej. `~A_2.ana`).
    pub output: &'a str,
    /// Duración de la entrada principal y de la segunda (segundos).
    pub dur: f64,
    pub dur2: f64,
}

/// Parámetro que elige el modo de un programa CDP con modos.
pub const MODE_PARAM: &str = "modo";

/// Descriptor de un proceso nativo: `nat.stretch factor=50 ventana=0.25 seed=922299`.
pub fn descriptor(id: &str, specs: &[ParamSpec], params: &ParamValues, seed: Option<u64>) -> String {
    let mut s = id.to_string();
    for spec in specs {
        let v = match params.value(spec.id) {
            Some(ParamValue::Envelope(bp)) => format!("brk[{}]", bp.puntos.len()),
            _ => {
                let x = params.get(spec.id);
                match spec.kind {
                    ParamKind::Float { decimals } => {
                        let t = format!("{x:.*}", decimals as usize);
                        if t.contains('.') {
                            t.trim_end_matches('0').trim_end_matches('.').to_string()
                        } else {
                            t
                        }
                    }
                    _ => format!("{}", x.round() as i64),
                }
            }
        };
        s.push_str(&format!(" {}={v}", spec.id));
    }
    if let Some(seed) = seed {
        s.push_str(&format!(" seed={seed}"));
    }
    s
}

/// Un proceso de SURSHAPE (nativo o CDP).
pub trait Process: Send + Sync {
    /// Identificador estable: se guarda en el patch. Nativos `nat.*`, CDP `cdp.*`.
    fn id(&self) -> &'static str;
    /// Versión del algoritmo: subirla cuando cambia el resultado, para que la
    /// cache no reutilice renders viejos.
    fn version(&self) -> u32 {
        1
    }
    fn family(&self) -> Family;
    fn engine(&self) -> Engine {
        Engine::Nativo
    }
    /// Versión del motor externo (p. ej. "7.1.0" de CDP): entra en la clave
    /// de cache y en el linaje.
    fn engine_version(&self) -> Option<String> {
        None
    }
    fn params(&self) -> &[ParamSpec];
    fn inputs(&self) -> Inputs {
        Inputs::Fixed(&[InputSpec::PRINCIPAL])
    }
    fn outputs(&self) -> Outputs {
        Outputs::Fixed(1)
    }
    /// ¿Admite parámetros distintos por canal? (el motor procesa entonces
    /// cada canal por separado y los vuelve a juntar)
    fn per_channel(&self) -> bool {
        false
    }
    /// ¿Usa la seed? (si no, la interfaz no la muestra y no entra al hash)
    fn uses_seed(&self) -> bool {
        false
    }
    /// Duración esperada de la salida.
    fn expected_len(&self, inputs: &[&AudioBuf], params: &ParamValues) -> LenRule;
    fn process(&self, ctx: &mut RenderCtx) -> Result<Vec<AudioBuf>, ProcessError>;

    /// Tipo de archivo de cada entrada (vacío = todas sonido).
    fn input_kinds(&self) -> &[FileKind] {
        &[]
    }
    /// Tipo de archivo de la entrada `i`.
    fn input_kind(&self, i: usize) -> FileKind {
        self.input_kinds().get(i).copied().unwrap_or(FileKind::Wav)
    }
    /// Tipo de archivo de las salidas.
    fn output_kind(&self) -> FileKind {
        FileKind::Wav
    }
    /// ¿Trabaja con archivos ([`Process::process_files`]) en lugar de audio
    /// en memoria? (cuando alguna entrada o la salida no es sonido)
    fn works_on_files(&self) -> bool {
        self.output_kind() != FileKind::Wav || self.input_kinds().iter().any(|k| *k != FileKind::Wav)
    }
    /// Proceso con archivos: lee `ctx.inputs` y escribe sus salidas en
    /// `ctx.workdir`. Solo lo implementan los que trabajan con archivos.
    fn process_files(&self, _ctx: &mut FileCtx) -> Result<Vec<DataSet>, ProcessError> {
        Err(ProcessError::new("err.render.no_archivos").arg("proceso", self.id()))
    }
    /// Línea(s) de comando equivalentes, para mostrar (sin ejecutar). Los
    /// nativos dan su descriptor; CDP, los comandos exactos.
    fn command(&self, c: &CmdCtx) -> Vec<String> {
        vec![descriptor(self.id(), self.params(), c.params, self.uses_seed().then_some(c.seed))]
    }
}

/// Clave i18n del nombre de un proceso.
pub fn name_key(id: &str) -> String {
    format!("proc.{id}.nombre")
}

/// Clave i18n de la descripción de un proceso.
pub fn desc_key(id: &str) -> String {
    format!("proc.{id}.desc")
}
