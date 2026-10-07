//! Waveshaper de tabla aleatoria (el "table shaper" de noisegek-dsp): una
//! curva de transferencia generada desde la seed, cruda o suavizada. Todos
//! los parámetros admiten breakpoints.

use crate::util::{map_samples, mix};
use noisegek_dsp::modules::{process, SlotState, Tables};
use noisegek_dsp::Module;
use surshape_audio::db_to_lin;
use surshape_engine::{seed32, AudioBuf, Family, LenRule, ParamSpec, ParamValues, Process, ProcessError, RenderCtx, Unit};

pub struct Waveshaper;

const PARAMS: [ParamSpec; 5] = [
    ParamSpec::float("drive", "param.nat.waveshaper.drive", 0.0, 36.0, 6.0).unit(Unit::Db).decimals(1),
    ParamSpec::float("cantidad", "param.nat.waveshaper.cantidad", 0.0, 100.0, 100.0).unit(Unit::Percent).decimals(0),
    ParamSpec::float("suavizado", "param.nat.waveshaper.suavizado", 0.0, 100.0, 50.0).unit(Unit::Percent).decimals(0),
    ParamSpec::float("mezcla", "param.nat.waveshaper.mezcla", 0.0, 100.0, 100.0).unit(Unit::Percent).decimals(0),
    ParamSpec::float("salida", "param.nat.waveshaper.salida", -24.0, 12.0, 0.0).unit(Unit::Db).decimals(1),
];

impl Process for Waveshaper {
    fn id(&self) -> &'static str {
        "nat.waveshaper"
    }
    fn family(&self) -> Family {
        Family::Destruccion
    }
    fn params(&self) -> &[ParamSpec] {
        &PARAMS
    }
    fn uses_seed(&self) -> bool {
        true
    }
    fn per_channel(&self) -> bool {
        true
    }
    fn expected_len(&self, _: &[&AudioBuf], _: &ParamValues) -> LenRule {
        LenRule::Same
    }
    fn process(&self, ctx: &mut RenderCtx) -> Result<Vec<AudioBuf>, ProcessError> {
        let mut tables = Tables::default();
        tables.ensure(seed32(ctx.seed));
        let curves = ["drive", "cantidad", "suavizado", "mezcla", "salida"].map(|id| ctx.curve(id));
        map_samples(
            ctx,
            |_| (SlotState::new(1), curves.clone()),
            |(st, [drive, a, b, wet, out]), x, t| {
                let y = process(Module::TableShaper, x * db_to_lin(drive.at(t)), a.at(t) / 100.0, b.at(t) / 100.0, st, &tables);
                mix(x, y, wet.at(t) / 100.0) * db_to_lin(out.at(t))
            },
        )
    }
}
