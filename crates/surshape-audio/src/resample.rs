//! Cambio de frecuencia de muestreo con sinc enventanado (Blackman, 32
//! puntos). Se usa cuando las entradas de un proceso tienen distinta
//! frecuencia: la secundaria se lleva a la de la principal. Al bajar la
//! frecuencia, el filtro se estrecha para evitar aliasing.

use crate::AudioBuf;

const HALF: i64 = 16;

fn sinc(x: f64) -> f64 {
    if x.abs() < 1e-12 {
        1.0
    } else {
        let px = std::f64::consts::PI * x;
        px.sin() / px
    }
}

fn blackman(x: f64) -> f64 {
    // x en [-1, 1]
    let t = (x + 1.0) * 0.5;
    0.42 - 0.5 * (2.0 * std::f64::consts::PI * t).cos() + 0.08 * (4.0 * std::f64::consts::PI * t).cos()
}

fn channel(x: &[f32], ratio: f64) -> Vec<f32> {
    let n_out = (x.len() as f64 * ratio).round() as usize;
    let cutoff = ratio.min(1.0);
    let half = (HALF as f64 / cutoff).ceil() as i64;
    (0..n_out)
        .map(|j| {
            let t = j as f64 / ratio;
            let c = t.floor() as i64;
            let (mut acc, mut wsum) = (0.0f64, 0.0f64);
            for k in (c - half + 1)..=(c + half) {
                let d = t - k as f64;
                let w = sinc(d * cutoff) * blackman((d / half as f64).clamp(-1.0, 1.0));
                wsum += w;
                if k >= 0 && (k as usize) < x.len() {
                    acc += x[k as usize] as f64 * w;
                }
            }
            if wsum.abs() > 1e-9 {
                (acc / wsum) as f32
            } else {
                0.0
            }
        })
        .collect()
}

/// Copia de `buf` a la frecuencia `sr`.
pub fn resample(buf: &AudioBuf, sr: u32) -> AudioBuf {
    if buf.sr == sr || buf.sr == 0 {
        return buf.clone();
    }
    let ratio = sr as f64 / buf.sr as f64;
    AudioBuf::from_channels(sr, buf.channels.iter().map(|c| channel(c, ratio)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sine_survives_44k_to_48k() {
        let f = 1000.0;
        let x: Vec<f32> = (0..44100).map(|i| (2.0 * std::f64::consts::PI * f * i as f64 / 44100.0).sin() as f32 * 0.5).collect();
        let y = resample(&AudioBuf::from_channels(44100, vec![x]), 48000);
        assert_eq!(y.frames(), 48000);
        // Comparar con el seno ideal a 48 kHz, lejos de los bordes.
        let err = (1000..47000)
            .map(|i| (y.channels[0][i] - (2.0 * std::f64::consts::PI * f * i as f64 / 48000.0).sin() as f32 * 0.5).abs())
            .fold(0.0f32, f32::max);
        assert!(err < 2e-3, "{err}");
    }
}
