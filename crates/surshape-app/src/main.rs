//! SURSHAPE · aplicación · por @ahrsml
//!
//! Diseño sonoro offline con estructura de Soundshaper: menús, toolbar,
//! barra lateral, visor + transporte, grilla tipo planilla y páginas de
//! parámetros, Graph-Edit, mezcla y consola, con el look Win32 clásico.
//!
//! `--captura <carpeta>`: guarda capturas de la ventana (1280x800 y
//! 1920x1080, en español e inglés) y se cierra.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // i18n-ok

mod app;
mod bpedit;
mod capture;
mod console;
mod credits;
mod extras;
mod grid;
mod mainpage;
mod markers;
mod ops;
mod page;
mod player;
mod prefs;
mod shell;
mod shortcuts;
mod sidebar;
mod theme;
mod viewer;
mod widgets;
mod win32;
mod windows;

#[cfg(test)]
mod layout_tests;

use std::sync::Arc;
use surshape_audio::peaks::Peaks;
use surshape_audio::{Analysis, AudioBuf};
use surshape_i18n::t;

/// Pico, RMS y continua de un canal.
#[derive(Clone, Copy, Debug, Default)]
pub struct ChStats {
    pub peak: f32,
    pub rms: f32,
    pub dc: f64,
}

impl ChStats {
    pub fn of(x: &[f32]) -> Self {
        let n = x.len().max(1) as f64;
        let (mut peak, mut sq, mut sum) = (0.0f32, 0.0f64, 0.0f64);
        for &v in x {
            peak = peak.max(v.abs());
            sq += (v as f64) * (v as f64);
            sum += v as f64;
        }
        Self { peak, rms: (sq / n).sqrt() as f32, dc: sum / n }
    }
    fn db(v: f32) -> f64 {
        if v > 0.0 {
            20.0 * (v as f64).log10()
        } else {
            f64::NEG_INFINITY
        }
    }
    pub fn peak_db(&self) -> f64 {
        Self::db(self.peak)
    }
    pub fn rms_db(&self) -> f64 {
        Self::db(self.rms)
    }
}

/// Audio decodificado + lo necesario para dibujarlo.
#[derive(Clone)]
pub struct Loaded {
    pub audio: Arc<AudioBuf>,
    pub peaks: Arc<Peaks>,
    pub analysis: Analysis,
    /// Por canal.
    pub stats: Vec<ChStats>,
}

impl Loaded {
    pub fn new(audio: AudioBuf) -> Self {
        Self::from_arc(Arc::new(audio))
    }

    /// Igual, para audio que ya está compartido (sin copiarlo).
    pub fn from_arc(audio: Arc<AudioBuf>) -> Self {
        let peaks = Peaks::of(&audio);
        let analysis = Analysis::of(&audio);
        let stats = audio.channels.iter().map(|c| ChStats::of(c)).collect();
        Self { audio, peaks: Arc::new(peaks), analysis, stats }
    }
}

const APP_ID: &str = "surshape"; // i18n-ok

/// Ícono de la ventana, dibujado en código con la paleta: fondo azul acero,
/// una onda que se estira en celeste (64 x 64, esquinas rectas).
fn icon() -> eframe::egui::IconData {
    const N: usize = 64;
    let bg = theme::pal::ACERO_1;
    let fg = theme::pal::CELESTE_4;
    let mut rgba = Vec::with_capacity(N * N * 4);
    for y in 0..N {
        for x in 0..N {
            let fx = x as f32 / (N - 1) as f32;
            // Onda cuya amplitud crece y cuyo período se alarga hacia la derecha.
            let wave = 0.5 + 0.32 * (0.25 + 0.75 * fx) * (fx.powf(0.6) * 18.0).sin();
            let d = (y as f32 / (N - 1) as f32 - wave).abs();
            let c = if (5..N - 5).contains(&x) && d < 0.045 { fg } else { bg };
            rgba.extend_from_slice(&[c.r(), c.g(), c.b(), 255]);
        }
    }
    eframe::egui::IconData { rgba, width: N as u32, height: N as u32 }
}
const LOCALES_DIR: &str = "locales"; // i18n-ok

fn main() -> eframe::Result {
    let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_path_buf()));
    let overrides = surshape_i18n::init(exe_dir.as_ref().map(|d| d.join(LOCALES_DIR)).as_deref());
    let args: Vec<std::path::PathBuf> = std::env::args_os().skip(1).map(Into::into).collect();
    let capture_dir = args.iter().position(|a| a.as_os_str() == "--captura").map(|i| args.get(i + 1).cloned().unwrap_or_else(|| "capturas".into())); // i18n-ok
    let prefs = if capture_dir.is_some() { prefs::Prefs::default() } else { prefs::Prefs::load() };
    surshape_i18n::set_lang(prefs.lang());

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title(t!("ui.app.titulo"))
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([1024.0, 700.0])
            .with_drag_and_drop(true)
            .with_icon(Arc::new(icon())),
        ..Default::default()
    };
    eframe::run_native(
        APP_ID,
        options,
        Box::new(move |cc| {
            let app = match capture_dir {
                Some(dir) => capture::start(&cc.egui_ctx, prefs, exe_dir, overrides, dir),
                None => {
                    let mut app = app::App::new(cc, prefs, exe_dir, overrides);
                    // Archivos pasados al ejecutable ("Abrir con…" del explorador).
                    if !args.is_empty() {
                        app.import(args);
                    }
                    app
                }
            };
            Ok(Box::new(app))
        }),
    )
}
