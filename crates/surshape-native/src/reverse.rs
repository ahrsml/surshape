//! Invertir: el sample al revés.

use surshape_engine::{AudioBuf, Family, LenRule, ParamSpec, ParamValues, Process, ProcessError, RenderCtx};

pub struct Reverse;

impl Process for Reverse {
    fn id(&self) -> &'static str {
        "nat.reverse"
    }
    fn family(&self) -> Family {
        Family::Tiempo
    }
    fn params(&self) -> &[ParamSpec] {
        &[]
    }
    fn expected_len(&self, _: &[&AudioBuf], _: &ParamValues) -> LenRule {
        LenRule::Same
    }
    fn process(&self, ctx: &mut RenderCtx) -> Result<Vec<AudioBuf>, ProcessError> {
        let mut out = ctx.input(0).clone();
        for c in &mut out.channels {
            ctx.check()?;
            c.reverse();
        }
        ctx.set_progress(1.0);
        Ok(vec![out])
    }
}
