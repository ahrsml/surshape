//! SURSHAPE · motor · por @ahrsml
//!
//! Interfaz común de todos los procesos (nativos y CDP): [`Process`], sus
//! parámetros ([`ParamSpec`], [`ParamValues`]) y el contexto de render
//! ([`RenderCtx`]). El módulo [`render`] corre un proceso en un hilo aparte,
//! con progreso, cancelación y validación del resultado.
//!
//! Ningún tipo de este crate lleva texto visible: solo claves i18n.

pub mod console;
pub mod data;
pub mod hash;
pub mod params;
pub mod process;
pub mod registry;
pub mod render;
pub mod task;

pub use hash::StableHasher;
pub use params::{
    randomize, Breakpoints, Curve, Interp, ParamKind, ParamSet, ParamSpec, ParamValue, ParamValues, Scale, TimeMode, Unit,
};
pub use data::{DataSet, FileKind};
pub use process::{
    descriptor, CmdCtx, Engine, Family, FileCtx, InputSpec, Inputs, LenRule, Outputs, Process, ProcessError, RenderCtx,
    MODE_PARAM,
};
pub use registry::Registry;
pub use render::{FileOut, FileRenderOutput, PvocSettings, RenderJob, RenderOptions, RenderOutput, RenderReport};
pub use task::Task;
pub use surshape_audio::AudioBuf;

/// Máximo de muestras (canales x frames) que puede producir un render. El
/// render vive entero en memoria: 400 M muestras f32 = 1,6 GB (~69 min en
/// estéreo a 48 kHz).
pub const MAX_RENDER_SAMPLES: usize = 400_000_000;

/// Reduce una seed de 64 bit a 32 bit (para los RNG de noisegek-dsp), nunca 0.
pub fn seed32(seed: u64) -> u32 {
    // splitmix64: mezcla todos los bits antes de recortar
    let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    ((z as u32) ^ ((z >> 32) as u32)).max(1)
}
