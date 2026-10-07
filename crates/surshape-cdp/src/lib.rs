//! SURSHAPE · wrapper CDP · por @ahrsml
//!
//! CDP (Composers Desktop Project, LGPL-2.1) se invoca SIEMPRE como procesos
//! externos: nunca se linkea.
//!
//! - [`CdpInstall`]: busca los programas (carpeta `cdp/` junto al exe de
//!   SURSHAPE, la configurada en preferencias o la instalación por defecto)
//!   y lee la versión.
//! - [`run`]: ejecuta un programa con progreso, cancelación y errores
//!   traducibles.
//! - [`brk`]: parámetros con breakpoints -> archivos de texto de CDP.
//! - [`procs`]: el subconjunto curado y su registro en el catálogo.
//!
//! Si un programa falta o falla, la app sigue funcionando: el proceso no se
//! registra (o su celda muestra el error) y el resto funciona igual.

pub mod brk;
pub mod procs;
pub mod run;

use std::path::{Path, PathBuf};

/// Carpeta de CDP instalada en la máquina de desarrollo.
pub const DEFAULT_DIR: &str = r"C:\CDPR8\_cdp\_cdprogs";

/// Programas que usa el subconjunto curado.
pub const CORE_PROGRAMS: &[&str] =
    &["pvoc", "blur", "focus", "hilite", "stretch", "distort", "extend", "combine", "morph", "strange", "spec", "modify", "filter"];

/// Programas que usan Info, la separación por canal y el script .bat.
pub const TOOL_PROGRAMS: &[&str] = &["sndinfo", "housekeep", "submix"];

/// Instalación de CDP encontrada.
#[derive(Clone, Debug, PartialEq)]
pub struct CdpInstall {
    pub dir: PathBuf,
    /// Nombres de los .exe presentes (sin extensión, en minúsculas).
    pub programs: Vec<String>,
    /// Versión informada por `blur --version` (p. ej. "7.1.0").
    pub version: Option<String>,
}

impl CdpInstall {
    /// Busca los programas en `dir`. None si la carpeta no existe o no tiene
    /// ningún .exe.
    pub fn discover(dir: &Path) -> Option<Self> {
        let mut programs: Vec<String> = std::fs::read_dir(dir)
            .ok()?
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let p = e.path();
                let is_exe = p.extension().and_then(|x| x.to_str()).is_some_and(|x| x.eq_ignore_ascii_case("exe"));
                if is_exe {
                    p.file_stem().and_then(|s| s.to_str()).map(|s| s.to_ascii_lowercase())
                } else {
                    None
                }
            })
            .collect();
        if programs.is_empty() {
            return None;
        }
        programs.sort();
        let mut inst = Self { dir: dir.to_path_buf(), programs, version: None };
        inst.version = inst.read_version();
        Some(inst)
    }

    /// Busca en orden: carpeta elegida por el usuario, `cdp/` junto al exe
    /// de SURSHAPE (distribución) y la instalación por defecto.
    pub fn find(custom: Option<&Path>, exe_dir: Option<&Path>) -> Option<Self> {
        custom
            .and_then(Self::discover)
            .or_else(|| exe_dir.map(|d| d.join("cdp")).and_then(|d| Self::discover(&d)))
            .or_else(|| Self::discover(Path::new(DEFAULT_DIR)))
    }

    pub fn has(&self, program: &str) -> bool {
        self.programs.binary_search(&program.to_ascii_lowercase()).is_ok()
    }

    pub fn path_of(&self, program: &str) -> PathBuf {
        self.dir.join(format!("{program}.exe"))
    }

    /// Programas del subconjunto curado que faltan.
    pub fn missing_core(&self) -> Vec<&'static str> {
        CORE_PROGRAMS.iter().copied().filter(|p| !self.has(p)).collect()
    }

    fn read_version(&self) -> Option<String> {
        if !self.has("blur") {
            return None;
        }
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let out = run::run(&self.path_of("blur"), &["--version".to_string()], &self.dir, &cancel, |_| {}).ok()?;
        let v = out.lines().map(str::trim).find(|l| !l.is_empty())?.to_string();
        // Compilación propia (tools/build_cdp.ps1): COMMIT.txt dice de qué
        // commit de CDP8 sale; va en la versión para distinguirla.
        let commit = std::fs::read_to_string(self.dir.join("COMMIT.txt"))
            .ok()
            .and_then(|t| t.split_whitespace().skip_while(|w| *w != "commit").nth(1).map(|c| c.chars().take(7).collect::<String>()));
        Some(match commit {
            Some(c) => format!("{v} (CDP8 {c})"),
            None => v,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_dir_is_none() {
        assert!(CdpInstall::discover(Path::new(r"C:\no\existe\cdp")).is_none());
    }

    /// Solo en la máquina de desarrollo: no falla si CDP no está.
    #[test]
    fn discover_default_if_present() {
        if let Some(c) = CdpInstall::discover(Path::new(DEFAULT_DIR)) {
            assert!(c.programs.len() > 100);
            assert!(c.missing_core().is_empty(), "faltan: {:?}", c.missing_core());
            assert!(c.version.as_deref().is_some_and(|v| v.chars().next().is_some_and(|ch| ch.is_ascii_digit())), "{:?}", c.version);
        }
    }
}
