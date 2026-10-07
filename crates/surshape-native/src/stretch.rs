//! Estiramiento extremo al estilo Paulstretch (algoritmo de Nasca Octavian
//! Paul, reimplementado aquí desde su descripción).
//!
//! Cada cuadro de salida toma una ventana de la entrada, conserva la magnitud
//! de su espectro, le pone fases aleatorias (de la seed) y vuelve al tiempo.
//! Los cuadros se solapan con salto = ventana / 4 y ventana de Hann en
//! análisis y síntesis. Por cuadro, la entrada avanza `salto / factor`.
//!
//! El factor admite breakpoints, leídos sobre el tiempo de la ENTRADA: un
//! tramo con factor 100 se estira 100 veces. La duración de salida es la
//! suma de cuadros, y [`plan`] la calcula igual para `expected_len`.
//!
//! Nivel: con fases aleatorias los cuadros se suman en potencia, no en
//! amplitud. Varianza de salida = (3/8) * 1,5 = 0,5625 veces la de entrada,
//! así que se escala por 1 / 0,75 para conservar el RMS.
//!
//! Las mismas fases se usan en todos los canales: la imagen estéreo se
//! mantiene coherente.

use noisegek_dsp::rng::XorShift;
use rustfft::num_complex::Complex32;
use rustfft::FftPlanner;
use surshape_engine::{
    seed32, AudioBuf, Curve, Family, LenRule, ParamSpec, ParamValue, ParamValues, Process, ProcessError, RenderCtx, Unit,
};

pub struct PaulStretch;

const PARAMS: [ParamSpec; 2] = [
    ParamSpec::float("factor", "param.nat.paulstretch.factor", 1.0, 10000.0, 8.0).unit(Unit::Factor).log(),
    ParamSpec::float("ventana", "param.nat.paulstretch.ventana", 0.02, 4.0, 0.25)
        .unit(Unit::Seconds)
        .log()
        .decimals(3)
        .fixed_only(),
];

/// Ventana par, mínimo 64 muestras.
fn window_len(secs: f64, sr: u32) -> usize {
    let w = ((secs * sr as f64).round() as usize).max(64);
    w + w % 2
}

/// Posición central en la entrada de cada cuadro, y largo de la salida.
struct Plan {
    centers: Vec<f64>,
    out_len: usize,
}

fn plan(in_len: usize, sr: u32, w: usize, factor: &ParamValue) -> Plan {
    let hop = (w / 4).max(1);
    let half = w / 2;
    let dur = in_len as f64 / sr.max(1) as f64;
    let mut curve = Curve::new(factor, dur);
    let out_len = match factor {
        ParamValue::Fixed(f) => (in_len as f64 * f).round() as usize,
        ParamValue::Envelope(_) => {
            // Cuadros hasta consumir la entrada.
            let (mut pos, mut n) = (0.0f64, 0usize);
            while pos < in_len as f64 {
                pos += hop as f64 / curve.at(pos / sr as f64).max(1.0);
                n += 1;
            }
            n * hop
        }
    };
    let n_frames = (out_len + half) / hop + 1;
    let mut centers = Vec::with_capacity(n_frames);
    match factor {
        ParamValue::Fixed(f) => centers.extend((0..n_frames).map(|i| (i * hop) as f64 / f)),
        ParamValue::Envelope(_) => {
            let mut pos = 0.0f64;
            for _ in 0..n_frames {
                centers.push(pos);
                pos += hop as f64 / curve.at(pos / sr as f64).max(1.0);
            }
        }
    }
    Plan { centers, out_len }
}

impl Process for PaulStretch {
    fn id(&self) -> &'static str {
        "nat.paulstretch"
    }
    fn family(&self) -> Family {
        Family::Tiempo
    }
    fn params(&self) -> &[ParamSpec] {
        &PARAMS
    }
    fn uses_seed(&self) -> bool {
        true
    }
    fn expected_len(&self, inputs: &[&AudioBuf], p: &ParamValues) -> LenRule {
        match p.value("factor") {
            Some(ParamValue::Envelope(_)) => {
                let b = inputs[0];
                let w = window_len(p.get("ventana"), b.sr);
                LenRule::Frames(plan(b.frames(), b.sr, w, p.value("factor").unwrap()).out_len)
            }
            _ => LenRule::Scaled(p.get("factor")),
        }
    }
    fn process(&self, ctx: &mut RenderCtx) -> Result<Vec<AudioBuf>, ProcessError> {
        let input = ctx.input(0);
        let in_len = input.frames();
        let w = window_len(ctx.p("ventana"), input.sr);
        let half = w / 2;
        let hop = (w / 4).max(1);
        let factor = ctx.params.value("factor").cloned().unwrap_or(ParamValue::Fixed(1.0));
        let Plan { centers, out_len } = plan(in_len, input.sr, w, &factor);
        let win: Vec<f32> =
            (0..w).map(|i| 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / w as f64).cos() as f32).collect();

        let mut planner = FftPlanner::<f32>::new();
        let fwd = planner.plan_fft_forward(w);
        let inv = planner.plan_fft_inverse(w);
        let mut scratch = vec![Complex32::default(); fwd.get_inplace_scratch_len().max(inv.get_inplace_scratch_len())];
        let mut spec = vec![Complex32::default(); w];
        let mut phase = vec![Complex32::default(); half + 1];
        // rustfft no normaliza: la inversa multiplica por w.
        let gain = (1.0 / 0.75) / w as f32;

        let nch = input.num_channels();
        let n_frames = centers.len();
        // El búfer de salida va desplazado media ventana: índice = t + half.
        let mut acc = vec![vec![0.0f32; n_frames * hop + w]; nch];
        let mut rng = XorShift::new(seed32(ctx.seed));

        for (f, &center) in centers.iter().enumerate() {
            if f % 16 == 0 {
                ctx.check()?;
                ctx.set_progress(f as f64 / n_frames as f64);
            }
            let out_pos = f * hop;
            let start = center.round() as i64 - half as i64;
            for p in phase.iter_mut().take(half).skip(1) {
                let a = (2.0 * std::f64::consts::PI * rng.next01()) as f32;
                *p = Complex32::new(a.cos(), a.sin());
            }
            for (ci, x) in input.channels.iter().enumerate() {
                for (i, s) in spec.iter_mut().enumerate() {
                    let j = start + i as i64;
                    let v = if j >= 0 && (j as usize) < in_len { x[j as usize] } else { 0.0 };
                    *s = Complex32::new(v * win[i], 0.0);
                }
                fwd.process_with_scratch(&mut spec, &mut scratch);
                // Magnitud con fase aleatoria; espectro hermítico -> salida real.
                spec[0] = Complex32::new(spec[0].norm(), 0.0);
                spec[half] = Complex32::new(spec[half].norm(), 0.0);
                for k in 1..half {
                    let v = phase[k] * spec[k].norm();
                    spec[k] = v;
                    spec[w - k] = v.conj();
                }
                inv.process_with_scratch(&mut spec, &mut scratch);
                let dst = &mut acc[ci][out_pos..out_pos + w];
                for ((d, s), wv) in dst.iter_mut().zip(&spec).zip(&win) {
                    *d += s.re * wv * gain;
                }
            }
        }
        let out = acc.into_iter().map(|c| c[half..half + out_len].to_vec()).collect();
        ctx.set_progress(1.0);
        Ok(vec![AudioBuf::from_channels(input.sr, out)])
    }
}
