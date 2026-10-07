//! Ayudas comunes a los procesos nativos.

use surshape_engine::{AudioBuf, ProcessError, RenderCtx};

/// Cada cuántas muestras se revisa la cancelación y se publica el progreso.
pub const CHUNK: usize = 1 << 15;

/// Aplica `f(estado, muestra, tiempo_en_segundos)` a cada canal de la
/// entrada 0. `make_state` crea el estado de cada canal (filtros, RNG,
/// lectores de breakpoints...). Revisa cancelación y publica el progreso.
pub fn map_samples<S>(
    ctx: &RenderCtx,
    mut make_state: impl FnMut(usize) -> S,
    mut f: impl FnMut(&mut S, f64, f64) -> f64,
) -> Result<Vec<AudioBuf>, ProcessError> {
    let input = ctx.input(0);
    let total = (input.frames() * input.num_channels()).max(1);
    let inv_sr = 1.0 / input.sr.max(1) as f64;
    let mut done = 0usize;
    let mut out = Vec::with_capacity(input.num_channels());
    for (ci, ch) in input.channels.iter().enumerate() {
        let mut st = make_state(ci);
        let mut y = Vec::with_capacity(ch.len());
        for (bi, block) in ch.chunks(CHUNK).enumerate() {
            ctx.check()?;
            let base = bi * CHUNK;
            y.extend(block.iter().enumerate().map(|(i, &x)| f(&mut st, x as f64, (base + i) as f64 * inv_sr) as f32));
            done += block.len();
            ctx.set_progress(done as f64 / total as f64);
        }
        out.push(y);
    }
    Ok(vec![AudioBuf::from_channels(input.sr, out)])
}

/// Mezcla seco/húmedo: `wet` en 0..1.
#[inline]
pub fn mix(dry: f64, wet_sig: f64, wet: f64) -> f64 {
    dry + (wet_sig - dry) * wet
}
