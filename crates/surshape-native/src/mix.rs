//! Mezcla de N entradas, cada una con ganancia, paneo y tiempo de inicio.
//! No está en el catálogo: la arma el ejecutor a partir de un nodo de
//! mezcla (`NodeKind::Mezcla`). Pasa por el mismo render que los procesos
//! (validación, limitador, remuestreo de entradas a la frecuencia de la
//! primera). La salida siempre es estéreo.

use surshape_audio::db_to_lin;
use surshape_engine::{AudioBuf, Family, Inputs, LenRule, ParamSpec, ParamValues, Process, ProcessError, RenderCtx};

/// Configuración de una entrada.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MixSetting {
    pub gain_db: f64,
    /// -1 = izquierda, 0 = centro, 1 = derecha.
    pub pan: f64,
    /// Segundos de retraso.
    pub start: f64,
}

pub struct Mix {
    pub settings: Vec<MixSetting>,
}

impl Mix {
    fn setting(&self, i: usize) -> MixSetting {
        self.settings.get(i).copied().unwrap_or(MixSetting { gain_db: 0.0, pan: 0.0, start: 0.0 })
    }
}

impl Process for Mix {
    fn id(&self) -> &'static str {
        "mezcla"
    }
    fn family(&self) -> Family {
        Family::Combinacion
    }
    fn params(&self) -> &[ParamSpec] {
        &[]
    }
    fn inputs(&self) -> Inputs {
        Inputs::Variadic
    }
    fn expected_len(&self, inputs: &[&AudioBuf], _: &ParamValues) -> LenRule {
        let sr = inputs.first().map_or(48000.0, |b| b.sr as f64);
        let n = inputs
            .iter()
            .enumerate()
            .map(|(i, b)| (self.setting(i).start.max(0.0) * sr).round() as usize + b.frames())
            .max()
            .unwrap_or(0);
        LenRule::Frames(n)
    }
    fn process(&self, ctx: &mut RenderCtx) -> Result<Vec<AudioBuf>, ProcessError> {
        let sr = ctx.input(0).sr;
        let len = match self.expected_len(&ctx.inputs.iter().map(|b| b.as_ref()).collect::<Vec<_>>(), ctx.params) {
            LenRule::Frames(n) => n,
            _ => 0,
        };
        let mut out = vec![vec![0.0f32; len]; 2];
        let n_in = ctx.inputs.len();
        for (i, b) in ctx.inputs.iter().enumerate() {
            ctx.check()?;
            let s = self.setting(i);
            let g = db_to_lin(s.gain_db) as f32;
            let off = (s.start.max(0.0) * sr as f64).round() as usize;
            let p = s.pan.clamp(-1.0, 1.0);
            let (gl, gr) = if b.num_channels() == 1 {
                // Paneo de igual potencia.
                let a = (p + 1.0) * std::f64::consts::FRAC_PI_4;
                (a.cos() as f32, a.sin() as f32)
            } else {
                // Balance para estéreo.
                ((1.0 - p).min(1.0) as f32, (1.0 + p).min(1.0) as f32)
            };
            let l = &b.channels[0];
            let r = &b.channels[1.min(b.num_channels() - 1)];
            for k in 0..b.frames() {
                out[0][off + k] += l[k] * g * gl;
                out[1][off + k] += r[k] * g * gr;
            }
            ctx.set_progress((i + 1) as f64 / n_in as f64);
        }
        Ok(vec![AudioBuf::from_channels(sr, out)])
    }
}
