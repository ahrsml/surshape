//! Convolución: la entrada 2 (cualquier celda de la sesión) es la respuesta
//! al impulso (IR). FFT por solapamiento-suma. Con "normalizar", la IR se
//! escala a energía 1 para que el nivel quede parecido al de la entrada.

use crate::util::mix;
use rustfft::num_complex::Complex32;
use rustfft::FftPlanner;
use surshape_engine::{
    AudioBuf, Family, InputSpec, Inputs, LenRule, ParamSpec, ParamValues, Process, ProcessError, RenderCtx, Unit,
};

pub struct Convolve;

const PARAMS: [ParamSpec; 3] = [
    ParamSpec::toggle("normalizar", "param.nat.convolve.normalizar", true),
    ParamSpec::toggle("cola", "param.nat.convolve.cola", true),
    ParamSpec::float("mezcla", "param.nat.convolve.mezcla", 0.0, 100.0, 100.0).unit(Unit::Percent).decimals(0),
];

/// Convolución lineal de `x` con `h` (largo x + h - 1), por bloques.
pub fn convolve(x: &[f32], h: &[f32], ctx: &RenderCtx, prog: (f64, f64)) -> Result<Vec<f32>, ProcessError> {
    if x.is_empty() || h.is_empty() {
        return Ok(vec![0.0; x.len() + h.len().saturating_sub(1)]);
    }
    let nfft = (2 * h.len()).next_power_of_two().max(4096);
    let block = nfft - h.len() + 1;
    let mut planner = FftPlanner::<f32>::new();
    let fwd = planner.plan_fft_forward(nfft);
    let inv = planner.plan_fft_inverse(nfft);
    let mut hf: Vec<Complex32> = h.iter().map(|&v| Complex32::new(v, 0.0)).collect();
    hf.resize(nfft, Complex32::default());
    fwd.process(&mut hf);
    let mut out = vec![0.0f32; x.len() + h.len() - 1];
    let mut buf = vec![Complex32::default(); nfft];
    let scale = 1.0 / nfft as f32;
    let nblocks = x.len().div_ceil(block);
    for (bi, chunk) in x.chunks(block).enumerate() {
        if bi % 8 == 0 {
            ctx.check()?;
            ctx.set_progress(prog.0 + prog.1 * bi as f64 / nblocks as f64);
        }
        buf.iter_mut().for_each(|z| *z = Complex32::default());
        for (z, &v) in buf.iter_mut().zip(chunk) {
            z.re = v;
        }
        fwd.process(&mut buf);
        for (z, hz) in buf.iter_mut().zip(&hf) {
            *z *= hz;
        }
        inv.process(&mut buf);
        let base = bi * block;
        for (i, z) in buf.iter().enumerate() {
            if base + i < out.len() {
                out[base + i] += z.re * scale;
            }
        }
    }
    Ok(out)
}

impl Process for Convolve {
    fn id(&self) -> &'static str {
        "nat.convolve"
    }
    fn family(&self) -> Family {
        Family::FiltroEspacio
    }
    fn params(&self) -> &[ParamSpec] {
        &PARAMS
    }
    fn inputs(&self) -> Inputs {
        Inputs::Fixed(&[InputSpec::PRINCIPAL, InputSpec::IR])
    }
    fn expected_len(&self, inputs: &[&AudioBuf], p: &ParamValues) -> LenRule {
        if p.get("cola") >= 0.5 {
            LenRule::Frames(inputs[0].frames() + inputs[1].frames() - 1)
        } else {
            LenRule::Same
        }
    }
    fn process(&self, ctx: &mut RenderCtx) -> Result<Vec<AudioBuf>, ProcessError> {
        let (x, ir) = (ctx.input(0), ctx.input(1));
        let tail = ctx.flag("cola");
        let nch = x.num_channels();
        let out_len = if tail { x.frames() + ir.frames() - 1 } else { x.frames() };
        let mut out = Vec::with_capacity(nch);
        for ch in 0..nch {
            let mut h = ir.channels[ch.min(ir.num_channels() - 1)].clone();
            if ctx.flag("normalizar") {
                let e = h.iter().map(|v| (*v as f64).powi(2)).sum::<f64>().sqrt();
                if e > 1e-12 {
                    h.iter_mut().for_each(|v| *v /= e as f32);
                }
            }
            let mut y = convolve(&x.channels[ch], &h, ctx, (ch as f64 / nch as f64, 1.0 / nch as f64))?;
            y.truncate(out_len);
            // Mezcla seco/húmedo (la parte seca solo existe mientras dura la entrada).
            let mut wet = ctx.curve("mezcla");
            let dry = &x.channels[ch];
            for (i, v) in y.iter_mut().enumerate() {
                let d = dry.get(i).copied().unwrap_or(0.0) as f64;
                *v = mix(d, *v as f64, wet.at(i as f64 / x.sr as f64) / 100.0) as f32;
            }
            out.push(y);
        }
        Ok(vec![AudioBuf::from_channels(x.sr, out)])
    }
}
