//! Limitador de picos offline con anticipación ("lookahead").
//!
//! Al no ser tiempo real se puede calcular la curva de ganancia completa:
//!   1. ganancia requerida por muestra: r[n] = min(1, techo / |x[n]|);
//!   2. mínimo móvil hacia adelante en una ventana W: g1[n] = min r[n..=n+W];
//!   3. recuperación exponencial (release), que solo puede bajar g1;
//!   4. media móvil hacia atrás de largo W+1.
//! Cada término del promedio de (4) es <= r[n], así que el promedio también:
//! la salida nunca pasa el techo y la ganancia cambia sin saltos.

use crate::AudioBuf;
use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LimiterSettings {
    /// Techo en dBFS (p. ej. -0.3).
    pub ceiling_db: f64,
    /// Anticipación / ataque en milisegundos.
    pub lookahead_ms: f64,
    /// Tiempo de recuperación en milisegundos.
    pub release_ms: f64,
}

impl Default for LimiterSettings {
    fn default() -> Self {
        Self { ceiling_db: -0.3, lookahead_ms: 5.0, release_ms: 120.0 }
    }
}

/// Limita `buf` en el lugar (todos los canales con la misma ganancia, para no
/// mover la imagen estéreo). Devuelve la reducción máxima aplicada en dB (<= 0).
pub fn limit(buf: &mut AudioBuf, s: &LimiterSettings) -> f64 {
    let n = buf.frames();
    if n == 0 {
        return 0.0;
    }
    let ceil = crate::db_to_lin(s.ceiling_db) as f32;
    let w = ((s.lookahead_ms * 0.001 * buf.sr as f64).round() as usize).max(1);

    // 1. ganancia requerida (pico entre canales)
    let req: Vec<f32> = (0..n)
        .map(|i| {
            let p = buf.channels.iter().fold(0.0f32, |m, c| m.max(c[i].abs()));
            if p > ceil {
                ceil / p
            } else {
                1.0
            }
        })
        .collect();
    if req.iter().all(|&g| g >= 1.0) {
        return 0.0;
    }

    // 2. mínimo móvil hacia adelante (deque monótona), O(n)
    let mut g1 = vec![1.0f32; n];
    let mut dq: VecDeque<usize> = VecDeque::new();
    let mut next = 0usize;
    for (i, g) in g1.iter_mut().enumerate() {
        let hi = (i + w).min(n - 1);
        while next <= hi {
            while dq.back().is_some_and(|&j| req[j] >= req[next]) {
                dq.pop_back();
            }
            dq.push_back(next);
            next += 1;
        }
        while dq.front().is_some_and(|&j| j < i) {
            dq.pop_front();
        }
        *g = req[dq[0]];
    }

    // 3. release: sube hacia 1 de forma exponencial, nunca por encima de g1
    let coef = (-1.0 / (s.release_ms.max(1.0) * 0.001 * buf.sr as f64)).exp() as f32;
    let mut env = 1.0f32;
    for g in g1.iter_mut() {
        env = 1.0 - (1.0 - env) * coef;
        env = env.min(*g);
        *g = env;
    }

    // 4. media móvil hacia atrás de largo w+1 (antes del inicio cuenta como 1)
    let mut sum = w as f64 + 1.0;
    let mut gain = vec![1.0f32; n];
    for i in 0..n {
        sum += g1[i] as f64;
        sum -= if i > w { g1[i - w - 1] as f64 } else { 1.0 };
        gain[i] = (sum / (w + 1) as f64) as f32;
    }

    for c in &mut buf.channels {
        for (x, &g) in c.iter_mut().zip(&gain) {
            // El clamp es seguridad numérica: el redondeo f32 no debe pasar.
            *x = (*x * g).clamp(-ceil, ceil);
        }
    }
    let min_g = gain.iter().fold(1.0f32, |m, &g| m.min(g));
    crate::lin_to_db(min_g as f64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Analysis;

    #[test]
    fn never_exceeds_ceiling_and_leaves_quiet_parts() {
        let sr = 48000;
        let mut l = vec![0.0f32; sr];
        for (i, x) in l.iter_mut().enumerate() {
            let t = i as f32 / sr as f32;
            let amp = if (0.4..0.5).contains(&t) { 3.0 } else { 0.25 };
            *x = amp * (2.0 * std::f32::consts::PI * 220.0 * t).sin();
        }
        let r = l.clone();
        let mut b = AudioBuf::from_channels(sr as u32, vec![l, r]);
        let red = limit(&mut b, &LimiterSettings::default());
        assert!(red < -9.0);
        let a = Analysis::of(&b);
        assert!(a.peak <= crate::db_to_lin(-0.3) as f32 + 1e-6, "pico {}", a.peak);
        // lejos del golpe, la señal queda intacta
        let t = (0.1 * sr as f32) as usize;
        let orig = 0.25 * (2.0 * std::f32::consts::PI * 220.0 * (t as f32 / sr as f32)).sin();
        assert!((b.channels[0][t] - orig).abs() < 1e-4);
    }
}
