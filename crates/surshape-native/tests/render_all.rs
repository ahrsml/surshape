//! Renderiza cada proceso nativo sobre ruido y sobre un seno, con sus
//! parámetros por defecto y en los extremos de cada rango, y verifica:
//!   - que el proceso no produzca NaN/Inf;
//!   - que la duración sea la esperada;
//!   - que nunca se pase de 0 dBFS sin aviso (y que con limitador no se pase).
//! También verifica que la seed haga los renders reproducibles, que cada
//! parámetro automatizable soporte una rampa de breakpoints de punta a punta
//! de su rango, y los valores por canal.

use noisegek_dsp::noise::NoiseChannel;
use std::sync::atomic::{AtomicBool, AtomicU32};
use std::sync::Arc;
use surshape_audio::limiter::LimiterSettings;
use surshape_engine::render::run;
use surshape_engine::{
    AudioBuf, Breakpoints, Interp, ParamSet, ParamValue, ParamValues, Process, ProcessError, RenderJob, RenderOptions,
    TimeMode,
};

fn noise(sr: u32, secs: f64, ch: usize) -> AudioBuf {
    let n = (sr as f64 * secs) as usize;
    let chans = (0..ch)
        .map(|c| {
            let mut g = NoiseChannel::new(1234 + c as u32 * 77, 1);
            (0..n).map(|_| (g.white() * 0.5) as f32).collect()
        })
        .collect();
    AudioBuf::from_channels(sr, chans)
}

fn sine(sr: u32, secs: f64) -> AudioBuf {
    let n = (sr as f64 * secs) as usize;
    let x = (0..n)
        .map(|i| (0.5 * (2.0 * std::f64::consts::PI * 440.0 * i as f64 / sr as f64).sin()) as f32)
        .collect();
    AudioBuf::from_channels(sr, vec![x])
}

/// Renderiza y verifica; devuelve el audio (None si el render se rechazó por
/// largo, que es un resultado válido y explícito).
fn check(p: &Arc<dyn Process>, input: &AudioBuf, params: ParamValues, seed: u64, limiter: bool) -> Option<AudioBuf> {
    check_set(p, input, ParamSet { comun: params, por_canal: None }, seed, limiter)
}

/// Entradas que pide el proceso: ninguna (generador), la de prueba, y una
/// segunda corta (IR, modulador) si hace falta.
fn inputs_for(p: &Arc<dyn Process>, input: &AudioBuf) -> Vec<Arc<AudioBuf>> {
    (0..p.inputs().min())
        .map(|i| Arc::new(if i == 0 { input.clone() } else { noise(input.sr, 0.2, 1) }))
        .collect()
}

fn check_set(p: &Arc<dyn Process>, input: &AudioBuf, params: ParamSet, seed: u64, limiter: bool) -> Option<AudioBuf> {
    let job = RenderJob {
        process: p.clone(),
        inputs: inputs_for(p, input),
        params: params.clone(),
        seed,
        options: RenderOptions { limiter: limiter.then(LimiterSettings::default), ..Default::default() },
    };
    let out = match run(&job, &AtomicU32::new(0), &AtomicBool::new(false)) {
        Ok(o) => o,
        Err(ProcessError::Failed { key: "err.render.demasiado_largo", .. }) => return None,
        Err(e) => panic!("{}: {e} con {params:?}", p.id()),
    };
    let r = &out.report;
    let ctx = format!("{} con {:?} (limitador {limiter})", p.id(), out.params);
    assert_eq!(r.nonfinite_fixed, 0, "NaN/Inf en {ctx}");
    assert!(r.len_ok, "duración {} != {:?} en {ctx}", r.frames, r.expected_frames);
    if let surshape_engine::Outputs::Fixed(n) = p.outputs() {
        assert_eq!(out.outputs.len(), n, "salidas en {ctx}");
    }
    for o in &out.outputs {
        assert_eq!(o.frames(), out.outputs[0].frames(), "todas las salidas del mismo largo en {ctx}");
    }
    if !p.inputs().is_generator() {
        assert_eq!(out.outputs[0].num_channels(), input.num_channels(), "canales en {ctx}");
    }
    // Pasarse de 0 dBFS solo está permitido con aviso.
    if r.peak_raw_db > 0.0 {
        assert!(r.clipped, "pasó 0 dBFS sin aviso en {ctx}");
    }
    if limiter {
        assert!(r.peak_db <= -0.29, "el limitador dejó pasar {} dBFS en {ctx}", r.peak_db);
    }
    Some(out.outputs.into_iter().next().unwrap())
}

fn ramp(lo: f64, hi: f64) -> ParamValue {
    ParamValue::Envelope(Breakpoints::new(TimeMode::Normalizado, Interp::Lineal, vec![(0.0, lo), (0.5, hi), (1.0, lo)]))
}

#[test]
fn every_process_on_noise_and_sine() {
    let reg = surshape_native::registry();
    let inputs = [noise(48000, 1.5, 2), sine(48000, 1.0)];
    for p in reg.all() {
        for input in &inputs {
            for limiter in [false, true] {
                check(p, input, ParamValues::defaults(p.params()), 7, limiter);
            }
        }
    }
}

#[test]
fn every_process_at_range_extremes() {
    // Entradas cortas a 8 kHz: los extremos (p. ej. 10000x) siguen siendo
    // baratos de calcular.
    let reg = surshape_native::registry();
    let inputs = [noise(8000, 0.1, 2), sine(8000, 0.1)];
    for p in reg.all() {
        for spec in p.params() {
            for v in [spec.min, spec.max] {
                let mut params = ParamValues::defaults(p.params());
                params.set(spec.id, v);
                for input in &inputs {
                    check(p, input, params.clone(), 99, false);
                }
            }
        }
    }
}

#[test]
fn seed_makes_renders_reproducible() {
    let reg = surshape_native::registry();
    let input = noise(48000, 0.5, 2);
    for p in reg.all() {
        let d = ParamValues::defaults(p.params());
        let a = check(p, &input, d.clone(), 42, false).unwrap();
        let b = check(p, &input, d.clone(), 42, false).unwrap();
        assert_eq!(a, b, "{} no es reproducible con la misma seed", p.id());
        if p.uses_seed() {
            let c = check(p, &input, d, 43, false).unwrap();
            assert_ne!(a, c, "{} ignora la seed", p.id());
        }
    }
}

#[test]
fn paulstretch_keeps_level_and_spectrum_of_a_sine() {
    let reg = surshape_native::registry();
    let p = reg.get("nat.paulstretch").unwrap();
    let input = sine(48000, 1.0);
    let mut params = ParamValues::defaults(p.params());
    params.set("factor", 4.0);
    let out = check(p, &input, params, 1, false).unwrap();
    assert_eq!(out.frames(), 192_000);
    // RMS del tramo central parecido al de entrada (0,5 / √2 ≈ 0,354)
    let mid = &out.channels[0][48_000..144_000];
    let rms = (mid.iter().map(|x| (*x as f64).powi(2)).sum::<f64>() / mid.len() as f64).sqrt();
    assert!((rms - 0.354).abs() < 0.12, "rms {rms}");
}

#[test]
fn reverse_and_normalize_are_exact() {
    let reg = surshape_native::registry();
    let input = AudioBuf::from_channels(48000, vec![vec![0.1, -0.25, 0.05]]);
    let r = check(reg.get("nat.reverse").unwrap(), &input, ParamValues::default(), 0, false).unwrap();
    assert_eq!(r.channels[0], vec![0.05, -0.25, 0.1]);
    let mut p = ParamValues::default();
    p.set("pico", -6.0);
    let n = check(reg.get("nat.normalize").unwrap(), &input, p, 0, false).unwrap();
    let peak = n.channels[0].iter().fold(0.0f32, |m, x| m.max(x.abs()));
    assert!((peak - 0.501187).abs() < 1e-5, "{peak}");
}

#[test]
fn every_automatable_param_accepts_a_full_range_ramp() {
    let reg = surshape_native::registry();
    let inputs = [noise(8000, 0.2, 2), sine(8000, 0.2)];
    for p in reg.all() {
        for spec in p.params().iter().filter(|s| s.automatable) {
            let mut params = ParamValues::defaults(p.params());
            params.set_value(spec.id, ramp(spec.min, spec.max));
            for input in &inputs {
                check(p, input, params.clone(), 5, false);
            }
        }
    }
}

#[test]
fn paulstretch_envelope_length_follows_the_curve() {
    let reg = surshape_native::registry();
    let p = reg.get("nat.paulstretch").unwrap();
    let input = sine(8000, 1.0);
    let mut params = ParamValues::defaults(p.params());
    params.set_value("factor", ParamValue::Envelope(Breakpoints::new(TimeMode::Normalizado, Interp::Lineal, vec![(0.0, 2.0), (1.0, 6.0)])));
    let out = check(p, &input, params, 1, false).unwrap();
    // Integral de 2 a 6 lineal sobre 1 s = 4 s (con tolerancia de cuadros).
    let secs = out.frames() as f64 / 8000.0;
    assert!((secs - 4.0).abs() < 0.3, "{secs}");
}

#[test]
fn per_channel_values_differ_between_channels() {
    let reg = surshape_native::registry();
    for id in ["nat.bitcrush", "nat.waveshaper"] {
        let p = reg.get(id).unwrap();
        assert!(p.per_channel());
        let input = noise(8000, 0.2, 2);
        let mut l = ParamValues::defaults(p.params());
        let mut r = l.clone();
        l.set("mezcla", 0.0);
        r.set("mezcla", 100.0);
        let set = ParamSet { comun: ParamValues::defaults(p.params()), por_canal: Some(vec![l, r]) };
        let out = check_set(p, &input, set, 3, false).unwrap();
        assert_eq!(out.channels[0], input.channels[0], "{id}: mezcla 0 deja el canal intacto");
        assert_ne!(out.channels[1], input.channels[1], "{id}: mezcla 100 lo cambia");
    }
}
