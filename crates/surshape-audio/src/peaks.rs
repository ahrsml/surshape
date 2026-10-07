//! Resúmenes mín/máx por bloques (niveles tipo "mipmap") para dibujar la forma
//! de onda rápido a cualquier zoom sin recorrer todo el audio en cada cuadro.

use crate::AudioBuf;

/// Tamaño de bloque del primer nivel; cada nivel siguiente es 4x mayor.
pub const BASE_BLOCK: usize = 64;

#[derive(Clone, Debug, Default)]
pub struct Level {
    pub block: usize,
    /// (mín, máx) por bloque.
    pub mm: Vec<(f32, f32)>,
}

/// Picos de un canal.
#[derive(Clone, Debug, Default)]
pub struct ChannelPeaks {
    pub levels: Vec<Level>,
}

#[derive(Clone, Debug, Default)]
pub struct Peaks {
    pub channels: Vec<ChannelPeaks>,
}

impl Peaks {
    pub fn of(buf: &AudioBuf) -> Self {
        Self { channels: buf.channels.iter().map(|c| ChannelPeaks::of(c)).collect() }
    }
}

fn fold_mm<'a>(it: impl Iterator<Item = &'a (f32, f32)>) -> (f32, f32) {
    it.fold((f32::MAX, f32::MIN), |(lo, hi), &(a, b)| (lo.min(a), hi.max(b)))
}

impl ChannelPeaks {
    pub fn of(x: &[f32]) -> Self {
        let first: Vec<(f32, f32)> = x
            .chunks(BASE_BLOCK)
            .map(|c| c.iter().fold((f32::MAX, f32::MIN), |(lo, hi), &v| (lo.min(v), hi.max(v))))
            .collect();
        let mut levels = vec![Level { block: BASE_BLOCK, mm: first }];
        loop {
            let prev = levels.last().unwrap();
            if prev.mm.len() <= 1 {
                break;
            }
            let mm = prev.mm.chunks(4).map(|c| fold_mm(c.iter())).collect();
            let block = prev.block * 4;
            levels.push(Level { block, mm });
        }
        Self { levels }
    }

    /// (mín, máx) del tramo [s, e) de muestras. Usa el nivel más grueso cuyo
    /// bloque cabe dos veces en el tramo; si el tramo es corto, lee el audio.
    pub fn range(&self, raw: &[f32], s: usize, e: usize) -> Option<(f32, f32)> {
        let e = e.min(raw.len());
        if s >= e {
            return None;
        }
        let span = e - s;
        if span < BASE_BLOCK * 2 || self.levels.is_empty() {
            return Some(raw[s..e].iter().fold((f32::MAX, f32::MIN), |(lo, hi), &v| (lo.min(v), hi.max(v))));
        }
        let lvl = self.levels.iter().rev().find(|l| l.block * 2 <= span).unwrap_or(&self.levels[0]);
        let b0 = (s / lvl.block).min(lvl.mm.len() - 1);
        let b1 = e.div_ceil(lvl.block).clamp(b0 + 1, lvl.mm.len());
        Some(fold_mm(lvl.mm[b0..b1].iter()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_finds_extremes() {
        let mut x = vec![0.0f32; 100_000];
        x[50_000] = 0.9;
        x[70_000] = -0.8;
        let p = ChannelPeaks::of(&x);
        assert_eq!(p.range(&x, 0, x.len()), Some((-0.8, 0.9)));
        assert_eq!(p.range(&x, 0, 40_000), Some((0.0, 0.0)));
        assert_eq!(p.range(&x, 49_990, 50_010), Some((0.0, 0.9)));
    }
}
