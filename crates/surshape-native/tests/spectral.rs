//! Tests de comportamiento de los procesos espectrales: que hagan lo que
//! dicen (no solo que no fallen; eso lo cubre render_all.rs).

use rustfft::num_complex::Complex32;
use std::sync::atomic::{AtomicBool, AtomicU32};
use std::sync::Arc;
use surshape_engine::render::run;
use surshape_engine::{AudioBuf, ParamSet, ParamValues, RenderJob, RenderOptions};
use surshape_native::spectral::{rms, Analyzer};

const SR: u32 = 48000;

fn sine(f: f32, secs: f32) -> AudioBuf {
    let n = (SR as f32 * secs) as usize;
    let x = (0..n).map(|i| 0.5 * (std::f32::consts::TAU * f * i as f32 / SR as f32).sin()).collect();
    AudioBuf::from_channels(SR, vec![x])
}

fn noise(secs: f32) -> AudioBuf {
    let mut g = noisegek_dsp::noise::NoiseChannel::new(99, 1);
    let n = (SR as f32 * secs) as usize;
    AudioBuf::from_channels(SR, vec![(0..n).map(|_| (g.white() * 0.5) as f32).collect()])
}

fn render(id: &str, input: &AudioBuf, set: &[(&str, f64)]) -> AudioBuf {
    let reg = surshape_native::registry();
    let p = reg.get(id).unwrap().clone();
    let mut params = ParamValues::defaults(p.params());
    for (k, v) in set {
        params.set(k, *v);
    }
    let job = RenderJob {
        process: p,
        inputs: vec![Arc::new(input.clone())],
        params: ParamSet { comun: params, por_canal: None },
        seed: 7,
        options: RenderOptions::default(),
    };
    run(&job, &AtomicU32::new(0), &AtomicBool::new(false)).unwrap().outputs.remove(0)
}

/// Frecuencia dominante en el tramo central.
fn dominant(x: &[f32]) -> f32 {
    let n = 8192;
    let mut an = Analyzer::new(n);
    let mut h = vec![Complex32::default(); an.bins()];
    an.frame(x, (x.len() / 2 - n / 2) as i64, &mut h);
    let k = (1..h.len()).max_by(|a, b| h[*a].norm().total_cmp(&h[*b].norm())).unwrap();
    k as f32 * SR as f32 / n as f32
}

fn mid(x: &[f32]) -> &[f32] {
    &x[x.len() / 4..x.len() * 3 / 4]
}

#[test]
fn shift_moves_a_sine_by_hz() {
    let y = render("nat.spec_shift", &sine(1000.0, 1.0), &[("hz", 500.0)]);
    let f = dominant(&y.channels[0]);
    assert!((f - 1500.0).abs() < 15.0, "{f}");
}

#[test]
fn stretch_maps_frequencies_above_the_anchor() {
    let y = render("nat.spec_stretch", &sine(1000.0, 1.0), &[("factor", 2.0), ("ancla", 100.0)]);
    let f = dominant(&y.channels[0]);
    assert!((f - 1900.0).abs() < 15.0, "{f}");
}

#[test]
fn neutral_settings_are_transparent() {
    let x = noise(0.5);
    for (id, set) in [
        ("nat.bin_shuffle", vec![("cantidad", 0.0)]),
        ("nat.spec_gate", vec![("umbral", -80.0)]),
    ] {
        let y = render(id, &x, &set);
        let a = mid(&x.channels[0]);
        let b = mid(&y.channels[0]);
        let err = a.iter().zip(b).fold(0.0f32, |m, (p, q)| m.max((p - q).abs()));
        assert!(err < 1e-3, "{id}: {err}");
    }
}

#[test]
fn gate_keeps_strong_bins_and_inverted_keeps_the_rest() {
    // Seno fuerte + ruido débil: con umbral alto queda casi solo el seno.
    let s = sine(2000.0, 1.0);
    let n = noise(1.0);
    let mix: Vec<f32> = s.channels[0].iter().zip(&n.channels[0]).map(|(a, b)| a + b * 0.05).collect();
    let x = AudioBuf::from_channels(SR, vec![mix]);
    let kept = render("nat.spec_gate", &x, &[("umbral", -20.0)]);
    let inv = render("nat.spec_gate", &x, &[("umbral", -20.0), ("invertir", 1.0)]);
    assert!((dominant(&kept.channels[0]) - 2000.0).abs() < 10.0);
    assert!(rms(mid(&inv.channels[0])) < rms(mid(&kept.channels[0])) * 0.3);
}

#[test]
fn freeze_has_requested_length_and_keeps_level() {
    let x = sine(440.0, 2.0);
    let y = render("nat.freeze", &x, &[("duracion", 3.0)]);
    assert_eq!(y.frames(), 3 * SR as usize);
    let (a, b) = (rms(mid(&x.channels[0])), rms(mid(&y.channels[0])));
    assert!((b / a - 1.0).abs() < 0.3, "{a} {b}");
    assert!((dominant(&y.channels[0]) - 440.0).abs() < 15.0);
}

#[test]
fn robotize_imposes_its_pitch() {
    let y = render("nat.robotize", &noise(1.0), &[("hz", 200.0)]);
    // Autocorrelación: máximo en el período (240 muestras a 48 kHz).
    let x = mid(&y.channels[0]);
    let ac = |lag: usize| x.iter().zip(&x[lag..]).map(|(a, b)| a * b).sum::<f32>();
    let best = (150..400).max_by(|a, b| ac(*a).total_cmp(&ac(*b))).unwrap();
    assert!((best as i32 - 240).abs() <= 2, "{best}");
}

#[test]
fn blur_smears_time_but_keeps_pitch_and_level() {
    // Seno que se corta a la mitad: el desenfoque largo lo hace sonar más allá.
    let mut x = sine(440.0, 1.0);
    let half = x.frames() / 2;
    x.channels[0][half..].iter_mut().for_each(|v| *v = 0.0);
    let y = render("nat.spec_blur", &x, &[("cuadros", 40.0)]);
    let tail = &y.channels[0][half + 2400..half + 4800];
    assert!(rms(tail) > 0.05, "la cola debería tener energía: {}", rms(tail));
    assert!((dominant(&y.channels[0][..half]) - 440.0).abs() < 15.0);
    let (a, b) = (rms(&x.channels[0][..half]), rms(&y.channels[0][..half]));
    assert!(b > a * 0.4 && b < a * 1.6, "{a} {b}");
}
