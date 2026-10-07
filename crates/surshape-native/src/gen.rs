//! Generador de ruido: los 5 ruidos de noisegek-dsp (blanco, rosa, marrón,
//! polvo, LFSR) como fuente, con duración y seed. Es un proceso sin
//! entradas: en la grilla empieza una fila propia.

use noisegek_dsp::noise::{NoiseChannel, NoiseCoefs, NoiseSource};
use noisegek_dsp::NoiseType;
use surshape_audio::db_to_lin;
use surshape_engine::{
    seed32, AudioBuf, Family, Inputs, LenRule, ParamSpec, ParamValues, Process, ProcessError, RenderCtx, Unit,
};

pub struct Noise;

const TIPOS: [&str; 5] = [
    "param.nat.noise.tipo.blanco",
    "param.nat.noise.tipo.rosa",
    "param.nat.noise.tipo.marron",
    "param.nat.noise.tipo.polvo",
    "param.nat.noise.tipo.lfsr",
];
const CANALES: [&str; 2] = ["param.nat.noise.canales.mono", "param.nat.noise.canales.estereo"];
const FRECUENCIAS: [&str; 3] = ["param.nat.noise.sr.44100", "param.nat.noise.sr.48000", "param.nat.noise.sr.96000"];
const SR_VALUES: [u32; 3] = [44100, 48000, 96000];

const PARAMS: [ParamSpec; 6] = [
    ParamSpec::choice("tipo", "param.nat.noise.tipo", &TIPOS, 1),
    ParamSpec::float("duracion", "param.nat.noise.duracion", 0.1, 600.0, 10.0).unit(Unit::Seconds).log().decimals(2).fixed_only(),
    ParamSpec::float("caracter", "param.nat.noise.caracter", 0.0, 100.0, 50.0).unit(Unit::Percent).decimals(0),
    ParamSpec::float("nivel", "param.nat.noise.nivel", -48.0, 0.0, -6.0).unit(Unit::Db).decimals(1),
    ParamSpec::choice("canales", "param.nat.noise.canales", &CANALES, 1),
    ParamSpec::choice("sr", "param.nat.noise.sr", &FRECUENCIAS, 1),
];

fn sr_of(p: &ParamValues) -> u32 {
    SR_VALUES[(p.get("sr") as usize).min(SR_VALUES.len() - 1)]
}

impl Process for Noise {
    fn id(&self) -> &'static str {
        "nat.noise"
    }
    fn family(&self) -> Family {
        Family::Generador
    }
    fn params(&self) -> &[ParamSpec] {
        &PARAMS
    }
    fn inputs(&self) -> Inputs {
        Inputs::Fixed(&[])
    }
    fn uses_seed(&self) -> bool {
        true
    }
    fn expected_len(&self, _: &[&AudioBuf], p: &ParamValues) -> LenRule {
        LenRule::Frames((p.get("duracion") * sr_of(p) as f64).round() as usize)
    }
    fn process(&self, ctx: &mut RenderCtx) -> Result<Vec<AudioBuf>, ProcessError> {
        let sr = sr_of(ctx.params);
        let n = (ctx.p("duracion") * sr as f64).round() as usize;
        let tipo = NoiseType::from_index(ctx.p("tipo") as usize + 1);
        let stereo = ctx.p("canales") >= 0.5;
        let s = seed32(ctx.seed);
        let mut src = NoiseSource {
            ch: [NoiseChannel::new(s, (s & 0x7fff) as u16 | 1), NoiseChannel::new(s.rotate_left(13) ^ 0x5bd1_e995, (s >> 17) as u16 | 1)],
        };
        let (mut car, mut lvl) = (ctx.curve("caracter"), ctx.curve("nivel"));
        let mut l = Vec::with_capacity(n);
        let mut r = Vec::with_capacity(if stereo { n } else { 0 });
        let inv_sr = 1.0 / sr as f64;
        for i in 0..n {
            if i % 32768 == 0 {
                ctx.check()?;
                ctx.set_progress(i as f64 / n.max(1) as f64);
            }
            let t = i as f64 * inv_sr;
            let c = (car.at(t) / 100.0).clamp(0.0, 1.0);
            let coefs = NoiseCoefs::new(c, sr as f64);
            let g = db_to_lin(lvl.at(t));
            let (a, b) = src.frame(tipo, c, &coefs);
            l.push((a * g) as f32);
            if stereo {
                r.push((b * g) as f32);
            }
        }
        let chans = if stereo { vec![l, r] } else { vec![l] };
        Ok(vec![AudioBuf::from_channels(sr, chans)])
    }
}
