//! Catálogo de procesos CDP.
//!
//! Criterio: resultados deterministas (o con seed propia de CDP), para que la
//! cache y la reproducibilidad valgan también aquí. Quedan fuera los
//! procesos aleatorios sin seed y los que piden datos por teclado.
//!
//! Como en Soundshaper, los tipos de archivo de CDP quedan a la vista: cada
//! proceso declara qué tipo lee y produce ([`FileKind`]). Los espectrales
//! leen y escriben análisis (.ana); "PVOC análisis" pasa de sonido a .ana y
//! "PVOC resíntesis" vuelve a sonido. Si una celda recibe un tipo distinto
//! del que pide, el runner convierte solo (auto-conversión) y lo muestra.
//!
//! CDP es mono: cada canal se procesa por separado y los datos se guardan un
//! archivo por canal.

use crate::brk::{arg_for, fmt_num};
use crate::run::run;
use crate::CdpInstall;
use std::path::Path;
use std::sync::Arc;
use surshape_audio::export::{write_wav, WavFormat};
use surshape_audio::{decode, AudioBuf};
use surshape_engine::{
    console, CmdCtx, DataSet, Engine, Family, FileCtx, FileKind, InputSpec, Inputs, LenRule, ParamKind, ParamSpec,
    ParamValue, ParamValues, Process, ProcessError, Registry, RenderCtx, Unit, MODE_PARAM,
};

const ONE: Inputs = Inputs::Fixed(&[InputSpec::PRINCIPAL]);
const TWO: Inputs = Inputs::Fixed(&[InputSpec::PRINCIPAL, InputSpec::SEGUNDA]);

const WAV: &[FileKind] = &[FileKind::Wav];
const ANA: &[FileKind] = &[FileKind::Ana];
const ANA2: &[FileKind] = &[FileKind::Ana, FileKind::Ana];

#[derive(Clone, Copy, Debug)]
pub enum Len {
    /// Mismo largo que la entrada (la salida se ajusta exacto).
    Same,
    /// Segundos dados por un parámetro (la salida se ajusta exacto).
    SecondsParam(&'static str),
    /// La entrada multiplicada por un parámetro (la salida se ajusta exacto).
    FactorParam(&'static str),
    /// Lo decide CDP.
    Unknown,
}

/// Lo que necesita un constructor de argumentos.
pub struct ArgCtx<'a> {
    pub params: &'a ParamValues,
    pub specs: &'static [ParamSpec],
    pub dur: f64,
    pub dir: &'a Path,
    pub ch: usize,
    pub seed: u64,
    pub inp: &'a str,
    /// Segunda entrada (procesos de dos entradas).
    pub inp2: Option<&'a str>,
    /// Duración de la segunda entrada (0 si no hay).
    pub dur2: f64,
    pub out: &'a str,
    /// Solo para mostrar la línea de comando: no escribe archivos .brk.
    pub dry: bool,
}

impl ArgCtx<'_> {
    /// Valor del parámetro como argumento (número o archivo .brk).
    pub fn p(&self, id: &str) -> Result<String, ProcessError> {
        let spec = self.specs.iter().find(|s| s.id == id).expect("parámetro definido");
        let v = self.params.value(id).cloned().unwrap_or(ParamValue::Fixed(spec.default));
        if self.dry {
            if let ParamValue::Envelope(bp) = &v {
                if spec.automatable && bp.puntos.len() > 1 {
                    return Ok(format!("{id}.brk"));
                }
            }
            return Ok(fmt_num(spec.clamp(v.initial()), spec.kind == ParamKind::Int));
        }
        arg_for(spec, &v, self.dur, self.dir, &format!("{id}_c{}", self.ch))
    }
    pub fn f(&self, id: &str) -> f64 {
        self.params.get(id)
    }
    /// Modo elegido, como lo cuenta CDP (1, 2, ...).
    pub fn mode(&self) -> String {
        format!("{}", self.params.get(MODE_PARAM).round() as i64 + 1)
    }
    fn io(&self) -> [String; 2] {
        [self.inp.to_string(), self.out.to_string()]
    }
    fn io2(&self) -> [String; 3] {
        [self.inp.to_string(), self.inp2.unwrap_or_default().to_string(), self.out.to_string()]
    }
}

type Build = fn(&ArgCtx) -> Result<Vec<String>, ProcessError>;

pub struct Def {
    pub id: &'static str,
    pub program: &'static str,
    pub family: Family,
    pub inputs: Inputs,
    /// Tipo de archivo de cada entrada.
    pub ins: &'static [FileKind],
    /// Tipo de archivo de la salida.
    pub out: FileKind,
    pub params: &'static [ParamSpec],
    pub len: Len,
    pub seed: bool,
    pub build: Build,
}

impl Def {
    /// ¿Trabaja con archivos de datos (alguna entrada o la salida no es .wav)?
    pub fn on_files(&self) -> bool {
        self.out != FileKind::Wav || self.ins.iter().any(|k| *k != FileKind::Wav)
    }
}

/// Prepara la entrada de los procesos de ciclos (distort): CDP busca cruces
/// por cero y falla ("cycle_search exceeds buffer size") si un tramo largo
/// no tiene ninguno, como el silencio exacto o un corrimiento de continua.
/// Se quita la continua (paso alto de 1 polo a ~5 Hz) y se suma una señal
/// inaudible (-110 dB, alternando cada 2 muestras) que garantiza cruces.
fn prep_cycles(x: &[f32], sr: u32) -> Vec<f32> {
    let r = 1.0 - (2.0 * std::f64::consts::PI * 5.0 / sr as f64);
    let (mut px, mut py) = (0.0f64, 0.0f64);
    const TINY: f64 = 3.2e-6;
    x.iter()
        .enumerate()
        .map(|(i, &s)| {
            let s = s as f64;
            py = s - px + r * py;
            px = s;
            (py + if (i / 2) % 2 == 0 { TINY } else { -TINY }) as f32
        })
        .collect()
}

fn v(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|s| s.to_string()).collect()
}

// --- Parámetros ----------------------------------------------------------------------

const PVOC_MODES: &[&str] = &["param.cdp.pvoc_anal.modo.1", "param.cdp.pvoc_anal.modo.2"];
const ANAL: [ParamSpec; 3] = [
    ParamSpec::choice(MODE_PARAM, "param.cdp.pvoc_anal.modo", PVOC_MODES, 0),
    ParamSpec::int("puntos", "param.cdp.pvoc_anal.puntos", 64.0, 8192.0, 1024.0).log().cdp("points"),
    ParamSpec::int("superposicion", "param.cdp.pvoc_anal.superposicion", 1.0, 4.0, 3.0).cdp("overlap"),
];
const BLUR: [ParamSpec; 1] =
    [ParamSpec::int("ventanas", "param.cdp.blur_blur.ventanas", 1.0, 200.0, 12.0).automatable().cdp("blurring")];
const ACCU: [ParamSpec; 2] = [
    ParamSpec::float("decaimiento", "param.cdp.focus_accu.decaimiento", 0.001, 1.0, 0.3).log().decimals(3).fixed_only().cdp("decay"),
    ParamSpec::float("glis", "param.cdp.focus_accu.glis", -4.0, 4.0, 0.0).unit(Unit::Factor).decimals(2).fixed_only().cdp("gliss"),
];
const EXAG: [ParamSpec; 1] =
    [ParamSpec::float("exag", "param.cdp.focus_exag.exag", 0.01, 100.0, 2.0).log().decimals(2).cdp("exaggeration")];
const TRACE: [ParamSpec; 1] = [ParamSpec::int("n", "param.cdp.hilite_trace.n", 1.0, 256.0, 8.0).automatable().cdp("N")];
const STRETCH: [ParamSpec; 1] = [ParamSpec::float("factor", "param.cdp.stretch_time.factor", 0.25, 100.0, 4.0)
    .unit(Unit::Factor)
    .log()
    .decimals(2)
    .cdp("timestretch")];
const REPEAT: [ParamSpec; 2] = [
    ParamSpec::int("veces", "param.cdp.distort_repeat.veces", 2.0, 32.0, 3.0).unit(Unit::Factor).automatable().cdp("multiplier"),
    ParamSpec::int("ciclos", "param.cdp.distort_repeat.ciclos", 1.0, 64.0, 1.0).automatable().cdp("cyclecnt"),
];
const FRACTAL: [ParamSpec; 2] = [
    ParamSpec::int("escala", "param.cdp.distort_fractal.escala", 2.0, 64.0, 4.0).automatable().cdp("scaling"),
    ParamSpec::float("nivel", "param.cdp.distort_fractal.nivel", 0.01, 1.0, 0.5).decimals(2).cdp("loudness"),
];
const ZIGZAG: [ParamSpec; 4] = [
    ParamSpec::float("inicio", "param.cdp.extend_zigzag.inicio", 0.0, 100.0, 10.0).unit(Unit::Percent).decimals(1).fixed_only().cdp("start"),
    ParamSpec::float("fin", "param.cdp.extend_zigzag.fin", 0.0, 100.0, 90.0).unit(Unit::Percent).decimals(1).fixed_only().cdp("end"),
    ParamSpec::float("largo", "param.cdp.extend_zigzag.largo", 1.0, 20.0, 2.0).unit(Unit::Factor).log().decimals(2).fixed_only().cdp("dur"),
    ParamSpec::float("minzig", "param.cdp.extend_zigzag.minzig", 0.05, 5.0, 0.3).unit(Unit::Seconds).log().decimals(3).fixed_only().cdp("minzig"),
];
const CROSS: [ParamSpec; 1] =
    [ParamSpec::float("cantidad", "param.cdp.combine_cross.cantidad", 0.0, 1.0, 1.0).decimals(2).cdp("interp")];
const MORPH: [ParamSpec; 6] = [
    ParamSpec::float("amp_inicio", "param.cdp.morph_morph.amp_inicio", 0.0, 100.0, 10.0).unit(Unit::Percent).decimals(1).fixed_only().cdp("as"),
    ParamSpec::float("amp_fin", "param.cdp.morph_morph.amp_fin", 0.0, 100.0, 90.0).unit(Unit::Percent).decimals(1).fixed_only().cdp("ae"),
    ParamSpec::float("frec_inicio", "param.cdp.morph_morph.frec_inicio", 0.0, 100.0, 10.0).unit(Unit::Percent).decimals(1).fixed_only().cdp("fs"),
    ParamSpec::float("frec_fin", "param.cdp.morph_morph.frec_fin", 0.0, 100.0, 90.0).unit(Unit::Percent).decimals(1).fixed_only().cdp("fe"),
    ParamSpec::float("exp_amp", "param.cdp.morph_morph.exp_amp", 0.1, 10.0, 1.0).log().decimals(2).fixed_only().cdp("expa"),
    ParamSpec::float("exp_frec", "param.cdp.morph_morph.exp_frec", 0.1, 10.0, 1.0).log().decimals(2).fixed_only().cdp("expf"),
];

// --- Ampliación del catálogo (fase 6) ---

const AVRG: [ParamSpec; 1] = [ParamSpec::int("n", "param.cdp.blur_avrg.n", 3.0, 255.0, 9.0).cdp("N")];
const SUPPRESS: [ParamSpec; 1] = [ParamSpec::int("n", "param.cdp.blur_suppress.n", 1.0, 256.0, 8.0).automatable().cdp("N")];
const SHIFT_MODES: &[&str] = &[
    "param.cdp.strange_shift.modo.1",
    "param.cdp.strange_shift.modo.2",
    "param.cdp.strange_shift.modo.3",
    "param.cdp.strange_shift.modo.4",
    "param.cdp.strange_shift.modo.5",
];
const SHIFT: [ParamSpec; 5] = [
    ParamSpec::choice(MODE_PARAM, "param.cdp.strange_shift.modo", SHIFT_MODES, 0),
    ParamSpec::float("desplazamiento", "param.cdp.strange_shift.desplazamiento", -5000.0, 5000.0, 150.0).unit(Unit::Hz).decimals(1).cdp("frqshift"),
    ParamSpec::float("division", "param.cdp.strange_shift.division", 20.0, 20000.0, 1000.0).unit(Unit::Hz).log().decimals(0).cdp("frq_divide").modes(0b00110),
    ParamSpec::float("desde", "param.cdp.strange_shift.desde", 20.0, 20000.0, 300.0).unit(Unit::Hz).log().decimals(0).cdp("frqlo").modes(0b11000),
    ParamSpec::float("hasta", "param.cdp.strange_shift.hasta", 20.0, 20000.0, 3000.0).unit(Unit::Hz).log().decimals(0).cdp("frqhi").modes(0b11000),
];
const SSTRETCH_MODES: &[&str] = &["param.cdp.stretch_spectrum.modo.1", "param.cdp.stretch_spectrum.modo.2"];
const SSTRETCH: [ParamSpec; 5] = [
    ParamSpec::choice(MODE_PARAM, "param.cdp.stretch_spectrum.modo", SSTRETCH_MODES, 0),
    ParamSpec::float("division", "param.cdp.stretch_spectrum.division", 20.0, 5000.0, 500.0).unit(Unit::Hz).log().decimals(0).fixed_only().cdp("frq_divide"),
    ParamSpec::float("maximo", "param.cdp.stretch_spectrum.maximo", 0.25, 4.0, 1.5).unit(Unit::Factor).log().decimals(2).fixed_only().cdp("maxstretch"),
    ParamSpec::float("exponente", "param.cdp.stretch_spectrum.exponente", 0.02, 50.0, 1.0).log().decimals(2).fixed_only().cdp("exponent"),
    ParamSpec::float("profundidad", "param.cdp.stretch_spectrum.profundidad", 0.01, 1.0, 1.0).decimals(2).cdp("depth"),
];
const FOLD: [ParamSpec; 2] = [
    ParamSpec::float("desde", "param.cdp.focus_fold.desde", 20.0, 20000.0, 200.0).unit(Unit::Hz).log().decimals(0).cdp("lofrq"),
    ParamSpec::float("hasta", "param.cdp.focus_fold.hasta", 20.0, 20000.0, 800.0).unit(Unit::Hz).log().decimals(0).cdp("hifrq"),
];
const HFILTER_MODES: &[&str] = &[
    "param.cdp.hilite_filter.modo.1",
    "param.cdp.hilite_filter.modo.2",
    "param.cdp.hilite_filter.modo.3",
    "param.cdp.hilite_filter.modo.4",
    "param.cdp.hilite_filter.modo.5",
    "param.cdp.hilite_filter.modo.6",
    "param.cdp.hilite_filter.modo.7",
    "param.cdp.hilite_filter.modo.8",
    "param.cdp.hilite_filter.modo.9",
    "param.cdp.hilite_filter.modo.10",
    "param.cdp.hilite_filter.modo.11",
    "param.cdp.hilite_filter.modo.12",
];
const HFILTER: [ParamSpec; 5] = [
    ParamSpec::choice(MODE_PARAM, "param.cdp.hilite_filter.modo", HFILTER_MODES, 6),
    ParamSpec::float("frec", "param.cdp.hilite_filter.frec", 20.0, 20000.0, 500.0).unit(Unit::Hz).log().decimals(0).cdp("frq1"),
    ParamSpec::float("frec2", "param.cdp.hilite_filter.frec2", 20.0, 20000.0, 2000.0).unit(Unit::Hz).log().decimals(0).cdp("frq2").modes(0xFC0),
    ParamSpec::float("q", "param.cdp.hilite_filter.q", 1.0, 500.0, 10.0).log().decimals(1).cdp("Q"),
    ParamSpec::float("ganancia", "param.cdp.hilite_filter.ganancia", 0.01, 100.0, 2.0).unit(Unit::Factor).log().decimals(2).cdp("gain").modes(0xC30),
];
const SGAIN: [ParamSpec; 1] = [ParamSpec::float("ganancia", "param.cdp.spec_gain.ganancia", 0.0, 100.0, 0.5).unit(Unit::Factor).decimals(2).cdp("gain")];
const AVERAGE: [ParamSpec; 1] = [ParamSpec::int("ciclos", "param.cdp.distort_average.ciclos", 2.0, 128.0, 4.0).automatable().cdp("cyclecnt")];
const MULTIPLY: [ParamSpec; 1] = [ParamSpec::int("n", "param.cdp.distort_multiply.n", 2.0, 16.0, 2.0).unit(Unit::Factor).cdp("N")];
const SPEED: [ParamSpec; 1] =
    [ParamSpec::float("semitonos", "param.cdp.modify_speed.semitonos", -48.0, 48.0, -12.0).unit(Unit::Semitones).decimals(2).fixed_only().cdp("semitone-transpos")];
const LOHI: [ParamSpec; 3] = [
    ParamSpec::float("atenuacion", "param.cdp.filter_lohi.atenuacion", -96.0, -6.0, -60.0).unit(Unit::Db).decimals(0).fixed_only().cdp("attenuation"),
    ParamSpec::float("paso", "param.cdp.filter_lohi.paso", 20.0, 20000.0, 1000.0).unit(Unit::Hz).log().decimals(0).fixed_only().cdp("pass-band"),
    ParamSpec::float("corte", "param.cdp.filter_lohi.corte", 20.0, 20000.0, 1300.0).unit(Unit::Hz).log().decimals(0).fixed_only().cdp("stop-band"),
];

/// Argumentos de `pvoc anal` (también los usa la auto-conversión).
pub fn anal_args(mode: usize, inp: &str, out: &str, points: u32, overlap: u32) -> Vec<String> {
    vec!["anal".into(), format!("{}", mode + 1), inp.into(), out.into(), format!("-c{points}"), format!("-o{overlap}")]
}

/// Argumentos de `pvoc synth`.
pub fn synth_args(inp: &str, out: &str) -> Vec<String> {
    v(&["synth", inp, out])
}

/// Los procesos del catálogo.
pub static DEFS: [Def; 23] = [
    Def {
        id: "cdp.pvoc_anal",
        program: "pvoc",
        family: Family::Espectral,
        inputs: ONE,
        ins: WAV,
        out: FileKind::Ana,
        params: &ANAL,
        len: Len::Unknown,
        seed: false,
        build: |c| {
            let pts = c.f("puntos").round().max(2.0) as u32;
            // CDP pide un número par de puntos.
            let pts = pts + pts % 2;
            Ok(anal_args(c.f(MODE_PARAM).round() as usize, c.inp, c.out, pts, c.f("superposicion").round() as u32))
        },
    },
    Def {
        id: "cdp.pvoc_synth",
        program: "pvoc",
        family: Family::Espectral,
        inputs: ONE,
        ins: ANA,
        out: FileKind::Wav,
        params: &[],
        len: Len::Unknown,
        seed: false,
        build: |c| Ok(synth_args(c.inp, c.out)),
    },
    Def {
        id: "cdp.blur_blur",
        program: "blur",
        family: Family::Espectral,
        inputs: ONE,
        ins: ANA,
        out: FileKind::Ana,
        params: &BLUR,
        len: Len::Unknown,
        seed: false,
        build: |c| {
            let [i, o] = c.io();
            Ok(vec!["blur".into(), i, o, c.p("ventanas")?])
        },
    },
    Def {
        id: "cdp.focus_accu",
        program: "focus",
        family: Family::Espectral,
        inputs: ONE,
        ins: ANA,
        out: FileKind::Ana,
        params: &ACCU,
        len: Len::Unknown,
        seed: false,
        build: |c| {
            let [i, o] = c.io();
            Ok(vec!["accu".into(), i, o, format!("-d{}", c.p("decaimiento")?), format!("-g{}", c.p("glis")?)])
        },
    },
    Def {
        id: "cdp.focus_exag",
        program: "focus",
        family: Family::Espectral,
        inputs: ONE,
        ins: ANA,
        out: FileKind::Ana,
        params: &EXAG,
        len: Len::Unknown,
        seed: false,
        build: |c| {
            let [i, o] = c.io();
            Ok(vec!["exag".into(), i, o, c.p("exag")?])
        },
    },
    Def {
        id: "cdp.hilite_trace",
        program: "hilite",
        family: Family::Espectral,
        inputs: ONE,
        ins: ANA,
        out: FileKind::Ana,
        params: &TRACE,
        len: Len::Unknown,
        seed: false,
        build: |c| {
            let [i, o] = c.io();
            Ok(vec!["trace".into(), "1".into(), i, o, c.p("n")?])
        },
    },
    Def {
        id: "cdp.stretch_time",
        program: "stretch",
        family: Family::Tiempo,
        inputs: ONE,
        ins: ANA,
        out: FileKind::Ana,
        params: &STRETCH,
        len: Len::Unknown,
        seed: false,
        build: |c| {
            let [i, o] = c.io();
            Ok(vec!["time".into(), "1".into(), i, o, c.p("factor")?])
        },
    },
    Def {
        id: "cdp.distort_repeat",
        program: "distort",
        family: Family::Tiempo,
        inputs: ONE,
        ins: WAV,
        out: FileKind::Wav,
        params: &REPEAT,
        len: Len::Unknown,
        seed: false,
        build: |c| {
            let [i, o] = c.io();
            Ok(vec!["repeat".into(), i, o, c.p("veces")?, format!("-c{}", c.p("ciclos")?)])
        },
    },
    Def {
        id: "cdp.distort_fractal",
        program: "distort",
        family: Family::Destruccion,
        inputs: ONE,
        ins: WAV,
        out: FileKind::Wav,
        params: &FRACTAL,
        len: Len::Same,
        seed: false,
        build: |c| {
            let [i, o] = c.io();
            Ok(vec!["fractal".into(), i, o, c.p("escala")?, c.p("nivel")?])
        },
    },
    Def {
        id: "cdp.extend_zigzag",
        program: "extend",
        family: Family::Tiempo,
        inputs: ONE,
        ins: WAV,
        out: FileKind::Wav,
        params: &ZIGZAG,
        len: Len::FactorParam("largo"),
        seed: true,
        build: |c| {
            let [i, o] = c.io();
            let (a, b) = (c.f("inicio"), c.f("fin"));
            let (a, b) = (a.min(b) / 100.0 * c.dur, a.max(b) / 100.0 * c.dur);
            let num = |x: f64| fmt_num(x, false);
            // CDP exige que el resultado no sea más corto que la entrada y
            // que el zigzag mínimo quepa en el tramo.
            let largo = c.f("largo").max(1.0) * c.dur;
            let minzig = c.f("minzig").min(((b - a) * 0.5).max(0.001));
            // CDP: seed > 0; la misma seed da la misma secuencia.
            let seed = (c.seed % 2_147_483_646) + 1;
            let mut a_v = v(&["zigzag", "1"]);
            a_v.extend([i, o, num(a), num(b), num(largo), num(minzig), format!("-r{seed}")]);
            Ok(a_v)
        },
    },
    Def {
        id: "cdp.combine_cross",
        program: "combine",
        family: Family::Combinacion,
        inputs: TWO,
        ins: ANA2,
        out: FileKind::Ana,
        params: &CROSS,
        len: Len::Unknown,
        seed: false,
        build: |c| {
            let [a, b, o] = c.io2();
            Ok(vec!["cross".into(), a, b, o, format!("-i{}", c.p("cantidad")?)])
        },
    },
    Def {
        id: "cdp.morph_morph",
        program: "morph",
        family: Family::Combinacion,
        inputs: TWO,
        ins: ANA2,
        out: FileKind::Ana,
        params: &MORPH,
        len: Len::Unknown,
        seed: false,
        build: |c| {
            let [a, b, o] = c.io2();
            let num = |x: f64| fmt_num(x, false);
            // Los tiempos tienen que caer dentro de los DOS archivos.
            let d = c.dur.min(c.dur2);
            let span = |i: &str, f: &str| {
                let (x, y) = (c.f(i), c.f(f));
                let lo = x.min(y) / 100.0 * d;
                (lo, (x.max(y) / 100.0 * d).max(lo + 0.01).min(d))
            };
            let (as_, ae) = span("amp_inicio", "amp_fin");
            let (fs, fe) = span("frec_inicio", "frec_fin");
            Ok(vec![
                "morph".into(),
                "1".into(),
                a,
                b,
                o,
                num(as_),
                num(ae),
                num(fs),
                num(fe),
                c.p("exp_amp")?,
                c.p("exp_frec")?,
            ])
        },
    },
    Def {
        id: "cdp.blur_avrg",
        program: "blur",
        family: Family::Espectral,
        inputs: ONE,
        ins: ANA,
        out: FileKind::Ana,
        params: &AVRG,
        len: Len::Unknown,
        seed: false,
        build: |c| {
            let [i, o] = c.io();
            // CDP pide un número impar.
            let n = (c.f("n").round() as i64) | 1;
            Ok(vec!["avrg".into(), i, o, n.to_string()])
        },
    },
    Def {
        id: "cdp.blur_suppress",
        program: "blur",
        family: Family::Espectral,
        inputs: ONE,
        ins: ANA,
        out: FileKind::Ana,
        params: &SUPPRESS,
        len: Len::Unknown,
        seed: false,
        build: |c| {
            let [i, o] = c.io();
            Ok(vec!["suppress".into(), i, o, c.p("n")?])
        },
    },
    Def {
        id: "cdp.strange_shift",
        program: "strange",
        family: Family::Espectral,
        inputs: ONE,
        ins: ANA,
        out: FileKind::Ana,
        params: &SHIFT,
        len: Len::Unknown,
        seed: false,
        build: |c| {
            let [i, o] = c.io();
            let m = c.f(MODE_PARAM).round() as usize;
            let mut a = vec!["shift".into(), c.mode(), i, o, c.p("desplazamiento")?];
            match m {
                1 | 2 => a.push(c.p("division")?),
                3 | 4 => {
                    let (lo, hi) = (c.f("desde").min(c.f("hasta")), c.f("desde").max(c.f("hasta")) + 1.0);
                    a.push(fmt_num(lo, false));
                    a.push(fmt_num(hi, false));
                }
                _ => {}
            }
            Ok(a)
        },
    },
    Def {
        id: "cdp.stretch_spectrum",
        program: "stretch",
        family: Family::Espectral,
        inputs: ONE,
        ins: ANA,
        out: FileKind::Ana,
        params: &SSTRETCH,
        len: Len::Unknown,
        seed: false,
        build: |c| {
            let [i, o] = c.io();
            Ok(vec![
                "spectrum".into(),
                c.mode(),
                i,
                o,
                c.p("division")?,
                c.p("maximo")?,
                c.p("exponente")?,
                format!("-d{}", c.p("profundidad")?),
            ])
        },
    },
    Def {
        id: "cdp.focus_fold",
        program: "focus",
        family: Family::Espectral,
        inputs: ONE,
        ins: ANA,
        out: FileKind::Ana,
        params: &FOLD,
        len: Len::Unknown,
        seed: false,
        build: |c| {
            let [i, o] = c.io();
            let (lo, hi) = (c.f("desde").min(c.f("hasta")), c.f("desde").max(c.f("hasta")));
            // CDP pide al menos una octava.
            let hi = hi.max(lo * 2.0);
            Ok(vec!["fold".into(), i, o, fmt_num(lo, false), fmt_num(hi, false)])
        },
    },
    Def {
        id: "cdp.hilite_filter",
        program: "hilite",
        family: Family::FiltroEspacio,
        inputs: ONE,
        ins: ANA,
        out: FileKind::Ana,
        params: &HFILTER,
        len: Len::Unknown,
        seed: false,
        build: |c| {
            let [i, o] = c.io();
            let m = c.f(MODE_PARAM).round() as usize + 1;
            let mut a = vec!["filter".into(), c.mode(), i, o, c.p("frec")?];
            if m >= 7 {
                a.push(c.p("frec2")?);
            }
            a.push(c.p("q")?);
            if matches!(m, 5 | 6 | 11 | 12) {
                a.push(c.p("ganancia")?);
            }
            Ok(a)
        },
    },
    Def {
        id: "cdp.spec_gain",
        program: "spec",
        family: Family::Espectral,
        inputs: ONE,
        ins: ANA,
        out: FileKind::Ana,
        params: &SGAIN,
        len: Len::Unknown,
        seed: false,
        build: |c| {
            let [i, o] = c.io();
            Ok(vec!["gain".into(), i, o, c.p("ganancia")?])
        },
    },
    Def {
        id: "cdp.distort_average",
        program: "distort",
        family: Family::Destruccion,
        inputs: ONE,
        ins: WAV,
        out: FileKind::Wav,
        params: &AVERAGE,
        len: Len::Unknown,
        seed: false,
        build: |c| {
            let [i, o] = c.io();
            Ok(vec!["average".into(), i, o, c.p("ciclos")?])
        },
    },
    Def {
        id: "cdp.distort_multiply",
        program: "distort",
        family: Family::Destruccion,
        inputs: ONE,
        ins: WAV,
        out: FileKind::Wav,
        params: &MULTIPLY,
        len: Len::Unknown,
        seed: false,
        build: |c| {
            let [i, o] = c.io();
            Ok(vec!["multiply".into(), i, o, c.p("n")?, "-s".into()])
        },
    },
    Def {
        id: "cdp.modify_speed",
        program: "modify",
        family: Family::Tono,
        inputs: ONE,
        ins: WAV,
        out: FileKind::Wav,
        params: &SPEED,
        len: Len::Unknown,
        seed: false,
        build: |c| {
            let [i, o] = c.io();
            Ok(vec!["speed".into(), "2".into(), i, o, c.p("semitonos")?])
        },
    },
    Def {
        id: "cdp.filter_lohi",
        program: "filter",
        family: Family::FiltroEspacio,
        inputs: ONE,
        ins: WAV,
        out: FileKind::Wav,
        params: &LOHI,
        len: Len::Unknown,
        seed: false,
        build: |c| {
            let [i, o] = c.io();
            // Si las bandas coinciden, el corte se separa un poco.
            let (pass, stop) = (c.f("paso"), c.f("corte"));
            let stop = if (stop - pass).abs() < 1.0 { pass * 1.2 } else { stop };
            Ok(vec!["lohi".into(), "1".into(), i, o, c.p("atenuacion")?, fmt_num(pass, false), fmt_num(stop, false)])
        },
    },
];

/// Un proceso CDP del catálogo.
pub struct CdpProcess {
    pub def: &'static Def,
    pub install: Arc<CdpInstall>,
}

impl CdpProcess {
    fn exe(&self, program: &str) -> Result<std::path::PathBuf, ProcessError> {
        if self.install.has(program) {
            Ok(self.install.path_of(program))
        } else {
            Err(ProcessError::new("err.cdp.no_encontrado").arg("programa", program))
        }
    }
}

/// Enlace duro (sin copiar datos) o, si no se puede, copia.
fn link_or_copy(from: &Path, to: &Path) -> Result<(), ProcessError> {
    let _ = std::fs::remove_file(to);
    if std::fs::hard_link(from, to).is_ok() {
        return Ok(());
    }
    std::fs::copy(from, to).map(|_| ()).map_err(|e| ProcessError::new("err.render.temporal").arg("detalle", e))
}

impl Process for CdpProcess {
    fn id(&self) -> &'static str {
        self.def.id
    }
    fn family(&self) -> Family {
        self.def.family
    }
    fn engine(&self) -> Engine {
        Engine::Cdp
    }
    fn engine_version(&self) -> Option<String> {
        self.install.version.clone()
    }
    fn params(&self) -> &[ParamSpec] {
        self.def.params
    }
    fn inputs(&self) -> Inputs {
        self.def.inputs
    }
    fn per_channel(&self) -> bool {
        true
    }
    fn uses_seed(&self) -> bool {
        self.def.seed
    }
    fn input_kinds(&self) -> &[FileKind] {
        self.def.ins
    }
    fn output_kind(&self) -> FileKind {
        self.def.out
    }
    fn expected_len(&self, inputs: &[&AudioBuf], p: &ParamValues) -> LenRule {
        match self.def.len {
            Len::Same => LenRule::Same,
            Len::SecondsParam(id) => LenRule::Frames((p.get(id) * inputs[0].sr as f64).round() as usize),
            Len::FactorParam(id) => LenRule::Frames((p.get(id).max(1.0) * inputs[0].frames() as f64).round() as usize),
            Len::Unknown => LenRule::Unknown,
        }
    }

    fn command(&self, c: &CmdCtx) -> Vec<String> {
        let none = String::new();
        let ac = ArgCtx {
            params: c.params,
            specs: self.def.params,
            dur: c.dur,
            dir: Path::new(""),
            ch: 0,
            seed: c.seed,
            inp: c.inputs.first().unwrap_or(&none),
            inp2: c.inputs.get(1).map(String::as_str),
            dur2: c.dur2,
            out: c.output,
            dry: true,
        };
        match (self.def.build)(&ac) {
            Ok(args) => vec![console::join_args(self.def.program, &args)],
            Err(_) => vec![self.def.program.to_string()],
        }
    }

    /// Procesos de sonido a sonido (distort, extend...): el audio se escribe
    /// por canal, CDP lo procesa y se vuelve a leer.
    fn process(&self, ctx: &mut RenderCtx) -> Result<Vec<AudioBuf>, ProcessError> {
        let input = ctx.input(0);
        let sr = input.sr;
        let dur = input.duration_secs();
        let nch = input.num_channels();
        let main = self.exe(self.def.program)?;
        let dir = ctx.workdir;
        let mut outs = Vec::with_capacity(nch);
        for (ch, x) in input.channels.iter().enumerate() {
            ctx.check()?;
            let wav_in = format!("ent_c{ch}.wav");
            let wav_out = format!("sal_c{ch}.wav");
            let x = if self.def.program == "distort" { prep_cycles(x, sr) } else { x.clone() };
            write_wav(&dir.join(&wav_in), &AudioBuf::from_channels(sr, vec![x]), WavFormat::Float32)?;
            // Segunda entrada (ya llevada a la misma frecuencia por el motor).
            let wav_in2 = format!("ent2_c{ch}.wav");
            let second = ctx.inputs.get(1).map(|b| b.channels[ch.min(b.num_channels() - 1)].clone());
            if let Some(y) = &second {
                write_wav(&dir.join(&wav_in2), &AudioBuf::from_channels(sr, vec![y.clone()]), WavFormat::Float32)?;
            }
            let dur2 = ctx.inputs.get(1).map_or(0.0, |b| b.duration_secs());
            let c = ArgCtx {
                params: ctx.params,
                specs: self.def.params,
                dur,
                dur2,
                dir,
                ch,
                seed: ctx.seed,
                inp: &wav_in,
                inp2: second.as_ref().map(|_| wav_in2.as_str()),
                out: &wav_out,
                dry: false,
            };
            let args = (self.def.build)(&c)?;
            run(&main, &args, dir, ctx.cancel, |s| {
                let f = (s / dur.max(1e-3)).clamp(0.0, 0.99);
                ctx.set_progress((ch as f64 + f) / nch as f64);
            })?;
            let out_path = dir.join(&wav_out);
            if !out_path.is_file() {
                return Err(ProcessError::new("err.cdp.sin_salida").arg("programa", self.def.program));
            }
            let mut y = decode::load(&out_path)?.channels.into_iter().next().unwrap_or_default();
            match self.expected_len(&[input], ctx.params).expected(input.frames()) {
                Some(n) if !matches!(self.def.len, Len::Unknown) => y.resize(n, 0.0),
                _ => {}
            }
            outs.push(y);
        }
        // Mismo largo en todos los canales.
        let len = outs.iter().map(|c| c.len()).max().unwrap_or(0);
        outs.iter_mut().for_each(|c| c.resize(len, 0.0));
        ctx.set_progress(1.0);
        Ok(vec![AudioBuf::from_channels(sr, outs)])
    }

    /// Procesos con archivos de datos (.ana...): cada canal es un archivo.
    fn process_files(&self, ctx: &mut FileCtx) -> Result<Vec<DataSet>, ProcessError> {
        let main = self.exe(self.def.program)?;
        let dir = ctx.workdir;
        let first = ctx.inputs.first().ok_or_else(|| ProcessError::new("err.render.entradas").arg("n", 1))?;
        let nch = first.channels();
        let dur = first.dur;
        let dur2 = ctx.inputs.get(1).map_or(0.0, |d| d.dur);
        let mut files = Vec::with_capacity(nch);
        for ch in 0..nch {
            ctx.check()?;
            // Nombres cortos en la carpeta de trabajo (CDP y las rutas largas
            // o con espacios no se llevan bien).
            let mut names = Vec::with_capacity(ctx.inputs.len());
            for (i, d) in ctx.inputs.iter().enumerate() {
                let n = if i == 0 { String::new() } else { (i + 1).to_string() };
                let name = format!("ent{n}_c{ch}.{}", d.kind.ext());
                link_or_copy(&d.files[ch.min(d.files.len() - 1)], &dir.join(&name))?;
                names.push(name);
            }
            let out = format!("sal_c{ch}.{}", self.def.out.ext());
            let c = ArgCtx {
                params: ctx.params,
                specs: self.def.params,
                dur,
                dur2,
                dir,
                ch,
                seed: ctx.seed,
                inp: &names[0],
                inp2: names.get(1).map(String::as_str),
                out: &out,
                dry: false,
            };
            let args = (self.def.build)(&c)?;
            run(&main, &args, dir, ctx.cancel, |s| {
                let f = (s / dur.max(1e-3)).clamp(0.0, 0.99);
                ctx.set_progress((ch as f64 + f) / nch as f64);
            })?;
            let p = dir.join(&out);
            if !p.is_file() {
                return Err(ProcessError::new("err.cdp.sin_salida").arg("programa", self.def.program));
            }
            files.push(p);
        }
        ctx.set_progress(1.0);
        Ok(vec![DataSet { kind: self.def.out, files, sr: first.sr, dur }])
    }
}

/// Agrega al catálogo los procesos cuyos programas están instalados (los que
/// usan archivos de análisis necesitan además `pvoc` para convertir).
pub fn register(reg: &mut Registry, install: &Arc<CdpInstall>) {
    for def in &DEFS {
        let pvoc_ok = !def.on_files() || install.has("pvoc");
        if install.has(def.program) && pvoc_ok {
            reg.add(Arc::new(CdpProcess { def, install: install.clone() }));
        }
    }
}
