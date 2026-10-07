//! Bitcrush + reducción de muestreo, con los módulos de noisegek-dsp
//! (`Module::Downsample` seguido de `Module::Bitcrush`). Resolución y mezcla
//! admiten breakpoints.

use crate::util::{map_samples, mix};
use noisegek_dsp::modules::{process, SlotState, Tables};
use noisegek_dsp::Module;
use surshape_engine::{AudioBuf, Family, LenRule, ParamSpec, ParamValues, Process, ProcessError, RenderCtx, Unit};

pub struct Bitcrush;

const PARAMS: [ParamSpec; 3] = [
    ParamSpec::float("bits", "param.nat.bitcrush.bits", 1.0, 16.0, 6.0).unit(Unit::Bits).decimals(1),
    ParamSpec::int("reduccion", "param.nat.bitcrush.reduccion", 1.0, 64.0, 1.0).unit(Unit::Factor),
    ParamSpec::float("mezcla", "param.nat.bitcrush.mezcla", 0.0, 100.0, 100.0).unit(Unit::Percent).decimals(0),
];

impl Process for Bitcrush {
    fn id(&self) -> &'static str {
        "nat.bitcrush"
    }
    fn family(&self) -> Family {
        Family::Destruccion
    }
    fn params(&self) -> &[ParamSpec] {
        &PARAMS
    }
    fn per_channel(&self) -> bool {
        true
    }
    fn expected_len(&self, _: &[&AudioBuf], _: &ParamValues) -> LenRule {
        LenRule::Same
    }
    fn process(&self, ctx: &mut RenderCtx) -> Result<Vec<AudioBuf>, ProcessError> {
        // Parámetros "mapeados" en el sentido de noisegek-dsp:
        //   Bitcrush   a = paso de cuantización = 1 / 2^(bits-1)
        //   Downsample a = N (mantiene 1 de cada N muestras)
        let n = ctx.p("reduccion");
        let tables = Tables::default(); // estos módulos no usan tablas
        let (bits, wet) = (ctx.curve("bits"), ctx.curve("mezcla"));
        map_samples(
            ctx,
            |_| (SlotState::new(1), bits.clone(), wet.clone()),
            |(st, bits, wet), x, t| {
                let step = 1.0 / 2f64.powf(bits.at(t) - 1.0);
                let held = process(Module::Downsample, x, n, 0.0, st, &tables);
                let y = process(Module::Bitcrush, held, step, 0.0, st, &tables);
                mix(x, y, wet.at(t) / 100.0)
            },
        )
    }
}
