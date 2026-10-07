//! Granular: el resultado se arma con muchos granos cortos leídos de la
//! entrada. Densidad, tamaño, posición, velocidad de lectura, dispersión,
//! transposición fija y al azar, forma de la envolvente y abertura estéreo;
//! todo con seed. Los parámetros continuos admiten breakpoints (leídos sobre
//! el tiempo de la SALIDA).

use noisegek_dsp::rng::XorShift;
use surshape_engine::{
    seed32, AudioBuf, Family, LenRule, ParamSpec, ParamValues, Process, ProcessError, RenderCtx, Unit,
};

pub struct Granular;

const ENV: [&str; 4] = [
    "param.nat.granular.envolvente.hann",
    "param.nat.granular.envolvente.triangulo",
    "param.nat.granular.envolvente.trapecio",
    "param.nat.granular.envolvente.percusiva",
];

const PARAMS: [ParamSpec; 10] = [
    ParamSpec::float("duracion", "param.nat.granular.duracion", 0.5, 600.0, 10.0).unit(Unit::Seconds).log().decimals(2).fixed_only(),
    ParamSpec::float("densidad", "param.nat.granular.densidad", 1.0, 500.0, 40.0).log().decimals(1),
    ParamSpec::float("tamano", "param.nat.granular.tamano", 5.0, 1000.0, 80.0).unit(Unit::Ms).log().decimals(1),
    ParamSpec::float("posicion", "param.nat.granular.posicion", 0.0, 100.0, 0.0).unit(Unit::Percent).decimals(1),
    ParamSpec::float("velocidad", "param.nat.granular.velocidad", 0.0, 4.0, 1.0).unit(Unit::Factor).decimals(2),
    ParamSpec::float("dispersion", "param.nat.granular.dispersion", 0.0, 2000.0, 50.0).unit(Unit::Ms).decimals(0),
    ParamSpec::float("transposicion", "param.nat.granular.transposicion", -24.0, 24.0, 0.0).unit(Unit::Semitones).decimals(1),
    ParamSpec::float("tono_azar", "param.nat.granular.tono_azar", 0.0, 24.0, 0.0).unit(Unit::Semitones).decimals(1),
    ParamSpec::choice("envolvente", "param.nat.granular.envolvente", &ENV, 0),
    ParamSpec::float("estereo", "param.nat.granular.estereo", 0.0, 100.0, 50.0).unit(Unit::Percent).decimals(0),
];

/// Envolvente del grano en x = 0..1.
fn envelope(kind: usize, x: f64) -> f64 {
    match kind {
        1 => 1.0 - (2.0 * x - 1.0).abs(),
        2 => (x / 0.2).min((1.0 - x) / 0.2).min(1.0),
        3 => (1.0 - x).powi(3) * (x / 0.01).min(1.0),
        _ => 0.5 - 0.5 * (2.0 * std::f64::consts::PI * x).cos(),
    }
}

impl Process for Granular {
    fn id(&self) -> &'static str {
        "nat.granular"
    }
    fn family(&self) -> Family {
        Family::Tiempo
    }
    fn params(&self) -> &[ParamSpec] {
        &PARAMS
    }
    fn uses_seed(&self) -> bool {
        true
    }
    fn expected_len(&self, inputs: &[&AudioBuf], p: &ParamValues) -> LenRule {
        LenRule::Frames((p.get("duracion") * inputs[0].sr as f64).round() as usize)
    }
    fn process(&self, ctx: &mut RenderCtx) -> Result<Vec<AudioBuf>, ProcessError> {
        let input = ctx.input(0);
        let sr = input.sr as f64;
        let in_len = input.frames();
        let nch = input.num_channels();
        let out_len = (ctx.p("duracion") * sr).round() as usize;
        let env_kind = ctx.p("envolvente") as usize;
        let mut c = ["densidad", "tamano", "posicion", "velocidad", "dispersion", "transposicion", "tono_azar", "estereo"]
            .map(|id| ctx.curve(id));
        let mut rng = XorShift::new(seed32(ctx.seed));
        let mut out = vec![vec![0.0f32; out_len]; nch];
        // Nivel: con muchos granos superpuestos se suman como ruido (potencia).
        let mut t_out = 0.0f64; // segundos
        let mut scan = 0.0f64; // avance acumulado de lectura, en segundos de entrada
        let mut last_t = 0.0f64;
        let mut count = 0usize;
        while t_out < out_len as f64 / sr {
            count += 1;
            if count % 256 == 0 {
                ctx.check()?;
                ctx.set_progress(t_out * sr / out_len.max(1) as f64);
            }
            let dens = c[0].at(t_out).max(0.1);
            let size = c[1].at(t_out) * 0.001;
            let pos0 = c[2].at(t_out) / 100.0;
            scan += (t_out - last_t) * c[3].at(t_out);
            last_t = t_out;
            let jitter = c[4].at(t_out) * 0.001 * (rng.next01() * 2.0 - 1.0);
            let semis = c[5].at(t_out) + c[6].at(t_out) * (rng.next01() * 2.0 - 1.0);
            let ratio = 2f64.powf(semis / 12.0);
            let spread = c[7].at(t_out) / 100.0;
            let pan = (rng.next01() * 2.0 - 1.0) * spread; // -1..1
            let gain = 1.0 / (dens * size).max(1.0).sqrt();

            let glen = ((size * sr) as usize).max(8);
            let start = (pos0 * in_len as f64 + (scan + jitter) * sr).rem_euclid(in_len.max(1) as f64);
            let o0 = (t_out * sr) as usize;
            for (ch, dst) in out.iter_mut().enumerate() {
                let x = &input.channels[ch];
                // Abertura estéreo: el canal 0 se atenúa con paneo a la derecha y viceversa.
                let pg = if nch == 2 { if ch == 0 { (1.0 - pan).min(1.0) } else { (1.0 + pan).min(1.0) } } else { 1.0 };
                for i in 0..glen {
                    let o = o0 + i;
                    if o >= out_len {
                        break;
                    }
                    let p = start + i as f64 * ratio;
                    let k = p.floor() as usize % in_len.max(1);
                    let f = (p - p.floor()) as f32;
                    let a = x[k];
                    let b = x[(k + 1) % in_len.max(1)];
                    let e = envelope(env_kind, i as f64 / glen as f64);
                    dst[o] += (a + (b - a) * f) * (e * gain * pg) as f32;
                }
            }
            // Próximo grano: intervalo medio 1/densidad, con algo de azar.
            t_out += (0.5 + rng.next01()) / dens;
        }
        ctx.set_progress(1.0);
        Ok(vec![AudioBuf::from_channels(input.sr, out)])
    }
}
