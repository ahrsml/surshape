//! Escritura WAV con hound (Apache-2.0): PCM 24 bit o float 32.
//!
//! Se escribe primero a `<archivo>.part` y se renombra al terminar: nunca queda
//! un WAV a medias con el nombre final, y nunca se pisa un archivo existente
//! salvo que el usuario lo haya elegido así en el diálogo de exportar.

use crate::{AudioBuf, AudioError};
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum WavFormat {
    /// PCM entero de 24 bit. Sin dither: cuantización por redondeo (a 24 bit
    /// el error queda ~144 dB por debajo del fondo de escala).
    Int24,
    /// Coma flotante de 32 bit: sin pérdida respecto del motor.
    #[default]
    Float32,
}

pub fn write_wav(path: &Path, buf: &AudioBuf, fmt: WavFormat) -> Result<(), AudioError> {
    let name = path.display().to_string();
    let err = |e: &dyn std::fmt::Display| AudioError::new("err.audio.escribir").arg("archivo", &name).arg("detalle", e);
    if buf.is_empty() {
        return Err(AudioError::new("err.audio.vacio"));
    }
    let spec = hound::WavSpec {
        channels: buf.num_channels() as u16,
        sample_rate: buf.sr,
        bits_per_sample: match fmt {
            WavFormat::Int24 => 24,
            WavFormat::Float32 => 32,
        },
        sample_format: match fmt {
            WavFormat::Int24 => hound::SampleFormat::Int,
            WavFormat::Float32 => hound::SampleFormat::Float,
        },
    };
    let mut part = path.as_os_str().to_owned();
    part.push(".part");
    let part = std::path::PathBuf::from(part);
    {
        let mut w = hound::WavWriter::create(&part, spec).map_err(|e| err(&e))?;
        let n = buf.frames();
        match fmt {
            WavFormat::Float32 => {
                for i in 0..n {
                    for c in &buf.channels {
                        w.write_sample(c[i]).map_err(|e| err(&e))?;
                    }
                }
            }
            WavFormat::Int24 => {
                const MAX: f32 = 8_388_607.0;
                for i in 0..n {
                    for c in &buf.channels {
                        let v = (c[i].clamp(-1.0, 1.0) * MAX).round() as i32;
                        w.write_sample(v).map_err(|e| err(&e))?;
                    }
                }
            }
        }
        w.finalize().map_err(|e| err(&e))?;
    }
    if path.exists() {
        std::fs::remove_file(path).map_err(|e| err(&e))?;
    }
    std::fs::rename(&part, path).map_err(|e| err(&e))?;
    Ok(())
}
