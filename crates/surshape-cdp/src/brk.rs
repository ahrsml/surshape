//! Parámetros -> argumentos de CDP. Un valor fijo va como número; uno con
//! breakpoints se escribe en un archivo de texto `tiempo valor` (el formato
//! de CDP) y se pasa su nombre.
//!
//! CDP pide que los tiempos empiecen en 0 y sean estrictamente crecientes:
//! si el primer punto no está en 0 se agrega uno con su valor, y los tiempos
//! repetidos se separan un microsegundo. Los tiempos son absolutos (los
//! normalizados se escalan a la duración de la entrada).

use std::path::Path;
use surshape_engine::{ParamKind, ParamSpec, ParamValue, ProcessError};

/// Número en texto, sin notación científica (CDP no la entiende).
pub fn fmt_num(x: f64, int: bool) -> String {
    if int {
        format!("{}", x.round() as i64)
    } else {
        let s = format!("{x:.6}");
        let s = s.trim_end_matches('0').trim_end_matches('.');
        if s.is_empty() || s == "-" {
            "0".into()
        } else {
            s.to_string()
        }
    }
}

/// Puntos listos para CDP (desde 0, tiempos crecientes, valores en rango).
pub fn cdp_points(spec: &ParamSpec, pts: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut out: Vec<(f64, f64)> = Vec::with_capacity(pts.len() + 1);
    for &(t, v) in pts {
        let t = t.max(0.0);
        let v = spec.clamp(v);
        if out.is_empty() && t > 0.0 {
            out.push((0.0, v));
        }
        let t = match out.last() {
            Some(&(last, _)) if t <= last => last + 1e-6,
            _ => t,
        };
        out.push((t, v));
    }
    out
}

/// Texto de breakpoints para CDP.
pub fn brk_text(spec: &ParamSpec, pts: &[(f64, f64)]) -> String {
    let int = spec.kind == ParamKind::Int;
    cdp_points(spec, pts).iter().map(|&(t, v)| format!("{} {}\n", fmt_num(t, false), fmt_num(v, int))).collect()
}

/// Argumento para `spec` con valor `v`: número, o nombre del archivo `.brk`
/// escrito en `dir` (`name`.brk).
pub fn arg_for(spec: &ParamSpec, v: &ParamValue, dur: f64, dir: &Path, name: &str) -> Result<String, ProcessError> {
    let int = spec.kind == ParamKind::Int;
    match v {
        ParamValue::Envelope(bp) if spec.automatable && bp.puntos.len() > 1 => {
            let file = format!("{name}.brk");
            std::fs::write(dir.join(&file), brk_text(spec, &bp.to_abs(dur)))
                .map_err(|e| ProcessError::new("err.render.temporal").arg("detalle", e))?;
            Ok(file)
        }
        v => Ok(fmt_num(spec.clamp(v.initial()), int)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brk_starts_at_zero_and_increases() {
        let spec = ParamSpec::int("n", "k", 1.0, 10.0, 2.0).automatable();
        let t = brk_text(&spec, &[(0.5, 3.4), (0.5, 20.0), (2.0, 1.0)]);
        assert_eq!(t, "0 3\n0.5 3\n0.500001 10\n2 1\n");
        assert_eq!(fmt_num(1e-7, false), "0");
        assert_eq!(fmt_num(-2.5, false), "-2.5");
    }
}
