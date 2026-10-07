//! Consola: registro global de ejecuciones (programas de CDP y procesos
//! nativos) con comando, salida, código, tiempo y archivos. La interfaz la
//! muestra en la página Consola.
//!
//! El hilo de render marca qué celda está calculando con [`set_cell`]; cada
//! ejecución queda asociada a esa celda.

use std::cell::RefCell;
use std::sync::Mutex;

/// Entradas que se conservan (las más viejas se descartan).
const MAX: usize = 500;
/// Máximo de texto de salida guardado por ejecución.
const MAX_TEXT: usize = 16 * 1024;

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    /// Número de ejecución (creciente).
    pub n: u64,
    /// Segundos desde 1970 (UTC).
    pub when: u64,
    /// Celda ("A_3"), o vacío si no hay.
    pub cell: String,
    /// Programa ("blur blur", "nat.stretch").
    pub program: String,
    /// Comando completo.
    pub command: String,
    /// Código de salida (None si es nativo o no terminó).
    pub code: Option<i32>,
    pub ok: bool,
    pub secs: f64,
    /// "entrada -> salida".
    pub files: String,
    /// stdout + stderr (recortado).
    pub output: String,
}

struct Log {
    next: u64,
    entries: Vec<Entry>,
}

static LOG: Mutex<Log> = Mutex::new(Log { next: 1, entries: Vec::new() });

thread_local! {
    static CELL: RefCell<String> = const { RefCell::new(String::new()) };
}

/// Celda que calcula este hilo (las ejecuciones siguientes quedan con ella).
pub fn set_cell(label: &str) {
    CELL.with(|c| *c.borrow_mut() = label.to_string());
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Registra una ejecución. Devuelve su número.
pub fn push(program: &str, command: &str, code: Option<i32>, ok: bool, secs: f64, files: &str, output: &str) -> u64 {
    let cell = CELL.with(|c| c.borrow().clone());
    let mut out = output.trim().to_string();
    if out.len() > MAX_TEXT {
        let cut = out.len() - MAX_TEXT;
        let cut = (cut..out.len()).find(|&i| out.is_char_boundary(i)).unwrap_or(cut);
        out.drain(..cut);
    }
    let Ok(mut log) = LOG.lock() else { return 0 };
    let n = log.next;
    log.next += 1;
    log.entries.push(Entry {
        n,
        when: now(),
        cell,
        program: program.to_string(),
        command: command.to_string(),
        code,
        ok,
        secs,
        files: files.to_string(),
        output: out,
    });
    if log.entries.len() > MAX {
        let extra = log.entries.len() - MAX;
        log.entries.drain(..extra);
    }
    n
}

/// Copia de todas las entradas.
pub fn entries() -> Vec<Entry> {
    LOG.lock().map(|l| l.entries.clone()).unwrap_or_default()
}

/// Número de la última entrada (0 si no hay): para saber si hay novedades.
pub fn last() -> u64 {
    LOG.lock().map(|l| l.entries.last().map_or(0, |e| e.n)).unwrap_or(0)
}

pub fn clear() {
    if let Ok(mut l) = LOG.lock() {
        l.entries.clear();
    }
}

/// Une argumentos en una línea de comando legible (con comillas si hace
/// falta).
pub fn join_args(program: &str, args: &[String]) -> String {
    let mut s = program.to_string();
    for a in args {
        s.push(' ');
        if a.is_empty() || a.contains(' ') {
            s.push('"');
            s.push_str(a);
            s.push('"');
        } else {
            s.push_str(a);
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_keeps_cell_and_order() {
        set_cell("A_2");
        let a = push("blur blur", "blur blur a b 3", Some(0), true, 0.1, "a -> b", "ok");
        let b = push("x", "x", None, false, 0.0, "", &"y".repeat(MAX_TEXT + 10));
        assert!(b > a);
        let e = entries();
        let ea = e.iter().find(|x| x.n == a).unwrap();
        assert_eq!(ea.cell, "A_2");
        assert!(e.iter().find(|x| x.n == b).unwrap().output.len() <= MAX_TEXT);
        assert_eq!(join_args("p", &["a b".into(), "c".into()]), "p \"a b\" c");
    }
}
