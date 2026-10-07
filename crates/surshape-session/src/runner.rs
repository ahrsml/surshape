//! Ejecución de un plan de render (varios nodos, en orden) en un hilo de
//! trabajo. Se arma en el hilo de la interfaz con [`build_steps`] (una foto
//! del patch: el hilo no toca el patch vivo) y se corre con [`run_steps`].
//!
//! Por cada paso: si la cache ya tiene la clave, se usa sin calcular; si no,
//! se cargan las entradas (de memoria, de la cache o de la fuente), se aplica
//! la región, se renderiza y se guarda en la cache. Si un paso falla, los que
//! dependen de él se saltan con un error claro; los independientes siguen.
//!
//! Tipos de archivo (como Soundshaper): un proceso que lee .ana recibe el
//! análisis de la celda anterior; si la celda anterior es sonido, se analiza
//! sola con `pvoc anal` (auto-conversión, cacheada por su propia clave). Una
//! celda que produce .ana guarda además su resíntesis (.wav) para verla,
//! escucharla y alimentar a los procesos de sonido que vengan después.
//!
//! Cada ejecución queda en la consola ([`surshape_engine::console`]) con la
//! celda que la pidió.

use crate::{cache_alias, cache_data_rel, cache_lookup, cache_rel, cache_store, cache_store_outs, Session, StoredOut, CACHE_DIR};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use surshape_audio::export::{write_wav, WavFormat};
use surshape_audio::{decode, AudioBuf};
use surshape_engine::render::{self, FileOut, RenderJob, RenderOptions, RenderReport};
use surshape_engine::{
    console, DataSet, Engine, FileKind, ParamSet, ParamValues, Process, ProcessError, Registry, StableHasher, MODE_PARAM,
};
use surshape_native::mix::{Mix, MixSetting};
use surshape_patch::{Keys, NodeId, NodeKind, NodeTemplate, Notice, Patch, Region, RenderRecord, SourceInfo};

/// Clave de una salida concreta: `<clave del nodo>_<salida>`.
pub fn port_key(node_key: &str, salida: u16) -> String {
    format!("{node_key}_{salida}")
}

/// Procesos de conversión (de CDP): sonido -> .ana y .ana -> sonido.
pub const ANAL_ID: &str = "cdp.pvoc_anal";
pub const SYNTH_ID: &str = "cdp.pvoc_synth";

#[derive(Clone, Debug)]
pub struct StepInput {
    pub port_key: String,
    /// Dónde leer su sonido si no está en memoria (si la salida son datos,
    /// su resíntesis).
    pub path: PathBuf,
    /// Tipo de archivo que produce la celda de entrada.
    pub kind: FileKind,
    /// Clave de la celda de entrada (None si es una fuente).
    pub node_key: Option<String>,
    pub salida: u16,
    /// Nombre de la celda de entrada ("A_1"), para la consola.
    pub label: String,
}

#[derive(Clone)]
pub struct Step {
    pub node: NodeId,
    pub key: String,
    /// Nombre de la celda ("A_2").
    pub label: String,
    pub process: Arc<dyn Process>,
    pub params: ParamSet,
    pub seed: u64,
    pub region: Option<Region>,
    pub iteraciones: u32,
    pub inputs: Vec<StepInput>,
    /// Conversiones disponibles (si CDP está).
    pub anal: Option<Arc<dyn Process>>,
    pub synth: Option<Arc<dyn Process>>,
    /// Sub-patch: sus pasos y el catálogo para calcularlos (el proceso de
    /// arriba es entonces el del último paso).
    pub sub: Option<(Vec<NodeTemplate>, Registry)>,
}

/// Arma los pasos de `plan` (ids en orden topológico). Procesos y mezclas;
/// los sub-patches (fase 7) y los nodos sin clave se omiten.
pub fn build_steps(session: &Session, plan: &[NodeId], keys: &Keys, reg: &Registry) -> Vec<Step> {
    let p: &Patch = &session.patch;
    plan.iter()
        .filter_map(|&id| {
            let n = p.node(id)?;
            let process: Arc<dyn Process> = match &n.tipo {
                NodeKind::Proceso { proceso } => reg.get(proceso)?.clone(),
                // La mezcla se arma con la configuración de cada entrada.
                NodeKind::Mezcla { canales } => Arc::new(Mix {
                    settings: canales.iter().map(|c| MixSetting { gain_db: c.ganancia_db, pan: c.paneo, start: c.inicio }).collect(),
                }),
                NodeKind::SubPatch { pasos, .. } => match pasos.last().map(|t| &t.tipo) {
                    Some(NodeKind::Proceso { proceso }) => reg.get(proceso)?.clone(),
                    _ => return None,
                },
                _ => return None,
            };
            let sub = match &n.tipo {
                NodeKind::SubPatch { pasos, .. } => Some((pasos.clone(), reg.clone())),
                _ => None,
            };
            let key = keys.get(&id)?.as_ref().ok()?.clone();
            let inputs = n
                .entradas
                .iter()
                .map(|port| {
                    let inn = p.node(port.nodo)?;
                    let k = keys.get(&port.nodo)?.as_ref().ok()?;
                    let (path, kind, node_key) = match &inn.tipo {
                        NodeKind::Fuente(s) => (s.ruta.clone(), FileKind::Wav, None),
                        other => {
                            let kind = match other {
                                NodeKind::Proceso { proceso } => reg.get(proceso).map_or(FileKind::Wav, |q| q.output_kind()),
                                _ => FileKind::Wav,
                            };
                            (session.dir.join(cache_rel(k, port.salida as usize)), kind, Some(k.clone()))
                        }
                    };
                    let label = p.cell_label(port.nodo).unwrap_or_default();
                    Some(StepInput { port_key: port_key(k, port.salida), path, kind, node_key, salida: port.salida, label })
                })
                .collect::<Option<Vec<_>>>()?;
            Some(Step {
                node: id,
                key,
                label: p.cell_label(id).unwrap_or_default(),
                process,
                params: n.params.clone(),
                seed: n.seed,
                region: n.region,
                iteraciones: n.iteraciones.max(1),
                inputs,
                anal: reg.get(ANAL_ID).cloned(),
                synth: reg.get(SYNTH_ID).cloned(),
                sub,
            })
        })
        .collect()
}

/// Qué está haciendo el hilo (lo lee la interfaz en cada cuadro).
#[derive(Default)]
pub struct RunStatus {
    /// Índice del paso en curso.
    pub step: AtomicU32,
    /// Nodo en curso.
    pub node: AtomicU32,
}

#[derive(Clone, Debug)]
pub struct StepResult {
    pub node: NodeId,
    pub key: String,
    pub result: Result<RenderRecord, ProcessError>,
    /// ¿Salió de la cache sin calcular?
    pub from_cache: bool,
    /// Informe completo (solo si se calculó ahora).
    pub report: Option<RenderReport>,
}

#[derive(Clone, Debug, Default)]
pub struct RunResult {
    pub steps: Vec<StepResult>,
    /// Audio de las salidas calculadas y de las entradas leídas, por clave de
    /// salida (para que la interfaz no vuelva a leer los archivos).
    pub audio: HashMap<String, Arc<AudioBuf>>,
    pub cancelled: bool,
}

/// Avisos de la validación como mensajes guardables.
pub fn report_notices(r: &RenderReport, sr: u32) -> Vec<Notice> {
    let fmt_db = |d: f64| format!("{d:.1}");
    let fmt_t = |n: usize| format!("{:.3}", n as f64 / sr.max(1) as f64);
    let mut v = Vec::new();
    if r.nonfinite_fixed > 0 {
        v.push(Notice::new("ui.informe.nan", &[("n", r.nonfinite_fixed.to_string())]));
    }
    // Reducciones menores a 0,05 dB son inaudibles: no se avisan.
    if let Some(db) = r.limited_db.filter(|d| *d < -0.05) {
        v.push(Notice::new("ui.informe.limitado", &[("db", fmt_db(db))]));
    }
    if r.over_0dbfs() {
        v.push(Notice::new("ui.informe.clip", &[("pico", fmt_db(r.peak_db))]));
    }
    if !r.len_ok {
        v.push(Notice::new(
            "ui.informe.duracion",
            &[("real", fmt_t(r.frames)), ("esperada", fmt_t(r.expected_frames.unwrap_or(0)))],
        ));
    }
    v
}

fn load(path: &Path, key: &str, mem: &mut HashMap<String, Arc<AudioBuf>>) -> Result<Arc<AudioBuf>, ProcessError> {
    if let Some(a) = mem.get(key) {
        return Ok(a.clone());
    }
    let a = Arc::new(decode::load(path)?);
    mem.insert(key.to_string(), a.clone());
    Ok(a)
}

/// Corre los pasos. `preloaded`: audio ya en memoria (por clave de salida).
pub fn run_steps(
    dir: &Path,
    steps: &[Step],
    opts: &RenderOptions,
    preloaded: HashMap<String, Arc<AudioBuf>>,
    status: &RunStatus,
    progress: &AtomicU32,
    cancel: &AtomicBool,
) -> RunResult {
    let mut out = RunResult { audio: preloaded, ..Default::default() };
    let mut failed: HashSet<String> = HashSet::new(); // claves de nodo fallidas
    for (i, st) in steps.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            out.cancelled = true;
            break;
        }
        status.step.store(i as u32, Ordering::Relaxed);
        status.node.store(st.node, Ordering::Relaxed);
        console::set_cell(&st.label);
        progress.store(0, Ordering::Relaxed);
        let mut res = StepResult { node: st.node, key: st.key.clone(), result: Err(ProcessError::Cancelled), from_cache: false, report: None };

        let upstream_failed = st.inputs.iter().any(|inp| failed.iter().any(|k| inp.port_key.starts_with(k.as_str())));
        if upstream_failed {
            res.result = Err(ProcessError::new("err.patch.entrada_con_error"));
            failed.insert(st.key.clone());
            out.steps.push(res);
            continue;
        }
        if let Some(rec) = cache_lookup(dir, &st.key) {
            res.result = Ok(rec);
            res.from_cache = true;
            out.steps.push(res);
            continue;
        }

        let computed = if st.sub.is_some() {
            run_subpatch_step(dir, st, opts, &mut out.audio, progress, cancel)
        } else if st.process.works_on_files() {
            run_file_step(dir, st, opts, &mut out.audio, progress, cancel)
        } else {
            run_audio_step(dir, st, opts, &mut out.audio, progress, cancel)
        };
        match computed {
            Ok((rec, report, outputs)) => {
                for (n, a) in outputs.into_iter().enumerate() {
                    out.audio.insert(port_key(&st.key, n as u16), Arc::new(a));
                }
                res.result = Ok(rec);
                res.report = Some(report);
            }
            Err(ProcessError::Cancelled) => {
                out.cancelled = true;
                res.result = Err(ProcessError::Cancelled);
                out.steps.push(res);
                break;
            }
            Err(e) => {
                failed.insert(st.key.clone());
                res.result = Err(e);
            }
        }
        out.steps.push(res);
    }
    out
}

/// Paso de un sub-patch: sus pasos se arman como una fila aparte sobre la
/// entrada y se calculan con el mismo mecanismo (cada paso queda en la cache
/// con su propia clave, así que cambiar solo el final no recalcula el
/// principio). El resultado del sub-patch es el del último paso.
fn run_subpatch_step(
    dir: &Path,
    st: &Step,
    opts: &RenderOptions,
    mem: &mut HashMap<String, Arc<AudioBuf>>,
    progress: &AtomicU32,
    cancel: &AtomicBool,
) -> Result<(RenderRecord, RenderReport, Vec<AudioBuf>), ProcessError> {
    let (pasos, reg) = st.sub.as_ref().expect("sub-patch");
    let inp = st.inputs.first().ok_or_else(|| ProcessError::new("err.patch.falta_entrada"))?;
    // La "fuente" del sub-patch es la entrada: su identidad es la clave de esa
    // salida (no hace falta leer el archivo para calcular las claves).
    let src = SourceInfo {
        ruta: inp.path.clone(),
        hash: format!("sub:{}", inp.port_key), // i18n-ok
        sr: 0,
        canales: 0,
        frames: 0,
        bytes: 0,
        modificado: 0,
        marcadores: Vec::new(),
    };
    let (mut patch, ids) = Patch::from_steps(src, pasos);
    if let (Some(r), Some(&first)) = (st.region, ids.first()) {
        if let Some(n) = patch.node_mut(first) {
            n.region = Some(r);
        }
    }
    let last = *ids.last().ok_or_else(|| ProcessError::new("err.render.salida_vacia"))?;
    let keys = patch.keys(reg, opts);
    let sess = Session { version: crate::FORMAT_VERSION, dir: dir.to_path_buf(), patch };
    // El audio de la entrada ya cargado sirve con la clave de la fuente interna.
    if let (Some(a), Some(Ok(k))) = (mem.get(&inp.port_key).cloned(), keys.get(&sess.patch.filas[0].celdas[0])) {
        mem.insert(port_key(k, 0), a);
    }
    let plan = sess.patch.render_plan(&[last], &keys);
    let mut steps = build_steps(&sess, &plan, &keys, reg);
    for s in &mut steps {
        // En la consola: "B_2>A_1" (celda del sub-patch > paso).
        s.label = format!("{}>{}", st.label, s.label);
    }
    let t0 = std::time::Instant::now();
    let r = run_steps(dir, &steps, opts, std::mem::take(mem), &RunStatus::default(), progress, cancel);
    *mem = r.audio;
    if r.cancelled {
        return Err(ProcessError::Cancelled);
    }
    for s in &r.steps {
        if let Err(e) = &s.result {
            return Err(e.clone());
        }
    }
    let last_key = keys.get(&last).and_then(|k| k.as_ref().ok()).ok_or_else(|| ProcessError::new("err.patch.entrada_con_error"))?;
    let rec = cache_lookup(dir, last_key).ok_or_else(|| ProcessError::new("err.render.salida_vacia"))?;
    let rec = cache_alias(dir, &st.key, &rec)?;
    let report = r.steps.iter().rev().find_map(|s| s.report.clone()).unwrap_or(RenderReport {
        nonfinite_fixed: 0,
        peak_raw_db: f64::NEG_INFINITY,
        clipped: false,
        limited_db: None,
        peak_db: f64::NEG_INFINITY,
        expected_frames: None,
        frames: rec.salidas.first().map_or(0, |o| o.frames as usize),
        len_ok: true,
        elapsed: t0.elapsed(),
    });
    let outputs = (0..rec.salidas.len()).filter_map(|n| mem.get(&port_key(last_key, n as u16)).map(|a| (**a).clone())).collect();
    Ok((rec, report, outputs))
}

/// Paso de un proceso de sonido (nativo, mezcla o CDP de sonido a sonido).
fn run_audio_step(
    dir: &Path,
    st: &Step,
    opts: &RenderOptions,
    mem: &mut HashMap<String, Arc<AudioBuf>>,
    progress: &AtomicU32,
    cancel: &AtomicBool,
) -> Result<(RenderRecord, RenderReport, Vec<AudioBuf>), ProcessError> {
    let mut inputs = Vec::with_capacity(st.inputs.len());
    for inp in &st.inputs {
        inputs.push(load(&inp.path, &inp.port_key, mem)?);
    }
    if let (Some(r), Some(first)) = (st.region, inputs.first_mut()) {
        *first = Arc::new(first.slice(r.inicio as usize, r.fin as usize));
    }
    let mut job = RenderJob { process: st.process.clone(), inputs, params: st.params.clone(), seed: st.seed, options: *opts };
    let mut r = render::run(&job, progress, cancel)?;
    // Meta-proceso "iterar": el resultado vuelve a entrar al proceso.
    for _ in 1..st.iteraciones {
        if job.inputs.is_empty() {
            break; // un generador no tiene entrada que realimentar
        }
        job.inputs[0] = Arc::new(r.outputs[0].clone());
        r = render::run(&job, progress, cancel)?;
    }
    // Los nativos (y la mezcla) se anotan en la consola con su descriptor;
    // CDP anota cada programa que corre.
    if st.process.engine() == Engine::Nativo {
        let p = r.params.completed(st.process.params());
        let cmd = surshape_engine::descriptor(st.process.id(), st.process.params(), &p.comun, st.process.uses_seed().then_some(st.seed));
        let ins: Vec<&str> = st.inputs.iter().map(|i| i.label.as_str()).collect();
        let files = format!("{} -> {}", ins.join(", "), st.label);
        console::push(st.process.id(), &cmd, None, true, r.report.elapsed.as_secs_f64(), &files, "");
    }
    let sr = r.outputs[0].sr;
    let rec = cache_store(dir, &st.key, &r.outputs, report_notices(&r.report, sr), r.report.elapsed.as_secs_f64(), st.process.engine_version())?;
    Ok((rec, r.report, r.outputs))
}

/// Escribe un audio como un .wav mono por canal en `dir` (`pre_c<n>.wav`).
fn split_wav(a: &AudioBuf, dir: &Path, pre: &str) -> Result<DataSet, ProcessError> {
    std::fs::create_dir_all(dir).map_err(|e| ProcessError::new("err.render.temporal").arg("detalle", e))?;
    let mut files = Vec::with_capacity(a.num_channels());
    for (ch, x) in a.channels.iter().enumerate() {
        let f = dir.join(format!("{pre}_c{ch}.wav"));
        write_wav(&f, &AudioBuf::from_channels(a.sr, vec![x.clone()]), WavFormat::Float32)?;
        files.push(f);
    }
    Ok(DataSet { kind: FileKind::Wav, files, sr: a.sr, dur: a.duration_secs() })
}

/// Carpeta temporal de un paso (se borra al soltarla).
struct StepDir(PathBuf);

impl Drop for StepDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Clave de la auto-conversión a .ana de una entrada de sonido.
fn conv_key(port_key: &str, region: Option<Region>, opts: &RenderOptions) -> String {
    let mut h = StableHasher::new();
    h.str("conv_anal"); // i18n-ok
    h.str(port_key);
    match region {
        None => h.u8(0),
        Some(r) => {
            h.u8(1);
            h.u64(r.inicio);
            h.u64(r.fin);
        }
    }
    opts.pvoc.hash_into(&mut h);
    h.finish_hex()
}

/// Parámetros de `pvoc anal` según las preferencias.
fn anal_params(opts: &RenderOptions) -> ParamSet {
    let mut v = ParamValues::default();
    v.set(MODE_PARAM, 0.0);
    v.set("puntos", opts.pvoc.puntos as f64);
    v.set("superposicion", opts.pvoc.superposicion as f64);
    ParamSet { comun: v, por_canal: None }
}

/// La entrada `i` del paso como archivos del tipo que pide el proceso,
/// convirtiendo si hace falta.
#[allow(clippy::too_many_arguments)]
fn data_input(
    dir: &Path,
    st: &Step,
    i: usize,
    opts: &RenderOptions,
    mem: &mut HashMap<String, Arc<AudioBuf>>,
    tmp: &Path,
    progress: &AtomicU32,
    cancel: &AtomicBool,
) -> Result<DataSet, ProcessError> {
    let inp = &st.inputs[i];
    let want = st.process.input_kind(i);
    let region = if i == 0 { st.region } else { None };
    let audio = |mem: &mut HashMap<String, Arc<AudioBuf>>| -> Result<Arc<AudioBuf>, ProcessError> {
        let a = load(&inp.path, &inp.port_key, mem)?;
        Ok(match region {
            Some(r) => Arc::new(a.slice(r.inicio as usize, r.fin as usize)),
            None => a,
        })
    };
    match (inp.kind, want) {
        // Sonido (o la resíntesis de un .ana) para un programa de sonido.
        (_, FileKind::Wav) => split_wav(audio(mem)?.as_ref(), tmp, &format!("e{i}")),
        // Datos del mismo tipo: los archivos de la celda anterior.
        (have, want) if have == want => {
            let k = inp.node_key.as_deref().ok_or_else(|| ProcessError::new("err.render.tipo_entrada").arg("esperado", want.ext()).arg("recibido", have.ext()))?;
            let rec = cache_lookup(dir, k).ok_or_else(|| ProcessError::new("err.patch.entrada_con_error"))?;
            let o = rec.salidas.get(inp.salida as usize).ok_or_else(|| ProcessError::new("err.patch.entrada_con_error"))?;
            Ok(DataSet {
                kind: want,
                files: o.datos.iter().map(|d| dir.join(d)).collect(),
                sr: o.sr,
                dur: o.frames as f64 / o.sr.max(1) as f64,
            })
        }
        // Sonido para un proceso de análisis: auto-conversión con pvoc.
        (FileKind::Wav, FileKind::Ana) => {
            let anal = st.anal.as_ref().ok_or_else(|| ProcessError::new("err.cdp.no_encontrado").arg("programa", "pvoc anal"))?;
            let ck = conv_key(&inp.port_key, region, opts);
            let marker = dir.join(CACHE_DIR).join(format!("{ck}.conv"));
            let a = audio(mem)?;
            let files: Vec<PathBuf> = (0..a.num_channels()).map(|ch| dir.join(cache_data_rel(&ck, 0, ch, FileKind::Ana))).collect();
            if !(marker.is_file() && files.iter().all(|f| f.is_file())) {
                let wav = split_wav(&a, tmp, &format!("conv{i}"))?;
                let dest = |_n: usize, ch: usize, k: FileKind| dir.join(cache_data_rel(&ck, 0, ch, k));
                let r = render::run_files(anal, &[wav], &anal_params(opts), 0, opts, &dest, progress, cancel)?;
                if !matches!(r.outputs.first(), Some(FileOut::Data(_))) {
                    return Err(ProcessError::new("err.render.salida_vacia"));
                }
                std::fs::write(&marker, format!("{}", a.num_channels())).map_err(|e| ProcessError::new("err.render.temporal").arg("detalle", e))?;
            }
            Ok(DataSet { kind: FileKind::Ana, files, sr: a.sr, dur: a.duration_secs() })
        }
        (have, want) => Err(ProcessError::new("err.render.tipo_entrada").arg("esperado", want.ext()).arg("recibido", have.ext())),
    }
}

/// Sonido de una salida de datos: la resíntesis de un .ana; los demás tipos
/// no se escuchan (un instante de silencio).
fn companion(st: &Step, d: &DataSet, opts: &RenderOptions, progress: &AtomicU32, cancel: &AtomicBool) -> Result<AudioBuf, ProcessError> {
    if d.kind == FileKind::Ana {
        let synth = st.synth.as_ref().ok_or_else(|| ProcessError::new("err.cdp.no_encontrado").arg("programa", "pvoc synth"))?;
        let none = |_: usize, _: usize, _: FileKind| PathBuf::new();
        let r = render::run_files(synth, std::slice::from_ref(d), &ParamSet::default(), 0, opts, &none, progress, cancel)?;
        if let Some(FileOut::Audio(a)) = r.outputs.into_iter().next() {
            return Ok(a);
        }
        return Err(ProcessError::new("err.render.salida_vacia"));
    }
    Ok(AudioBuf::from_channels(d.sr.max(1), vec![vec![0.0; 1]; d.channels().max(1)]))
}

/// Paso de un proceso que trabaja con archivos (CDP con .ana...).
fn run_file_step(
    dir: &Path,
    st: &Step,
    opts: &RenderOptions,
    mem: &mut HashMap<String, Arc<AudioBuf>>,
    progress: &AtomicU32,
    cancel: &AtomicBool,
) -> Result<(RenderRecord, RenderReport, Vec<AudioBuf>), ProcessError> {
    let t0 = std::time::Instant::now();
    let tmp = StepDir(std::env::temp_dir().join("surshape").join(format!("paso_{}_{}", std::process::id(), st.key)));
    let mut inputs = Vec::with_capacity(st.inputs.len());
    for i in 0..st.inputs.len() {
        inputs.push(data_input(dir, st, i, opts, mem, &tmp.0, progress, cancel)?);
    }
    let out_kind = st.process.output_kind();
    let iters = if out_kind == st.process.input_kind(0) { st.iteraciones.max(1) } else { 1 };
    let mut result = None;
    for it in 0..iters {
        let last = it + 1 == iters;
        // Las vueltas intermedias van a archivos aparte (la siguiente las lee).
        let key = &st.key;
        let dest = move |n: usize, ch: usize, k: FileKind| {
            if last {
                dir.join(cache_data_rel(key, n, ch, k))
            } else {
                dir.join(CACHE_DIR).join(format!("{key}_it{it}_{n}_c{ch}.{}", k.ext()))
            }
        };
        let r = render::run_files(&st.process, &inputs, &st.params, st.seed, opts, &dest, progress, cancel)?;
        if !last {
            inputs[0] = match r.outputs.first() {
                Some(FileOut::Data(d)) => d.clone(),
                Some(FileOut::Audio(a)) => split_wav(a, &tmp.0, &format!("it{it}"))?,
                None => return Err(ProcessError::new("err.render.salida_vacia")),
            };
        }
        result = Some(r);
    }
    let r = result.ok_or_else(|| ProcessError::new("err.render.salida_vacia"))?;
    let mut stored = Vec::with_capacity(r.outputs.len());
    for o in r.outputs {
        stored.push(match o {
            FileOut::Audio(a) => StoredOut { audio: a, data: None },
            FileOut::Data(d) => StoredOut { audio: companion(st, &d, opts, progress, cancel)?, data: Some(d) },
        });
    }
    // Quitar los intermedios de las iteraciones.
    if iters > 1 {
        if let Ok(rd) = std::fs::read_dir(dir.join(CACHE_DIR)) {
            let pre = format!("{}_it", st.key);
            for e in rd.flatten() {
                if e.file_name().to_string_lossy().starts_with(&pre) {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }
    }
    let mut report = r.report;
    report.elapsed = t0.elapsed();
    let sr = stored.first().map_or(48000, |o| o.audio.sr);
    let rec = cache_store_outs(dir, &st.key, &stored, report_notices(&report, sr), report.elapsed.as_secs_f64(), st.process.engine_version())?;
    Ok((rec, report, stored.into_iter().map(|o| o.audio).collect()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::tmp;
    use surshape_audio::export::{write_wav, WavFormat};
    use surshape_patch::SourceInfo;

    fn run(s: &Session, reg: &Registry, opts: &RenderOptions) -> RunResult {
        let keys = s.patch.keys(reg, opts);
        let plan = s.patch.render_plan(&[], &keys);
        let steps = build_steps(s, &plan, &keys, reg);
        run_steps(&s.dir, &steps, opts, HashMap::new(), &RunStatus::default(), &AtomicU32::new(0), &AtomicBool::new(false))
    }

    fn apply(s: &mut Session, r: &RunResult) {
        for st in &r.steps {
            if let Ok(rec) = &st.result {
                s.patch.node_mut(st.node).unwrap().render = Some(rec.clone());
            }
        }
    }

    #[test]
    fn plan_runs_caches_and_recomputes_only_what_changed() {
        let dir = tmp("runner");
        let mut s = Session::create(&dir).unwrap();
        let src = dir.join("fuente.wav");
        let x: Vec<f32> = (0..4000).map(|i| (i as f32 * 0.03).sin() * 0.5).collect();
        write_wav(&src, &AudioBuf::from_channels(8000, vec![x.clone(), x]), WavFormat::Float32).unwrap();
        let hash = crate::hash_file(&src, None, None).unwrap();
        let (_, row) = s.patch.add_source(SourceInfo {
            ruta: src.clone(),
            hash,
            sr: 8000,
            canales: 2,
            frames: 4000,
            bytes: 0,
            modificado: 0,
            marcadores: vec![],
        });
        let reg = surshape_native::registry();
        let opts = RenderOptions::default();
        let a = s.patch.append(row, reg.get("nat.reverse").unwrap().as_ref(), 1).unwrap();
        let b = s.patch.append(row, reg.get("nat.bitcrush").unwrap().as_ref(), 1).unwrap();
        s.patch.node_mut(a).unwrap().region = Some(Region { inicio: 1000, fin: 3000 });

        let r1 = run(&s, &reg, &opts);
        assert_eq!(r1.steps.len(), 2);
        assert!(r1.steps.iter().all(|st| st.result.is_ok() && !st.from_cache));
        let rec_b = r1.steps[1].result.as_ref().unwrap();
        assert_eq!(rec_b.salidas[0].frames, 2000, "la región recorta la entrada");
        apply(&mut s, &r1);
        let keys = s.patch.keys(&reg, &opts);
        assert!(s.patch.render_plan(&[], &keys).is_empty());

        // Cambia `b` y vuelve al valor original: la segunda vez sale de la cache.
        s.patch.node_mut(b).unwrap().params.comun.set("bits", 3.0);
        let r2 = run(&s, &reg, &opts);
        assert_eq!(r2.steps.len(), 1);
        assert!(!r2.steps[0].from_cache);
        apply(&mut s, &r2);
        s.patch.node_mut(b).unwrap().params.comun.set("bits", 6.0);
        let r3 = run(&s, &reg, &opts);
        assert_eq!(r3.steps.len(), 1);
        assert!(r3.steps[0].from_cache, "mismos parámetros = misma clave = cache");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn iterations_feed_the_result_back() {
        let dir = tmp("runner_iter");
        let mut s = Session::create(&dir).unwrap();
        let src = dir.join("f.wav");
        let x: Vec<f32> = (0..1000).map(|i| i as f32 / 2000.0).collect();
        write_wav(&src, &AudioBuf::from_channels(8000, vec![x.clone()]), WavFormat::Float32).unwrap();
        let (_, row) = s.patch.add_source(SourceInfo {
            ruta: src.clone(),
            hash: crate::hash_file(&src, None, None).unwrap(),
            sr: 8000,
            canales: 1,
            frames: 1000,
            bytes: 0,
            modificado: 0,
            marcadores: vec![],
        });
        let reg = surshape_native::registry();
        let id = s.patch.append(row, reg.get("nat.reverse").unwrap().as_ref(), 1).unwrap();
        s.patch.node_mut(id).unwrap().iteraciones = 2;
        let r = run(&s, &reg, &RenderOptions::default());
        let rec = r.steps[0].result.as_ref().unwrap();
        let y = decode::load(&dir.join(&rec.salidas[0].archivo)).unwrap();
        assert_eq!(y.channels[0], x, "invertir dos veces = original");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn mix_node_renders_through_the_runner() {
        let dir = tmp("runner_mix");
        let mut s = Session::create(&dir).unwrap();
        let mut src = |name: &str, v: f32| {
            let f = dir.join(name);
            write_wav(&f, &AudioBuf::from_channels(8000, vec![vec![v; 800]]), WavFormat::Float32).unwrap();
            let info = SourceInfo {
                ruta: f.clone(),
                hash: crate::hash_file(&f, None, None).unwrap(),
                sr: 8000,
                canales: 1,
                frames: 800,
                bytes: 0,
                modificado: 0,
                marcadores: vec![],
            };
            s.patch.add_source(info).0
        };
        let (a, b) = (src("a.wav", 0.1), src("b.wav", 0.2));
        let (m, _) = s.patch.add_mix();
        s.patch.connect(m, 0, surshape_patch::PortRef::main(a)).unwrap();
        s.patch.connect(m, 1, surshape_patch::PortRef::main(b)).unwrap();
        if let NodeKind::Mezcla { canales } = &mut s.patch.node_mut(m).unwrap().tipo {
            canales[1].inicio = 0.05; // 400 muestras después
        }
        let reg = surshape_native::registry();
        let r = run(&s, &reg, &RenderOptions::default());
        let rec = r.steps[0].result.as_ref().unwrap();
        assert_eq!(rec.salidas[0].frames, 1200);
        let y = decode::load(&dir.join(&rec.salidas[0].archivo)).unwrap();
        let g = std::f32::consts::FRAC_1_SQRT_2; // paneo al centro
        assert!((y.channels[0][100] - 0.1 * g).abs() < 1e-5);
        assert!((y.channels[0][500] - 0.3 * g).abs() < 1e-5);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failing_step_skips_its_dependents() {
        let dir = tmp("runner_err");
        let mut s = Session::create(&dir).unwrap();
        let (_, row) = s.patch.add_source(SourceInfo {
            ruta: dir.join("no_existe.wav"),
            hash: "x".into(),
            sr: 8000,
            canales: 1,
            frames: 10,
            bytes: 0,
            modificado: 0,
            marcadores: vec![],
        });
        let reg = surshape_native::registry();
        s.patch.append(row, reg.get("nat.reverse").unwrap().as_ref(), 1).unwrap();
        s.patch.append(row, reg.get("nat.reverse").unwrap().as_ref(), 1).unwrap();
        let r = run(&s, &reg, &RenderOptions::default());
        assert_eq!(r.steps.len(), 2);
        assert!(matches!(&r.steps[0].result, Err(ProcessError::Failed { key: "err.audio.abrir", .. })));
        assert!(matches!(&r.steps[1].result, Err(ProcessError::Failed { key: "err.patch.entrada_con_error", .. })));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn subpatch_equals_its_chain_and_reuses_the_cache() {
        let dir = tmp("runner_sub");
        let mut s = Session::create(&dir).unwrap();
        let src = dir.join("f.wav");
        let x: Vec<f32> = (0..2000).map(|i| (i as f32 * 0.02).sin() * 0.6).collect();
        write_wav(&src, &AudioBuf::from_channels(8000, vec![x]), WavFormat::Float32).unwrap();
        let (_, row) = s.patch.add_source(SourceInfo {
            ruta: src.clone(),
            hash: crate::hash_file(&src, None, None).unwrap(),
            sr: 8000,
            canales: 1,
            frames: 2000,
            bytes: 0,
            modificado: 0,
            marcadores: vec![],
        });
        let reg = surshape_native::registry();
        let a = s.patch.append(row, reg.get("nat.reverse").unwrap().as_ref(), 1).unwrap();
        let b = s.patch.append(row, reg.get("nat.bitcrush").unwrap().as_ref(), 1).unwrap();
        s.patch.node_mut(b).unwrap().params.comun.set("bits", 4.0);
        let pasos = s.patch.copy_cells(&[a, b]);
        let br = s.patch.add_branch(surshape_patch::PortRef::main(s.patch.filas[row].celdas[0])).unwrap();
        let sp = s.patch.append_subpatch(br, "x", "x".into(), &pasos).unwrap();
        let r = run(&s, &reg, &RenderOptions::default());
        assert!(r.steps.iter().all(|st| st.result.is_ok()), "{:?}", r.steps.iter().map(|s| &s.result).collect::<Vec<_>>());
        apply(&mut s, &r);
        let load = |id| decode::load(&dir.join(&s.patch.node(id).unwrap().render.as_ref().unwrap().salidas[0].archivo)).unwrap();
        assert_eq!(load(sp).channels, load(b).channels, "el sub-patch da lo mismo que su cadena");
        // Cambiar solo el último paso: el primero sale de la cache.
        if let NodeKind::SubPatch { pasos, .. } = &mut s.patch.node_mut(sp).unwrap().tipo {
            pasos[1].params.comun.set("bits", 6.0);
        }
        let keys = s.patch.keys(&reg, &RenderOptions::default());
        assert_eq!(s.patch.render_plan(&[], &keys), vec![sp]);
        let r2 = run(&s, &reg, &RenderOptions::default());
        assert!(r2.steps[0].result.is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
