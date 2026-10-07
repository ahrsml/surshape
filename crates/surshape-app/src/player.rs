//! Reproducción con cpal (Apache-2.0). El callback de audio solo lee: el
//! búfer se cambia con `try_lock` (si la interfaz lo está cambiando justo en
//! ese instante, el callback entrega silencio un bloque, nunca se bloquea).
//!
//! Si la frecuencia del dispositivo no coincide con la del sample se
//! remuestrea con interpolación lineal (solo para escuchar; los renders nunca
//! pasan por aquí). El dispositivo de salida se elige en Preferencias.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use surshape_audio::AudioBuf;

struct Shared {
    buf: Mutex<Option<Arc<AudioBuf>>>,
    /// Posición actual en muestras de la fuente (f64 en bits).
    pos: AtomicU64,
    /// Tramo que se reproduce [start, end) en muestras.
    start: AtomicU64,
    end: AtomicU64,
    playing: AtomicBool,
    looping: AtomicBool,
}

pub struct Player {
    shared: Arc<Shared>,
    stream: Option<cpal::Stream>,
    device_sr: u32,
    /// Nombre del dispositivo elegido (None = el del sistema).
    device: Option<String>,
    paused: bool,
    /// Clave i18n + detalle del último error de dispositivo.
    pub error: Option<(&'static str, String)>,
}

impl Default for Player {
    fn default() -> Self {
        Self {
            shared: Arc::new(Shared {
                buf: Mutex::new(None),
                pos: AtomicU64::new(0f64.to_bits()),
                start: AtomicU64::new(0),
                end: AtomicU64::new(0),
                playing: AtomicBool::new(false),
                looping: AtomicBool::new(false),
            }),
            stream: None,
            device_sr: 0,
            device: None,
            paused: false,
            error: None,
        }
    }
}

/// Nombres de los dispositivos de salida.
pub fn output_devices() -> Vec<String> {
    cpal::default_host().output_devices().map(|d| d.filter_map(|x| x.name().ok()).collect()).unwrap_or_default()
}

impl Player {
    /// Elige el dispositivo (se abre la próxima vez que se reproduce).
    pub fn set_device(&mut self, name: Option<String>) {
        if self.device != name {
            self.stop();
            self.stream = None;
            self.device = name;
        }
    }

    /// Abre el dispositivo de salida (la primera vez que se reproduce).
    /// Devuelve false si no hay salida de audio.
    fn ensure_stream(&mut self) -> bool {
        if self.stream.is_some() {
            return true;
        }
        match self.open() {
            Ok(()) => {
                self.error = None;
                true
            }
            Err(e) => {
                self.error = Some(("err.reproduccion.dispositivo", e));
                false
            }
        }
    }

    fn open(&mut self) -> Result<(), String> {
        let host = cpal::default_host();
        let named = self.device.as_ref().and_then(|n| host.output_devices().ok()?.find(|d| d.name().ok().as_deref() == Some(n.as_str())));
        let device = named.or_else(|| host.default_output_device()).ok_or_else(String::new)?;
        let supported = device.default_output_config().map_err(|e| e.to_string())?;
        let fmt = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();
        self.device_sr = config.sample_rate.0;
        let stream = match fmt {
            cpal::SampleFormat::F32 => self.build::<f32>(&device, &config),
            cpal::SampleFormat::I16 => self.build::<i16>(&device, &config),
            cpal::SampleFormat::U16 => self.build::<u16>(&device, &config),
            cpal::SampleFormat::I32 => self.build::<i32>(&device, &config),
            other => return Err(format!("{other:?}")),
        }?;
        stream.play().map_err(|e| e.to_string())?;
        self.stream = Some(stream);
        Ok(())
    }

    fn build<T>(&self, device: &cpal::Device, config: &cpal::StreamConfig) -> Result<cpal::Stream, String>
    where
        T: cpal::SizedSample + cpal::FromSample<f32>,
    {
        let sh = self.shared.clone();
        let out_ch = config.channels as usize;
        let dev_sr = config.sample_rate.0 as f64;
        device
            .build_output_stream(
                config,
                move |data: &mut [T], _: &cpal::OutputCallbackInfo| fill(&sh, data, out_ch, dev_sr),
                |_e| {},
                None,
            )
            .map_err(|e| e.to_string())
    }

    /// Reproduce `buf` desde `from` hasta `to` (muestras de la fuente).
    pub fn play(&mut self, buf: Arc<AudioBuf>, from: usize, to: usize, looping: bool) {
        if !self.ensure_stream() {
            return;
        }
        self.paused = false;
        let to = to.min(buf.frames());
        let from = from.min(to);
        if let Ok(mut b) = self.shared.buf.lock() {
            *b = Some(buf);
        }
        self.shared.start.store(from as u64, Ordering::Relaxed);
        self.shared.end.store(to as u64, Ordering::Relaxed);
        self.shared.pos.store((from as f64).to_bits(), Ordering::Relaxed);
        self.shared.looping.store(looping, Ordering::Relaxed);
        self.shared.playing.store(true, Ordering::Release);
    }

    pub fn stop(&mut self) {
        self.paused = false;
        self.shared.playing.store(false, Ordering::Release);
    }

    /// Pausa (la posición se conserva para seguir).
    pub fn pause(&mut self) {
        if self.is_playing() {
            self.shared.playing.store(false, Ordering::Release);
            self.paused = true;
        }
    }

    pub fn resume(&mut self) {
        if self.paused {
            self.paused = false;
            self.shared.playing.store(true, Ordering::Release);
        }
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn is_playing(&self) -> bool {
        self.shared.playing.load(Ordering::Acquire)
    }

    /// Posición actual (muestras de la fuente).
    pub fn position(&self) -> usize {
        f64::from_bits(self.shared.pos.load(Ordering::Relaxed)) as usize
    }

    /// ¿Está sonando (o en pausa) este búfer?
    pub fn is_playing_buf(&self, buf: &Arc<AudioBuf>) -> bool {
        (self.is_playing() || self.paused) && self.shared.buf.lock().ok().is_some_and(|b| b.as_ref().is_some_and(|b| Arc::ptr_eq(b, buf)))
    }
}

fn fill<T: cpal::SizedSample + cpal::FromSample<f32>>(sh: &Shared, data: &mut [T], out_ch: usize, dev_sr: f64) {
    let silence = T::from_sample(0.0f32);
    let guard = if sh.playing.load(Ordering::Acquire) { sh.buf.try_lock().ok() } else { None };
    let Some(buf) = guard.as_ref().and_then(|g| g.as_ref()) else {
        data.fill(silence);
        return;
    };
    let step = buf.sr as f64 / dev_sr;
    let start = sh.start.load(Ordering::Relaxed) as f64;
    let end = sh.end.load(Ordering::Relaxed) as f64;
    let looping = sh.looping.load(Ordering::Relaxed);
    let mut pos = f64::from_bits(sh.pos.load(Ordering::Relaxed));
    let nch = buf.num_channels();
    let n = buf.frames();
    let mut stopped = false;
    for frame in data.chunks_mut(out_ch.max(1)) {
        if pos >= end {
            if looping && end > start {
                pos = start + (pos - end);
            } else {
                stopped = true;
            }
        }
        if stopped {
            frame.fill(silence);
            continue;
        }
        let i = pos as usize;
        let f = (pos - i as f64) as f32;
        let read = |c: usize| {
            let ch = &buf.channels[c.min(nch - 1)];
            let a = ch[i.min(n - 1)];
            let b = ch[(i + 1).min(n - 1)];
            a + (b - a) * f
        };
        for (oc, s) in frame.iter_mut().enumerate() {
            // Mono -> todas las salidas; estéreo -> L/R; más canales: los dos
            // primeros (las salidas extra quedan en silencio).
            let v = if nch == 1 {
                read(0)
            } else if oc < 2 {
                read(oc)
            } else {
                0.0
            };
            *s = T::from_sample(v.clamp(-1.0, 1.0));
        }
        pos += step;
    }
    sh.pos.store(pos.min(end).to_bits(), Ordering::Relaxed);
    if stopped {
        sh.playing.store(false, Ordering::Release);
    }
}
