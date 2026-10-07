//! Renderiza cada proceso CDP curado sobre ruido y seno (mono y estéreo), en
//! defaults, extremos de rango y con breakpoints, y verifica lo mismo que
//! los nativos: sin NaN/Inf, duración esperada, nunca > 0 dBFS sin aviso,
//! reproducible. Necesita CDP instalado: si no está, los tests avisan y
//! pasan (la app también funciona sin CDP).

use std::sync::atomic::{AtomicBool, AtomicU32};
use std::sync::Arc;
use surshape_cdp::CdpInstall;
use surshape_engine::render::{run, run_files, FileOut};
use surshape_engine::{
    AudioBuf, Breakpoints, DataSet, FileKind, Interp, ParamSet, ParamValue, ParamValues, Process, ProcessError, Registry,
    RenderJob, RenderOptions, TimeMode,
};

/// Conversiones (no "cambian" el sonido: análisis y resíntesis).
const CONVERSIONS: [&str; 2] = ["cdp.pvoc_anal", "cdp.pvoc_synth"];

static TMP_N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Carpeta temporal que se borra al soltarla.
struct Tmp(std::path::PathBuf);
impl Tmp {
    fn new() -> Self {
        let n = TMP_N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("surshape_cdptest_{}_{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Self(d)
    }
}
impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Un .wav mono por canal.
fn split(a: &AudioBuf, dir: &std::path::Path, pre: &str) -> DataSet {
    let files = a
        .channels
        .iter()
        .enumerate()
        .map(|(ch, x)| {
            let f = dir.join(format!("{pre}_c{ch}.wav"));
            surshape_audio::export::write_wav(&f, &AudioBuf::from_channels(a.sr, vec![x.clone()]), surshape_audio::WavFormat::Float32).unwrap();
            f
        })
        .collect();
    DataSet { kind: FileKind::Wav, files, sr: a.sr, dur: a.duration_secs() }
}

/// Renderiza como lo haría el patch: los procesos de sonido directamente;
/// los de archivos, analizando antes la entrada si piden .ana y
/// resintetizando la salida .ana para obtener sonido.
fn render_any(reg: &Registry, p: &Arc<dyn Process>, inputs: &[Arc<AudioBuf>], params: &ParamValues, seed: u64) -> Result<(AudioBuf, Option<surshape_engine::RenderReport>), ProcessError> {
    let set = ParamSet { comun: params.clone(), por_canal: None };
    let (prog, cancel) = (AtomicU32::new(0), AtomicBool::new(false));
    if !p.works_on_files() {
        let job = RenderJob { process: p.clone(), inputs: inputs.to_vec(), params: set, seed, options: RenderOptions::default() };
        let mut o = run(&job, &prog, &cancel)?;
        return Ok((o.outputs.remove(0), Some(o.report)));
    }
    let tmp = Tmp::new();
    let dir = tmp.0.clone();
    let dest = move |n: usize, ch: usize, k: FileKind| dir.join(format!("o{n}_{ch}_{}.{}", TMP_N.fetch_add(1, std::sync::atomic::Ordering::Relaxed), k.ext()));
    let opts = RenderOptions::default();
    let anal = reg.get("cdp.pvoc_anal").unwrap();
    let synth = reg.get("cdp.pvoc_synth").unwrap();
    let mut ds = Vec::new();
    for (i, a) in inputs.iter().enumerate() {
        let wav = split(a, &tmp.0, &format!("e{i}"));
        ds.push(match p.input_kind(i) {
            FileKind::Ana => match run_files(anal, &[wav], &ParamSet::default(), 0, &opts, &dest, &prog, &cancel)?.outputs.remove(0) {
                FileOut::Data(d) => d,
                FileOut::Audio(_) => panic!("pvoc anal dio sonido"),
            },
            _ => wav,
        });
    }
    let out = run_files(p, &ds, &set, seed, &opts, &dest, &prog, &cancel)?.outputs.remove(0);
    let a = match out {
        FileOut::Audio(a) => a,
        FileOut::Data(d) => match run_files(synth, &[d], &ParamSet::default(), 0, &opts, &dest, &prog, &cancel)?.outputs.remove(0) {
            FileOut::Audio(a) => a,
            FileOut::Data(_) => panic!("pvoc synth dio datos"),
        },
    };
    Ok((a, None))
}

fn registry() -> Option<Registry> {
    // SURSHAPE_CDP_DIR permite probar otra compilación (p. ej. la propia).
    let custom = std::env::var_os("SURSHAPE_CDP_DIR").map(std::path::PathBuf::from);
    let Some(inst) = CdpInstall::find(custom.as_deref(), None) else {
        eprintln!("CDP no está instalado: se omiten los tests de CDP");
        return None;
    };
    eprintln!("CDP: {} ({:?})", inst.dir.display(), inst.version);
    let mut r = Registry::new();
    surshape_cdp::procs::register(&mut r, &Arc::new(inst));
    Some(r)
}

fn noise(sr: u32, secs: f64, ch: usize) -> AudioBuf {
    let n = (sr as f64 * secs) as usize;
    let chans = (0..ch)
        .map(|c| {
            let mut g = noisegek_dsp::noise::NoiseChannel::new(321 + c as u32, 1);
            (0..n).map(|_| (g.white() * 0.4) as f32).collect()
        })
        .collect();
    AudioBuf::from_channels(sr, chans)
}

fn sine(sr: u32, secs: f64) -> AudioBuf {
    let n = (sr as f64 * secs) as usize;
    let x = (0..n).map(|i| (0.5 * (std::f64::consts::TAU * 330.0 * i as f64 / sr as f64).sin()) as f32).collect();
    AudioBuf::from_channels(sr, vec![x])
}

/// Renderiza y verifica. Un error de CDP por un valor que el programa
/// rechaza para ESA entrada (p. ej. más ventanas de las que tiene un archivo
/// corto) es válido si llega como error claro, no como pánico.
fn check(reg: &Registry, p: &Arc<dyn Process>, input: &AudioBuf, params: ParamValues, seed: u64) -> Option<AudioBuf> {
    // Segunda entrada (procesos de dos entradas): un seno, distinto de la primera.
    let inputs: Vec<Arc<AudioBuf>> =
        (0..p.inputs().min()).map(|i| Arc::new(if i == 0 { input.clone() } else { sine(input.sr, 1.2) })).collect();
    let (out, report) = match render_any(reg, p, &inputs, &params, seed) {
        Ok(o) => o,
        Err(ProcessError::Failed { key, args }) if key.starts_with("err.cdp.") && key != "err.cdp.lanzar" => {
            eprintln!("{} rechazó {params:?}: {key} {args:?}", p.id());
            return None;
        }
        Err(e) => panic!("{}: {e} con {params:?}", p.id()),
    };
    let ctx = format!("{} con {params:?}", p.id());
    assert!(out.channels.iter().flatten().all(|x| x.is_finite()), "NaN/Inf en {ctx}");
    assert_eq!(out.num_channels(), input.num_channels(), "canales en {ctx}");
    if let Some(r) = report {
        assert_eq!(r.nonfinite_fixed, 0, "NaN/Inf en {ctx}");
        assert!(r.len_ok, "duración {} != {:?} en {ctx}", r.frames, r.expected_frames);
        if r.peak_raw_db > 0.0 {
            assert!(r.clipped, "pasó 0 dBFS sin aviso en {ctx}");
        }
    }
    Some(out)
}

#[test]
fn every_cdp_process_on_noise_and_sine() {
    let Some(reg) = registry() else { return };
    assert_eq!(reg.all().len(), surshape_cdp::procs::DEFS.len(), "deberían estar todos los procesos del catálogo");
    for p in reg.all() {
        for input in [noise(44100, 1.5, 2), sine(44100, 1.5)] {
            let out = check(&reg, p, &input, ParamValues::defaults(p.params()), 3);
            assert!(out.is_some(), "{} falló con sus valores por defecto", p.id());
        }
    }
}

#[test]
fn every_cdp_process_at_range_extremes_and_with_breakpoints() {
    let Some(reg) = registry() else { return };
    let input = noise(44100, 1.0, 1);
    for p in reg.all() {
        for spec in p.params() {
            for v in [spec.min, spec.max] {
                let mut params = ParamValues::defaults(p.params());
                params.set(spec.id, v);
                check(&reg, p, &input, params, 4);
            }
            if spec.automatable {
                let mut params = ParamValues::defaults(p.params());
                let ramp = Breakpoints::new(TimeMode::Normalizado, Interp::Lineal, vec![(0.0, spec.min), (1.0, spec.default)]);
                params.set_value(spec.id, ParamValue::Envelope(ramp));
                assert!(check(&reg, p, &input, params, 4).is_some(), "{} no aceptó breakpoints en {}", p.id(), spec.id);
            }
        }
    }
}

#[test]
fn cdp_renders_are_reproducible() {
    let Some(reg) = registry() else { return };
    let input = noise(44100, 1.0, 2);
    for p in reg.all() {
        let d = ParamValues::defaults(p.params());
        let a = check(&reg, p, &input, d.clone(), 11).unwrap();
        let b = check(&reg, p, &input, d.clone(), 11).unwrap();
        assert_eq!(a, b, "{} no es reproducible", p.id());
    }
}

#[test]
fn cdp_processes_actually_change_the_sound() {
    let Some(reg) = registry() else { return };
    let input = noise(44100, 1.0, 1);
    for p in reg.all().iter().filter(|p| !CONVERSIONS.contains(&p.id())) {
        let y = check(&reg, p, &input, ParamValues::defaults(p.params()), 1).unwrap();
        let n = y.frames().min(input.frames());
        let diff = (0..n).map(|i| (y.channels[0][i] - input.channels[0][i]).abs()).fold(0.0f32, f32::max);
        assert!(diff > 1e-3, "{} no cambió el audio", p.id());
    }
}

#[test]
fn stretch_time_makes_it_longer() {
    let Some(reg) = registry() else { return };
    let p = reg.get("cdp.stretch_time").unwrap();
    let mut params = ParamValues::defaults(p.params());
    params.set("factor", 3.0);
    let y = check(&reg, p, &sine(44100, 1.0), params, 1).unwrap();
    let secs = y.frames() as f64 / 44100.0;
    assert!((secs - 3.0).abs() < 0.2, "{secs}");
}

/// Con los valores por defecto ningún proceso puede fallar, tampoco con
/// entradas difíciles como las reales: archivo más largo que cualquier
/// duración por defecto, corrimiento de continua y un silencio exacto largo
/// (sin cruces por cero, que rompía los procesos de ciclos de CDP).
#[test]
fn defaults_never_fail_on_hard_input() {
    let Some(reg) = registry() else { return };
    let sr = 44_100;
    let mut a = noise(sr, 30.0, 2);
    for ch in &mut a.channels {
        for s in ch.iter_mut() {
            *s = *s * 0.5 + 0.3;
        }
        ch.extend(std::iter::repeat_n(0.0, sr as usize * 15));
    }
    let a = Arc::new(a);
    for p in reg.all() {
        let inputs: Vec<Arc<AudioBuf>> = (0..p.inputs().min()).map(|_| a.clone()).collect();
        if let Err(e) = render_any(&reg, p, &inputs, &ParamValues::defaults(p.params()), 3) {
            panic!("{} falló con valores por defecto: {e:?}", p.id());
        }
    }
}
