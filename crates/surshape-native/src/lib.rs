//! SURSHAPE · procesos nativos · por @ahrsml
//!
//! Cada proceso implementa [`surshape_engine::Process`]. Los de destrucción
//! usan los módulos de `noisegek-dsp` tal cual.
//!
//! Fase 1: normalizar, invertir, estiramiento extremo, bitcrush, waveshaper.
//! Fase 2: motor STFT ([`spectral`]) y procesos espectrales ([`spec_procs`]).
//! Fase 4: granular, convolución, síntesis cruzada, generador de ruido.
//! Fase 5: dividir en bandas (varias salidas) y la mezcla ([`mix`]).
//! Fase 6: edición no destructiva ([`edit`]: extraer, quitar, silenciar,
//! fundidos, polaridad, ganancia).

mod bands;
mod bitcrush;
mod convolve;
mod cross;
pub mod edit;
mod gen;
mod granular;
pub mod mix;
mod normalize;
mod reverse;
pub mod spec_procs;
pub mod spectral;
mod stretch;
mod util;
mod waveshaper;

use std::sync::Arc;
use surshape_engine::Registry;

pub use bitcrush::Bitcrush;
pub use normalize::Normalize;
pub use reverse::Reverse;
pub use stretch::PaulStretch;
pub use waveshaper::Waveshaper;

/// Agrega todos los procesos nativos al catálogo.
pub fn register(reg: &mut Registry) {
    reg.add(Arc::new(PaulStretch));
    reg.add(Arc::new(granular::Granular));
    reg.add(Arc::new(Reverse));
    reg.add(Arc::new(spec_procs::Freeze));
    reg.add(Arc::new(spec_procs::SpecBlur));
    reg.add(Arc::new(spec_procs::SpecGate));
    reg.add(Arc::new(spec_procs::SpecShift));
    reg.add(Arc::new(spec_procs::SpecStretch));
    reg.add(Arc::new(spec_procs::BinShuffle));
    reg.add(Arc::new(spec_procs::Robotize));
    reg.add(Arc::new(spec_procs::Whisperize));
    reg.add(Arc::new(cross::CrossSynth));
    reg.add(Arc::new(bands::BandSplit));
    reg.add(Arc::new(convolve::Convolve));
    reg.add(Arc::new(Bitcrush));
    reg.add(Arc::new(Waveshaper));
    reg.add(Arc::new(Normalize));
    reg.add(Arc::new(edit::Extract));
    reg.add(Arc::new(edit::Remove));
    reg.add(Arc::new(edit::Silence));
    reg.add(Arc::new(edit::Fade));
    reg.add(Arc::new(edit::Polarity));
    reg.add(Arc::new(edit::Gain));
    reg.add(Arc::new(gen::Noise));
}

/// Catálogo con solo los procesos nativos.
pub fn registry() -> Registry {
    let mut r = Registry::new();
    register(&mut r);
    r
}
