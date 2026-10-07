//! Presets por proceso, en archivos de texto legibles y editables a mano:
//!
//! ```text
//! <base>/<proceso>/<nombre>.preset
//! <base>/<proceso>/_defecto.txt     nombre del preset por defecto
//! ```
//!
//! Formato (una línea por parámetro; `#` comenta):
//!
//! ```text
//! factor = 8
//! bits = curva normalizado lineal: 0 2; 0.5 12; 1 4
//! ```
//!
//! "curva" lleva modo de tiempo (absoluto | normalizado), forma (lineal |
//! escalon | exponencial) y pares "tiempo valor" separados por `;`.

use std::path::{Path, PathBuf};
use surshape_engine::{Breakpoints, Interp, ParamValue, ParamValues, TimeMode};

const EXT: &str = "preset";
const DEFAULT_FILE: &str = "_defecto.txt";

fn time_word(t: TimeMode) -> &'static str {
    match t {
        TimeMode::Absoluto => "absoluto",
        TimeMode::Normalizado => "normalizado",
    }
}

fn interp_word(i: Interp) -> &'static str {
    match i {
        Interp::Lineal => "lineal",
        Interp::Escalon => "escalon",
        Interp::Exponencial => "exponencial",
    }
}

/// Texto de un preset.
pub fn to_text(proc_id: &str, values: &ParamValues) -> String {
    let mut s = format!("# SURSHAPE · preset de {proc_id}\n");
    for (k, v) in &values.0 {
        match v {
            ParamValue::Fixed(x) => s.push_str(&format!("{k} = {x}\n")),
            ParamValue::Envelope(bp) => {
                let pts: Vec<String> = bp.puntos.iter().map(|(t, v)| format!("{t} {v}")).collect();
                s.push_str(&format!("{k} = curva {} {}: {}\n", time_word(bp.tiempo), interp_word(bp.curva), pts.join("; ")));
            }
        }
    }
    s
}

/// Lee un preset. Las líneas que no se entienden se ignoran (y se cuentan).
pub fn from_text(src: &str) -> (ParamValues, usize) {
    let mut v = ParamValues::default();
    let mut bad = 0;
    for line in src.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let Some((k, val)) = line.split_once('=') else {
            bad += 1;
            continue;
        };
        let (k, val) = (k.trim(), val.trim());
        if let Some(rest) = val.strip_prefix("curva") {
            let Some((head, pts)) = rest.split_once(':') else {
                bad += 1;
                continue;
            };
            let words: Vec<&str> = head.split_whitespace().collect();
            let tiempo = if words.contains(&"normalizado") { TimeMode::Normalizado } else { TimeMode::Absoluto };
            let curva = if words.contains(&"escalon") {
                Interp::Escalon
            } else if words.contains(&"exponencial") {
                Interp::Exponencial
            } else {
                Interp::Lineal
            };
            let txt = pts.replace(';', "\n");
            match Breakpoints::from_text(&txt, tiempo, curva) {
                Ok(bp) => v.set_value(k, ParamValue::Envelope(bp)),
                Err(_) => bad += 1,
            }
        } else if let Ok(x) = val.parse::<f64>() {
            v.set(k, x);
        } else {
            bad += 1;
        }
    }
    (v, bad)
}

/// Nombre de archivo seguro.
fn safe(name: &str) -> String {
    let s: String = name.trim().chars().map(|c| if c.is_alphanumeric() || " -_.".contains(c) { c } else { '_' }).take(60).collect();
    if s.trim().is_empty() {
        "preset".into()
    } else {
        s.trim().to_string()
    }
}

fn proc_dir(base: &Path, proc_id: &str) -> PathBuf {
    base.join(safe(proc_id))
}

/// Nombres de los presets de un proceso, ordenados.
pub fn list(base: &Path, proc_id: &str) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(proc_dir(base, proc_id))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            (p.extension().and_then(|x| x.to_str()) == Some(EXT)).then(|| p.file_stem()?.to_str().map(String::from)).flatten()
        })
        .collect();
    v.sort_by_key(|s| s.to_lowercase());
    v
}

pub fn save(base: &Path, proc_id: &str, name: &str, values: &ParamValues) -> std::io::Result<String> {
    let dir = proc_dir(base, proc_id);
    std::fs::create_dir_all(&dir)?;
    let name = safe(name);
    std::fs::write(dir.join(format!("{name}.{EXT}")), to_text(proc_id, values))?;
    Ok(name)
}

pub fn load(base: &Path, proc_id: &str, name: &str) -> std::io::Result<ParamValues> {
    let txt = std::fs::read_to_string(proc_dir(base, proc_id).join(format!("{}.{EXT}", safe(name))))?;
    Ok(from_text(&txt).0)
}

pub fn delete(base: &Path, proc_id: &str, name: &str) -> std::io::Result<()> {
    if default_name(base, proc_id).as_deref() == Some(name) {
        set_default(base, proc_id, None)?;
    }
    std::fs::remove_file(proc_dir(base, proc_id).join(format!("{}.{EXT}", safe(name))))
}

pub fn default_name(base: &Path, proc_id: &str) -> Option<String> {
    let n = std::fs::read_to_string(proc_dir(base, proc_id).join(DEFAULT_FILE)).ok()?;
    let n = n.trim().to_string();
    list(base, proc_id).contains(&n).then_some(n)
}

pub fn set_default(base: &Path, proc_id: &str, name: Option<&str>) -> std::io::Result<()> {
    let dir = proc_dir(base, proc_id);
    let f = dir.join(DEFAULT_FILE);
    match name {
        Some(n) => {
            std::fs::create_dir_all(&dir)?;
            std::fs::write(f, safe(n))
        }
        None => match std::fs::remove_file(f) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        },
    }
}

/// Valores del preset por defecto, si hay uno.
pub fn load_default(base: &Path, proc_id: &str) -> Option<ParamValues> {
    load(base, proc_id, &default_name(base, proc_id)?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::tmp;

    #[test]
    fn text_roundtrip_and_files() {
        let mut v = ParamValues::default();
        v.set("factor", 8.5);
        v.set_value(
            "bits",
            ParamValue::Envelope(Breakpoints::new(TimeMode::Normalizado, Interp::Exponencial, vec![(0.0, 2.0), (1.0, 12.0)])),
        );
        let txt = to_text("nat.x", &v);
        let (back, bad) = from_text(&txt);
        assert_eq!((back.clone(), bad), (v.clone(), 0));
        assert_eq!(from_text("a = 1\nbasura\nb = curva: x y").1, 2);

        let base = tmp("presets");
        assert!(list(&base, "nat.x").is_empty());
        save(&base, "nat.x", "Mi preset/1", &v).unwrap();
        save(&base, "nat.x", "otro", &ParamValues::default()).unwrap();
        assert_eq!(list(&base, "nat.x"), vec!["Mi preset_1", "otro"]);
        assert_eq!(load(&base, "nat.x", "Mi preset_1").unwrap(), v);
        assert!(load_default(&base, "nat.x").is_none());
        set_default(&base, "nat.x", Some("Mi preset_1")).unwrap();
        assert_eq!(load_default(&base, "nat.x"), Some(v));
        delete(&base, "nat.x", "Mi preset_1").unwrap();
        assert!(default_name(&base, "nat.x").is_none());
        let _ = std::fs::remove_dir_all(&base);
    }
}
