//! Búfer de audio f32, planar (un `Vec` por canal).

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AudioBuf {
    pub sr: u32,
    pub channels: Vec<Vec<f32>>,
}

impl AudioBuf {
    /// Búfer de `ch` canales y `frames` muestras en silencio.
    pub fn silent(sr: u32, ch: usize, frames: usize) -> Self {
        Self { sr, channels: vec![vec![0.0; frames]; ch] }
    }

    pub fn from_channels(sr: u32, channels: Vec<Vec<f32>>) -> Self {
        debug_assert!(channels.windows(2).all(|w| w[0].len() == w[1].len()));
        Self { sr, channels }
    }

    pub fn num_channels(&self) -> usize {
        self.channels.len()
    }

    /// Cantidad de muestras por canal.
    pub fn frames(&self) -> usize {
        self.channels.first().map_or(0, |c| c.len())
    }

    pub fn duration_secs(&self) -> f64 {
        if self.sr == 0 {
            0.0
        } else {
            self.frames() as f64 / self.sr as f64
        }
    }

    pub fn is_empty(&self) -> bool {
        self.frames() == 0 || self.channels.is_empty()
    }

    /// Copia del tramo [start, end) (en muestras; se recorta a los límites).
    pub fn slice(&self, start: usize, end: usize) -> Self {
        let n = self.frames();
        let s = start.min(n);
        let e = end.min(n).max(s);
        Self { sr: self.sr, channels: self.channels.iter().map(|c| c[s..e].to_vec()).collect() }
    }

    /// Multiplica todo por `g`.
    pub fn apply_gain(&mut self, g: f32) {
        for c in &mut self.channels {
            for x in c.iter_mut() {
                *x *= g;
            }
        }
    }

    /// Muestras intercaladas (L R L R ...), para exportar.
    pub fn interleaved(&self) -> Vec<f32> {
        let n = self.frames();
        let mut out = Vec::with_capacity(self.num_channels() * n);
        for i in 0..n {
            for c in &self.channels {
                out.push(c[i]);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slice_clamps() {
        let b = AudioBuf::from_channels(48000, vec![vec![1.0, 2.0, 3.0], vec![4.0, 5.0, 6.0]]);
        assert_eq!(b.slice(1, 10).channels, vec![vec![2.0, 3.0], vec![5.0, 6.0]]);
        assert_eq!(b.slice(5, 2).frames(), 0);
        assert_eq!(b.interleaved(), vec![1.0, 4.0, 2.0, 5.0, 3.0, 6.0]);
    }
}
