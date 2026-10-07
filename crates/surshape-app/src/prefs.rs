//! Preferencias del usuario, en `%APPDATA%\SURSHAPE\prefs.json`.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use surshape_audio::WavFormat;
use surshape_i18n::Lang;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")] // i18n-ok
pub enum ExportFormat {
    Int24,
    #[default]
    Float32,
}

impl ExportFormat {
    pub const ALL: [ExportFormat; 2] = [ExportFormat::Int24, ExportFormat::Float32];
    pub fn key(self) -> &'static str {
        match self {
            ExportFormat::Int24 => "ui.formato.wav24",
            ExportFormat::Float32 => "ui.formato.wav32f",
        }
    }
    pub fn wav(self) -> WavFormat {
        match self {
            ExportFormat::Int24 => WavFormat::Int24,
            ExportFormat::Float32 => WavFormat::Float32,
        }
    }
}

/// Unidades de tiempo en todo el programa.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")] // i18n-ok
pub enum TimeUnit {
    #[default]
    Segundos,
    Muestras,
    /// h:m:s.ms
    Hms,
}

impl TimeUnit {
    pub const ALL: [TimeUnit; 3] = [TimeUnit::Segundos, TimeUnit::Muestras, TimeUnit::Hms];
    pub fn key(self) -> &'static str {
        match self {
            TimeUnit::Segundos => "ui.unidades.segundos",
            TimeUnit::Muestras => "ui.unidades.muestras",
            TimeUnit::Hms => "ui.unidades.hms",
        }
    }
    /// Un tiempo en estas unidades.
    pub fn fmt(self, secs: f64, sr: u32) -> String {
        let s = secs.max(0.0);
        match self {
            TimeUnit::Segundos => format!("{s:.6}"),
            TimeUnit::Muestras => format!("{}", (s * sr as f64).round() as u64),
            TimeUnit::Hms => {
                let ms = (s.fract() * 1000.0).floor() as u64;
                let t = s.floor() as u64;
                format!("{}:{:02}:{:02}.{ms:03}", t / 3600, (t % 3600) / 60, t % 60)
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    /// Código de idioma ("es" / "en").
    pub idioma: String,
    pub formato_export: ExportFormat,
    /// Limitador al final de cada render (por defecto).
    pub limitador: bool,
    /// Techo del limitador en dBFS.
    pub techo_db: f64,
    /// Última sesión abierta (se reabre al iniciar).
    pub ultima_sesion: Option<PathBuf>,
    /// Carpeta donde se crean las sesiones nuevas.
    pub carpeta_sesiones: Option<PathBuf>,
    /// Carpeta de los programas de CDP elegida por el usuario (None = buscar
    /// junto al exe y en la instalación por defecto).
    pub carpeta_cdp: Option<PathBuf>,
    /// Carpeta temporal de trabajo (None = la del sistema).
    pub carpeta_temporal: Option<PathBuf>,
    /// Última carpeta usada al importar / exportar.
    pub ultima_carpeta: Option<PathBuf>,
    /// Archivos importados hace poco (el primero es el más reciente).
    pub recientes: Vec<PathBuf>,
    /// Análisis PVOC por defecto (auto-conversión).
    pub pvoc_puntos: u32,
    pub pvoc_superposicion: u32,
    /// Dispositivo de salida (None = el del sistema).
    pub dispositivo: Option<String>,
    pub unidades: TimeUnit,
    /// Tras RENDER en la página de parámetros, volver a la principal.
    pub volver_tras_render: bool,
    /// Zoom de la interfaz (1.0, 1.25, 1.5): accesibilidad.
    pub zoom: f32,
    /// Editor externo de sonido (p. ej. Audacity).
    pub editor_externo: Option<PathBuf>,
    /// Tamaños de los paneles (se recuerdan).
    pub ancho_lateral: f32,
    pub alto_visor: f32,
    /// Secciones de la barra lateral abiertas.
    pub secciones: [bool; 4],
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            idioma: Lang::Es.code().into(),
            formato_export: ExportFormat::Int24,
            limitador: true,
            techo_db: -0.3,
            ultima_sesion: None,
            carpeta_sesiones: None,
            carpeta_cdp: None,
            carpeta_temporal: None,
            ultima_carpeta: None,
            recientes: Vec::new(),
            pvoc_puntos: 1024,
            pvoc_superposicion: 3,
            dispositivo: None,
            unidades: TimeUnit::Segundos,
            volver_tras_render: true,
            zoom: 1.0,
            editor_externo: None,
            ancho_lateral: 230.0,
            alto_visor: 250.0,
            secciones: [true, true, false, false],
        }
    }
}

const APP_DIR: &str = "SURSHAPE"; // i18n-ok
const PREFS_FILE: &str = "prefs.json"; // i18n-ok
const SESSIONS_DIR: &str = "Sesiones"; // i18n-ok

/// Carpeta de configuración (`%APPDATA%\SURSHAPE`).
pub fn config_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join(APP_DIR))
}

/// Carpeta de recetas (`%APPDATA%\SURSHAPE\recetas`).
pub fn recipes_dir() -> Option<PathBuf> {
    config_dir().map(|d| d.join("recetas")) // i18n-ok
}

/// Carpeta de presets por proceso (`%APPDATA%\SURSHAPE\presets`).
pub fn presets_dir() -> Option<PathBuf> {
    config_dir().map(|d| d.join("presets")) // i18n-ok
}

/// Carpeta de patches de texto (`%APPDATA%\SURSHAPE\patches`).
pub fn patches_dir() -> Option<PathBuf> {
    config_dir().map(|d| d.join("patches")) // i18n-ok
}

fn prefs_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join(PREFS_FILE))
}

/// Si está activo, las preferencias no se escriben a disco (modo captura y
/// tests: nunca deben tocar las preferencias del usuario).
static NO_SAVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// No guardar preferencias en esta ejecución.
pub fn disable_saving() {
    NO_SAVE.store(true, std::sync::atomic::Ordering::Relaxed);
}

impl Prefs {
    pub fn load() -> Self {
        prefs_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(s.trim_start_matches('\u{feff}')).ok())
            .unwrap_or_default()
    }

    /// Guarda; si falla, devuelve el detalle (la app lo muestra como aviso).
    pub fn save(&self) -> Result<(), String> {
        if NO_SAVE.load(std::sync::atomic::Ordering::Relaxed) {
            return Ok(());
        }
        let p = prefs_path().ok_or_else(String::new)?;
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        let txt = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(p, txt).map_err(|e| e.to_string())
    }

    pub fn lang(&self) -> Lang {
        Lang::from_code(&self.idioma).unwrap_or_default()
    }

    /// Carpeta base de las sesiones (Documentos\SURSHAPE\Sesiones por defecto).
    pub fn sessions_dir(&self) -> PathBuf {
        self.carpeta_sesiones.clone().unwrap_or_else(|| {
            dirs::document_dir().or_else(dirs::home_dir).unwrap_or_else(std::env::temp_dir).join(APP_DIR).join(SESSIONS_DIR)
        })
    }

    /// Agrega un archivo a los recientes (máximo 10, sin repetir).
    pub fn add_recent(&mut self, p: PathBuf) {
        self.recientes.retain(|x| x != &p);
        self.recientes.insert(0, p);
        self.recientes.truncate(10);
    }

    /// Opciones de PVOC para el motor.
    pub fn pvoc(&self) -> surshape_engine::PvocSettings {
        surshape_engine::PvocSettings { puntos: self.pvoc_puntos.clamp(64, 8192), superposicion: self.pvoc_superposicion.clamp(1, 4) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_units() {
        assert_eq!(TimeUnit::Segundos.fmt(1.5, 48000), "1.500000");
        assert_eq!(TimeUnit::Muestras.fmt(1.5, 48000), "72000");
        assert_eq!(TimeUnit::Hms.fmt(83.456, 48000), "0:01:23.456");
    }

    #[test]
    fn old_prefs_still_load() {
        // Un prefs.json de la versión anterior (sin los campos nuevos), con BOM.
        let txt = "\u{feff}{\"idioma\":\"en\",\"limitador\":false}";
        let p: Prefs = serde_json::from_str(txt.trim_start_matches('\u{feff}')).unwrap();
        assert_eq!(p.idioma, "en");
        assert!(!p.limitador);
        assert_eq!(p.pvoc_puntos, 1024);
    }
}
