//! SURSHAPE · audio · por @ahrsml
//!
//! Búfer f32 planar ([`AudioBuf`]), lectura de WAV/AIFF/FLAC ([`decode`]),
//! escritura WAV 24 bit / 32 float ([`export`]), análisis de pico, NaN/Inf y
//! clipping ([`analysis`]), limitador offline ([`limiter`]) y resúmenes de
//! picos para dibujar la forma de onda ([`peaks`]).
//!
//! Los errores no llevan texto: llevan una clave i18n (`err.*`) y argumentos,
//! que la interfaz traduce.

pub mod analysis;
pub mod buf;
pub mod decode;
pub mod export;
pub mod limiter;
pub mod peaks;
pub mod resample;

pub use analysis::Analysis;
pub use buf::AudioBuf;
pub use export::WavFormat;

/// Error con clave i18n y argumentos `{nombre}` para el mensaje.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioError {
    pub key: &'static str,
    pub args: Vec<(&'static str, String)>,
}

impl AudioError {
    pub fn new(key: &'static str) -> Self {
        Self { key, args: Vec::new() }
    }
    pub fn arg(mut self, name: &'static str, v: impl ToString) -> Self {
        self.args.push((name, v.to_string()));
        self
    }
}

impl std::fmt::Display for AudioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.key)?;
        for (k, v) in &self.args {
            write!(f, " {k}={v}")?;
        }
        Ok(())
    }
}

impl std::error::Error for AudioError {}

/// dBFS -> ganancia lineal.
#[inline]
pub fn db_to_lin(db: f64) -> f64 {
    10f64.powf(db / 20.0)
}

/// Ganancia lineal -> dBFS (-inf para 0).
#[inline]
pub fn lin_to_db(x: f64) -> f64 {
    if x > 0.0 {
        20.0 * x.log10()
    } else {
        f64::NEG_INFINITY
    }
}
