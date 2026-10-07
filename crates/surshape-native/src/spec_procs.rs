//! Procesos espectrales (familia Espectral), sobre el motor STFT de
//! [`crate::spectral`]:
//!
//! - Congelar espectro, Desenfoque espectral, Compuerta espectral,
//!   Barajar bins: modifican cada cuadro (o varios) y resintetizan.
//! - Desplazar / Estirar espectro: vocoder de fase (frecuencia real por bin).
//! - Robotizar (fases a cero, salto = período) y Susurrar (fases al azar).
//!
//! La ventana de análisis se elige en milisegundos y se redondea a la
//! potencia de 2 siguiente. Salto = ventana / 4 salvo en Robotizar.

use crate::spectral::{fft_size, frame_starts, match_rms, rms, Analyzer, PhaseVocoder, Synth};
use noisegek_dsp::rng::XorShift;
use rustfft::num_complex::Complex32;
use std::collections::VecDeque;
use surshape_audio::db_to_lin;
use surshape_engine::{
    seed32, AudioBuf, Family, LenRule, ParamSpec, ParamValues, Process, ProcessError, RenderCtx, Unit,
};

const TAU: f32 = std::f32::consts::TAU;

/// Ventana (ms) común; `def` según el proceso.
const fn ventana(key: &'static str, min: f64, max: f64, def: f64) -> ParamSpec {
    ParamSpec::float("ventana", key, min, max, def).unit(Unit::Ms).log().decimals(1).fixed_only()
}
const fn mezcla(key: &'static str) -> ParamSpec {
    ParamSpec::float("mezcla", key, 0.0, 100.0, 100.0).unit(Unit::Percent).decimals(0)
}

/// Recorre cada canal de la entrada 0 cuadro a cuadro (salto n/4), llama a
/// `f(canal, cuadro, tiempo, medio_espectro)` y resintetiza.
fn per_frame(
    ctx: &RenderCtx,
    n: usize,
    mut f: impl FnMut(usize, usize, f64, &mut [Complex32]),
) -> Result<Vec<Vec<f32>>, ProcessError> {
    let input = ctx.input(0);
    let hop = n / 4;
    let len = input.frames();
    let starts: Vec<i64> = frame_starts(len, n, hop).collect();
    let total = (starts.len() * input.num_channels()).max(1);
    let mut done = 0;
    let mut out = Vec::with_capacity(input.num_channels());
    let mut an = Analyzer::new(n);
    let mut half = vec![Complex32::default(); an.bins()];
    for (ch, x) in input.channels.iter().enumerate() {
        let mut sy = Synth::new(n, len);
        for (i, &s) in starts.iter().enumerate() {
            if i % 32 == 0 {
                ctx.check()?;
                ctx.set_progress(done as f64 / total as f64);
            }
            an.frame(x, s, &mut half);
            let t = (s + n as i64 / 2).max(0) as f64 / input.sr as f64;
            f(ch, i, t, &mut half);
            sy.add(s, &half);
            done += 1;
        }
        out.push(sy.finish(len));
    }
    Ok(out)
}

/// Mezcla seco/húmedo con la curva "mezcla" y arma el búfer.
fn finish_mix(ctx: &RenderCtx, wet: Vec<Vec<f32>>) -> Vec<AudioBuf> {
    let input = ctx.input(0);
    let inv_sr = 1.0 / input.sr as f64;
    let chans = wet
        .into_iter()
        .zip(&input.channels)
        .map(|(w, d)| {
            let mut m = ctx.curve("mezcla");
            w.iter().zip(d).enumerate().map(|(i, (&w, &d))| {
                let k = (m.at(i as f64 * inv_sr) / 100.0) as f32;
                d + (w - d) * k
            }).collect()
        })
        .collect();
    vec![AudioBuf::from_channels(input.sr, chans)]
}

macro_rules! process_common {
    ($id:literal, $params:ident) => {
        fn id(&self) -> &'static str {
            $id
        }
        fn family(&self) -> Family {
            Family::Espectral
        }
        fn params(&self) -> &[ParamSpec] {
            &$params
        }
        fn per_channel(&self) -> bool {
            true
        }
    };
}

// --- Compuerta espectral -------------------------------------------------------

pub struct SpecGate;
const GATE: [ParamSpec; 4] = [
    ventana("param.nat.spec_gate.ventana", 10.0, 500.0, 46.0),
    ParamSpec::float("umbral", "param.nat.spec_gate.umbral", -80.0, 0.0, -30.0).unit(Unit::Db).decimals(1),
    ParamSpec::toggle("invertir", "param.nat.spec_gate.invertir", false),
    mezcla("param.nat.spec_gate.mezcla"),
];

impl Process for SpecGate {
    process_common!("nat.spec_gate", GATE);
    fn expected_len(&self, _: &[&AudioBuf], _: &ParamValues) -> LenRule {
        LenRule::Same
    }
    fn process(&self, ctx: &mut RenderCtx) -> Result<Vec<AudioBuf>, ProcessError> {
        let n = fft_size(ctx.p("ventana"), ctx.input(0).sr);
        let invert = ctx.flag("invertir");
        let mut thr = ctx.curve("umbral");
        let wet = per_frame(ctx, n, |_, _, t, half| {
            let peak = half.iter().fold(0.0f32, |m, z| m.max(z.norm()));
            let th = peak * db_to_lin(thr.at(t)) as f32;
            for b in half.iter_mut() {
                if (b.norm() >= th) == invert {
                    *b = Complex32::default();
                }
            }
        })?;
        Ok(finish_mix(ctx, wet))
    }
}

// --- Barajar bins ----------------------------------------------------------------------

pub struct BinShuffle;
const SHUFFLE: [ParamSpec; 5] = [
    ventana("param.nat.bin_shuffle.ventana", 10.0, 500.0, 46.0),
    ParamSpec::int("bloque", "param.nat.bin_shuffle.bloque", 1.0, 128.0, 8.0),
    ParamSpec::float("cantidad", "param.nat.bin_shuffle.cantidad", 0.0, 100.0, 50.0).unit(Unit::Percent).decimals(0),
    ParamSpec::int("retener", "param.nat.bin_shuffle.retener", 1.0, 64.0, 1.0),
    mezcla("param.nat.bin_shuffle.mezcla"),
];

impl Process for BinShuffle {
    process_common!("nat.bin_shuffle", SHUFFLE);
    fn uses_seed(&self) -> bool {
        true
    }
    fn expected_len(&self, _: &[&AudioBuf], _: &ParamValues) -> LenRule {
        LenRule::Same
    }
    fn process(&self, ctx: &mut RenderCtx) -> Result<Vec<AudioBuf>, ProcessError> {
        let n = fft_size(ctx.p("ventana"), ctx.input(0).sr);
        let block = ctx.p("bloque") as usize;
        let hold = ctx.p("retener") as usize;
        let mut amount = ctx.curve("cantidad");
        let nblocks = (n / 2 + 1).div_ceil(block);
        let seed = seed32(ctx.seed);
        // El mismo azar en todos los canales: la imagen estéreo se mantiene.
        let mut rng = XorShift::new(seed);
        let mut perm: Vec<usize> = (0..nblocks).collect();
        let mut last_ch = usize::MAX;
        let mut tmp = vec![Complex32::default(); n / 2 + 1];
        let wet = per_frame(ctx, n, |ch, i, t, half| {
            if ch != last_ch {
                rng = XorShift::new(seed);
                last_ch = ch;
            }
            if i % hold == 0 {
                let p = amount.at(t) / 100.0;
                perm.iter_mut().enumerate().for_each(|(k, v)| *v = k);
                for k in 0..nblocks {
                    if rng.next01() < p {
                        let j = (rng.next01() * nblocks as f64) as usize % nblocks;
                        perm.swap(k, j);
                    }
                }
            }
            tmp.copy_from_slice(half);
            for (dst_block, &src_block) in perm.iter().enumerate() {
                for o in 0..block {
                    let (d, s) = (dst_block * block + o, src_block * block + o);
                    if d < half.len() && s < half.len() {
                        half[d] = tmp[s];
                    }
                }
            }
        })?;
        Ok(finish_mix(ctx, wet))
    }
}

// --- Desenfoque espectral --------------------------------------------------------------

pub struct SpecBlur;
const BLUR: [ParamSpec; 4] = [
    ventana("param.nat.spec_blur.ventana", 10.0, 500.0, 46.0),
    ParamSpec::int("cuadros", "param.nat.spec_blur.cuadros", 1.0, 200.0, 12.0),
    ParamSpec::toggle("fases", "param.nat.spec_blur.fases", true),
    mezcla("param.nat.spec_blur.mezcla"),
];

impl Process for SpecBlur {
    process_common!("nat.spec_blur", BLUR);
    fn uses_seed(&self) -> bool {
        true
    }
    fn expected_len(&self, _: &[&AudioBuf], _: &ParamValues) -> LenRule {
        LenRule::Same
    }
    fn process(&self, ctx: &mut RenderCtx) -> Result<Vec<AudioBuf>, ProcessError> {
        let input = ctx.input(0);
        let n = fft_size(ctx.p("ventana"), input.sr);
        let hop = n / 4;
        let radius = ctx.p("cuadros") as usize;
        let random = ctx.flag("fases");
        let len = input.frames();
        let starts: Vec<i64> = frame_starts(len, n, hop).collect();
        let nf = starts.len();
        let total = (nf * input.num_channels()).max(1);
        let mut done = 0;
        let mut an = Analyzer::new(n);
        let bins = an.bins();
        let mut wet = Vec::with_capacity(input.num_channels());
        for x in &input.channels {
            let mut rng = XorShift::new(seed32(ctx.seed));
            let mut sy = Synth::new(n, len);
            // Ventana deslizante de cuadros [c - radio, c + radio].
            let mut queue: VecDeque<Vec<Complex32>> = VecDeque::new();
            let mut q0 = 0usize; // índice del primer cuadro en la cola
            let mut sum = vec![0.0f32; bins];
            let mut out = vec![Complex32::default(); bins];
            for i in 0..nf + radius {
                if i % 32 == 0 {
                    ctx.check()?;
                    ctx.set_progress(done as f64 / total as f64);
                }
                if i < nf {
                    let mut h = vec![Complex32::default(); bins];
                    an.frame(x, starts[i], &mut h);
                    for (s, z) in sum.iter_mut().zip(&h) {
                        *s += z.norm();
                    }
                    queue.push_back(h);
                }
                if i < radius {
                    continue;
                }
                let c = i - radius;
                while c > q0 + radius {
                    let old = queue.pop_front().expect("cola");
                    for (s, z) in sum.iter_mut().zip(&old) {
                        *s -= z.norm();
                    }
                    q0 += 1;
                }
                let count = queue.len() as f32;
                let center = &queue[c - q0];
                for k in 0..bins {
                    let m = (sum[k] / count).max(0.0);
                    let ph = if random { TAU * rng.next01() as f32 } else { center[k].arg() };
                    out[k] = Complex32::from_polar(m, ph);
                }
                sy.add(starts[c], &out);
                done += 1;
            }
            let mut y = sy.finish(len);
            if random {
                match_rms(&mut y, rms(x));
            }
            wet.push(y);
        }
        Ok(finish_mix(ctx, wet))
    }
}

// --- Congelar espectro ----------------------------------------------------------------

pub struct Freeze;
const FREEZE: [ParamSpec; 4] = [
    ParamSpec::float("posicion", "param.nat.freeze.posicion", 0.0, 100.0, 50.0).unit(Unit::Percent).decimals(1).fixed_only(),
    ParamSpec::float("duracion", "param.nat.freeze.duracion", 0.5, 600.0, 10.0).unit(Unit::Seconds).log().decimals(2).fixed_only(),
    ventana("param.nat.freeze.ventana", 20.0, 1000.0, 120.0),
    ParamSpec::int("promedio", "param.nat.freeze.promedio", 1.0, 64.0, 4.0),
];

impl Process for Freeze {
    process_common!("nat.freeze", FREEZE);
    fn uses_seed(&self) -> bool {
        true
    }
    fn expected_len(&self, inputs: &[&AudioBuf], p: &ParamValues) -> LenRule {
        LenRule::Frames((p.get("duracion") * inputs[0].sr as f64).round() as usize)
    }
    fn process(&self, ctx: &mut RenderCtx) -> Result<Vec<AudioBuf>, ProcessError> {
        let input = ctx.input(0);
        let n = fft_size(ctx.p("ventana"), input.sr);
        let hop = n / 4;
        let avg = ctx.p("promedio") as usize;
        let center = (ctx.p("posicion") / 100.0 * input.frames() as f64) as i64;
        let out_len = (ctx.p("duracion") * input.sr as f64).round() as usize;
        let mut an = Analyzer::new(n);
        let bins = an.bins();
        let mut half = vec![Complex32::default(); bins];
        let starts: Vec<i64> = frame_starts(out_len, n, hop).collect();
        let total = (starts.len() * input.num_channels()).max(1);
        let mut done = 0;
        let mut wet = Vec::with_capacity(input.num_channels());
        for x in &input.channels {
            // Magnitud promedio de `avg` cuadros alrededor de la posición.
            let mut mags = vec![0.0f32; bins];
            let first = center - n as i64 / 2 - ((avg as i64 - 1) * hop as i64) / 2;
            for k in 0..avg {
                an.frame(x, first + (k * hop) as i64, &mut half);
                for (m, z) in mags.iter_mut().zip(&half) {
                    *m += z.norm() / avg as f32;
                }
            }
            let seg_a = (first.max(0) as usize).min(x.len());
            let seg_b = ((first + (avg * hop + n) as i64).max(0) as usize).min(x.len());
            let target = rms(&x[seg_a..seg_b.max(seg_a)]);
            // Mismas fases en todos los canales.
            let mut rng = XorShift::new(seed32(ctx.seed));
            let mut sy = Synth::new(n, out_len);
            for (i, &s) in starts.iter().enumerate() {
                if i % 32 == 0 {
                    ctx.check()?;
                    ctx.set_progress(done as f64 / total as f64);
                }
                for (z, &m) in half.iter_mut().zip(&mags) {
                    *z = Complex32::from_polar(m, TAU * rng.next01() as f32);
                }
                sy.add(s, &half);
                done += 1;
            }
            let mut y = sy.finish(out_len);
            match_rms(&mut y, target);
            wet.push(y);
        }
        Ok(vec![AudioBuf::from_channels(input.sr, wet)])
    }
}

// --- Desplazar y estirar espectro (vocoder de fase) -------------------------------------

/// Vocoder de fase con un mapeo de frecuencia `map(f, t) -> f'`.
fn pv_map(ctx: &RenderCtx, n: usize, mut map: impl FnMut(f32, f64) -> f32) -> Result<Vec<Vec<f32>>, ProcessError> {
    let input = ctx.input(0);
    let sr = input.sr;
    let nyq = sr as f32 / 2.0;
    let bin_hz = sr as f32 / n as f32;
    let bins = n / 2 + 1;
    let mut pvs: Vec<PhaseVocoder> = (0..input.num_channels()).map(|_| PhaseVocoder::new(n, n / 4, sr)).collect();
    let (mut m, mut f) = (vec![0.0f32; bins], vec![0.0f32; bins]);
    let (mut nm, mut nf, mut best) = (vec![0.0f32; bins], vec![0.0f32; bins], vec![0.0f32; bins]);
    per_frame(ctx, n, |ch, _, t, half| {
        let pv = &mut pvs[ch];
        pv.analyze(half, &mut m, &mut f);
        nm.iter_mut().for_each(|v| *v = 0.0);
        best.iter_mut().for_each(|v| *v = 0.0);
        for k in 0..bins {
            let f2 = map(f[k], t);
            if f2 <= 0.0 || f2 >= nyq {
                continue;
            }
            let j = (f2 / bin_hz).round() as usize;
            if j < bins {
                nm[j] += m[k];
                if m[k] > best[j] {
                    best[j] = m[k];
                    nf[j] = f2;
                }
            }
        }
        pv.synth(&nm, &nf, half);
    })
}

pub struct SpecShift;
const SHIFT: [ParamSpec; 3] = [
    ventana("param.nat.spec_shift.ventana", 20.0, 500.0, 85.0),
    ParamSpec::float("hz", "param.nat.spec_shift.hz", -4000.0, 4000.0, 300.0).unit(Unit::Hz).decimals(1),
    mezcla("param.nat.spec_shift.mezcla"),
];

impl Process for SpecShift {
    process_common!("nat.spec_shift", SHIFT);
    fn expected_len(&self, _: &[&AudioBuf], _: &ParamValues) -> LenRule {
        LenRule::Same
    }
    fn process(&self, ctx: &mut RenderCtx) -> Result<Vec<AudioBuf>, ProcessError> {
        let n = fft_size(ctx.p("ventana"), ctx.input(0).sr);
        let mut hz = ctx.curve("hz");
        let wet = pv_map(ctx, n, |f, t| f + hz.at(t) as f32)?;
        Ok(finish_mix(ctx, wet))
    }
}

pub struct SpecStretch;
const STRETCH: [ParamSpec; 4] = [
    ventana("param.nat.spec_stretch.ventana", 20.0, 500.0, 85.0),
    ParamSpec::float("factor", "param.nat.spec_stretch.factor", 0.25, 4.0, 1.5).unit(Unit::Factor).log().decimals(3),
    ParamSpec::float("ancla", "param.nat.spec_stretch.ancla", 20.0, 5000.0, 100.0).unit(Unit::Hz).log().decimals(0),
    mezcla("param.nat.spec_stretch.mezcla"),
];

impl Process for SpecStretch {
    process_common!("nat.spec_stretch", STRETCH);
    fn expected_len(&self, _: &[&AudioBuf], _: &ParamValues) -> LenRule {
        LenRule::Same
    }
    fn process(&self, ctx: &mut RenderCtx) -> Result<Vec<AudioBuf>, ProcessError> {
        let n = fft_size(ctx.p("ventana"), ctx.input(0).sr);
        let (mut fac, mut anc) = (ctx.curve("factor"), ctx.curve("ancla"));
        let wet = pv_map(ctx, n, |f, t| {
            let a = anc.at(t) as f32;
            if f <= a {
                f
            } else {
                a + (f - a) * fac.at(t) as f32
            }
        })?;
        Ok(finish_mix(ctx, wet))
    }
}

// --- Robotizar y susurrar -----------------------------------------------------------------

pub struct Robotize;
const ROBOT: [ParamSpec; 3] = [
    ventana("param.nat.robotize.ventana", 5.0, 200.0, 30.0),
    ParamSpec::float("hz", "param.nat.robotize.hz", 20.0, 800.0, 110.0).unit(Unit::Hz).log().decimals(1),
    mezcla("param.nat.robotize.mezcla"),
];

impl Process for Robotize {
    process_common!("nat.robotize", ROBOT);
    fn expected_len(&self, _: &[&AudioBuf], _: &ParamValues) -> LenRule {
        LenRule::Same
    }
    fn process(&self, ctx: &mut RenderCtx) -> Result<Vec<AudioBuf>, ProcessError> {
        let input = ctx.input(0);
        let sr = input.sr as f64;
        let n = fft_size(ctx.p("ventana"), input.sr);
        let len = input.frames();
        // Cuadros separados por un período de la frecuencia (variable).
        let mut hz = ctx.curve("hz");
        let mut centers = Vec::new();
        let mut c = 0.0f64;
        while c < len as f64 {
            centers.push(c);
            c += (sr / hz.at(c / sr)).max(1.0);
        }
        let total = (centers.len() * input.num_channels()).max(1);
        let mut done = 0;
        let mut an = Analyzer::new(n);
        let mut half = vec![Complex32::default(); an.bins()];
        let mut wet = Vec::with_capacity(input.num_channels());
        for x in &input.channels {
            let mut sy = Synth::new(n, len);
            for (i, &c) in centers.iter().enumerate() {
                if i % 64 == 0 {
                    ctx.check()?;
                    ctx.set_progress(done as f64 / total as f64);
                }
                let s = c.round() as i64 - n as i64 / 2;
                an.frame(x, s, &mut half);
                for z in half.iter_mut() {
                    *z = Complex32::new(z.norm(), 0.0);
                }
                sy.add(s, &half);
                done += 1;
            }
            let mut y = sy.finish(len);
            match_rms(&mut y, rms(x));
            wet.push(y);
        }
        Ok(finish_mix(ctx, wet))
    }
}

pub struct Whisperize;
const WHISPER: [ParamSpec; 2] = [ventana("param.nat.whisperize.ventana", 2.0, 100.0, 12.0), mezcla("param.nat.whisperize.mezcla")];

impl Process for Whisperize {
    process_common!("nat.whisperize", WHISPER);
    fn uses_seed(&self) -> bool {
        true
    }
    fn expected_len(&self, _: &[&AudioBuf], _: &ParamValues) -> LenRule {
        LenRule::Same
    }
    fn process(&self, ctx: &mut RenderCtx) -> Result<Vec<AudioBuf>, ProcessError> {
        let n = fft_size(ctx.p("ventana"), ctx.input(0).sr);
        let seed = seed32(ctx.seed);
        let mut rng = XorShift::new(seed);
        let mut last_ch = usize::MAX;
        let mut wet = per_frame(ctx, n, |ch, _, _, half| {
            if ch != last_ch {
                rng = XorShift::new(seed);
                last_ch = ch;
            }
            for z in half.iter_mut() {
                *z = Complex32::from_polar(z.norm(), TAU * rng.next01() as f32);
            }
        })?;
        for (y, x) in wet.iter_mut().zip(&ctx.input(0).channels) {
            match_rms(y, rms(x));
        }
        Ok(finish_mix(ctx, wet))
    }
}

