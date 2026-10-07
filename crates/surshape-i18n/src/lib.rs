//! SURSHAPE · i18n · por @ahrsml
//!
//! Textos de la interfaz en español e inglés. Cada idioma es un archivo de
//! texto `locales/<código>.lang` con líneas `clave = valor`:
//!
//! ```text
//! # comentario
//! # [TÉRMINO] marca un término técnico que decide el traductor
//! ui.render = Render
//! err.audio.abrir = No se pudo abrir {archivo}:\n{detalle}
//! ```
//!
//! `\n` es un salto de línea y `{nombre}` un argumento. Los archivos se
//! embeben en el ejecutable; si junto al exe hay una carpeta `locales/` con
//! archivos válidos, esos tienen prioridad (para revisar traducciones sin
//! recompilar). El idioma se cambia en caliente con [`set_lang`].
//!
//! Uso: `t!("ui.render")` devuelve `&'static str`;
//! `t!("err.audio.abrir", archivo = p, detalle = e)` devuelve `String`.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Mutex, OnceLock};

/// Archivos embebidos.
pub const ES_SRC: &str = include_str!("../locales/es.lang");
pub const EN_SRC: &str = include_str!("../locales/en.lang");

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Hash)]
pub enum Lang {
    #[default]
    Es,
    En,
}

impl Lang {
    pub const ALL: [Lang; 2] = [Lang::Es, Lang::En];

    /// Código ISO 639-1 ("es", "en"); se usa en preferencias.
    pub fn code(self) -> &'static str {
        match self {
            Lang::Es => "es",
            Lang::En => "en",
        }
    }

    pub fn from_code(s: &str) -> Option<Lang> {
        Lang::ALL.into_iter().find(|l| l.code() == s)
    }

    /// Clave del nombre del idioma (escrito en su propio idioma).
    pub fn name_key(self) -> &'static str {
        match self {
            Lang::Es => "ui.idioma.es",
            Lang::En => "ui.idioma.en",
        }
    }

    fn idx(self) -> usize {
        self as usize
    }
}

/// Una entrada del archivo, con su línea (para los tests).
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub key: String,
    pub value: String,
    pub line: usize,
}

/// Resultado de leer un archivo `.lang`.
#[derive(Clone, Debug, Default)]
pub struct Parsed {
    pub entries: Vec<Entry>,
    /// Problemas de formato: (línea, descripción técnica).
    pub problems: Vec<(usize, String)>,
}

/// Reemplaza las secuencias de escape `\n`, `\t` y `\\`.
fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('\\') => out.push('\\'),
                Some(o) => {
                    out.push('\\');
                    out.push(o);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Lee el contenido de un archivo `.lang`. No falla: reporta los problemas.
pub fn parse(src: &str) -> Parsed {
    let mut p = Parsed::default();
    let mut seen: HashMap<String, usize> = HashMap::new();
    for (i, raw) in src.lines().enumerate() {
        let line = i + 1;
        let l = raw.trim();
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        let Some((k, v)) = l.split_once('=') else {
            p.problems.push((line, "missing '='".into()));
            continue;
        };
        let key = k.trim();
        let value = unescape(v.trim());
        if key.is_empty() || key.contains(char::is_whitespace) {
            p.problems.push((line, format!("invalid key '{key}'")));
            continue;
        }
        if value.is_empty() {
            p.problems.push((line, format!("empty value for '{key}'")));
        }
        if let Some(prev) = seen.insert(key.to_string(), line) {
            p.problems.push((line, format!("duplicate key '{key}' (first at line {prev})")));
            continue;
        }
        p.entries.push(Entry { key: key.to_string(), value, line });
    }
    p
}

type Table = HashMap<&'static str, &'static str>;

fn leak_table(src: &str) -> Table {
    parse(src)
        .entries
        .into_iter()
        .map(|e| (&*Box::leak(e.key.into_boxed_str()), &*Box::leak(e.value.into_boxed_str())))
        .collect()
}

static TABLES: OnceLock<[Table; 2]> = OnceLock::new();
static CURRENT: AtomicU8 = AtomicU8::new(0);
static MISSING: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());

fn embedded() -> [Table; 2] {
    [leak_table(ES_SRC), leak_table(EN_SRC)]
}

/// Inicializa las tablas. Si `override_dir` tiene `es.lang` / `en.lang`, sus
/// claves reemplazan a las embebidas (las que falten siguen embebidas).
/// Llamar una vez al arrancar, antes del primer `t!`. Devuelve los archivos
/// externos que se usaron.
pub fn init(override_dir: Option<&Path>) -> Vec<std::path::PathBuf> {
    let mut used = Vec::new();
    let mut tables = embedded();
    if let Some(dir) = override_dir {
        for lang in Lang::ALL {
            let f = dir.join(format!("{}.lang", lang.code()));
            if let Ok(src) = std::fs::read_to_string(&f) {
                tables[lang.idx()].extend(leak_table(&src));
                used.push(f);
            }
        }
    }
    let _ = TABLES.set(tables);
    used
}

fn tables() -> &'static [Table; 2] {
    TABLES.get_or_init(embedded)
}

pub fn set_lang(l: Lang) {
    CURRENT.store(l as u8, Ordering::Relaxed);
}

pub fn lang() -> Lang {
    match CURRENT.load(Ordering::Relaxed) {
        1 => Lang::En,
        _ => Lang::Es,
    }
}

/// Texto de `key` en el idioma activo. Si falta, devuelve la clave misma
/// (visible a propósito, para notarlo) y la anota en [`missing`].
pub fn tr(key: &str) -> &'static str {
    tr_in(lang(), key)
}

pub fn tr_in(l: Lang, key: &str) -> &'static str {
    if let Some(v) = tables()[l.idx()].get(key) {
        return v;
    }
    let mut m = MISSING.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(k) = m.iter().find(|k| **k == key) {
        return k;
    }
    let k: &'static str = Box::leak(key.to_string().into_boxed_str());
    m.push(k);
    k
}

/// ¿Existe la clave en el idioma activo?
pub fn has(key: &str) -> bool {
    tables()[lang().idx()].contains_key(key)
}

/// Claves pedidas que no existían (diagnóstico).
pub fn missing() -> Vec<&'static str> {
    MISSING.lock().map(|m| m.clone()).unwrap_or_default()
}

/// Sustituye `{nombre}` por su valor.
pub fn fill(template: &str, args: &[(&str, String)]) -> String {
    let mut s = template.to_string();
    for (k, v) in args {
        s = s.replace(&format!("{{{k}}}"), v);
    }
    s
}

pub fn tr_args(key: &str, args: &[(&str, String)]) -> String {
    fill(tr(key), args)
}

/// `t!("clave")` -> `&'static str`;
/// `t!("clave", nombre = valor, ...)` -> `String` con `{nombre}` reemplazado.
#[macro_export]
macro_rules! t {
    ($key:expr) => {
        $crate::tr($key)
    };
    ($key:expr, $($name:ident = $val:expr),+ $(,)?) => {
        $crate::tr_args($key, &[$((stringify!($name), ($val).to_string())),+])
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_handles_comments_escapes_and_problems() {
        let p = parse("# hola\n\na.b = uno\\ndos\nmal\na.b = otra\nc =\n");
        assert_eq!(p.entries.len(), 2);
        assert_eq!(p.entries[0].value, "uno\ndos");
        assert_eq!(p.problems.len(), 3, "{:?}", p.problems);
    }

    #[test]
    fn fill_and_switch() {
        assert_eq!(fill("a {x} b {y}", &[("x", "1".into()), ("y", "2".into())]), "a 1 b 2");
        set_lang(Lang::En);
        assert_eq!(tr("ui.idioma.es"), "Español");
        set_lang(Lang::Es);
        assert_eq!(tr("no.existe"), "no.existe");
        assert!(missing().contains(&"no.existe"));
    }
}
