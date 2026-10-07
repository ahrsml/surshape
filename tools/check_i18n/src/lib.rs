//! SURSHAPE · verificación de i18n · por @ahrsml
//!
//! `cargo test -p check_i18n` falla si:
//!   - un archivo `.lang` tiene líneas mal formadas, claves repetidas o vacías;
//!   - los dos idiomas no tienen exactamente las mismas claves, en el mismo
//!     orden (para poder revisarlos lado a lado);
//!   - el código usa una clave que no existe;
//!   - hay claves huérfanas (en los `.lang` pero sin uso en el código);
//!   - hay un literal de texto visible en `surshape-app` fuera del sistema
//!     i18n. Los literales no visibles (ids, extensiones, nombres de archivo)
//!     se marcan con `// i18n-ok` en la misma línea. Los nombres de
//!     argumento de mensajes (`.arg("archivo", ...)`) no son texto visible.
//!     Los bloques marcados con `#[cfg(test)]` no se revisan.
//!
//! Claves "dinámicas" (armadas en tiempo de ejecución) que también se exigen:
//! nombre y descripción de cada proceso registrado y nombre y descripción de
//! cada parámetro.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use surshape_engine::process::{desc_key, name_key};

/// Prefijos de las secciones de los archivos `.lang`.
pub const SECTIONS: &[&str] = &["ui.", "proc.", "param.", "err.", "creditos."];

pub fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// ¿Parece una clave i18n?
pub fn looks_like_key(s: &str) -> bool {
    SECTIONS.iter().any(|p| s.starts_with(p))
        && s.len() > 4
        && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '_')
        && !s.ends_with('.')
}

/// Literal de cadena encontrado en el código.
#[derive(Clone, Debug, PartialEq)]
pub struct Lit {
    pub line: usize,
    pub text: String,
    /// ¿Está dentro de un módulo de test?
    pub in_test: bool,
    /// ¿Es el nombre de un argumento de mensaje (`.arg("nombre", ...)`)?
    pub arg_name: bool,
}

/// Extrae los literales de cadena de código Rust (cadenas normales, crudas y
/// de bytes), saltando comentarios y literales de carácter.
pub fn string_literals(src: &str) -> Vec<Lit> {
    let b: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    let mut line = 1;
    let mut in_test = false;
    // Módulo de test: tras `#[cfg(test)]`, el siguiente bloque `{ ... }`.
    let mut test_pending = false;
    let mut depth = 0usize;
    let mut test_depth = 0usize;
    let n = b.len();
    let starts_with = |i: usize, s: &str| s.chars().enumerate().all(|(k, c)| b.get(i + k) == Some(&c));
    while i < n {
        let c = b[i];
        if c == '\n' {
            line += 1;
            i += 1;
            continue;
        }
        if starts_with(i, "#[cfg(test)]") {
            test_pending = true;
            i += "#[cfg(test)]".len();
            continue;
        }
        if c == '{' {
            depth += 1;
            if test_pending {
                test_pending = false;
                in_test = true;
                test_depth = depth;
            }
        } else if c == '}' {
            if in_test && depth == test_depth {
                in_test = false;
            }
            depth = depth.saturating_sub(1);
        } else if c == ';' && test_pending {
            // `#[cfg(test)] use ...;` o `fn` sin cuerpo: no abre bloque
            test_pending = false;
        }
        // Comentarios
        if starts_with(i, "//") {
            while i < n && b[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if starts_with(i, "/*") {
            let mut depth = 0;
            while i < n {
                if starts_with(i, "/*") {
                    depth += 1;
                    i += 2;
                } else if starts_with(i, "*/") {
                    depth -= 1;
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    if b[i] == '\n' {
                        line += 1;
                    }
                    i += 1;
                }
            }
            continue;
        }
        // Carácter o lifetime
        if c == '\'' {
            if b.get(i + 1) == Some(&'\\') {
                // '\n', '\'', '\u{..}'
                i += 2;
                while i < n && b[i] != '\'' {
                    i += 1;
                }
                i += 1;
            } else if b.get(i + 2) == Some(&'\'') {
                i += 3;
            } else {
                i += 1; // lifetime
            }
            continue;
        }
        // Cadena cruda r"..." / r#"..."# (y br)
        let ident_before = i > 0 && (b[i - 1].is_alphanumeric() || b[i - 1] == '_');
        let raw_start = if !ident_before && starts_with(i, "br") && matches!(b.get(i + 2), Some('"') | Some('#')) {
            Some(i + 2)
        } else if !ident_before && c == 'r' && matches!(b.get(i + 1), Some('"') | Some('#')) {
            Some(i + 1)
        } else {
            None
        };
        if let Some(mut j) = raw_start {
            let mut hashes = 0;
            while b.get(j) == Some(&'#') {
                hashes += 1;
                j += 1;
            }
            if b.get(j) == Some(&'"') {
                let start_line = line;
                j += 1;
                let mut s = String::new();
                while j < n {
                    if b[j] == '"' && (0..hashes).all(|k| b.get(j + 1 + k) == Some(&'#')) {
                        break;
                    }
                    if b[j] == '\n' {
                        line += 1;
                    }
                    s.push(b[j]);
                    j += 1;
                }
                out.push(Lit { line: start_line, text: s, in_test, arg_name: false });
                i = j + 1 + hashes;
                continue;
            }
        }
        // Cadena normal (y b"...")
        if c == '"' {
            let start_line = line;
            let before: String = b[i.saturating_sub(5)..i].iter().collect();
            let arg_name = before == ".arg(";
            let mut s = String::new();
            i += 1;
            while i < n && b[i] != '"' {
                if b[i] == '\\' && i + 1 < n {
                    match b[i + 1] {
                        'n' => s.push('\n'),
                        't' => s.push('\t'),
                        '\n' => {
                            // continuación de línea: salta espacios iniciales
                            line += 1;
                            i += 2;
                            while i < n && b[i].is_whitespace() {
                                if b[i] == '\n' {
                                    line += 1;
                                }
                                i += 1;
                            }
                            continue;
                        }
                        o => s.push(o),
                    }
                    i += 2;
                    continue;
                }
                if b[i] == '\n' {
                    line += 1;
                }
                s.push(b[i]);
                i += 1;
            }
            out.push(Lit { line: start_line, text: s, in_test, arg_name });
            i += 1;
            continue;
        }
        i += 1;
    }
    out
}

/// Archivos `.rs` bajo `dir` (recursivo).
pub fn rust_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else { return out };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            if p.file_name().is_some_and(|n| n == "target") {
                continue;
            }
            out.extend(rust_files(&p));
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
    out.sort();
    out
}

/// Claves literales usadas en el código de todos los crates (fuera de tests),
/// más las de `creditos.tsv`.
pub fn literal_keys() -> BTreeSet<String> {
    let root = workspace_root().join("crates");
    let mut keys = BTreeSet::new();
    for f in rust_files(&root) {
        let src = std::fs::read_to_string(&f).unwrap_or_default();
        for l in string_literals(&src) {
            if !l.in_test && looks_like_key(&l.text) {
                keys.insert(l.text);
            }
        }
    }
    let tsv = std::fs::read_to_string(root.join("surshape-app").join("assets").join("creditos.tsv")).unwrap_or_default();
    for line in tsv.lines().filter(|l| !l.starts_with('#') && !l.trim().is_empty()) {
        if let Some(k) = line.split('\t').nth(3) {
            keys.insert(k.trim().to_string());
        }
    }
    keys
}

/// Claves armadas en tiempo de ejecución a partir del catálogo de procesos.
pub fn dynamic_keys() -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    for p in surshape_native::registry().all() {
        keys.insert(name_key(p.id()));
        keys.insert(desc_key(p.id()));
        for s in p.params() {
            keys.insert(s.key.to_string());
            keys.insert(s.desc_key());
        }
    }
    // Procesos CDP: de la tabla (no hace falta CDP instalado).
    for d in &surshape_cdp::procs::DEFS {
        keys.insert(name_key(d.id));
        keys.insert(desc_key(d.id));
        for s in d.params {
            keys.insert(s.key.to_string());
            keys.insert(s.desc_key());
        }
    }
    keys
}

/// Quita los marcadores de formato `{...}` de una cadena.
pub fn strip_placeholders(s: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    for c in s.chars() {
        match c {
            '{' => depth += 1,
            '}' if depth > 0 => depth -= 1,
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

/// Literales visibles sospechosos en `surshape-app`: (archivo, línea, texto).
pub fn visible_literals(known: &BTreeSet<String>) -> Vec<(String, usize, String)> {
    let dir = workspace_root().join("crates").join("surshape-app").join("src");
    let mut bad = Vec::new();
    for f in rust_files(&dir) {
        let src = std::fs::read_to_string(&f).unwrap_or_default();
        // Archivos enteros de test (`#![cfg(test)]`): no se revisan.
        if src.contains("#![cfg(test)]") {
            continue;
        }
        let lines: Vec<&str> = src.lines().collect();
        for l in string_literals(&src) {
            if l.in_test || l.arg_name || known.contains(&l.text) {
                continue;
            }
            if !strip_placeholders(&l.text).chars().any(|c| c.is_alphabetic()) {
                continue;
            }
            if lines.get(l.line - 1).is_some_and(|s| s.contains("i18n-ok")) {
                continue;
            }
            let name = f.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            bad.push((name, l.line, l.text));
        }
    }
    bad
}

#[cfg(test)]
mod tests {
    use super::*;
    use surshape_i18n::{parse, EN_SRC, ES_SRC};

    fn keys(src: &str) -> Vec<String> {
        parse(src).entries.into_iter().map(|e| e.key).collect()
    }

    #[test]
    fn lexer_handles_tricky_code() {
        let src = "let a = \"uno\"; // \"no\"\nlet c = '\"'; let l: &'static str = r#\"dos \"x\"\"#;\n/* \"tres\" */ let d = \"a\\\"b\";";
        let t: Vec<String> = string_literals(src).into_iter().map(|l| l.text).collect();
        assert_eq!(t, vec!["uno", "dos \"x\"", "a\"b"]);
        let src = "fn a() { \"x\" }
#[cfg(test)]
mod t { fn b() { \"y\" } }
fn c() { \"z\" }";
        let flags: Vec<(String, bool)> = string_literals(src).into_iter().map(|l| (l.text, l.in_test)).collect();
        assert_eq!(flags, vec![("x".into(), false), ("y".into(), true), ("z".into(), false)]);
        assert_eq!(strip_placeholders("{v:.1} dB {}"), " dB ");
    }

    #[test]
    fn lang_files_are_well_formed() {
        for (name, src) in [("es", ES_SRC), ("en", EN_SRC)] {
            let p = parse(src);
            assert!(p.problems.is_empty(), "{name}.lang: {:?}", p.problems);
            for e in &p.entries {
                assert!(looks_like_key(&e.key), "{name}.lang:{}: clave fuera de las secciones: {}", e.line, e.key);
            }
        }
    }

    #[test]
    fn both_languages_have_the_same_keys_in_the_same_order() {
        let (es, en) = (keys(ES_SRC), keys(EN_SRC));
        let es_set: BTreeSet<_> = es.iter().collect();
        let en_set: BTreeSet<_> = en.iter().collect();
        let only_es: Vec<_> = es_set.difference(&en_set).collect();
        let only_en: Vec<_> = en_set.difference(&es_set).collect();
        assert!(only_es.is_empty(), "faltan en en.lang: {only_es:?}");
        assert!(only_en.is_empty(), "faltan en es.lang: {only_en:?}");
        let first_diff = es.iter().zip(&en).position(|(a, b)| a != b);
        assert!(first_diff.is_none(), "orden distinto desde la clave {:?}", first_diff.map(|i| &es[i]));
    }

    /// Argumentos `{nombre}` de un texto, ordenados.
    fn placeholders(s: &str) -> Vec<String> {
        let mut v: Vec<String> = s.split('{').skip(1).filter_map(|p| p.split_once('}').map(|(a, _)| a.to_string())).collect();
        v.sort();
        v.dedup();
        v
    }

    #[test]
    fn both_languages_use_the_same_arguments() {
        let es: std::collections::HashMap<String, String> = parse(ES_SRC).entries.into_iter().map(|e| (e.key, e.value)).collect();
        let mut bad = Vec::new();
        for e in parse(EN_SRC).entries {
            if let Some(v) = es.get(&e.key) {
                if placeholders(v) != placeholders(&e.value) {
                    bad.push(format!("{}: es {:?} / en {:?}", e.key, placeholders(v), placeholders(&e.value)));
                }
            }
        }
        assert!(bad.is_empty(), "argumentos distintos entre idiomas:\n{}", bad.join("\n"));
    }

    #[test]
    fn every_used_key_exists() {
        let defined: BTreeSet<String> = keys(ES_SRC).into_iter().collect();
        let used: BTreeSet<String> = literal_keys().union(&dynamic_keys()).cloned().collect();
        let missing: Vec<_> = used.difference(&defined).collect();
        assert!(missing.is_empty(), "claves usadas que no existen ({}):\n{}", missing.len(), missing.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("\n"));
    }

    #[test]
    fn no_orphan_keys() {
        let defined: BTreeSet<String> = keys(ES_SRC).into_iter().collect();
        let used: BTreeSet<String> = literal_keys().union(&dynamic_keys()).cloned().collect();
        let orphans: Vec<_> = defined.difference(&used).collect();
        assert!(orphans.is_empty(), "claves huérfanas ({}):\n{}", orphans.len(), orphans.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("\n"));
    }

    #[test]
    fn no_visible_literals_outside_i18n() {
        let known: BTreeSet<String> = keys(ES_SRC).into_iter().collect();
        let bad = visible_literals(&known);
        let list: Vec<String> = bad.iter().map(|(f, l, t)| format!("{f}:{l}: {t:?}")).collect();
        assert!(bad.is_empty(), "literales visibles fuera de i18n (usar t!() o marcar // i18n-ok):\n{}", list.join("\n"));
    }
}
