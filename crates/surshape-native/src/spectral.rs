//! Motor STFT nativo (rustfft).
//!
//! - [`Analyzer`]: ventana de Hann + FFT de un cuadro -> medio espectro
//!   (`n/2 + 1` bins complejos). Los cuadros pueden empezar antes del inicio
//!   o terminar después del final: se rellena con ceros.
//! - [`Synth`]: medio espectro -> IFFT + ventana + solapamiento-suma, con la
//!   suma de ventanas² acumulada para normalizar exacto con cualquier salto.
//!   Analizar y sintetizar sin cambios devuelve la entrada.
//! - [`PhaseVocoder`]: magnitud + frecuencia real por bin, y resíntesis con
//!   fase acumulada (para mover frecuencias sin "phasing").
//! - [`spectrogram`]: matriz 0..1 (dB) en escala logarítmica de frecuencia,
//!   para el visor.
//!
//! Convención de cuadros: el cuadro `i` empieza en `i * hop - n / 2`, así su
//! centro cae en `i * hop` y la salida queda alineada con la entrada (sin
//! retardo).

use rustfft::num_complex::Complex32;
use rustfft::{Fft, FftPlanner};
use std::f32::consts::TAU;
use std::sync::Arc;

/// Tamaño de FFT (potencia de 2) para una ventana de `ms` milisegundos.
pub fn fft_size(ms: f64, sr: u32) -> usize {
    let n = (ms * 0.001 * sr as f64).round().max(64.0) as usize;
    n.next_power_of_two().min(1 << 16)
}

fn hann(n: usize) -> Vec<f32> {
    (0..n).map(|i| 0.5 - 0.5 * (TAU * i as f32 / n as f32).cos()).collect()
}

/// Inicios de cuadro que cubren `len` muestras con salto `hop`.
pub fn frame_starts(len: usize, n: usize, hop: usize) -> impl Iterator<Item = i64> {
    let half = (n / 2) as i64;
    let count = len / hop.max(1) + 1;
    (0..count as i64).map(move |i| i * hop as i64 - half)
}

pub struct Analyzer {
    n: usize,
    win: Vec<f32>,
    fft: Arc<dyn Fft<f32>>,
    buf: Vec<Complex32>,
    scratch: Vec<Complex32>,
}

impl Analyzer {
    pub fn new(n: usize) -> Self {
        let fft = FftPlanner::new().plan_fft_forward(n);
        let scratch = vec![Complex32::default(); fft.get_inplace_scratch_len()];
        Self { n, win: hann(n), fft, buf: vec![Complex32::default(); n], scratch }
    }

    pub fn bins(&self) -> usize {
        self.n / 2 + 1
    }

    /// Medio espectro del cuadro que empieza en `start` (en `out`, largo
    /// `bins()`).
    pub fn frame(&mut self, x: &[f32], start: i64, out: &mut [Complex32]) {
        for (i, b) in self.buf.iter_mut().enumerate() {
            let j = start + i as i64;
            let v = if j >= 0 && (j as usize) < x.len() { x[j as usize] } else { 0.0 };
            *b = Complex32::new(v * self.win[i], 0.0);
        }
        self.fft.process_with_scratch(&mut self.buf, &mut self.scratch);
        out.copy_from_slice(&self.buf[..self.n / 2 + 1]);
    }
}

pub struct Synth {
    n: usize,
    win: Vec<f32>,
    ifft: Arc<dyn Fft<f32>>,
    buf: Vec<Complex32>,
    scratch: Vec<Complex32>,
    /// Salida desplazada `n` muestras (los cuadros pueden empezar en -n/2).
    out: Vec<f32>,
    wsum: Vec<f32>,
}

impl Synth {
    /// Para una salida de `len` muestras.
    pub fn new(n: usize, len: usize) -> Self {
        let ifft = FftPlanner::new().plan_fft_inverse(n);
        let scratch = vec![Complex32::default(); ifft.get_inplace_scratch_len()];
        Self {
            n,
            win: hann(n),
            ifft,
            buf: vec![Complex32::default(); n],
            scratch,
            out: vec![0.0; len + 2 * n],
            wsum: vec![0.0; len + 2 * n],
        }
    }

    /// Suma el cuadro `half` (medio espectro) empezando en `start`.
    pub fn add(&mut self, start: i64, half: &[Complex32]) {
        let n = self.n;
        self.buf[..n / 2 + 1].copy_from_slice(half);
        // Espectro hermítico -> salida real. DC y Nyquist sin parte imaginaria.
        self.buf[0].im = 0.0;
        self.buf[n / 2].im = 0.0;
        for k in 1..n / 2 {
            self.buf[n - k] = self.buf[k].conj();
        }
        self.ifft.process_with_scratch(&mut self.buf, &mut self.scratch);
        let base = start + n as i64;
        if base < 0 {
            return;
        }
        let inv_n = 1.0 / n as f32;
        for i in 0..n {
            let j = base as usize + i;
            if j >= self.out.len() {
                break;
            }
            self.out[j] += self.buf[i].re * inv_n * self.win[i];
            self.wsum[j] += self.win[i] * self.win[i];
        }
    }

    /// Resultado normalizado, de largo `len`.
    pub fn finish(self, len: usize) -> Vec<f32> {
        let n = self.n;
        let peak_w = self.wsum.iter().fold(0.0f32, |m, &w| m.max(w));
        let floor = peak_w * 1e-3;
        (0..len)
            .map(|i| {
                let w = self.wsum[i + n];
                if w > floor {
                    self.out[i + n] / w
                } else {
                    0.0
                }
            })
            .collect()
    }
}

/// Fase envuelta a [-π, π).
#[inline]
pub fn wrap(p: f32) -> f32 {
    (p + std::f32::consts::PI).rem_euclid(TAU) - std::f32::consts::PI
}

/// Vocoder de fase: análisis a (magnitud, frecuencia en Hz) por bin y
/// resíntesis con fase acumulada. El salto de síntesis es el de análisis.
pub struct PhaseVocoder {
    n: usize,
    hop: usize,
    sr: f32,
    prev: Vec<f32>,
    acc: Vec<f32>,
}

impl PhaseVocoder {
    pub fn new(n: usize, hop: usize, sr: u32) -> Self {
        let b = n / 2 + 1;
        Self { n, hop, sr: sr as f32, prev: vec![0.0; b], acc: vec![0.0; b] }
    }

    pub fn analyze(&mut self, half: &[Complex32], mags: &mut [f32], freqs: &mut [f32]) {
        let bin_hz = self.sr / self.n as f32;
        for k in 0..half.len() {
            let (m, ph) = half[k].to_polar();
            let expected = TAU * k as f32 * self.hop as f32 / self.n as f32;
            let dev = wrap(ph - self.prev[k] - expected);
            self.prev[k] = ph;
            mags[k] = m;
            freqs[k] = (k as f32 + dev * self.n as f32 / (TAU * self.hop as f32)) * bin_hz;
        }
    }

    pub fn synth(&mut self, mags: &[f32], freqs: &[f32], out: &mut [Complex32]) {
        let step = TAU * self.hop as f32 / self.sr;
        for k in 0..out.len() {
            self.acc[k] = wrap(self.acc[k] + freqs[k] * step);
            out[k] = Complex32::from_polar(mags[k], self.acc[k]);
        }
    }
}

/// RMS de una señal.
pub fn rms(x: &[f32]) -> f64 {
    if x.is_empty() {
        return 0.0;
    }
    (x.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / x.len() as f64).sqrt()
}

/// Escala `y` para que su RMS sea `target` (para resíntesis con fases
/// nuevas, cuyo nivel no se conserva).
pub fn match_rms(y: &mut [f32], target: f64) {
    let r = rms(y);
    if r > 1e-12 && target > 0.0 {
        let g = (target / r) as f32;
        y.iter_mut().for_each(|v| *v *= g);
    }
}

/// Espectrograma de `x`: `rows` filas (fila 0 = frecuencia más alta, escala
/// logarítmica desde `f_min` hasta Nyquist) por hasta `max_cols` columnas.
/// Valores 0..1 = -90..0 dB respecto del máximo. Devuelve (columnas, datos
/// fila por fila).
pub fn spectrogram(x: &[f32], sr: u32, max_cols: usize, rows: usize, f_min: f32) -> (usize, Vec<f32>) {
    let n = if sr >= 32000 { 2048 } else { 1024 };
    let hop = (x.len() / max_cols.max(1)).max(n / 4).max(1);
    let mut an = Analyzer::new(n);
    let bins = an.bins();
    let mut half = vec![Complex32::default(); bins];
    let starts: Vec<i64> = frame_starts(x.len(), n, hop).collect();
    let cols = starts.len().max(1);
    // Límites de bin de cada fila (escala log).
    let nyq = sr as f32 / 2.0;
    let edge = |r: usize| f_min * (nyq / f_min).powf(r as f32 / rows as f32);
    let ranges: Vec<(usize, usize)> = (0..rows)
        .map(|r| {
            let lo = (edge(r) / nyq * (bins - 1) as f32).floor() as usize;
            let hi = (edge(r + 1) / nyq * (bins - 1) as f32).ceil() as usize;
            (lo.min(bins - 1), hi.clamp(lo + 1, bins))
        })
        .collect();
    let mut mag = vec![0.0f32; rows * cols];
    let mut peak = 1e-12f32;
    for (c, &s) in starts.iter().enumerate() {
        an.frame(x, s, &mut half);
        for (r, &(lo, hi)) in ranges.iter().enumerate() {
            let m = half[lo..hi].iter().fold(0.0f32, |a, z| a.max(z.norm()));
            mag[(rows - 1 - r) * cols + c] = m;
            peak = peak.max(m);
        }
    }
    for v in mag.iter_mut() {
        let db = 20.0 * (*v / peak).max(1e-9).log10();
        *v = ((db + 90.0) / 90.0).clamp(0.0, 1.0);
    }
    (cols, mag)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analysis_synthesis_is_transparent() {
        let x: Vec<f32> = (0..5000).map(|i| ((i as f32 * 0.037).sin() + (i as f32 * 0.21).cos()) * 0.4).collect();
        for (n, hop) in [(256, 64), (512, 128), (1024, 100)] {
            let mut an = Analyzer::new(n);
            let mut sy = Synth::new(n, x.len());
            let mut half = vec![Complex32::default(); an.bins()];
            for s in frame_starts(x.len(), n, hop) {
                an.frame(&x, s, &mut half);
                sy.add(s, &half);
            }
            let y = sy.finish(x.len());
            let err = x.iter().zip(&y).fold(0.0f32, |m, (a, b)| m.max((a - b).abs()));
            assert!(err < 1e-4, "n={n} hop={hop}: {err}");
        }
    }

    #[test]
    fn phase_vocoder_measures_frequency() {
        let sr = 48000;
        let f = 1234.5f32;
        let x: Vec<f32> = (0..20000).map(|i| (TAU * f * i as f32 / sr as f32).sin()).collect();
        let (n, hop) = (2048, 512);
        let mut an = Analyzer::new(n);
        let mut pv = PhaseVocoder::new(n, hop, sr);
        let mut half = vec![Complex32::default(); an.bins()];
        let (mut m, mut fr) = (vec![0.0; an.bins()], vec![0.0; an.bins()]);
        for s in frame_starts(x.len(), n, hop).skip(4).take(4) {
            an.frame(&x, s, &mut half);
            pv.analyze(&half, &mut m, &mut fr);
        }
        let k = (0..m.len()).max_by(|a, b| m[*a].total_cmp(&m[*b])).unwrap();
        assert!((fr[k] - f).abs() < 1.0, "{}", fr[k]);
    }

    #[test]
    fn spectrogram_has_requested_shape() {
        let x: Vec<f32> = (0..48000).map(|i| (i as f32 * 0.1).sin()).collect();
        let (cols, d) = spectrogram(&x, 48000, 300, 128, 30.0);
        assert!(cols <= 301 && cols > 50, "{cols}");
        assert_eq!(d.len(), cols * 128);
        assert!(d.iter().all(|v| (0.0..=1.0).contains(v)));
    }
}
