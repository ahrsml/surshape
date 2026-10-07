//! Normalizar: lleva el pico (de todos los canales) a un nivel dado.

use surshape_audio::{db_to_lin, Analysis};
use surshape_engine::{AudioBuf, Family, LenRule, ParamSpec, ParamValues, Process, ProcessError, RenderCtx, Unit};

pub struct Normalize;

const PARAMS: [ParamSpec; 1] =
    [ParamSpec::float("pico", "param.nat.normalize.pico", -30.0, 0.0, -1.0).unit(Unit::Db).decimals(1).fixed_only()];

impl Process for Normalize {
    fn id(&self) -> &'static str {
        "nat.normalize"
    }
    fn family(&self) -> Family {
        Family::Utilidad
    }
    fn params(&self) -> &[ParamSpec] {
        &PARAMS
    }
    fn expected_len(&self, _: &[&AudioBuf], _: &ParamValues) -> LenRule {
        LenRule::Same
    }
    fn process(&self, ctx: &mut RenderCtx) -> Result<Vec<AudioBuf>, ProcessError> {
        let mut out = ctx.input(0).clone();
        let peak = Analysis::of(&out).peak;
        ctx.check()?;
        // Silencio: no hay nada que normalizar.
        if peak > 0.0 {
            out.apply_gain((db_to_lin(ctx.p("pico")) / peak as f64) as f32);
        }
        ctx.set_progress(1.0);
        Ok(vec![out])
    }
}
