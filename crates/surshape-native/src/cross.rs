//! Síntesis cruzada: la entrada 1 (portadora) aporta las fases y la
//! estructura fina; la entrada 2 (modulador, cualquier celda) aporta las
//! amplitudes o la envolvente espectral.
//!
//! - Amplitudes: |Y| = |A|^(1-k) · |B|^k.
//! - Envolvente (vocoder): |Y| = |A| · (envB / envA)^k, con envolventes
//!   suavizadas sobre `suavizado` bins.
//!
//! Si el modulador es más corto que la portadora, se repite (o queda en
//! silencio, según "repetir").

use crate::spectral::{fft_size, frame_starts, Analyzer, Synth};
use rustfft::num_complex::Complex32;
use surshape_engine::{
    AudioBuf, Family, InputSpec, Inputs, LenRule, ParamSpec, ParamValues, Process, ProcessError, RenderCtx, Unit,
};

pub struct CrossSynth;

const MODOS: [&str; 2] = ["param.nat.cross_synth.modo.amplitudes", "param.nat.cross_synth.modo.envolvente"];

const PARAMS: [ParamSpec; 5] = [
    ParamSpec::choice("modo", "param.nat.cross_synth.modo", &MODOS, 1),
    ParamSpec::float("cantidad", "param.nat.cross_synth.cantidad", 0.0, 100.0, 100.0).unit(Unit::Percent).decimals(0),
    ParamSpec::int("suavizado", "param.nat.cross_synth.suavizado", 1.0, 64.0, 8.0),
    ParamSpec::float("ventana", "param.nat.cross_synth.ventana", 10.0, 500.0, 46.0).unit(Unit::Ms).log().decimals(1).fixed_only(),
    ParamSpec::toggle("repetir", "param.nat.cross_synth.repetir", true),
];

/// Media móvil de ancho 2r+1 (envolvente espectral).
fn smooth(m: &[f32], r: usize, out: &mut [f32]) {
    let mut prefix = vec![0.0f32; m.len() + 1];
    for i in 0..m.len() {
        prefix[i + 1] = prefix[i] + m[i];
    }
    for i in 0..m.len() {
        let a = i.saturating_sub(r);
        let b = (i + r + 1).min(m.len());
        out[i] = (prefix[b] - prefix[a]) / (b - a) as f32;
    }
}

impl Process for CrossSynth {
    fn id(&self) -> &'static str {
        "nat.cross_synth"
    }
    fn family(&self) -> Family {
        Family::Espectral
    }
    fn params(&self) -> &[ParamSpec] {
        &PARAMS
    }
    fn inputs(&self) -> Inputs {
        Inputs::Fixed(&[InputSpec::PRINCIPAL, InputSpec::MODULADOR])
    }
    fn per_channel(&self) -> bool {
        true
    }
    fn expected_len(&self, _: &[&AudioBuf], _: &ParamValues) -> LenRule {
        LenRule::Same
    }
    fn process(&self, ctx: &mut RenderCtx) -> Result<Vec<AudioBuf>, ProcessError> {
        let (a, b) = (ctx.input(0), ctx.input(1));
        let n = fft_size(ctx.p("ventana"), a.sr);
        let hop = n / 4;
        let vocoder = ctx.p("modo") >= 0.5;
        let r = ctx.p("suavizado") as usize;
        let repeat = ctx.flag("repetir");
        let len = a.frames();
        let starts: Vec<i64> = frame_starts(len, n, hop).collect();
        let bstarts: Vec<i64> = frame_starts(b.frames(), n, hop).collect();
        let mut an_a = Analyzer::new(n);
        let mut an_b = Analyzer::new(n);
        let bins = an_a.bins();
        let (mut ha, mut hb) = (vec![Complex32::default(); bins], vec![Complex32::default(); bins]);
        let (mut ma, mut mb, mut ea, mut eb) = (vec![0.0f32; bins], vec![0.0f32; bins], vec![0.0f32; bins], vec![0.0f32; bins]);
        let total = (starts.len() * a.num_channels()).max(1);
        let mut done = 0;
        let mut out = Vec::with_capacity(a.num_channels());
        for ch in 0..a.num_channels() {
            let xa = &a.channels[ch];
            let xb = &b.channels[ch.min(b.num_channels() - 1)];
            let mut amount = ctx.curve("cantidad");
            let mut sy = Synth::new(n, len);
            for (i, &s) in starts.iter().enumerate() {
                if i % 32 == 0 {
                    ctx.check()?;
                    ctx.set_progress(done as f64 / total as f64);
                }
                an_a.frame(xa, s, &mut ha);
                let bi = if repeat { i % bstarts.len().max(1) } else { i };
                match bstarts.get(bi) {
                    Some(&bs) => an_b.frame(xb, bs, &mut hb),
                    None => hb.iter_mut().for_each(|z| *z = Complex32::default()),
                }
                let k = (amount.at(s.max(0) as f64 / a.sr as f64) / 100.0) as f32;
                for j in 0..bins {
                    ma[j] = ha[j].norm();
                    mb[j] = hb[j].norm();
                }
                if vocoder {
                    smooth(&ma, r, &mut ea);
                    smooth(&mb, r, &mut eb);
                }
                for j in 0..bins {
                    let m = if vocoder {
                        ma[j] * ((eb[j] + 1e-9) / (ea[j] + 1e-9)).powf(k)
                    } else {
                        ma[j].powf(1.0 - k) * mb[j].powf(k)
                    };
                    let ph = ha[j].arg();
                    ha[j] = Complex32::from_polar(m, ph);
                }
                sy.add(s, &ha);
                done += 1;
            }
            out.push(sy.finish(len));
        }
        Ok(vec![AudioBuf::from_channels(a.sr, out)])
    }
}
