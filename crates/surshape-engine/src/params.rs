//! Parámetros: descripción ([`ParamSpec`]), valores fijos o con breakpoints
//! ([`ParamValue`], [`Breakpoints`]) y juegos completos ([`ParamSet`], con
//! valores opcionales por canal).

use crate::hash::StableHasher;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Unidad mostrada junto al valor. Cada una tiene su clave i18n.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    None,
    Db,
    Seconds,
    Ms,
    Percent,
    Factor,
    Hz,
    Semitones,
    Bits,
}

impl Unit {
    /// Clave i18n del símbolo (None = sin unidad).
    pub fn key(self) -> Option<&'static str> {
        match self {
            Unit::None => None,
            Unit::Db => Some("ui.unidad.db"),
            Unit::Seconds => Some("ui.unidad.s"),
            Unit::Ms => Some("ui.unidad.ms"),
            Unit::Percent => Some("ui.unidad.porcentaje"),
            Unit::Factor => Some("ui.unidad.factor"),
            Unit::Hz => Some("ui.unidad.hz"),
            Unit::Semitones => Some("ui.unidad.st"),
            Unit::Bits => Some("ui.unidad.bits"),
        }
    }
}

/// Escala del control: lineal o logarítmica (rangos amplios: 1x a 10000x).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scale {
    Lin,
    Log,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParamKind {
    /// Número real con `decimals` decimales en pantalla.
    Float { decimals: u8 },
    /// Entero.
    Int,
    /// Interruptor: 0 = no, 1 = sí.
    Toggle,
    /// Lista de opciones (claves i18n); el valor es el índice.
    Choice(&'static [&'static str]),
}

/// Un parámetro de un proceso. `key` es la clave i18n del nombre; la de la
/// descripción (tooltip) es `key` + ".desc".
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParamSpec {
    /// Identificador estable dentro del proceso (se guarda en el patch).
    pub id: &'static str,
    pub key: &'static str,
    pub min: f64,
    pub max: f64,
    pub default: f64,
    pub unit: Unit,
    pub scale: Scale,
    pub kind: ParamKind,
    /// ¿Admite variar en el tiempo (breakpoints)?
    pub automatable: bool,
    /// Nombre del parámetro en la documentación de CDP ("" si no es CDP).
    pub cdp: &'static str,
    /// Modos del programa en que el parámetro existe (bit n = modo n + 1;
    /// 0 = en todos).
    pub modos: u16,
}

impl ParamSpec {
    /// Real, automatizable por defecto.
    pub const fn float(id: &'static str, key: &'static str, min: f64, max: f64, default: f64) -> Self {
        Self {
            id,
            key,
            min,
            max,
            default,
            unit: Unit::None,
            scale: Scale::Lin,
            kind: ParamKind::Float { decimals: 2 },
            automatable: true,
            cdp: "",
            modos: 0,
        }
    }
    /// Entero, fijo por defecto.
    pub const fn int(id: &'static str, key: &'static str, min: f64, max: f64, default: f64) -> Self {
        Self { id, key, min, max, default, unit: Unit::None, scale: Scale::Lin, kind: ParamKind::Int, automatable: false, cdp: "", modos: 0 }
    }
    pub const fn toggle(id: &'static str, key: &'static str, default: bool) -> Self {
        let d = if default { 1.0 } else { 0.0 };
        Self {
            id,
            key,
            min: 0.0,
            max: 1.0,
            default: d,
            unit: Unit::None,
            scale: Scale::Lin,
            kind: ParamKind::Toggle,
            automatable: false,
            cdp: "",
            modos: 0,
        }
    }
    /// Lista de opciones; el valor es el índice (fijo, sin breakpoints).
    pub const fn choice(id: &'static str, key: &'static str, options: &'static [&'static str], default: usize) -> Self {
        Self {
            id,
            key,
            min: 0.0,
            max: (options.len() - 1) as f64,
            default: default as f64,
            unit: Unit::None,
            scale: Scale::Lin,
            kind: ParamKind::Choice(options),
            automatable: false,
            cdp: "",
            modos: 0,
        }
    }
    pub const fn unit(mut self, u: Unit) -> Self {
        self.unit = u;
        self
    }
    pub const fn log(mut self) -> Self {
        self.scale = Scale::Log;
        self
    }
    pub const fn decimals(mut self, d: u8) -> Self {
        self.kind = ParamKind::Float { decimals: d };
        self
    }
    /// Admite breakpoints (para enteros, que por defecto no los admiten).
    pub const fn automatable(mut self) -> Self {
        self.automatable = true;
        self
    }
    /// Nombre original del parámetro en CDP.
    pub const fn cdp(mut self, name: &'static str) -> Self {
        self.cdp = name;
        self
    }
    /// Solo en estos modos (bit n = modo n + 1).
    pub const fn modes(mut self, mask: u16) -> Self {
        self.modos = mask;
        self
    }
    /// ¿Existe en el modo `m` (0 = primer modo)?
    pub fn in_mode(&self, m: usize) -> bool {
        self.modos == 0 || (m < 16 && self.modos & (1 << m) != 0)
    }
    /// Solo valor fijo (sin breakpoints).
    pub const fn fixed_only(mut self) -> Self {
        self.automatable = false;
        self
    }

    /// Lleva un valor al rango (y a entero / 0-1 según el tipo).
    pub fn clamp(&self, v: f64) -> f64 {
        let v = if v.is_finite() { v } else { self.default };
        let v = v.clamp(self.min, self.max);
        match self.kind {
            ParamKind::Float { .. } => v,
            ParamKind::Int | ParamKind::Choice(_) => v.round(),
            ParamKind::Toggle => {
                if v >= 0.5 {
                    1.0
                } else {
                    0.0
                }
            }
        }
    }

    pub fn desc_key(&self) -> String {
        format!("{}.desc", self.key)
    }
}

// --- Breakpoints ------------------------------------------------------------------

/// Cómo se leen los tiempos de los breakpoints.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, Hash)]
#[serde(rename_all = "snake_case")]
pub enum TimeMode {
    /// Segundos desde el inicio de la entrada.
    #[default]
    Absoluto,
    /// 0..1 = principio..fin de la entrada.
    Normalizado,
}

/// Forma entre dos puntos.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Interp {
    #[default]
    Lineal,
    /// Mantiene el valor hasta el punto siguiente.
    Escalon,
    /// Curva exponencial (si ambos valores tienen el mismo signo y no son 0;
    /// si no, lineal).
    Exponencial,
}

/// Pares (tiempo, valor) ordenados por tiempo. Antes del primer punto vale
/// el primero; después del último, el último.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct Breakpoints {
    #[serde(default)]
    pub tiempo: TimeMode,
    #[serde(default)]
    pub curva: Interp,
    pub puntos: Vec<(f64, f64)>,
}

/// Error al leer breakpoints de texto: línea (1..) y clave i18n.
#[derive(Clone, Debug, PartialEq)]
pub struct BreakpointError {
    pub line: usize,
    pub key: &'static str,
}

fn interp(curva: Interp, (t0, v0): (f64, f64), (t1, v1): (f64, f64), t: f64) -> f64 {
    if t1 <= t0 {
        return v1;
    }
    let f = ((t - t0) / (t1 - t0)).clamp(0.0, 1.0);
    match curva {
        Interp::Lineal => v0 + (v1 - v0) * f,
        Interp::Escalon => v0,
        Interp::Exponencial => {
            if v0 != 0.0 && v1 != 0.0 && (v0 > 0.0) == (v1 > 0.0) {
                v0 * (v1 / v0).powf(f)
            } else {
                v0 + (v1 - v0) * f
            }
        }
    }
}

impl Breakpoints {
    pub fn new(tiempo: TimeMode, curva: Interp, mut puntos: Vec<(f64, f64)>) -> Self {
        puntos.retain(|(t, v)| t.is_finite() && v.is_finite());
        puntos.sort_by(|a, b| a.0.total_cmp(&b.0));
        Self { tiempo, curva, puntos }
    }

    /// Tiempo en segundos de un punto, según el modo.
    fn abs_time(&self, t: f64, dur: f64) -> f64 {
        match self.tiempo {
            TimeMode::Absoluto => t,
            TimeMode::Normalizado => t * dur,
        }
    }

    /// Puntos con tiempos absolutos en segundos (para CDP y para [`Curve`]).
    pub fn to_abs(&self, dur: f64) -> Vec<(f64, f64)> {
        self.puntos.iter().map(|&(t, v)| (self.abs_time(t, dur), v)).collect()
    }

    /// Valor en `t` segundos, para una entrada de `dur` segundos.
    pub fn value_at(&self, t: f64, dur: f64) -> f64 {
        let pts = &self.puntos;
        match pts.len() {
            0 => 0.0,
            1 => pts[0].1,
            _ => {
                let at = |i: usize| (self.abs_time(pts[i].0, dur), pts[i].1);
                if t <= at(0).0 {
                    return pts[0].1;
                }
                let last = pts.len() - 1;
                if t >= at(last).0 {
                    return pts[last].1;
                }
                // primer punto con tiempo > t
                let i = pts.partition_point(|p| self.abs_time(p.0, dur) <= t);
                interp(self.curva, at(i - 1), at(i), t)
            }
        }
    }

    /// Texto "tiempo valor" por línea (el formato de breakpoints de CDP).
    /// Las líneas con `#` son comentarios.
    pub fn to_text(&self) -> String {
        let mut s = String::new();
        for (t, v) in &self.puntos {
            s.push_str(&format!("{t} {v}\n"));
        }
        s
    }

    /// Lee texto "tiempo valor" (separado por espacios, tabs o comas).
    pub fn from_text(src: &str, tiempo: TimeMode, curva: Interp) -> Result<Self, BreakpointError> {
        let mut pts = Vec::new();
        for (i, raw) in src.lines().enumerate() {
            let l = raw.trim();
            if l.is_empty() || l.starts_with('#') {
                continue;
            }
            let nums: Vec<&str> = l.split(|c: char| c.is_whitespace() || c == ',' || c == ';').filter(|s| !s.is_empty()).collect();
            let err = BreakpointError { line: i + 1, key: "err.breakpoints.linea" };
            if nums.len() != 2 {
                return Err(err);
            }
            let t: f64 = nums[0].parse().map_err(|_| err.clone())?;
            let v: f64 = nums[1].parse().map_err(|_| err.clone())?;
            if !t.is_finite() || !v.is_finite() || t < 0.0 {
                return Err(err);
            }
            pts.push((t, v));
        }
        if pts.is_empty() {
            return Err(BreakpointError { line: 0, key: "err.breakpoints.vacio" });
        }
        Ok(Self::new(tiempo, curva, pts))
    }
}

/// Lector rápido de un parámetro a lo largo del tiempo: valor fijo o
/// breakpoints con cursor (O(1) amortizado si el tiempo avanza).
#[derive(Clone, Debug)]
pub enum Curve {
    Fixed(f64),
    Env { pts: Vec<(f64, f64)>, curva: Interp, i: usize },
}

impl Curve {
    pub fn new(v: &ParamValue, dur: f64) -> Self {
        match v {
            ParamValue::Fixed(x) => Curve::Fixed(*x),
            ParamValue::Envelope(bp) if bp.puntos.len() <= 1 => Curve::Fixed(bp.value_at(0.0, dur)),
            ParamValue::Envelope(bp) => Curve::Env { pts: bp.to_abs(dur), curva: bp.curva, i: 0 },
        }
    }

    pub fn is_fixed(&self) -> bool {
        matches!(self, Curve::Fixed(_))
    }

    /// Valor en `t` segundos.
    #[inline]
    pub fn at(&mut self, t: f64) -> f64 {
        match self {
            Curve::Fixed(v) => *v,
            Curve::Env { pts, curva, i } => {
                if t <= pts[0].0 {
                    *i = 0;
                    return pts[0].1;
                }
                let last = pts.len() - 1;
                if t >= pts[last].0 {
                    return pts[last].1;
                }
                if pts[*i].0 > t {
                    *i = 0; // el tiempo retrocedió
                }
                while *i + 1 < last && pts[*i + 1].0 <= t {
                    *i += 1;
                }
                interp(*curva, pts[*i], pts[*i + 1], t)
            }
        }
    }
}

// --- Valores --------------------------------------------------------------------------

/// Valor de un parámetro: fijo o variable en el tiempo. En JSON un valor fijo
/// es un número a secas.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ParamValue {
    Fixed(f64),
    Envelope(Breakpoints),
}

impl ParamValue {
    /// Valor al inicio (el que muestra un control simple).
    pub fn initial(&self) -> f64 {
        match self {
            ParamValue::Fixed(v) => *v,
            ParamValue::Envelope(bp) => bp.puntos.first().map_or(0.0, |p| p.1),
        }
    }

    fn clamped(&self, spec: &ParamSpec) -> Self {
        match self {
            ParamValue::Envelope(bp) if spec.automatable && !bp.puntos.is_empty() => {
                let mut bp = bp.clone();
                for p in &mut bp.puntos {
                    p.1 = spec.clamp(p.1);
                }
                ParamValue::Envelope(bp)
            }
            v => ParamValue::Fixed(spec.clamp(v.initial())),
        }
    }

    fn hash_into(&self, h: &mut StableHasher) {
        match self {
            ParamValue::Fixed(v) => {
                h.u8(0);
                h.f64(*v);
            }
            ParamValue::Envelope(bp) => {
                h.u8(1);
                h.u8(bp.tiempo as u8);
                h.u8(bp.curva as u8);
                h.u64(bp.puntos.len() as u64);
                for (t, v) in &bp.puntos {
                    h.f64(*t);
                    h.f64(*v);
                }
            }
        }
    }
}

/// Valores por id. Ordenado (BTreeMap) para que el JSON y el hash sean estables.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ParamValues(pub BTreeMap<String, ParamValue>);

impl ParamValues {
    pub fn defaults(specs: &[ParamSpec]) -> Self {
        Self(specs.iter().map(|s| (s.id.to_string(), ParamValue::Fixed(s.default))).collect())
    }

    /// Completa los que falten con su default, recorta todo a su rango,
    /// convierte en fijos los breakpoints de parámetros no automatizables y
    /// descarta ids desconocidos.
    pub fn completed(&self, specs: &[ParamSpec]) -> Self {
        Self(
            specs
                .iter()
                .map(|s| {
                    let v = self.0.get(s.id).cloned().unwrap_or(ParamValue::Fixed(s.default));
                    (s.id.to_string(), v.clamped(s))
                })
                .collect(),
        )
    }

    /// Valor inicial de `id` (0 si no existe).
    pub fn get(&self, id: &str) -> f64 {
        self.0.get(id).map_or(0.0, |v| v.initial())
    }

    pub fn value(&self, id: &str) -> Option<&ParamValue> {
        self.0.get(id)
    }

    pub fn set(&mut self, id: &str, v: f64) {
        self.0.insert(id.to_string(), ParamValue::Fixed(v));
    }

    pub fn set_value(&mut self, id: &str, v: ParamValue) {
        self.0.insert(id.to_string(), v);
    }

    pub fn hash_into(&self, h: &mut StableHasher) {
        h.u64(self.0.len() as u64);
        for (k, v) in &self.0 {
            h.str(k);
            v.hash_into(h);
        }
    }
}

/// Meta-proceso "aleatorizar": mueve cada valor fijo al azar hasta
/// `amount` (0..1) de su rango, alrededor del valor actual (en escala log
/// para los parámetros logarítmicos). Las listas cambian de opción con
/// probabilidad `amount`. Los breakpoints no se tocan. Determinista: la
/// misma seed da el mismo resultado.
pub fn randomize(specs: &[ParamSpec], values: &ParamValues, amount: f64, seed: u64) -> ParamValues {
    let mut s = (crate::seed32(seed) as u64) | 1;
    let mut next = move || {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        (s >> 11) as f64 / (1u64 << 53) as f64
    };
    let a = amount.clamp(0.0, 1.0);
    let mut out = values.completed(specs);
    if a == 0.0 {
        return out;
    }
    for spec in specs {
        let Some(ParamValue::Fixed(v)) = out.value(spec.id).cloned() else { continue };
        let r = next() * 2.0 - 1.0;
        let nv = match spec.kind {
            ParamKind::Toggle => continue,
            ParamKind::Choice(opts) => {
                if next() < a {
                    (next() * opts.len() as f64).floor()
                } else {
                    v
                }
            }
            _ if spec.scale == Scale::Log && spec.min > 0.0 => {
                let (lo, hi) = (spec.min.ln(), spec.max.ln());
                (v.max(spec.min).ln() + r * a * (hi - lo)).clamp(lo, hi).exp()
            }
            _ => v + r * a * (spec.max - spec.min),
        };
        out.set(spec.id, spec.clamp(nv));
    }
    out
}

/// Juego completo de parámetros de un nodo: valores comunes y, si el proceso
/// lo admite, valores distintos por canal (estéreo).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ParamSet {
    pub comun: ParamValues,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub por_canal: Option<Vec<ParamValues>>,
}

impl ParamSet {
    pub fn defaults(specs: &[ParamSpec]) -> Self {
        Self { comun: ParamValues::defaults(specs), por_canal: None }
    }

    pub fn completed(&self, specs: &[ParamSpec]) -> Self {
        Self {
            comun: self.comun.completed(specs),
            por_canal: self.por_canal.as_ref().map(|v| v.iter().map(|p| p.completed(specs)).collect()),
        }
    }

    /// Valores para el canal `ch` (los comunes si no hay por canal).
    pub fn for_channel(&self, ch: usize) -> &ParamValues {
        self.por_canal.as_ref().and_then(|v| v.get(ch)).unwrap_or(&self.comun)
    }

    pub fn hash_into(&self, h: &mut StableHasher) {
        self.comun.hash_into(h);
        match &self.por_canal {
            None => h.u8(0),
            Some(v) => {
                h.u8(1);
                h.u64(v.len() as u64);
                for p in v {
                    p.hash_into(h);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completed_fills_clamps_and_freezes_non_automatable() {
        let specs = [
            ParamSpec::float("a", "k.a", 0.0, 1.0, 0.5),
            ParamSpec::int("b", "k.b", 1.0, 16.0, 8.0),
            ParamSpec::toggle("c", "k.c", true),
        ];
        let mut v = ParamValues::default();
        v.set("b", 99.4);
        v.set("x", 3.0);
        v.set_value("a", ParamValue::Envelope(Breakpoints::new(TimeMode::Absoluto, Interp::Lineal, vec![(0.0, -1.0), (1.0, 2.0)])));
        let c = v.completed(&specs);
        assert_eq!(c.get("b"), 16.0);
        assert_eq!(c.get("c"), 1.0);
        assert!(!c.0.contains_key("x"));
        match c.value("a").unwrap() {
            ParamValue::Envelope(bp) => assert_eq!(bp.puntos, vec![(0.0, 0.0), (1.0, 1.0)]),
            v => panic!("{v:?}"),
        }
        // un entero no automatizable con breakpoints queda fijo
        let mut w = ParamValues::default();
        w.set_value("b", ParamValue::Envelope(Breakpoints::new(TimeMode::Absoluto, Interp::Lineal, vec![(0.0, 4.0), (1.0, 9.0)])));
        assert_eq!(w.completed(&specs).value("b"), Some(&ParamValue::Fixed(4.0)));
    }

    #[test]
    fn randomize_is_deterministic_bounded_and_scaled() {
        let specs = [
            ParamSpec::float("a", "k.a", 0.0, 10.0, 5.0),
            ParamSpec::float("l", "k.l", 1.0, 1000.0, 10.0).log(),
            ParamSpec::toggle("t", "k.t", true),
        ];
        let v = ParamValues::defaults(&specs);
        let r1 = randomize(&specs, &v, 0.5, 7);
        assert_eq!(r1, randomize(&specs, &v, 0.5, 7));
        assert_ne!(r1, randomize(&specs, &v, 0.5, 8));
        assert_eq!(randomize(&specs, &v, 0.0, 7), v.completed(&specs));
        for seed in 0..50 {
            let r = randomize(&specs, &v, 1.0, seed);
            assert!((0.0..=10.0).contains(&r.get("a")) && (1.0..=1000.0).contains(&r.get("l")));
            assert_eq!(r.get("t"), 1.0);
        }
    }

    #[test]
    fn breakpoints_interpolate_and_roundtrip_text() {
        let bp = Breakpoints::new(TimeMode::Normalizado, Interp::Lineal, vec![(1.0, 10.0), (0.0, 0.0)]);
        assert_eq!(bp.value_at(1.0, 4.0), 2.5); // 1 s de 4 s = 0,25
        assert_eq!(bp.value_at(-1.0, 4.0), 0.0);
        assert_eq!(bp.value_at(9.0, 4.0), 10.0);
        let mut c = Curve::new(&ParamValue::Envelope(bp.clone()), 4.0);
        assert_eq!(c.at(2.0), 5.0);
        assert_eq!(c.at(1.0), 2.5); // hacia atrás también
        let back = Breakpoints::from_text(&bp.to_text(), TimeMode::Normalizado, Interp::Lineal).unwrap();
        assert_eq!(back, bp);
        let e = Breakpoints::new(TimeMode::Absoluto, Interp::Exponencial, vec![(0.0, 1.0), (2.0, 100.0)]);
        assert!((e.value_at(1.0, 0.0) - 10.0).abs() < 1e-9);
        assert_eq!(Breakpoints::from_text("0 1\n# c\n1,2\nx y", TimeMode::Absoluto, Interp::Lineal).unwrap_err().line, 4);
    }
}
