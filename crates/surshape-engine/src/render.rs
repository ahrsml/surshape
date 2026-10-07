//! Render: corre un proceso con progreso y cancelación y valida cada salida
//! (NaN/Inf, duración, pico; limitador opcional).
//!
//! [`run`] es la versión síncrona (la usan los tests y los hilos de trabajo);
//! [`spawn`] la lanza en un hilo ([`Task`]) y la interfaz la consulta en cada
//! cuadro sin bloquearse.
//!
//! Si el proceso admite parámetros por canal y el juego trae valores por
//! canal, cada canal se procesa por separado (como mono) con sus valores y
//! las salidas se vuelven a juntar.

use crate::hash::StableHasher;
use crate::params::{ParamSet, ParamValues};
use crate::data::{DataSet, FileKind};
use crate::process::{FileCtx, Outputs, Process, ProcessError, RenderCtx};
use crate::task::Task;
use crate::MAX_RENDER_SAMPLES;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use surshape_audio::analysis::{sanitize, Analysis};
use surshape_audio::limiter::{limit, LimiterSettings};
use surshape_audio::AudioBuf;

/// Análisis PVOC por defecto (auto-conversión de sonido a .ana).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PvocSettings {
    /// Puntos de análisis (canales de frecuencia x 2).
    pub puntos: u32,
    /// Superposición de ventanas (1..4).
    pub superposicion: u32,
}

impl Default for PvocSettings {
    fn default() -> Self {
        Self { puntos: 1024, superposicion: 3 }
    }
}

impl PvocSettings {
    pub fn hash_into(&self, h: &mut StableHasher) {
        h.u64(self.puntos as u64);
        h.u64(self.superposicion as u64);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RenderOptions {
    /// Limitador al final (None = sin limitador).
    pub limiter: Option<LimiterSettings>,
    /// Máximo de muestras de salida (canales x frames).
    pub max_samples: usize,
    /// Análisis para la auto-conversión (solo entra en la clave de los
    /// procesos que leen .ana).
    pub pvoc: PvocSettings,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self { limiter: None, max_samples: MAX_RENDER_SAMPLES, pvoc: PvocSettings::default() }
    }
}

impl RenderOptions {
    /// Lo que cambia el audio resultante entra al hash de la cache.
    pub fn hash_into(&self, h: &mut StableHasher) {
        match &self.limiter {
            None => h.u8(0),
            Some(l) => {
                h.u8(1);
                h.f64(l.ceiling_db);
                h.f64(l.lookahead_ms);
                h.f64(l.release_ms);
            }
        }
    }
}

#[derive(Clone)]
pub struct RenderJob {
    pub process: Arc<dyn Process>,
    pub inputs: Vec<Arc<AudioBuf>>,
    pub params: ParamSet,
    pub seed: u64,
    pub options: RenderOptions,
}

/// Lo que se verificó del resultado (sumado sobre todas las salidas).
#[derive(Clone, Debug, PartialEq)]
pub struct RenderReport {
    /// Muestras NaN/Inf que el proceso produjo (se reemplazaron por 0).
    pub nonfinite_fixed: usize,
    /// Pico de la salida cruda del proceso, en dBFS.
    pub peak_raw_db: f64,
    /// ¿La salida cruda superaba 0 dBFS?
    pub clipped: bool,
    /// Reducción máxima del limitador en dB (None si estaba apagado o no
    /// tuvo que actuar).
    pub limited_db: Option<f64>,
    /// Pico final en dBFS.
    pub peak_db: f64,
    /// Duración esperada y real de la salida principal.
    pub expected_frames: Option<usize>,
    pub frames: usize,
    /// ¿La duración coincide con la esperada?
    pub len_ok: bool,
    pub elapsed: Duration,
}

impl RenderReport {
    /// ¿La salida final supera 0 dBFS? (solo posible sin limitador)
    pub fn over_0dbfs(&self) -> bool {
        self.peak_db > 0.0
    }
}

#[derive(Clone, Debug)]
pub struct RenderOutput {
    /// Salidas; la 0 es la principal.
    pub outputs: Vec<AudioBuf>,
    pub report: RenderReport,
    /// Parámetros efectivos (completos y en rango).
    pub params: ParamSet,
    pub seed: u64,
}

static WORKDIR_N: AtomicUsize = AtomicUsize::new(0);

/// Carpeta temporal propia de un render; se borra al soltarla.
struct WorkDir(PathBuf);

impl WorkDir {
    fn new() -> Result<Self, ProcessError> {
        let n = WORKDIR_N.fetch_add(1, Ordering::Relaxed);
        let p = std::env::temp_dir().join("surshape").join(format!("{}_{}", std::process::id(), n));
        std::fs::create_dir_all(&p).map_err(|e| ProcessError::new("err.render.temporal").arg("detalle", e))?;
        Ok(Self(p))
    }
}

impl Drop for WorkDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn fmt_secs(s: f64) -> String {
    let s = s.max(0.0);
    if s >= 3600.0 {
        format!("{}:{:02}:{:02}", (s / 3600.0) as u64, ((s % 3600.0) / 60.0) as u64, (s % 60.0) as u64)
    } else {
        format!("{}:{:02}", (s / 60.0) as u64, (s % 60.0) as u64)
    }
}

/// Una pasada del proceso (protegida contra pánicos).
fn call(
    proc_: &dyn Process,
    inputs: &[Arc<AudioBuf>],
    params: &ParamValues,
    seed: u64,
    progress: &AtomicU32,
    cancel: &AtomicBool,
    work: &WorkDir,
) -> Result<Vec<AudioBuf>, ProcessError> {
    let mut ctx = RenderCtx { inputs, params, seed, progress, cancel, workdir: &work.0 };
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| proc_.process(&mut ctx))) {
        Ok(r) => r,
        Err(_) => Err(ProcessError::new("err.render.panico").arg("proceso", proc_.id())),
    }
}

/// Junta salidas mono (una por canal) en salidas multicanal. Si los largos
/// difieren, completa con silencio.
fn merge_channels(per_ch: Vec<Vec<AudioBuf>>, sr: u32) -> Vec<AudioBuf> {
    let n_out = per_ch.iter().map(|v| v.len()).min().unwrap_or(0);
    (0..n_out)
        .map(|o| {
            let len = per_ch.iter().map(|v| v[o].frames()).max().unwrap_or(0);
            let chans = per_ch
                .iter()
                .flat_map(|v| v[o].channels.iter().cloned())
                .map(|mut c| {
                    c.resize(len, 0.0);
                    c
                })
                .collect();
            AudioBuf::from_channels(sr, chans)
        })
        .collect()
}

/// Render síncrono.
pub fn run(job: &RenderJob, progress: &AtomicU32, cancel: &AtomicBool) -> Result<RenderOutput, ProcessError> {
    let t0 = Instant::now();
    let proc_ = job.process.as_ref();
    if !proc_.inputs().accepts(job.inputs.len()) {
        return Err(ProcessError::new("err.render.entradas").arg("n", proc_.inputs().min()));
    }
    if job.inputs.iter().any(|b| b.is_empty()) {
        return Err(ProcessError::new("err.render.entrada_vacia"));
    }
    let params = job.params.completed(proc_.params());
    // Las entradas secundarias se llevan a la frecuencia de la principal.
    let inputs: Vec<Arc<AudioBuf>> = match job.inputs.first() {
        Some(first) => {
            let sr = first.sr;
            job.inputs
                .iter()
                .map(|b| if b.sr == sr { b.clone() } else { Arc::new(surshape_audio::resample::resample(b, sr)) })
                .collect()
        }
        None => Vec::new(),
    };
    let refs: Vec<&AudioBuf> = inputs.iter().map(|b| b.as_ref()).collect();
    let rule = proc_.expected_len(&refs, &params.comun);
    let in_frames = refs.first().map_or(0, |b| b.frames());
    let expected = rule.expected(in_frames);
    if let Some(e) = expected {
        let ch = refs.iter().map(|b| b.num_channels()).max().unwrap_or(2);
        if e.saturating_mul(ch) > job.options.max_samples {
            let sr = refs.first().map_or(48000.0, |b| b.sr as f64);
            return Err(ProcessError::new("err.render.demasiado_largo")
                .arg("duracion", fmt_secs(e as f64 / sr))
                .arg("maximo", fmt_secs(job.options.max_samples as f64 / ch as f64 / sr)));
        }
    }

    let work = WorkDir::new()?;
    progress.store(0, Ordering::Relaxed);
    let nch = refs.first().map_or(0, |b| b.num_channels());
    let split = proc_.per_channel() && params.por_canal.is_some() && nch > 1;
    let mut outputs = if split {
        let mut per_ch = Vec::with_capacity(nch);
        for ch in 0..nch {
            // Cada entrada aporta su canal `ch` (o el último que tenga).
            let mono: Vec<Arc<AudioBuf>> = inputs
                .iter()
                .map(|b| {
                    let c = ch.min(b.num_channels() - 1);
                    Arc::new(AudioBuf::from_channels(b.sr, vec![b.channels[c].clone()]))
                })
                .collect();
            per_ch.push(call(proc_, &mono, params.for_channel(ch), job.seed, progress, cancel, &work)?);
        }
        merge_channels(per_ch, refs[0].sr)
    } else {
        call(proc_, &inputs, &params.comun, job.seed, progress, cancel, &work)?
    };
    if cancel.load(Ordering::Relaxed) {
        return Err(ProcessError::Cancelled);
    }
    if outputs.is_empty() || outputs.iter().any(|o| o.is_empty()) {
        return Err(ProcessError::new("err.render.salida_vacia"));
    }
    if let Outputs::Fixed(n) = proc_.outputs() {
        if outputs.len() != n {
            return Err(ProcessError::new("err.render.salidas").arg("n", n).arg("real", outputs.len()));
        }
    }

    let mut report = validate(&mut outputs, &job.options);
    report.expected_frames = expected;
    report.len_ok = rule.matches(in_frames, report.frames);
    report.elapsed = t0.elapsed();
    progress.store(1_000_000, Ordering::Relaxed);
    Ok(RenderOutput { report, outputs, params, seed: job.seed })
}

/// Validación de cada salida de audio: NaN/Inf, pico, limitador.
fn validate(outputs: &mut [AudioBuf], options: &RenderOptions) -> RenderReport {
    let mut nonfinite_fixed = 0;
    let (mut raw_peak, mut fin_peak) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    let mut clipped = false;
    let mut limited_db: Option<f64> = None;
    for audio in outputs.iter_mut() {
        nonfinite_fixed += sanitize(audio);
        let raw = Analysis::of(audio);
        raw_peak = raw_peak.max(raw.peak_db());
        clipped |= raw.clips();
        if let Some(ls) = options.limiter {
            let red = limit(audio, &ls);
            if red < 0.0 {
                limited_db = Some(limited_db.map_or(red, |d| d.min(red)));
            }
        }
        fin_peak = fin_peak.max(Analysis::of(audio).peak_db());
    }
    RenderReport {
        nonfinite_fixed,
        peak_raw_db: raw_peak,
        clipped,
        limited_db,
        peak_db: fin_peak,
        expected_frames: None,
        frames: outputs.first().map_or(0, |o| o.frames()),
        len_ok: true,
        elapsed: Duration::ZERO,
    }
}

/// Salida de un proceso con archivos: sonido (ya en memoria y validado) o
/// datos (archivos movidos a su lugar definitivo).
#[derive(Clone, Debug)]
pub enum FileOut {
    Audio(AudioBuf),
    Data(DataSet),
}

#[derive(Clone, Debug)]
pub struct FileRenderOutput {
    pub outputs: Vec<FileOut>,
    /// Informe de las salidas de sonido (vacío si solo hay datos).
    pub report: RenderReport,
    pub params: ParamSet,
}

/// Mueve (o copia, si está en otro disco) un archivo.
fn move_file(from: &Path, to: &Path) -> Result<(), ProcessError> {
    if let Some(d) = to.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    std::fs::copy(from, to).map(|_| ()).map_err(|e| ProcessError::new("err.render.temporal").arg("detalle", e))
}

/// Render de un proceso que trabaja con archivos ([`Process::process_files`]).
/// Las salidas de datos se mueven a `dest(salida, canal, tipo)`; las de
/// sonido se leen, se juntan por canal y se validan como en [`run`].
#[allow(clippy::too_many_arguments)]
pub fn run_files(
    process: &Arc<dyn Process>,
    inputs: &[DataSet],
    params: &ParamSet,
    seed: u64,
    options: &RenderOptions,
    dest: &dyn Fn(usize, usize, FileKind) -> PathBuf,
    progress: &AtomicU32,
    cancel: &AtomicBool,
) -> Result<FileRenderOutput, ProcessError> {
    let t0 = Instant::now();
    let proc_ = process.as_ref();
    if !proc_.inputs().accepts(inputs.len()) {
        return Err(ProcessError::new("err.render.entradas").arg("n", proc_.inputs().min()));
    }
    for (i, d) in inputs.iter().enumerate() {
        let want = proc_.input_kind(i);
        if d.kind != want {
            return Err(ProcessError::new("err.render.tipo_entrada").arg("esperado", want.ext()).arg("recibido", d.kind.ext()));
        }
        if d.files.is_empty() {
            return Err(ProcessError::new("err.render.entrada_vacia"));
        }
    }
    let params = params.completed(proc_.params());
    let work = WorkDir::new()?;
    progress.store(0, Ordering::Relaxed);
    let nch = inputs.first().map_or(1, |d| d.channels());
    let call = |ins: &[DataSet], values: &crate::params::ParamValues, sub: &Path| -> Result<Vec<DataSet>, ProcessError> {
        let _ = std::fs::create_dir_all(sub);
        let mut ctx = FileCtx { inputs: ins, params: values, seed, progress, cancel, workdir: sub };
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| proc_.process_files(&mut ctx))) {
            Ok(r) => r,
            Err(_) => Err(ProcessError::new("err.render.panico").arg("proceso", proc_.id())),
        }
    };
    let split = proc_.per_channel() && params.por_canal.is_some() && nch > 1;
    let outs: Vec<DataSet> = if split {
        // Cada canal con sus valores; luego se juntan los archivos.
        let mut per_ch: Vec<Vec<DataSet>> = Vec::new();
        for ch in 0..nch {
            let mono: Vec<DataSet> = inputs
                .iter()
                .map(|d| DataSet { files: vec![d.files[ch.min(d.files.len() - 1)].clone()], ..d.clone() })
                .collect();
            per_ch.push(call(&mono, params.for_channel(ch), &work.0.join(format!("c{ch}")))?);
        }
        let n_out = per_ch.iter().map(|v| v.len()).min().unwrap_or(0);
        (0..n_out)
            .map(|o| DataSet { files: per_ch.iter().flat_map(|v| v[o].files.clone()).collect(), ..per_ch[0][o].clone() })
            .collect()
    } else {
        call(inputs, &params.comun, &work.0)?
    };
    if cancel.load(Ordering::Relaxed) {
        return Err(ProcessError::Cancelled);
    }
    if outs.is_empty() || outs.iter().any(|o| o.files.is_empty() || o.files.iter().any(|f| !f.is_file())) {
        return Err(ProcessError::new("err.render.salida_vacia"));
    }
    let mut slots = Vec::with_capacity(outs.len());
    let mut audio = Vec::new();
    for (n, o) in outs.into_iter().enumerate() {
        if o.kind == FileKind::Wav {
            let mut chans: Vec<Vec<f32>> = Vec::new();
            let mut sr = o.sr;
            for f in &o.files {
                let b = surshape_audio::decode::load(f)?;
                sr = b.sr;
                chans.extend(b.channels);
            }
            let len = chans.iter().map(|c| c.len()).max().unwrap_or(0);
            chans.iter_mut().for_each(|c| c.resize(len, 0.0));
            audio.push(AudioBuf::from_channels(sr, chans));
            slots.push(None);
        } else {
            let mut files = Vec::with_capacity(o.files.len());
            for (ch, f) in o.files.iter().enumerate() {
                let to = dest(n, ch, o.kind);
                move_file(f, &to)?;
                files.push(to);
            }
            slots.push(Some(DataSet { files, ..o }));
        }
    }
    let mut report = validate(&mut audio, options);
    report.elapsed = t0.elapsed();
    let mut audio = audio.into_iter();
    let outputs = slots
        .into_iter()
        .map(|o| match o {
            Some(d) => FileOut::Data(d),
            None => FileOut::Audio(audio.next().expect("salida de sonido")),
        })
        .collect();
    progress.store(1_000_000, Ordering::Relaxed);
    Ok(FileRenderOutput { outputs, report, params })
}

/// Lanza un render en un hilo aparte.
pub fn spawn(job: RenderJob) -> Task<RenderOutput> {
    Task::spawn("surshape-render", move |p, c| run(&job, p, c))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::ParamSpec;
    use crate::process::{Family, LenRule};

    /// Proceso de prueba: ganancia, y opcionalmente NaN.
    struct Gain;
    const P: [ParamSpec; 2] =
        [ParamSpec::float("g", "x.g", 0.0, 10.0, 2.0), ParamSpec::toggle("nan", "x.nan", false)];
    impl Process for Gain {
        fn id(&self) -> &'static str {
            "test.gain"
        }
        fn family(&self) -> Family {
            Family::Utilidad
        }
        fn params(&self) -> &[ParamSpec] {
            &P
        }
        fn per_channel(&self) -> bool {
            true
        }
        fn expected_len(&self, _: &[&AudioBuf], _: &ParamValues) -> LenRule {
            LenRule::Same
        }
        fn process(&self, ctx: &mut RenderCtx) -> Result<Vec<AudioBuf>, ProcessError> {
            let mut b = ctx.input(0).clone();
            b.apply_gain(ctx.p("g") as f32);
            if ctx.flag("nan") {
                b.channels[0][3] = f32::NAN;
            }
            ctx.check()?;
            Ok(vec![b])
        }
    }

    fn job(g: f64, nan: bool, limiter: bool) -> RenderJob {
        let sine: Vec<f32> = (0..4800).map(|i| (i as f32 * 0.05).sin() * 0.9).collect();
        let mut params = ParamSet::default();
        params.comun.set("g", g);
        params.comun.set("nan", if nan { 1.0 } else { 0.0 });
        RenderJob {
            process: Arc::new(Gain),
            inputs: vec![Arc::new(AudioBuf::from_channels(48000, vec![sine]))],
            params,
            seed: 1,
            options: RenderOptions { limiter: limiter.then(LimiterSettings::default), ..Default::default() },
        }
    }

    #[test]
    fn validation_reports_clipping_nan_and_limits() {
        let (p, c) = (AtomicU32::new(0), AtomicBool::new(false));
        let out = run(&job(2.0, true, false), &p, &c).unwrap();
        assert_eq!(out.report.nonfinite_fixed, 1);
        assert!(out.report.clipped && out.report.over_0dbfs());
        assert!(out.report.len_ok);
        let out = run(&job(2.0, false, true), &p, &c).unwrap();
        assert!(out.report.clipped && !out.report.over_0dbfs());
        assert!(out.report.limited_db.is_some());
    }

    #[test]
    fn per_channel_values_are_applied_to_each_channel() {
        let (p, c) = (AtomicU32::new(0), AtomicBool::new(false));
        let mut j = job(1.0, false, false);
        j.inputs = vec![Arc::new(AudioBuf::from_channels(48000, vec![vec![0.1; 10], vec![0.1; 10]]))];
        let (mut l, mut r) = (ParamValues::default(), ParamValues::default());
        l.set("g", 2.0);
        r.set("g", 3.0);
        j.params.por_canal = Some(vec![l, r]);
        let out = run(&j, &p, &c).unwrap();
        assert!((out.outputs[0].channels[0][0] - 0.2).abs() < 1e-6);
        assert!((out.outputs[0].channels[1][0] - 0.3).abs() < 1e-6);
    }

    #[test]
    fn cancellation_and_thread() {
        let (p, c) = (AtomicU32::new(0), AtomicBool::new(true));
        assert_eq!(run(&job(1.0, false, false), &p, &c).unwrap_err(), ProcessError::Cancelled);
        let mut h = spawn(job(0.5, false, false));
        let r = loop {
            if let Some(r) = h.poll() {
                break r;
            }
            std::thread::yield_now();
        };
        assert!(!r.unwrap().report.clipped);
    }

    #[test]
    fn too_long_is_refused() {
        let (p, c) = (AtomicU32::new(0), AtomicBool::new(false));
        let mut j = job(1.0, false, false);
        j.options.max_samples = 100;
        match run(&j, &p, &c) {
            Err(ProcessError::Failed { key, .. }) => assert_eq!(key, "err.render.demasiado_largo"),
            r => panic!("{r:?}"),
        }
    }
}
