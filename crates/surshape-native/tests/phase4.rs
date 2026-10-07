//! Comportamiento de los procesos de la fase 4: generador de ruido,
//! convolución, síntesis cruzada y granular.

use std::sync::atomic::{AtomicBool, AtomicU32};
use std::sync::Arc;
use surshape_engine::render::run;
use surshape_engine::{AudioBuf, ParamSet, ParamValues, RenderJob, RenderOptions};

fn render(id: &str, inputs: Vec<AudioBuf>, set: &[(&str, f64)], seed: u64) -> AudioBuf {
    let reg = surshape_native::registry();
    let p = reg.get(id).unwrap().clone();
    let mut params = ParamValues::defaults(p.params());
    for (k, v) in set {
        params.set(k, *v);
    }
    let job = RenderJob {
        process: p,
        inputs: inputs.into_iter().map(Arc::new).collect(),
        params: ParamSet { comun: params, por_canal: None },
        seed,
        options: RenderOptions::default(),
    };
    run(&job, &AtomicU32::new(0), &AtomicBool::new(false)).unwrap().outputs.remove(0)
}

fn noise(sr: u32, n: usize) -> AudioBuf {
    let mut g = noisegek_dsp::noise::NoiseChannel::new(5, 1);
    AudioBuf::from_channels(sr, vec![(0..n).map(|_| (g.white() * 0.5) as f32).collect()])
}

fn rms(x: &[f32]) -> f64 {
    (x.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / x.len().max(1) as f64).sqrt()
}

#[test]
fn noise_generator_respects_duration_channels_and_rate() {
    let y = render("nat.noise", vec![], &[("duracion", 1.5), ("canales", 0.0), ("sr", 0.0)], 1);
    assert_eq!((y.sr, y.num_channels(), y.frames()), (44100, 1, 66150));
    let st = render("nat.noise", vec![], &[("duracion", 0.5)], 1);
    assert_eq!((st.sr, st.num_channels()), (48000, 2));
    assert_ne!(st.channels[0], st.channels[1], "estéreo: canales independientes");
    // Los cinco tipos suenan distinto y todos tienen energía.
    let kinds: Vec<AudioBuf> = (0..5).map(|t| render("nat.noise", vec![], &[("duracion", 0.5), ("tipo", t as f64)], 1)).collect();
    for (i, k) in kinds.iter().enumerate() {
        assert!(rms(&k.channels[0]) > 1e-3, "tipo {i} sin energía");
        for j in 0..i {
            assert_ne!(kinds[j].channels[0], k.channels[0], "tipos {j} y {i} iguales");
        }
    }
    // -6 dB menos de nivel = la mitad de amplitud.
    let a = render("nat.noise", vec![], &[("duracion", 0.5), ("tipo", 0.0), ("nivel", -6.0)], 9);
    let b = render("nat.noise", vec![], &[("duracion", 0.5), ("tipo", 0.0), ("nivel", -12.0)], 9);
    assert!((rms(&a.channels[0]) / rms(&b.channels[0]) - 1.995).abs() < 0.01);
}

#[test]
fn convolution_with_an_impulse_is_identity_and_with_a_delayed_one_delays() {
    let x = noise(8000, 4000);
    let mut ir = vec![0.0f32; 100];
    ir[0] = 1.0;
    let y = render("nat.convolve", vec![x.clone(), AudioBuf::from_channels(8000, vec![ir.clone()])], &[], 0);
    assert_eq!(y.frames(), 4099, "con cola: entrada + IR - 1");
    let err = x.channels[0].iter().zip(&y.channels[0]).fold(0.0f32, |m, (a, b)| m.max((a - b).abs()));
    assert!(err < 1e-4, "{err}");
    ir[0] = 0.0;
    ir[99] = 1.0;
    let y = render("nat.convolve", vec![x.clone(), AudioBuf::from_channels(8000, vec![ir])], &[("cola", 0.0)], 0);
    assert_eq!(y.frames(), 4000);
    assert!((y.channels[0][1099] - x.channels[0][1000]).abs() < 1e-4);
}

#[test]
fn convolution_accepts_an_ir_at_another_sample_rate() {
    let x = noise(48000, 4800);
    let mut ir = vec![0.0f32; 441];
    ir[0] = 1.0;
    let y = render("nat.convolve", vec![x, AudioBuf::from_channels(44100, vec![ir])], &[], 0);
    assert_eq!(y.sr, 48000);
}

#[test]
fn cross_synthesis_amount_zero_keeps_the_carrier_and_silent_modulator_silences() {
    let a = noise(8000, 8000);
    let b = noise(8000, 2000);
    let same = render("nat.cross_synth", vec![a.clone(), b.clone()], &[("modo", 0.0), ("cantidad", 0.0)], 0);
    let mid = 1000..7000;
    let err = mid.clone().map(|i| (same.channels[0][i] - a.channels[0][i]).abs()).fold(0.0f32, f32::max);
    assert!(err < 1e-3, "{err}");
    let silent = AudioBuf::silent(8000, 1, 8000);
    let y = render("nat.cross_synth", vec![a, silent], &[("modo", 0.0), ("cantidad", 100.0)], 0);
    assert!(rms(&y.channels[0][mid]) < 1e-4);
}

#[test]
fn granular_fills_the_requested_length_with_sound() {
    let x = noise(8000, 8000);
    let y = render("nat.granular", vec![x], &[("duracion", 3.0)], 2);
    assert_eq!(y.frames(), 24000);
    // Todos los tramos de medio segundo tienen sonido.
    for c in y.channels[0].chunks(4000) {
        assert!(rms(c) > 0.01, "tramo sin granos");
    }
}

#[test]
fn bands_split_into_n_outputs_that_sum_to_the_input() {
    let reg = surshape_native::registry();
    let p = reg.get("nat.band_split").unwrap().clone();
    let x = noise(48000, 24000);
    let mut params = ParamValues::defaults(p.params());
    params.set("bandas", 4.0);
    let job = RenderJob {
        process: p,
        inputs: vec![Arc::new(x.clone())],
        params: ParamSet { comun: params, por_canal: None },
        seed: 0,
        options: RenderOptions::default(),
    };
    let out = run(&job, &AtomicU32::new(0), &AtomicBool::new(false)).unwrap().outputs;
    assert_eq!(out.len(), 4);
    let err = (0..x.frames())
        .map(|i| (out.iter().map(|b| b.channels[0][i]).sum::<f32>() - x.channels[0][i]).abs())
        .fold(0.0f32, f32::max);
    assert!(err < 1e-4, "{err}");
    // Cada banda tiene energía (ruido blanco: todas las frecuencias).
    assert!(out.iter().all(|b| rms(&b.channels[0]) > 0.01));
}

#[test]
fn mix_places_gains_pans_and_offsets() {
    use surshape_native::mix::{Mix, MixSetting};
    let a = AudioBuf::from_channels(8000, vec![vec![0.5; 100]]);
    let b = AudioBuf::from_channels(8000, vec![vec![0.25; 100]]);
    let mix = Mix {
        settings: vec![
            MixSetting { gain_db: 0.0, pan: -1.0, start: 0.0 },
            MixSetting { gain_db: -6.0206, pan: 1.0, start: 0.01 },
        ],
    };
    let job = RenderJob {
        process: Arc::new(mix),
        inputs: vec![Arc::new(a), Arc::new(b)],
        params: ParamSet::default(),
        seed: 0,
        options: RenderOptions::default(),
    };
    let y = run(&job, &AtomicU32::new(0), &AtomicBool::new(false)).unwrap().outputs.remove(0);
    assert_eq!((y.num_channels(), y.frames()), (2, 180), "80 muestras de retraso + 100");
    assert!((y.channels[0][10] - 0.5).abs() < 1e-5 && y.channels[1][10].abs() < 1e-5, "A todo a la izquierda");
    assert!((y.channels[1][150] - 0.125).abs() < 1e-4 && y.channels[0][150].abs() < 1e-5, "B a la derecha, -6 dB");
}
