//! SURSHAPE · sesión · por @ahrsml
//!
//! Una sesión es una carpeta:
//!
//! ```text
//! <sesión>/
//!   sesion.json            el patch (grafo + grilla), versión del formato
//!   cache/<clave>_<n>.wav  salida n del render con esa clave (32 float)
//!   cache/<clave>.json     datos del render (salidas, avisos, tiempo)
//! ```
//!
//! Las fuentes se referencian por ruta y nunca se modifican. `sesion.json` se
//! escribe de forma atómica (archivo temporal + renombrar): eso es el
//! autosave. La cache se indexa por la clave del nodo ([`surshape_patch`]):
//! si una clave ya tiene su `.json`, el render no se repite.
//!
//! [`runner`] ejecuta un plan de render (varios nodos en orden) en un hilo
//! de trabajo, leyendo y escribiendo la cache.

pub mod presets;
pub mod recipes;
pub mod runner;

use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use surshape_audio::{export, AudioBuf, WavFormat};
use surshape_engine::{DataSet, FileKind, ProcessError, StableHasher};
use surshape_patch::{Notice, OutputInfo, Patch, RenderRecord};

pub const SESSION_FILE: &str = "sesion.json";
pub const CACHE_DIR: &str = "cache";
/// Versión del formato de `sesion.json` (1 = árbol de linaje, ya no se lee).
pub const FORMAT_VERSION: u32 = 2;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub version: u32,
    #[serde(skip)]
    pub dir: PathBuf,
    pub patch: Patch,
}

/// Error con clave i18n.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionError {
    pub key: &'static str,
    pub detalle: String,
}

fn err(key: &'static str, e: impl ToString) -> SessionError {
    SessionError { key, detalle: e.to_string() }
}

impl From<SessionError> for ProcessError {
    fn from(e: SessionError) -> Self {
        ProcessError::new(e.key).arg("detalle", e.detalle)
    }
}

pub fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Escribe `txt` en `path` de forma atómica.
fn write_atomic(path: &Path, txt: &str) -> std::io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, txt)?;
    std::fs::rename(&tmp, path)
}

impl Session {
    /// Crea una sesión nueva en `dir`.
    pub fn create(dir: &Path) -> Result<Self, SessionError> {
        std::fs::create_dir_all(dir.join(CACHE_DIR)).map_err(|e| err("err.sesion.crear", e))?;
        let s = Self { version: FORMAT_VERSION, dir: dir.to_path_buf(), patch: Patch::default() };
        s.save()?;
        Ok(s)
    }

    pub fn open(dir: &Path) -> Result<Self, SessionError> {
        let txt = std::fs::read_to_string(dir.join(SESSION_FILE)).map_err(|e| err("err.sesion.leer", e))?;
        let v: serde_json::Value = serde_json::from_str(&txt).map_err(|e| err("err.sesion.leer", e))?;
        if v.get("version").and_then(|x| x.as_u64()) != Some(FORMAT_VERSION as u64) {
            return Err(err("err.sesion.version", v.get("version").map(|x| x.to_string()).unwrap_or_default()));
        }
        let mut s: Session = serde_json::from_value(v).map_err(|e| err("err.sesion.leer", e))?;
        s.dir = dir.to_path_buf();
        let _ = std::fs::create_dir_all(s.dir.join(CACHE_DIR));
        Ok(s)
    }

    /// Guarda `sesion.json` de forma atómica.
    pub fn save(&self) -> Result<(), SessionError> {
        let txt = serde_json::to_string_pretty(self).map_err(|e| err("err.sesion.guardar", e))?;
        write_atomic(&self.dir.join(SESSION_FILE), &txt).map_err(|e| err("err.sesion.guardar", e))
    }

    /// Ruta absoluta de un archivo guardado con ruta relativa a la sesión.
    pub fn abs(&self, rel: &Path) -> PathBuf {
        if rel.is_absolute() {
            rel.to_path_buf()
        } else {
            self.dir.join(rel)
        }
    }
}

// --- Cache en disco -----------------------------------------------------------------

/// Ruta relativa (a la sesión) de la salida `n` de la clave `key`.
pub fn cache_rel(key: &str, n: usize) -> PathBuf {
    Path::new(CACHE_DIR).join(format!("{key}_{n}.wav"))
}

fn cache_meta(dir: &Path, key: &str) -> PathBuf {
    dir.join(CACHE_DIR).join(format!("{key}.json"))
}

/// Ruta relativa de un archivo de datos: canal `ch` de la salida `n`.
pub fn cache_data_rel(key: &str, n: usize, ch: usize, kind: FileKind) -> PathBuf {
    Path::new(CACHE_DIR).join(format!("{key}_{n}_c{ch}.{}", kind.ext()))
}

/// Render guardado para `key`, si está completo (datos y todos los archivos).
pub fn cache_lookup(dir: &Path, key: &str) -> Option<RenderRecord> {
    let txt = std::fs::read_to_string(cache_meta(dir, key)).ok()?;
    let rec: RenderRecord = serde_json::from_str(&txt).ok()?;
    let complete = rec.salidas.iter().all(|o| dir.join(&o.archivo).is_file() && o.datos.iter().all(|d| dir.join(d).is_file()));
    (rec.clave == key && complete).then_some(rec)
}

/// Una salida para guardar: su sonido y, si la salida son datos, sus
/// archivos (ya escritos en la cache, rutas absolutas).
pub struct StoredOut {
    pub audio: AudioBuf,
    pub data: Option<DataSet>,
}

/// Guarda las salidas de un render en la cache. El `.json` se escribe al
/// final: una entrada sin `.json` está incompleta y no se usa.
pub fn cache_store(
    dir: &Path,
    key: &str,
    outputs: &[AudioBuf],
    avisos: Vec<Notice>,
    segundos: f64,
    version_motor: Option<String>,
) -> Result<RenderRecord, SessionError> {
    let outs: Vec<StoredOut> = outputs.iter().map(|a| StoredOut { audio: a.clone(), data: None }).collect();
    cache_store_outs(dir, key, &outs, avisos, segundos, version_motor)
}

/// Como [`cache_store`], con salidas que pueden ser datos (.ana...).
pub fn cache_store_outs(
    dir: &Path,
    key: &str,
    outputs: &[StoredOut],
    avisos: Vec<Notice>,
    segundos: f64,
    version_motor: Option<String>,
) -> Result<RenderRecord, SessionError> {
    std::fs::create_dir_all(dir.join(CACHE_DIR)).map_err(|e| err("err.sesion.guardar", e))?;
    let mut salidas = Vec::with_capacity(outputs.len());
    for (n, o) in outputs.iter().enumerate() {
        let a = &o.audio;
        let rel = cache_rel(key, n);
        export::write_wav(&dir.join(&rel), a, WavFormat::Float32).map_err(|e| err(e.key, e))?;
        let (tipo, datos) = match &o.data {
            Some(d) => (
                Some(d.kind.ext().to_string()),
                d.files.iter().map(|f| f.strip_prefix(dir).map(Path::to_path_buf).unwrap_or_else(|_| f.clone())).collect(),
            ),
            None => (None, Vec::new()),
        };
        salidas.push(OutputInfo {
            archivo: rel,
            sr: a.sr,
            canales: a.num_channels() as u16,
            frames: a.frames() as u64,
            tipo,
            datos,
        });
    }
    let rec = RenderRecord { clave: key.to_string(), salidas, avisos, segundos, version_motor };
    let txt = serde_json::to_string_pretty(&rec).map_err(|e| err("err.sesion.guardar", e))?;
    write_atomic(&cache_meta(dir, key), &txt).map_err(|e| err("err.sesion.guardar", e))?;
    Ok(rec)
}

/// Registra en la cache, con otra clave, un render que ya existe (los
/// archivos se comparten). Lo usan los sub-patches: su resultado es el del
/// último paso.
pub fn cache_alias(dir: &Path, key: &str, rec: &RenderRecord) -> Result<RenderRecord, SessionError> {
    let mut r = rec.clone();
    r.clave = key.to_string();
    let txt = serde_json::to_string_pretty(&r).map_err(|e| err("err.sesion.guardar", e))?;
    write_atomic(&cache_meta(dir, key), &txt).map_err(|e| err("err.sesion.guardar", e))?;
    Ok(r)
}

/// Tamaño y fecha de modificación (segundos) de un archivo.
pub fn file_stamp(path: &Path) -> Option<(u64, u64)> {
    let m = std::fs::metadata(path).ok()?;
    let t = m.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    Some((m.len(), t))
}

/// Hash del contenido de un archivo (para identificar fuentes).
pub fn hash_file(path: &Path, progress: Option<&AtomicU32>, cancel: Option<&AtomicBool>) -> Result<String, ProcessError> {
    let detail = |e: std::io::Error| ProcessError::new("err.audio.abrir").arg("archivo", path.display()).arg("detalle", e);
    let total = std::fs::metadata(path).map_err(detail)?.len().max(1);
    let mut f = std::fs::File::open(path).map_err(detail)?;
    let mut h = StableHasher::new();
    let mut buf = vec![0u8; 1 << 20];
    let mut done = 0u64;
    loop {
        if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
            return Err(ProcessError::Cancelled);
        }
        let n = f.read(&mut buf).map_err(detail)?;
        if n == 0 {
            break;
        }
        h.bytes(&buf[..n]);
        done += n as u64;
        if let Some(p) = progress {
            p.store((done as f64 / total as f64 * 1_000_000.0) as u32, Ordering::Relaxed);
        }
    }
    Ok(h.finish_hex())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::path::PathBuf;
    use surshape_patch::SourceInfo;

    pub(crate) fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("surshape_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn create_save_reopen_and_reject_old_format() {
        let dir = tmp("ses");
        let mut s = Session::create(&dir).unwrap();
        s.patch.add_source(SourceInfo {
            ruta: dir.join("x.wav"),
            hash: "h".into(),
            sr: 48000,
            canales: 1,
            frames: 10,
            bytes: 0,
            modificado: 0,
            marcadores: vec![],
        });
        s.save().unwrap();
        let r = Session::open(&dir).unwrap();
        assert_eq!(r.patch, s.patch);
        std::fs::write(dir.join(SESSION_FILE), r#"{"version":1,"nodos":[]}"#).unwrap();
        assert_eq!(Session::open(&dir).unwrap_err().key, "err.sesion.version");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cache_store_and_lookup() {
        let dir = tmp("cache");
        std::fs::create_dir_all(&dir).unwrap();
        assert!(cache_lookup(&dir, "abc").is_none());
        let a = AudioBuf::from_channels(8000, vec![vec![0.1, 0.2, 0.3]]);
        let rec = cache_store(&dir, "abc", &[a.clone(), a], vec![Notice::new("ui.informe.clip", &[("pico", "1.0".into())])], 0.5, None).unwrap();
        assert_eq!(rec.salidas.len(), 2);
        assert_eq!(cache_lookup(&dir, "abc"), Some(rec.clone()));
        // Si falta un WAV, la entrada no vale.
        std::fs::remove_file(dir.join(&rec.salidas[1].archivo)).unwrap();
        assert!(cache_lookup(&dir, "abc").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn file_hash_changes_with_content() {
        let dir = tmp("hash");
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("a.bin");
        std::fs::write(&f, b"uno").unwrap();
        let h1 = hash_file(&f, None, None).unwrap();
        std::fs::write(&f, b"dos").unwrap();
        assert_ne!(h1, hash_file(&f, None, None).unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
