//! Tipos de archivo de CDP y conjuntos de archivos por canal.
//!
//! Los procesos nativos trabajan con audio en memoria ([`AudioBuf`]); los de
//! CDP trabajan con archivos. Una celda puede producir sonido (.wav) o datos
//! de CDP: análisis espectral (.ana), pitch (.frq), formantes (.for),
//! envolvente (.env), breakpoints (.brk) o texto (.txt). Como CDP es mono,
//! los datos van **un archivo por canal** ([`DataSet`]).
//!
//! [`AudioBuf`]: surshape_audio::AudioBuf

use std::path::PathBuf;

/// Tipo de archivo de una entrada o salida.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FileKind {
    Wav,
    Ana,
    Frq,
    For,
    Env,
    Brk,
    Txt,
}

impl FileKind {
    pub const ALL: [FileKind; 7] =
        [FileKind::Wav, FileKind::Ana, FileKind::Frq, FileKind::For, FileKind::Env, FileKind::Brk, FileKind::Txt];

    /// Extensión (también es el identificador que se guarda).
    pub fn ext(self) -> &'static str {
        match self {
            FileKind::Wav => "wav",
            FileKind::Ana => "ana",
            FileKind::Frq => "frq",
            FileKind::For => "for",
            FileKind::Env => "env",
            FileKind::Brk => "brk",
            FileKind::Txt => "txt",
        }
    }

    pub fn from_ext(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.ext().eq_ignore_ascii_case(s))
    }

    /// Clave i18n del nombre largo ("análisis espectral"...).
    pub fn key(self) -> &'static str {
        match self {
            FileKind::Wav => "ui.tipo.wav",
            FileKind::Ana => "ui.tipo.ana",
            FileKind::Frq => "ui.tipo.frq",
            FileKind::For => "ui.tipo.for",
            FileKind::Env => "ui.tipo.env",
            FileKind::Brk => "ui.tipo.brk",
            FileKind::Txt => "ui.tipo.txt",
        }
    }

    /// ¿Es sonido (reproducible tal cual)?
    pub fn is_audio(self) -> bool {
        self == FileKind::Wav
    }
}

/// Archivos de un mismo tipo, uno por canal (o uno solo si el tipo es
/// sonido multicanal escrito por SURSHAPE).
#[derive(Clone, Debug, PartialEq)]
pub struct DataSet {
    pub kind: FileKind,
    pub files: Vec<PathBuf>,
    /// Frecuencia de muestreo del sonido de origen.
    pub sr: u32,
    /// Duración del sonido de origen, en segundos.
    pub dur: f64,
}

impl DataSet {
    pub fn channels(&self) -> usize {
        self.files.len()
    }
}
