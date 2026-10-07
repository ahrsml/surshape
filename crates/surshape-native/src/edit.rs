//! Edición no destructiva (menú Edición/Mezcla, como las herramientas de
//! onda de Soundshaper): cada operación es una celda nueva y nunca toca el
//! original. Los tiempos van en segundos y se cargan desde la selección del
//! visor.

use surshape_audio::{db_to_lin, AudioBuf};
use surshape_engine::{Family, LenRule, ParamSpec, ParamValues, Process, ProcessError, RenderCtx, Unit};

const T_MAX: f64 = 36_000.0;

const fn t(id: &'static str, key: &'static str, def: f64) -> ParamSpec {
    ParamSpec::float(id, key, 0.0, T_MAX, def).unit(Unit::Seconds).decimals(3).fixed_only()
}

/// Muestras [a, b) de un tramo en segundos, dentro del audio.
fn span(ctx: &RenderCtx, frames: usize, sr: u32) -> (usize, usize) {
    let (x, y) = (ctx.p("inicio"), ctx.p("fin"));
    let (x, y) = (x.min(y), x.max(y));
    let a = ((x * sr as f64).round() as usize).min(frames);
    let b = ((y * sr as f64).round() as usize).min(frames);
    (a, b.max(a))
}

fn span_of(p: &ParamValues, frames: usize, sr: u32) -> (usize, usize) {
    let (x, y) = (p.get("inicio"), p.get("fin"));
    let (x, y) = (x.min(y), x.max(y));
    let a = ((x * sr as f64).round() as usize).min(frames);
    let b = ((y * sr as f64).round() as usize).min(frames);
    (a, b.max(a))
}

// --- Extraer (cortar) ------------------------------------------------------------------

/// Se queda solo con el tramo elegido.
pub struct Extract;
const EXTRACT: [ParamSpec; 2] = [t("inicio", "param.nat.extraer.inicio", 0.0), t("fin", "param.nat.extraer.fin", 1.0)];

impl Process for Extract {
    fn id(&self) -> &'static str {
        "nat.extraer"
    }
    fn family(&self) -> Family {
        Family::Utilidad
    }
    fn params(&self) -> &[ParamSpec] {
        &EXTRACT
    }
    fn expected_len(&self, i: &[&AudioBuf], p: &ParamValues) -> LenRule {
        let (a, b) = span_of(p, i[0].frames(), i[0].sr);
        LenRule::Frames((b - a).max(1))
    }
    fn process(&self, ctx: &mut RenderCtx) -> Result<Vec<AudioBuf>, ProcessError> {
        let x = ctx.input(0);
        let (a, b) = span(ctx, x.frames(), x.sr);
        let mut out = x.slice(a, b.max(a + 1).min(x.frames().max(a + 1)));
        if out.frames() == 0 {
            out = AudioBuf::from_channels(x.sr, vec![vec![0.0; 1]; x.num_channels()]);
        }
        Ok(vec![out])
    }
}

// --- Quitar (recortar) -----------------------------------------------------------------

/// Quita el tramo elegido y une lo de antes con lo de después.
pub struct Remove;
const REMOVE: [ParamSpec; 2] = [t("inicio", "param.nat.quitar.inicio", 0.0), t("fin", "param.nat.quitar.fin", 1.0)];

impl Process for Remove {
    fn id(&self) -> &'static str {
        "nat.quitar"
    }
    fn family(&self) -> Family {
        Family::Utilidad
    }
    fn params(&self) -> &[ParamSpec] {
        &REMOVE
    }
    fn expected_len(&self, i: &[&AudioBuf], p: &ParamValues) -> LenRule {
        let (a, b) = span_of(p, i[0].frames(), i[0].sr);
        LenRule::Frames((i[0].frames() - (b - a)).max(1))
    }
    fn process(&self, ctx: &mut RenderCtx) -> Result<Vec<AudioBuf>, ProcessError> {
        let x = ctx.input(0);
        let (a, b) = span(ctx, x.frames(), x.sr);
        let chans = x
            .channels
            .iter()
            .map(|c| {
                let mut v = c[..a].to_vec();
                v.extend_from_slice(&c[b..]);
                if v.is_empty() {
                    v.push(0.0);
                }
                v
            })
            .collect();
        Ok(vec![AudioBuf::from_channels(x.sr, chans)])
    }
}

// --- Silenciar -------------------------------------------------------------------------

/// Pone en silencio el tramo elegido (mismo largo).
pub struct Silence;
const SILENCE: [ParamSpec; 2] = [t("inicio", "param.nat.silenciar.inicio", 0.0), t("fin", "param.nat.silenciar.fin", 1.0)];

impl Process for Silence {
    fn id(&self) -> &'static str {
        "nat.silenciar"
    }
    fn family(&self) -> Family {
        Family::Utilidad
    }
    fn params(&self) -> &[ParamSpec] {
        &SILENCE
    }
    fn expected_len(&self, _: &[&AudioBuf], _: &ParamValues) -> LenRule {
        LenRule::Same
    }
    fn process(&self, ctx: &mut RenderCtx) -> Result<Vec<AudioBuf>, ProcessError> {
        let mut out = ctx.input(0).clone();
        let (a, b) = span(ctx, out.frames(), out.sr);
        for c in &mut out.channels {
            c[a..b].fill(0.0);
        }
        Ok(vec![out])
    }
}

// --- Fundidos --------------------------------------------------------------------------

const CURVES: &[&str] = &["param.nat.fundido.curva.lineal", "param.nat.fundido.curva.exponencial", "param.nat.fundido.curva.coseno"];

/// Fundido de entrada y de salida.
pub struct Fade;
const FADE: [ParamSpec; 3] = [
    ParamSpec::float("entrada", "param.nat.fundido.entrada", 0.0, 600.0, 0.5).unit(Unit::Seconds).decimals(3).fixed_only(),
    ParamSpec::float("salida", "param.nat.fundido.salida", 0.0, 600.0, 0.5).unit(Unit::Seconds).decimals(3).fixed_only(),
    ParamSpec::choice("curva", "param.nat.fundido.curva", CURVES, 2),
];

fn shape(curve: usize, x: f64) -> f64 {
    let x = x.clamp(0.0, 1.0);
    match curve {
        0 => x,
        1 => x * x * x,
        _ => 0.5 - 0.5 * (std::f64::consts::PI * x).cos(),
    }
}

impl Process for Fade {
    fn id(&self) -> &'static str {
        "nat.fundido"
    }
    fn family(&self) -> Family {
        Family::Utilidad
    }
    fn params(&self) -> &[ParamSpec] {
        &FADE
    }
    fn expected_len(&self, _: &[&AudioBuf], _: &ParamValues) -> LenRule {
        LenRule::Same
    }
    fn process(&self, ctx: &mut RenderCtx) -> Result<Vec<AudioBuf>, ProcessError> {
        let mut out = ctx.input(0).clone();
        let n = out.frames();
        let sr = out.sr as f64;
        let fi = ((ctx.p("entrada") * sr) as usize).min(n);
        let fo = ((ctx.p("salida") * sr) as usize).min(n);
        let curve = ctx.p("curva").round() as usize;
        for c in &mut out.channels {
            for (i, s) in c.iter_mut().enumerate().take(fi) {
                *s *= shape(curve, i as f64 / fi.max(1) as f64) as f32;
            }
            for k in 0..fo {
                let i = n - 1 - k;
                c[i] *= shape(curve, k as f64 / fo.max(1) as f64) as f32;
            }
        }
        Ok(vec![out])
    }
}

// --- Invertir polaridad ------------------------------------------------------------------

pub struct Polarity;

impl Process for Polarity {
    fn id(&self) -> &'static str {
        "nat.polaridad"
    }
    fn family(&self) -> Family {
        Family::Utilidad
    }
    fn params(&self) -> &[ParamSpec] {
        &[]
    }
    fn expected_len(&self, _: &[&AudioBuf], _: &ParamValues) -> LenRule {
        LenRule::Same
    }
    fn process(&self, ctx: &mut RenderCtx) -> Result<Vec<AudioBuf>, ProcessError> {
        let mut out = ctx.input(0).clone();
        out.apply_gain(-1.0);
        Ok(vec![out])
    }
}

// --- Ganancia ------------------------------------------------------------------------------

/// Ganancia en dB (admite variar en el tiempo).
pub struct Gain;
const GAIN: [ParamSpec; 1] = [ParamSpec::float("db", "param.nat.ganancia.db", -60.0, 24.0, 0.0).unit(Unit::Db).decimals(1)];

impl Process for Gain {
    fn id(&self) -> &'static str {
        "nat.ganancia"
    }
    fn family(&self) -> Family {
        Family::Utilidad
    }
    fn params(&self) -> &[ParamSpec] {
        &GAIN
    }
    fn per_channel(&self) -> bool {
        true
    }
    fn expected_len(&self, _: &[&AudioBuf], _: &ParamValues) -> LenRule {
        LenRule::Same
    }
    fn process(&self, ctx: &mut RenderCtx) -> Result<Vec<AudioBuf>, ProcessError> {
        let mut out = ctx.input(0).clone();
        let sr = out.sr as f64;
        let mut curve = ctx.curve("db");
        if curve.is_fixed() {
            out.apply_gain(db_to_lin(curve.at(0.0)) as f32);
        } else {
            let n = out.frames();
            let g: Vec<f32> = (0..n).map(|i| db_to_lin(curve.at(i as f64 / sr)) as f32).collect();
            for c in &mut out.channels {
                for (s, k) in c.iter_mut().zip(&g) {
                    *s *= k;
                }
            }
        }
        Ok(vec![out])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicU32};
    use std::sync::Arc;
    use surshape_engine::render::{run, RenderJob};
    use surshape_engine::{ParamSet, RenderOptions};

    fn go(p: Arc<dyn Process>, x: &AudioBuf, set: &[(&str, f64)]) -> AudioBuf {
        let mut params = ParamSet::default();
        for (k, v) in set {
            params.comun.set(k, *v);
        }
        let job = RenderJob { process: p, inputs: vec![Arc::new(x.clone())], params, seed: 1, options: RenderOptions::default() };
        let r = run(&job, &AtomicU32::new(0), &AtomicBool::new(false)).unwrap();
        assert!(r.report.len_ok);
        r.outputs.into_iter().next().unwrap()
    }

    #[test]
    fn edit_operations() {
        let x = AudioBuf::from_channels(10, vec![(0..100).map(|i| i as f32 / 100.0).collect()]);
        let e = go(Arc::new(Extract), &x, &[("inicio", 2.0), ("fin", 5.0)]);
        assert_eq!(e.frames(), 30);
        assert_eq!(e.channels[0][0], 0.2);
        let q = go(Arc::new(Remove), &x, &[("inicio", 2.0), ("fin", 5.0)]);
        assert_eq!(q.frames(), 70);
        assert_eq!(q.channels[0][20], 0.5);
        let s = go(Arc::new(Silence), &x, &[("inicio", 1.0), ("fin", 2.0)]);
        assert_eq!(s.channels[0][15], 0.0);
        assert_eq!(s.channels[0][25], 0.25);
        let f = go(Arc::new(Fade), &x, &[("entrada", 5.0), ("salida", 0.0), ("curva", 0.0)]);
        assert_eq!(f.channels[0][0], 0.0);
        assert!((f.channels[0][60] - 0.6).abs() < 1e-6);
        let p = go(Arc::new(Polarity), &x, &[]);
        assert_eq!(p.channels[0][50], -0.5);
        let g = go(Arc::new(Gain), &x, &[("db", -6.0206)]);
        assert!((g.channels[0][50] - 0.25).abs() < 1e-3);
    }
}
