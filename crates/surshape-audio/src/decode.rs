//! Lectura de WAV, AIFF y FLAC con symphonia (MPL-2.0). Todo se convierte a
//! f32 planar a la frecuencia de muestreo original (no se remuestrea).

use crate::{AudioBuf, AudioError};
use std::fs::File;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

/// Extensiones que se intentan abrir (en minúsculas).
pub const EXTENSIONS: &[&str] = &["wav", "wave", "aif", "aiff", "aifc", "flac"];

/// ¿La extensión del archivo es de un formato soportado?
pub fn is_supported(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

/// Carga un archivo completo.
pub fn load(path: &Path) -> Result<AudioBuf, AudioError> {
    load_with(path, None, None)
}

/// Carga con progreso (0..=1_000_000 en `progress`) y cancelación opcionales.
pub fn load_with(
    path: &Path,
    progress: Option<&AtomicU32>,
    cancel: Option<&AtomicBool>,
) -> Result<AudioBuf, AudioError> {
    let name = path.display().to_string();
    let file = File::open(path).map_err(|e| AudioError::new("err.audio.abrir").arg("archivo", &name).arg("detalle", e))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|_| AudioError::new("err.audio.formato").arg("archivo", &name))?;
    let mut format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| AudioError::new("err.audio.sin_audio").arg("archivo", &name))?;
    let track_id = track.id;
    let total = track.codec_params.n_frames.unwrap_or(0);
    let mut sr = track.codec_params.sample_rate.unwrap_or(0);
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|_| AudioError::new("err.audio.codec").arg("archivo", &name))?;

    let mut channels: Vec<Vec<f32>> = Vec::new();
    let mut sample_buf: Option<SampleBuffer<f32>> = None;
    loop {
        if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
            return Err(AudioError::new("err.cancelado"));
        }
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(SymError::ResetRequired) => break,
            Err(e) => return Err(AudioError::new("err.audio.leer").arg("archivo", &name).arg("detalle", e)),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            // Paquete dañado: se salta, como hacen los reproductores.
            Err(SymError::DecodeError(_)) => continue,
            Err(e) => return Err(AudioError::new("err.audio.leer").arg("archivo", &name).arg("detalle", e)),
        };
        let spec = *decoded.spec();
        let nch = spec.channels.count();
        if channels.is_empty() {
            channels = vec![Vec::with_capacity(total as usize); nch];
            sr = spec.rate;
        }
        if nch != channels.len() {
            return Err(AudioError::new("err.audio.leer").arg("archivo", &name).arg("detalle", "channels"));
        }
        let cap = decoded.capacity() as u64;
        let sb = match &mut sample_buf {
            Some(sb) if sb.capacity() as u64 >= cap => sb,
            _ => sample_buf.insert(SampleBuffer::<f32>::new(cap, spec)),
        };
        sb.copy_interleaved_ref(decoded);
        for frame in sb.samples().chunks_exact(nch) {
            for (c, &v) in channels.iter_mut().zip(frame) {
                c.push(v);
            }
        }
        if let (Some(p), true) = (progress, total > 0) {
            let done = channels[0].len() as f64 / total as f64;
            p.store((done.min(1.0) * 1_000_000.0) as u32, Ordering::Relaxed);
        }
    }
    if channels.is_empty() || channels[0].is_empty() || sr == 0 {
        return Err(AudioError::new("err.audio.sin_audio").arg("archivo", &name));
    }
    Ok(AudioBuf::from_channels(sr, channels))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::{write_wav, WavFormat};

    #[test]
    fn roundtrip_float_and_24() {
        let dir = std::env::temp_dir().join(format!("surshape_dec_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let l: Vec<f32> = (0..4800).map(|i| (i as f32 * 0.01).sin() * 0.8).collect();
        let r: Vec<f32> = l.iter().map(|x| -x).collect();
        let b = AudioBuf::from_channels(44100, vec![l, r]);
        for (fmt, tol) in [(WavFormat::Float32, 0.0), (WavFormat::Int24, 1.0 / 8_000_000.0)] {
            let p = dir.join(format!("t_{fmt:?}.wav"));
            write_wav(&p, &b, fmt).unwrap();
            let back = load(&p).unwrap();
            assert_eq!(back.sr, 44100);
            assert_eq!(back.frames(), 4800);
            assert_eq!(back.num_channels(), 2);
            let err = back.channels[1].iter().zip(&b.channels[1]).fold(0.0f32, |m, (a, b)| m.max((a - b).abs()));
            assert!(err <= tol, "{fmt:?}: {err}");
        }
        assert!(load(&dir.join("no_existe.wav")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
