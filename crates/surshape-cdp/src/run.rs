//! Ejecución de un programa de CDP como proceso externo.
//!
//! - stdin nulo (algunos programas piden datos por teclado y se colgarían);
//! - sin ventana de consola (SURSHAPE es una app gráfica);
//! - progreso leído de stdout ("N min S sec" = segundos procesados);
//! - cancelación: se mata el proceso;
//! - errores: las líneas `ERROR:` se clasifican en claves i18n; si no se
//!   reconocen, el texto original de CDP va como detalle.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use surshape_engine::{console, ProcessError};

/// Archivos de una línea de comando, "entradas -> salida", para la consola.
fn files_of(args: &[String]) -> String {
    const EXT: [&str; 6] = [".wav", ".ana", ".frq", ".for", ".env", ".txt"];
    let f: Vec<&str> = args.iter().map(String::as_str).filter(|a| EXT.iter().any(|e| a.ends_with(e))).collect();
    match f.split_last() {
        Some((out, ins)) if !ins.is_empty() => format!("{} -> {out}", ins.join(", ")),
        Some((only, _)) => only.to_string(),
        None => String::new(),
    }
}

/// Segundos procesados según la última marca "N min S sec" del texto.
pub fn parse_progress_secs(out: &str) -> Option<f64> {
    let i = out.rfind(" sec")?;
    let head = &out[..i];
    let s_start = head.rfind(|c: char| !(c.is_ascii_digit() || c == '.'))? + 1;
    let secs: f64 = head[s_start..].parse().ok()?;
    let before = head[..s_start].trim_end();
    let before = before.strip_suffix("min")?.trim_end();
    let m_start = before.rfind(|c: char| !c.is_ascii_digit()).map_or(0, |p| p + 1);
    let mins: f64 = before[m_start..].parse().ok()?;
    Some(mins * 60.0 + secs)
}

/// Quita las marcas de progreso "N min S sec" (CDP las escribe pegadas, sin
/// saltos de línea) y las líneas vacías.
pub fn strip_progress(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let b = text.as_bytes();
    let mut i = 0;
    while i < b.len() {
        // ¿Empieza aquí "<dígitos> min <espacios><número> sec"?
        let mut j = i;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        if j > i && text[j..].starts_with(" min") {
            let mut k = j + 4;
            while k < b.len() && b[k] == b' ' {
                k += 1;
            }
            let n0 = k;
            while k < b.len() && (b[k].is_ascii_digit() || b[k] == b'.') {
                k += 1;
            }
            if k > n0 && text[k..].starts_with(" sec") {
                i = k + 4;
                continue;
            }
        }
        let ch = text[i..].chars().next().unwrap_or(' ');
        out.push(ch);
        i += ch.len_utf8();
    }
    out.lines().map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join("
")
}

/// Convierte la salida de error de CDP en un [`ProcessError`] con clave i18n.
pub fn classify_error(program: &str, text: &str) -> ProcessError {
    let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let errors: Vec<&str> = lines.iter().filter_map(|l| l.strip_prefix("ERROR:")).map(str::trim).collect();
    let detail = if errors.is_empty() { lines.last().copied().unwrap_or_default().to_string() } else { errors.join(" · ") };
    // "Parameter[1] Value (99999.0) out of range (1.0 to 698.0)" o
    // "Value (-10.0) out of range (0.001 to 1000.0) in brkpntfile e.brk."
    if let Some(e) = errors.iter().find(|e| e.contains("out of range")) {
        let between = |s: &str, open: &str| -> Option<String> {
            let i = s.find(open)? + open.len();
            let j = s[i..].find(')')? + i;
            Some(s[i..j].trim().to_string())
        };
        let num = |s: &str| s.trim().parse::<f64>().ok().map(|x| format!("{x}"));
        let value = between(e, "Value (").and_then(|v| num(&v));
        let range = between(e, "range (");
        let bounds = range.as_deref().and_then(|r| r.split_once(" to ")).and_then(|(a, b)| Some((num(a)?, num(b)?)));
        if let (Some(value), Some((min, max))) = (value, bounds) {
            let key = if e.contains("brkpntfile") { "err.cdp.rango_brk" } else { "err.cdp.rango" };
            return ProcessError::new(key).arg("programa", program).arg("valor", value).arg("min", min).arg("max", max);
        }
    }
    let lower = detail.to_ascii_lowercase();
    let key = if lower.contains("can't open file") || lower.contains("cannot open file") {
        "err.cdp.abrir_entrada"
    } else if lower.contains("cannot open output file") {
        "err.cdp.salida"
    } else if lower.contains("doesn't work with this type of infile") {
        "err.cdp.tipo_archivo"
    } else {
        "err.cdp.fallo"
    };
    ProcessError::new(key).arg("programa", program).arg("detalle", detail)
}

fn reader(mut r: impl Read + Send + 'static, buf: Arc<Mutex<String>>) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut chunk = [0u8; 4096];
        while let Ok(n) = r.read(&mut chunk) {
            if n == 0 {
                break;
            }
            if let Ok(mut b) = buf.lock() {
                b.push_str(&String::from_utf8_lossy(&chunk[..n]));
                // Solo interesa el final (progreso y errores).
                if b.len() > 64 * 1024 {
                    let cut = b.len() - 16 * 1024;
                    let cut = (cut..b.len()).find(|&i| b.is_char_boundary(i)).unwrap_or(cut);
                    b.drain(..cut);
                }
            }
        }
    })
}

/// Corre `exe args...` en `cwd`. `on_progress(segundos_procesados)` se llama
/// mientras corre. Devuelve stdout+stderr si terminó bien.
pub fn run(
    exe: &Path,
    args: &[String],
    cwd: &Path,
    cancel: &AtomicBool,
    mut on_progress: impl FnMut(f64),
) -> Result<String, ProcessError> {
    let program = exe.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let label = format!("{program} {}", args.first().map(String::as_str).unwrap_or(""));
    let mut cmd = Command::new(exe);
    cmd.args(args).current_dir(cwd).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let t0 = std::time::Instant::now();
    let command = console::join_args(&program, args);
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            console::push(&label, &command, None, false, 0.0, &files_of(args), &e.to_string());
            return Err(ProcessError::new("err.cdp.lanzar").arg("programa", &label).arg("detalle", e));
        }
    };
    let out = Arc::new(Mutex::new(String::new()));
    let err = Arc::new(Mutex::new(String::new()));
    let t1 = reader(child.stdout.take().expect("stdout"), out.clone());
    let t2 = reader(child.stderr.take().expect("stderr"), err.clone());
    let status = loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(ProcessError::Cancelled);
        }
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) => {
                if let Some(secs) = out.lock().ok().and_then(|o| parse_progress_secs(&o)) {
                    on_progress(secs);
                }
                std::thread::sleep(Duration::from_millis(15));
            }
            Err(e) => return Err(ProcessError::new("err.cdp.lanzar").arg("programa", &label).arg("detalle", e)),
        }
    };
    let _ = (t1.join(), t2.join());
    let text = format!("{}\n{}", out.lock().map(|s| s.clone()).unwrap_or_default(), err.lock().map(|s| s.clone()).unwrap_or_default());
    let ok = status.success() && !text.contains("ERROR:");
    // Las marcas de progreso ("0 min 1.25 sec") no le sirven a la consola.
    let shown = strip_progress(&text);
    // La lectura de la versión de CDP no es una ejecución del usuario.
    if args.first().map(String::as_str) != Some("--version") {
        console::push(&label, &command, status.code(), ok, t0.elapsed().as_secs_f64(), &files_of(args), &shown);
    }
    if !ok {
        return Err(classify_error(&label, &text));
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_and_errors_are_parsed() {
        assert_eq!(parse_progress_secs("0 min  0.05 sec0 min  1.50 sec"), Some(1.5));
        assert_eq!(parse_progress_secs("2 min  3.25 sec"), Some(123.25));
        assert_eq!(parse_progress_secs("nada"), None);
        assert_eq!(strip_progress("0 min  0.00 sec0 min  5.46 secWARNING: x

End 1 min 2 sec"), "WARNING: x
End");
        let e = classify_error("blur blur", "ERROR: INCORRECT USE\nERROR: Parameter[1] Value (99999.000000) out of range (1.000000 to 698.000000)\n");
        let (k, a) = e.message();
        assert_eq!(k, "err.cdp.rango");
        assert!(a.contains(&("max", "698".to_string())), "{a:?}");
        assert_eq!(classify_error("x", "ERROR: INVALID DATA\nERROR: Can't open file nada.wav to read data.").message().0, "err.cdp.abrir_entrada");
        assert_eq!(classify_error("x", "Application doesn't work with this type of infile.").message().0, "err.cdp.tipo_archivo");
        assert_eq!(classify_error("x", "ERROR: algo raro").message().0, "err.cdp.fallo");
    }
}
