//! Análisis: pico, RMS, muestras no finitas (NaN/Inf) y clipping.

use crate::AudioBuf;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Analysis {
    /// Pico absoluto lineal (ignora NaN/Inf).
    pub peak: f32,
    pub rms: f32,
    /// Cantidad de muestras NaN o ±Inf.
    pub nonfinite: usize,
    /// Muestras con |x| > 1.0 (superan 0 dBFS).
    pub over: usize,
}

impl Analysis {
    pub fn of(buf: &AudioBuf) -> Self {
        let mut a = Analysis::default();
        let mut acc = 0.0f64;
        let mut n = 0usize;
        for c in &buf.channels {
            for &x in c {
                if !x.is_finite() {
                    a.nonfinite += 1;
                    continue;
                }
                let ax = x.abs();
                a.peak = a.peak.max(ax);
                if ax > 1.0 {
                    a.over += 1;
                }
                acc += (x as f64) * (x as f64);
                n += 1;
            }
        }
        if n > 0 {
            a.rms = (acc / n as f64).sqrt() as f32;
        }
        a
    }

    pub fn peak_db(&self) -> f64 {
        crate::lin_to_db(self.peak as f64)
    }

    /// ¿Supera 0 dBFS?
    pub fn clips(&self) -> bool {
        self.over > 0
    }
}

/// Reemplaza NaN/Inf por 0. Devuelve cuántas muestras corrigió.
pub fn sanitize(buf: &mut AudioBuf) -> usize {
    let mut fixed = 0;
    for c in &mut buf.channels {
        for x in c.iter_mut() {
            if !x.is_finite() {
                *x = 0.0;
                fixed += 1;
            }
        }
    }
    fixed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_everything() {
        let mut b = AudioBuf::from_channels(48000, vec![vec![0.5, -1.5, f32::NAN, f32::INFINITY]]);
        let a = Analysis::of(&b);
        assert_eq!(a.peak, 1.5);
        assert_eq!(a.nonfinite, 2);
        assert_eq!(a.over, 1);
        assert!(a.clips());
        assert_eq!(sanitize(&mut b), 2);
        assert_eq!(Analysis::of(&b).nonfinite, 0);
    }
}
