//! Dividir en bandas: reparte el espectro en N bandas de frecuencia (cortes
//! en escala logarítmica entre la frecuencia mínima y la máxima) y entrega
//! cada banda como una salida aparte. En la grilla, cada salida extra abre
//! una fila propia. Las bandas suman exactamente la entrada.

use crate::spectral::{fft_size, frame_starts, Analyzer, Synth};
use rustfft::num_complex::Complex32;
use surshape_engine::{AudioBuf, Family, LenRule, Outputs, ParamSpec, ParamValues, Process, ProcessError, RenderCtx, Unit};

pub struct BandSplit;

const PARAMS: [ParamSpec; 4] = [
    ParamSpec::int("bandas", "param.nat.band_split.bandas", 2.0, 8.0, 3.0),
    ParamSpec::float("frec_min", "param.nat.band_split.frec_min", 30.0, 2000.0, 200.0).unit(Unit::Hz).log().decimals(0).fixed_only(),
    ParamSpec::float("frec_max", "param.nat.band_split.frec_max", 500.0, 16000.0, 4000.0).unit(Unit::Hz).log().decimals(0).fixed_only(),
    ParamSpec::float("ventana", "param.nat.band_split.ventana", 10.0, 500.0, 46.0).unit(Unit::Ms).log().decimals(1).fixed_only(),
];

/// Frecuencias de corte (n - 1 cortes) en escala logarítmica.
pub fn cutoffs(n: usize, fmin: f64, fmax: f64) -> Vec<f64> {
    let (lo, hi) = (fmin.min(fmax), fmax.max(fmin));
    if n < 2 {
        return Vec::new();
    }
    if n == 2 {
        return vec![(lo * hi).sqrt()];
    }
    (0..n - 1).map(|i| lo * (hi / lo).powf(i as f64 / (n - 2) as f64)).collect()
}

impl Process for BandSplit {
    fn id(&self) -> &'static str {
        "nat.band_split"
    }
    fn family(&self) -> Family {
        Family::Espectral
    }
    fn params(&self) -> &[ParamSpec] {
        &PARAMS
    }
    fn outputs(&self) -> Outputs {
        Outputs::Dynamic
    }
    fn expected_len(&self, _: &[&AudioBuf], _: &ParamValues) -> LenRule {
        LenRule::Same
    }
    fn process(&self, ctx: &mut RenderCtx) -> Result<Vec<AudioBuf>, ProcessError> {
        let input = ctx.input(0);
        let nb = ctx.p("bandas") as usize;
        let cuts = cutoffs(nb, ctx.p("frec_min"), ctx.p("frec_max"));
        let n = fft_size(ctx.p("ventana"), input.sr);
        let hop = n / 4;
        let bin_hz = input.sr as f64 / n as f64;
        // Banda de cada bin.
        let band_of: Vec<usize> = (0..=n / 2).map(|k| cuts.iter().filter(|&&c| k as f64 * bin_hz >= c).count()).collect();
        let len = input.frames();
        let starts: Vec<i64> = frame_starts(len, n, hop).collect();
        let mut an = Analyzer::new(n);
        let mut half = vec![Complex32::default(); an.bins()];
        let mut part = vec![Complex32::default(); an.bins()];
        let total = (starts.len() * input.num_channels()).max(1);
        let mut done = 0;
        let mut out: Vec<Vec<Vec<f32>>> = vec![Vec::with_capacity(input.num_channels()); nb];
        for x in &input.channels {
            let mut synths: Vec<Synth> = (0..nb).map(|_| Synth::new(n, len)).collect();
            for (i, &s) in starts.iter().enumerate() {
                if i % 32 == 0 {
                    ctx.check()?;
                    ctx.set_progress(done as f64 / total as f64);
                }
                an.frame(x, s, &mut half);
                for (b, sy) in synths.iter_mut().enumerate() {
                    for (k, z) in part.iter_mut().enumerate() {
                        *z = if band_of[k] == b { half[k] } else { Complex32::default() };
                    }
                    sy.add(s, &part);
                }
                done += 1;
            }
            for (b, sy) in synths.into_iter().enumerate() {
                out[b].push(sy.finish(len));
            }
        }
        Ok(out.into_iter().map(|ch| AudioBuf::from_channels(input.sr, ch)).collect())
    }
}
