//! Tema Win32 clásico y tipografía. Todos los colores de la interfaz salen
//! de aquí, con nombres semánticos. Reglas (ver CLAUDE.md):
//!   - paleta "Crepúsculo de invierno": azules fríos y lavandas, oscuros de
//!     fondo y claros para texto, mapeada a los colores del sistema de
//!     Windows 2000/XP (ButtonFace, sombras, campos, selección, títulos);
//!   - prohibidos el negro y el rojo; el piso es #142A4E (azul marino), para
//!     que SURSHAPE no se confunda con NOISEGEK;
//!   - todo texto con contraste WCAG AA (>= 4,5:1) contra su fondo: lo
//!     verifica el test `contraste_text_pairs_meet_wcag_aa` (y lo imprime con
//!     `cargo test -p surshape-app contraste -- --nocapture`).
//!
//! Tipografía: Tahoma (o Segoe UI) leída en tiempo de ejecución de
//! `C:\Windows\Fonts` (no se distribuye); si no está, IBM Plex Sans (OFL,
//! embebida). Courier Prime solo en lecturas del visor, línea de comando y
//! consola.

use eframe::egui::{
    self, epaint::Shadow, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle,
};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

/// Paleta "Crepúsculo de invierno" (cielo invernal de Los Ríos). Solo estos
/// valores; el test `paleta_solo_valores_permitidos` lo exige.
#[allow(dead_code)]
pub mod pal {
    use eframe::egui::Color32;
    const fn c(hex: u32) -> Color32 {
        Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
    }
    // Base del usuario
    pub const AZUL_1: Color32 = c(0x29559C);
    pub const AZUL_2: Color32 = c(0x376BBF);
    pub const AZUL_3: Color32 = c(0x748FD2);
    pub const LAVANDA_1: Color32 = c(0x9D96C0);
    pub const LAVANDA_2: Color32 = c(0x9397C7);
    // Escala oscura: #29559C con negro al 15/25/35/42/50 %. Piso: NOCHE_50.
    pub const NOCHE_15: Color32 = c(0x234885);
    pub const NOCHE_25: Color32 = c(0x1F4075);
    pub const NOCHE_35: Color32 = c(0x1B3765);
    pub const NOCHE_42: Color32 = c(0x18315A);
    pub const NOCHE_50: Color32 = c(0x142A4E);
    // Claros para texto (base con blanco)
    pub const NIEVE: Color32 = c(0xEAEEF8);
    pub const NIEVE_LAVANDA: Color32 = c(0xD4D5E9);
    pub const NIEVE_AZUL: Color32 = c(0xC7D2ED);

    /// Todos los valores permitidos.
    pub const TODOS: [Color32; 13] = [
        AZUL_1, AZUL_2, AZUL_3, LAVANDA_1, LAVANDA_2, NOCHE_15, NOCHE_25, NOCHE_35, NOCHE_42, NOCHE_50, NIEVE,
        NIEVE_LAVANDA, NIEVE_AZUL,
    ];
}

/// Paleta clásica clara (acero/celeste/turquesa), hasta la 0.9.0. Se conserva
/// para la opción "Tema: Claro (clásico)" de Preferencias.
#[allow(dead_code)]
pub mod pal_clasico {
    use eframe::egui::Color32;
    const fn c(hex: u32) -> Color32 {
        Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
    }
    pub const ACERO_1: Color32 = c(0x2887AC);
    pub const ACERO_2: Color32 = c(0x69A3C1);
    pub const ACERO_3: Color32 = c(0x93B9D2);
    pub const ACERO_4: Color32 = c(0xC5D8E5);
    pub const CELESTE_1: Color32 = c(0x47D8F9);
    pub const CELESTE_2: Color32 = c(0x70E1FE);
    pub const CELESTE_3: Color32 = c(0x9DEAFE);
    pub const CELESTE_4: Color32 = c(0xCFF6FE);
    pub const TURQUESA_1: Color32 = c(0x2DC9C8);
    pub const TURQUESA_2: Color32 = c(0x88E1DE);
    pub const TURQUESA_3: Color32 = c(0xB4ECE9);
    pub const TURQUESA_4: Color32 = c(0xDCF6F5);
    pub const CIELO_1: Color32 = c(0x04B5E9);
    pub const CIELO_2: Color32 = c(0x46C5EF);
    pub const CIELO_3: Color32 = c(0x83D1F5);
    pub const CIELO_4: Color32 = c(0xC1E9FC);
    pub const MARINO: Color32 = c(0x16323F);
    pub const MARINO_SUAVE: Color32 = c(0x2B5468);
    pub const AVISO: Color32 = c(0x0B4A78);
    pub const ERROR: Color32 = c(0x082F66);
    pub const BLANCO: Color32 = c(0xFFFFFF);
}

// --- Temas ---------------------------------------------------------------------------

/// Tema elegido en Preferencias → Accesibilidad. Los tests y el modo captura
/// usan Crepúsculo (el valor por defecto).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ThemeKind {
    #[default]
    Crepusculo,
    Clasico,
}

impl ThemeKind {
    pub const ALL: [ThemeKind; 2] = [ThemeKind::Crepusculo, ThemeKind::Clasico];
    pub fn key(self) -> &'static str {
        match self {
            ThemeKind::Crepusculo => "ui.prefs.tema_crepusculo",
            ThemeKind::Clasico => "ui.prefs.tema_clasico",
        }
    }
}

static KIND: AtomicU8 = AtomicU8::new(0);

/// Cambia el tema activo (luego hay que llamar a [`apply`]).
pub fn set(kind: ThemeKind) {
    KIND.store(kind as u8, Ordering::Relaxed);
}

pub fn kind() -> ThemeKind {
    if KIND.load(Ordering::Relaxed) == ThemeKind::Clasico as u8 { ThemeKind::Clasico } else { ThemeKind::Crepusculo }
}

/// Colores del tema activo.
pub fn t() -> &'static Theme {
    match kind() {
        ThemeKind::Crepusculo => &CREPUSCULO,
        ThemeKind::Clasico => &CLASICO,
    }
}

/// Colores de la interfaz con nombres semánticos (los del sistema Win32 y los
/// propios de SURSHAPE). En mayúsculas porque antes eran constantes.
#[allow(non_snake_case)]
pub struct Theme {
    /// egui parte de `Visuals::dark()` o de `Visuals::light()`.
    pub dark_base: bool,
    /// ButtonFace / fondo de ventanas.
    pub FACE: Color32,
    /// Paneles y barra lateral.
    pub PANEL: Color32,
    /// Bisel claro (borde superior izquierdo de lo que sobresale).
    pub LIGHT: Color32,
    /// Sombra del bisel.
    pub SHADOW: Color32,
    /// Sombra oscura (borde exterior). Nunca negro.
    pub DARK: Color32,
    /// Borde superior izquierdo de lo hundido (campos, radios).
    pub SUNKEN_EDGE: Color32,
    /// Campos editables (hundidos), listas.
    pub FIELD: Color32,
    /// Selección (Highlight) y su texto.
    pub HIGHLIGHT: Color32,
    pub HIGHLIGHT_TEXT: Color32,
    /// Barras de título de páginas y diálogos: degradado y texto.
    pub TITLE_A: Color32,
    pub TITLE_B: Color32,
    pub TITLE_TEXT: Color32,
    pub TEXT: Color32,
    pub TEXT_MUTED: Color32,
    /// Texto en lavanda: solo sobre FIELD o FACE.
    #[allow(dead_code)]
    pub TEXT_LAVENDER: Color32,
    /// Texto deshabilitado (exento de AA, como en Windows).
    pub TEXT_DISABLED: Color32,
    /// Aviso: texto con ícono propio.
    pub WARNING: Color32,
    /// Error: texto sobre franja `ERROR_BG`, con ícono de error.
    pub ERROR: Color32,
    pub ERROR_BG: Color32,
    /// Acentos: Render, foco, hot-tracking de la toolbar.
    pub ACCENT: Color32,
    pub ACCENT_2: Color32,
    /// Botón de toolbar "encendido": cara aclarada.
    pub BUTTON_ON: Color32,
    /// Canal de las barras de desplazamiento.
    pub SCROLL_TRACK: Color32,
    /// Íconos 16x16: acento y carpeta.
    pub ICON_ACCENT: Color32,
    pub ICON_FOLDER: Color32,
    /// Ícono de la ventana.
    pub APP_ICON_BG: Color32,
    pub APP_ICON_FG: Color32,
    /// Visor.
    pub VIEW_BG: Color32,
    pub VIEW_WAVE: Color32,
    pub VIEW_TEXT: Color32,
    pub VIEW_CURSOR: Color32,
    /// Regla, línea central y separación de canales.
    pub VIEW_GRID: Color32,
    /// Marcadores de la fuente.
    pub VIEW_MARKER: Color32,
    /// Bucle de la celda.
    pub VIEW_LOOP: Color32,
    /// Etiquetas de frecuencia del espectrograma.
    pub SPECTRO_AXIS_TEXT: Color32,
    pub SPECTRO_AXIS_BG: Color32,
    /// Gradiente del espectrograma, de silencio a máximo.
    pub SPECTRO: &'static [Color32],
    /// Encabezados de las secciones de la barra lateral y su texto.
    pub SIDE_HEADS: [Color32; 4],
    pub SIDE_HEAD_TEXT: [Color32; 4],
    pub GRID_HEAD: Color32,
    /// Celda con contenido.
    pub GRID_CELL: Color32,
    /// Celda vacía / alterna.
    pub GRID_EMPTY: Color32,
    /// Columna 0 (fuentes, generadores, mezclas, referencias).
    pub GRID_COL0: Color32,
    /// Líneas de la grilla (decorativas).
    pub GRID_LINE: Color32,
    /// Celda que es entrada 2 de la elegida / celda bajo el puntero al elegir.
    pub GRID_MARK: Color32,
    /// Borde de foco de la celda elegida.
    pub GRID_FOCUS: Color32,
    /// Bases de los colores semitransparentes.
    pub DROP_BASE: Color32,
    pub BACKDROP_BASE: Color32,
    pub GHOST_BASE: Color32,
    pub VIEW_SEL_BASE: Color32,
}

/// "Crepúsculo de invierno" (por defecto). Asignación aprobada en la 0.9.1.
pub static CREPUSCULO: Theme = {
    use pal::*;
    Theme {
        dark_base: true,
        FACE: NOCHE_42,
        PANEL: NOCHE_35,
        // Bisel claro #748FD2 y no #376BBF: con #376BBF el relieve daba 2,47:1
        // contra la cara y los botones se veían planos.
        LIGHT: AZUL_3,
        SHADOW: NOCHE_50,
        DARK: NOCHE_50,
        // El piso (#142A4E) no se ve sobre la cara: lo hundido lleva #376BBF
        // arriba a la izquierda y #748FD2 abajo a la derecha.
        SUNKEN_EDGE: AZUL_2,
        FIELD: NOCHE_50,
        HIGHLIGHT: AZUL_1,
        HIGHLIGHT_TEXT: NIEVE,
        TITLE_A: NOCHE_35,
        TITLE_B: AZUL_2,
        TITLE_TEXT: NIEVE,
        TEXT: NIEVE,
        TEXT_MUTED: NIEVE_LAVANDA,
        TEXT_LAVENDER: LAVANDA_1,
        TEXT_DISABLED: AZUL_3,
        WARNING: NIEVE_LAVANDA,
        ERROR: NIEVE,
        ERROR_BG: AZUL_1,
        ACCENT: AZUL_3,
        ACCENT_2: LAVANDA_1,
        BUTTON_ON: NOCHE_15,
        SCROLL_TRACK: NOCHE_50,
        ICON_ACCENT: AZUL_3,
        ICON_FOLDER: LAVANDA_1,
        APP_ICON_BG: AZUL_1,
        APP_ICON_FG: NIEVE,
        VIEW_BG: NOCHE_50,
        VIEW_WAVE: LAVANDA_1,
        VIEW_TEXT: NIEVE_LAVANDA,
        VIEW_CURSOR: NIEVE,
        VIEW_GRID: NOCHE_15,
        VIEW_MARKER: AZUL_3,
        VIEW_LOOP: LAVANDA_2,
        SPECTRO_AXIS_TEXT: NIEVE,
        SPECTRO_AXIS_BG: NOCHE_50,
        SPECTRO: &[NOCHE_50, AZUL_1, AZUL_2, AZUL_3, LAVANDA_1, NIEVE],
        // El cuarto es #C7D2ED y no #748FD2: con texto #142A4E daba 4,49:1.
        SIDE_HEADS: [AZUL_1, AZUL_2, LAVANDA_1, NIEVE_AZUL],
        SIDE_HEAD_TEXT: [NIEVE, NIEVE, NOCHE_50, NOCHE_50],
        GRID_HEAD: NOCHE_42,
        GRID_CELL: NOCHE_35,
        GRID_EMPTY: NOCHE_42,
        GRID_COL0: NOCHE_25,
        GRID_LINE: NOCHE_15,
        GRID_MARK: NOCHE_15,
        GRID_FOCUS: LAVANDA_1,
        DROP_BASE: AZUL_1,
        BACKDROP_BASE: NOCHE_50,
        GHOST_BASE: LAVANDA_1,
        VIEW_SEL_BASE: AZUL_2,
    }
};

/// "Claro (clásico)": la paleta acero/celeste/turquesa hasta la 0.9.0.
pub static CLASICO: Theme = {
    use pal_clasico::*;
    Theme {
        dark_base: false,
        FACE: ACERO_4,
        PANEL: ACERO_4,
        LIGHT: BLANCO,
        SHADOW: ACERO_2,
        DARK: MARINO,
        SUNKEN_EDGE: ACERO_2,
        FIELD: BLANCO,
        HIGHLIGHT: AVISO,
        HIGHLIGHT_TEXT: BLANCO,
        TITLE_A: AVISO,
        TITLE_B: ACERO_1,
        TITLE_TEXT: BLANCO,
        TEXT: MARINO,
        TEXT_MUTED: MARINO_SUAVE,
        TEXT_LAVENDER: MARINO_SUAVE,
        TEXT_DISABLED: ACERO_2,
        WARNING: AVISO,
        ERROR: ERROR,
        ERROR_BG: ACERO_4,
        // #2887AC y no #04B5E9: la línea de envolvente sobre blanco daba 2,3:1.
        ACCENT: ACERO_1,
        ACCENT_2: TURQUESA_1,
        BUTTON_ON: TURQUESA_4,
        SCROLL_TRACK: TURQUESA_4,
        ICON_ACCENT: ACERO_1,
        ICON_FOLDER: CIELO_3,
        APP_ICON_BG: ACERO_1,
        APP_ICON_FG: CELESTE_4,
        VIEW_BG: MARINO,
        VIEW_WAVE: CELESTE_1,
        VIEW_TEXT: CELESTE_3,
        VIEW_CURSOR: CELESTE_4,
        VIEW_GRID: ACERO_1,
        VIEW_MARKER: CIELO_2,
        VIEW_LOOP: TURQUESA_1,
        SPECTRO_AXIS_TEXT: CELESTE_4,
        SPECTRO_AXIS_BG: MARINO,
        SPECTRO: &[MARINO, ACERO_1, CIELO_1, CELESTE_1, CELESTE_4],
        SIDE_HEADS: [ACERO_2, CELESTE_2, TURQUESA_2, CIELO_2],
        SIDE_HEAD_TEXT: [MARINO, MARINO, MARINO, MARINO],
        GRID_HEAD: ACERO_4,
        GRID_CELL: BLANCO,
        GRID_EMPTY: TURQUESA_4,
        GRID_COL0: TURQUESA_3,
        GRID_LINE: ACERO_3,
        GRID_MARK: CELESTE_3,
        GRID_FOCUS: MARINO,
        DROP_BASE: AVISO,
        BACKDROP_BASE: MARINO,
        GHOST_BASE: CELESTE_1,
        VIEW_SEL_BASE: ACERO_1,
    }
};

fn with_alpha(c: Color32, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
}
/// Capa al soltar archivos.
pub fn drop_overlay() -> Color32 {
    with_alpha(t().DROP_BASE, 90)
}
/// Fondo detrás de los diálogos modales.
pub fn modal_backdrop() -> Color32 {
    with_alpha(t().BACKDROP_BASE, 90)
}
/// Onda de referencia bajo el editor de envolventes.
pub fn bp_ghost() -> Color32 {
    with_alpha(t().GHOST_BASE, 60)
}
/// Selección del visor.
pub fn view_selection() -> Color32 {
    with_alpha(t().VIEW_SEL_BASE, 56)
}

/// Color del espectrograma para un valor 0..1 (gradiente `SPECTRO`).
pub fn spectro_color(v: f32) -> Color32 {
    let s = t().SPECTRO;
    let x = v.clamp(0.0, 1.0) * (s.len() - 1) as f32;
    let i = (x.floor() as usize).min(s.len() - 2);
    let f = x - i as f32;
    let (a, b) = (s[i], s[i + 1]);
    let mix = |p: u8, q: u8| (p as f32 + (q as f32 - p as f32) * f).round() as u8;
    Color32::from_rgb(mix(a.r(), b.r()), mix(a.g(), b.g()), mix(a.b(), b.b()))
}

/// Mezcla dos colores (0 = a, 1 = b).
#[cfg_attr(not(test), allow(dead_code))]
pub fn lerp(a: Color32, b: Color32, f: f32) -> Color32 {
    let f = f.clamp(0.0, 1.0);
    let m = |p: u8, q: u8| (p as f32 + (q as f32 - p as f32) * f).round() as u8;
    Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}

// --- Tipografía -------------------------------------------------------------------

// Fuentes libres (SIL OFL 1.1), embebidas.
const PLEX: &[u8] = include_bytes!("../assets/fonts/IBMPlexSans-Regular.ttf"); // i18n-ok
const PLEX_SEMI: &[u8] = include_bytes!("../assets/fonts/IBMPlexSans-SemiBold.ttf"); // i18n-ok
const COURIER: &[u8] = include_bytes!("../assets/fonts/CourierPrime-Regular.ttf"); // i18n-ok
const COURIER_BOLD: &[u8] = include_bytes!("../assets/fonts/CourierPrime-Bold.ttf"); // i18n-ok

const F_BOLD: &str = "negrita"; // i18n-ok
const F_MONO_BOLD: &str = "mono_bold"; // i18n-ok

/// Tamaño de la letra de la interfaz (Tahoma 8-9 pt).
pub const UI_SIZE: f32 = 12.0;
pub const SMALL_SIZE: f32 = 11.0;
pub const MONO_SIZE: f32 = 12.5;

/// Tahoma / Segoe UI: todo el texto de la interfaz.
pub fn body_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}
/// Negrita: títulos de grupos, barras de título.
pub fn bold_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(F_BOLD.into()))
}
/// Courier Prime: lecturas del visor, línea de comando y consola.
pub fn mono_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}
pub fn mono_bold_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(F_MONO_BOLD.into()))
}
pub fn ui_font() -> FontId {
    body_font(UI_SIZE)
}

/// Fuente del sistema que se usa (para Acerca de / pruebas).
pub fn system_font_name() -> &'static str {
    SYSTEM_FONT.get().map(|(n, _, _)| *n).unwrap_or("IBM Plex Sans") // i18n-ok
}

static SYSTEM_FONT: std::sync::OnceLock<(&'static str, Vec<u8>, Vec<u8>)> = std::sync::OnceLock::new();

/// Lee Tahoma o Segoe UI de la carpeta de fuentes de Windows.
fn system_font() -> Option<&'static (&'static str, Vec<u8>, Vec<u8>)> {
    if let Some(f) = SYSTEM_FONT.get() {
        return Some(f);
    }
    let dir = std::env::var_os("WINDIR").map(std::path::PathBuf::from).unwrap_or_else(|| r"C:\Windows".into()).join("Fonts"); // i18n-ok
    for (name, reg, bold) in [("Tahoma", "tahoma.ttf", "tahomabd.ttf"), ("Segoe UI", "segoeui.ttf", "segoeuib.ttf")] { // i18n-ok
        if let (Ok(r), Ok(b)) = (std::fs::read(dir.join(reg)), std::fs::read(dir.join(bold))) {
            let _ = SYSTEM_FONT.set((name, r, b));
            return SYSTEM_FONT.get();
        }
    }
    None
}

fn fonts() -> FontDefinitions {
    // Sin las fuentes por defecto de egui (licencia UFL).
    let mut f = FontDefinitions::empty();
    for (name, data) in [
        ("plex", PLEX),              // i18n-ok
        ("plex_semi", PLEX_SEMI),    // i18n-ok
        ("courier", COURIER),        // i18n-ok
        ("courier_b", COURIER_BOLD), // i18n-ok
    ] {
        f.font_data.insert(name.into(), Arc::new(FontData::from_static(data)));
    }
    let (main, bold) = match system_font() {
        Some((_, r, b)) => {
            f.font_data.insert("sistema".into(), Arc::new(FontData::from_owned(r.clone()))); // i18n-ok
            f.font_data.insert("sistema_b".into(), Arc::new(FontData::from_owned(b.clone()))); // i18n-ok
            ("sistema", "sistema_b") // i18n-ok
        }
        None => ("plex", "plex_semi"), // i18n-ok
    };
    // Plex como respaldo de glifos que falten.
    let fam = |m: &str| vec![m.to_string(), "plex".to_string()]; // i18n-ok
    f.families.insert(FontFamily::Proportional, fam(main));
    f.families.insert(FontFamily::Monospace, fam("courier")); // i18n-ok
    f.families.insert(FontFamily::Name(F_BOLD.into()), fam(bold));
    f.families.insert(FontFamily::Name(F_MONO_BOLD.into()), fam("courier_b")); // i18n-ok
    f
}

/// Aplica fuentes, tamaños y colores a todo egui.
pub fn apply(ctx: &egui::Context) {
    ctx.set_fonts(fonts());
    // Siempre el tema propio, aunque Windows esté en modo oscuro. Sin
    // animaciones (Win32 clásico).
    ctx.options_mut(|o| {
        o.theme_preference = if t().dark_base { egui::ThemePreference::Dark } else { egui::ThemePreference::Light };
        o.zoom_with_keyboard = false;
    });
    let base = if t().dark_base { egui::Theme::Dark } else { egui::Theme::Light };
    ctx.style_mut_of(base, |s| {
        s.text_styles = [
            (TextStyle::Heading, bold_font(13.0)),
            (TextStyle::Body, body_font(UI_SIZE)),
            (TextStyle::Button, body_font(UI_SIZE)),
            (TextStyle::Small, body_font(SMALL_SIZE)),
            (TextStyle::Monospace, mono_font(MONO_SIZE)),
        ]
        .into();
        s.animation_time = 0.0;
        s.spacing.item_spacing = egui::vec2(6.0, 4.0);
        s.spacing.button_padding = egui::vec2(6.0, 2.0);
        s.spacing.interact_size = egui::vec2(20.0, 20.0);
        s.spacing.slider_width = 120.0;
        s.spacing.icon_width = 13.0;
        s.spacing.icon_width_inner = 9.0;
        s.spacing.menu_margin = egui::Margin::same(2);
        s.spacing.window_margin = egui::Margin::same(6);
        s.spacing.scroll = egui::style::ScrollStyle {
            floating: false,
            bar_width: 16.0,
            handle_min_length: 16.0,
            bar_inner_margin: 0.0,
            bar_outer_margin: 0.0,
            floating_width: 16.0,
            floating_allocated_width: 16.0,
            foreground_color: false,
            dormant_background_opacity: 1.0,
            active_background_opacity: 1.0,
            interact_background_opacity: 1.0,
            dormant_handle_opacity: 1.0,
            active_handle_opacity: 1.0,
            interact_handle_opacity: 1.0,
        };
        s.interaction.tooltip_delay = 0.4;

        let th = t();
        let v = &mut s.visuals;
        *v = if th.dark_base { egui::Visuals::dark() } else { egui::Visuals::light() };
        v.override_text_color = Some(th.TEXT);
        v.panel_fill = th.PANEL;
        v.window_fill = th.FACE;
        v.window_stroke = Stroke::new(1.0_f32, th.DARK);
        v.window_shadow = Shadow::NONE;
        v.popup_shadow = Shadow::NONE;
        v.window_corner_radius = CornerRadius::ZERO;
        v.menu_corner_radius = CornerRadius::ZERO;
        v.extreme_bg_color = th.FIELD;
        v.faint_bg_color = th.GRID_EMPTY;
        v.code_bg_color = th.FIELD;
        v.hyperlink_color = th.HIGHLIGHT;
        v.warn_fg_color = th.WARNING;
        v.error_fg_color = th.ERROR;
        v.selection.bg_fill = th.HIGHLIGHT;
        v.selection.stroke = Stroke::new(1.0_f32, th.HIGHLIGHT_TEXT);
        v.text_cursor.stroke = Stroke::new(1.0_f32, th.TEXT);
        v.slider_trailing_fill = false;
        v.striped = false;
        v.button_frame = true;
        v.collapsing_header_frame = false;
        v.handle_shape = egui::style::HandleShape::Rect { aspect_ratio: 0.5 };
        for (w, fill, stroke) in [
            (&mut v.widgets.noninteractive, th.FACE, Stroke::new(1.0_f32, th.SHADOW)),
            (&mut v.widgets.inactive, th.FACE, Stroke::new(1.0_f32, th.LIGHT)),
            (&mut v.widgets.hovered, th.FACE, Stroke::new(1.0_f32, th.ACCENT)),
            (&mut v.widgets.active, th.BUTTON_ON, Stroke::new(1.0_f32, th.ACCENT)),
            (&mut v.widgets.open, th.FACE, Stroke::new(1.0_f32, th.LIGHT)),
        ] {
            w.bg_fill = fill;
            w.weak_bg_fill = fill;
            w.bg_stroke = stroke;
            w.fg_stroke = Stroke::new(1.0_f32, th.TEXT);
            w.corner_radius = CornerRadius::ZERO;
            w.expansion = 0.0;
        }
    });
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn channel_lum(c: u8) -> f64 {
        let s = c as f64 / 255.0;
        if s <= 0.04045 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    }

    /// Luminancia relativa (WCAG 2.x).
    pub fn luminance(c: Color32) -> f64 {
        0.2126 * channel_lum(c.r()) + 0.7152 * channel_lum(c.g()) + 0.0722 * channel_lum(c.b())
    }

    /// Relación de contraste WCAG entre dos colores opacos (1..21).
    pub fn contrast(a: Color32, b: Color32) -> f64 {
        let (la, lb) = (luminance(a), luminance(b));
        let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
        (hi + 0.05) / (lo + 0.05)
    }

    /// Color sobre fondo con alfa (para la selección del visor).
    fn over(fg: Color32, bg: Color32) -> Color32 {
        let a = fg.a() as f32 / 255.0;
        // Color32 está premultiplicado: r() ya viene por alfa.
        let m = |f: u8, b: u8| (f as f32 + b as f32 * (1.0 - a)).round().min(255.0) as u8;
        Color32::from_rgb(m(fg.r(), bg.r()), m(fg.g(), bg.g()), m(fg.b(), bg.b()))
    }

    fn themes() -> [(&'static str, &'static Theme); 2] {
        [("crepusculo", &CREPUSCULO), ("clasico", &CLASICO)]
    }

    fn sel_over(th: &Theme) -> Color32 {
        over(with_alpha(th.VIEW_SEL_BASE, 56), th.VIEW_BG)
    }

    /// Todos los pares texto/fondo que usa la interfaz: (uso, texto, fondo).
    pub fn text_pairs(th: &Theme) -> Vec<(&'static str, Color32, Color32)> {
        let mut v = Vec::new();
        for (bg_name, bg) in [
            ("cara", th.FACE),
            ("panel", th.PANEL),
            ("campo", th.FIELD),
            ("celda", th.GRID_CELL),
            ("celda_vacia", th.GRID_EMPTY),
            ("columna0", th.GRID_COL0),
            ("marca", th.GRID_MARK),
            ("boton_on", th.BUTTON_ON),
        ] {
            v.push((bg_name, th.TEXT, bg));
            v.push((bg_name, th.TEXT_MUTED, bg));
        }
        for (i, (bg, fg)) in th.SIDE_HEADS.iter().zip(th.SIDE_HEAD_TEXT.iter()).enumerate() {
            v.push((["lateral1", "lateral2", "lateral3", "lateral4"][i], *fg, *bg));
        }
        v.push(("lavanda/campo", th.TEXT_LAVENDER, th.FIELD));
        v.push(("lavanda/cara", th.TEXT_LAVENDER, th.FACE));
        for bg in [th.FACE, th.PANEL, th.FIELD] {
            v.push(("aviso", th.WARNING, bg));
        }
        v.push(("error", th.ERROR, th.ERROR_BG));
        v.push(("seleccion", th.HIGHLIGHT_TEXT, th.HIGHLIGHT));
        // Barra de título: el texto va a la izquierda, sobre el primer 40 %
        // del degradado (hasta ahí lo verifica este par).
        v.push(("titulo_inicio", th.TITLE_TEXT, th.TITLE_A));
        v.push(("titulo_40", th.TITLE_TEXT, lerp(th.TITLE_A, th.TITLE_B, 0.4)));
        v.push(("visor_lectura", th.VIEW_TEXT, th.VIEW_BG));
        v.push(("visor_cursor", th.VIEW_CURSOR, th.VIEW_BG));
        v.push(("visor_sel", th.VIEW_TEXT, sel_over(th)));
        v.push(("eje_espectro", th.SPECTRO_AXIS_TEXT, th.SPECTRO_AXIS_BG));
        v
    }

    /// Gráficos y bordes funcionales (WCAG 1.4.11, >= 3:1): (uso, color, fondo).
    pub fn graphic_pairs(th: &Theme) -> Vec<(&'static str, Color32, Color32)> {
        vec![
            ("onda", th.VIEW_WAVE, th.VIEW_BG),
            ("cursor", th.VIEW_CURSOR, th.VIEW_BG),
            ("marcador", th.VIEW_MARKER, th.VIEW_BG),
            ("bucle", th.VIEW_LOOP, th.VIEW_BG),
            ("acento/campo", th.ACCENT, th.FIELD),
            ("acento/celda", th.ACCENT, th.GRID_CELL),
            ("foco/celda", th.GRID_FOCUS, th.GRID_CELL),
            ("foco/vacia", th.GRID_FOCUS, th.GRID_EMPTY),
            ("borde_ctrl", if th.dark_base { th.LIGHT } else { th.DARK }, th.FACE),
            ("tinta_icono", th.TEXT, th.FACE),
        ]
    }

    /// Pares decorativos (sin mínimo): se imprimen para el registro.
    pub fn decorative_pairs(th: &Theme) -> Vec<(&'static str, Color32, Color32)> {
        vec![
            ("bisel/cara", th.LIGHT, th.FACE),
            ("bisel/sombra", th.LIGHT, th.SHADOW),
            ("hundido/cara", th.SUNKEN_EDGE, th.FACE),
            ("campo/cara", th.FIELD, th.FACE),
            ("linea_grilla", th.GRID_LINE, th.GRID_CELL),
            ("regla_visor", th.VIEW_GRID, th.VIEW_BG),
            ("foco/seleccion", th.GRID_FOCUS, th.HIGHLIGHT),
            ("icono/cara", th.ICON_ACCENT, th.FACE),
            ("carpeta/cara", th.ICON_FOLDER, th.FACE),
            ("deshab/campo", th.TEXT_DISABLED, th.FIELD),
            ("deshab/cara", th.TEXT_DISABLED, th.FACE),
        ]
    }

    fn hex(c: Color32) -> String {
        format!("#{:02X}{:02X}{:02X}", c.r(), c.g(), c.b())
    }

    fn check(title: &str, pairs: Vec<(&'static str, Color32, Color32)>, min: f64) -> Vec<String> {
        let mut fails = Vec::new();
        println!("-- {title}");
        println!("{:<16} {:>8} {:>8} {:>7}", "uso", "color", "fondo", "ratio");
        for (name, fg, bg) in pairs {
            let r = contrast(fg, bg);
            println!("{:<16} {:>8} {:>8} {:>7.2}", name, hex(fg), hex(bg), r);
            if r < min {
                fails.push(format!("{title} {name}: {} sobre {} = {r:.2}", hex(fg), hex(bg)));
            }
        }
        fails
    }

    #[test]
    fn contraste_text_pairs_meet_wcag_aa() {
        let mut fails = Vec::new();
        for (name, th) in themes() {
            println!("==== tema {name}");
            fails.extend(check("texto (>= 4,5:1)", text_pairs(th), 4.5));
            fails.extend(check("graficos y bordes funcionales (>= 3:1)", graphic_pairs(th), 3.0));
            let _ = check("decorativos y deshabilitado (sin minimo)", decorative_pairs(th), 0.0);
        }
        assert!(fails.is_empty(), "pares bajo el minimo: {fails:?}");
    }

    fn all_colors(th: &Theme) -> Vec<Color32> {
        let mut v = vec![
            th.FACE, th.PANEL, th.LIGHT, th.SHADOW, th.DARK, th.SUNKEN_EDGE, th.FIELD, th.HIGHLIGHT, th.HIGHLIGHT_TEXT,
            th.TITLE_A, th.TITLE_B, th.TITLE_TEXT, th.TEXT, th.TEXT_MUTED, th.TEXT_LAVENDER, th.TEXT_DISABLED,
            th.WARNING, th.ERROR, th.ERROR_BG, th.ACCENT, th.ACCENT_2, th.BUTTON_ON, th.SCROLL_TRACK, th.ICON_ACCENT,
            th.ICON_FOLDER, th.APP_ICON_BG, th.APP_ICON_FG, th.VIEW_BG, th.VIEW_WAVE, th.VIEW_TEXT, th.VIEW_CURSOR,
            th.VIEW_GRID, th.VIEW_MARKER, th.VIEW_LOOP, th.SPECTRO_AXIS_TEXT, th.SPECTRO_AXIS_BG, th.GRID_HEAD,
            th.GRID_CELL, th.GRID_EMPTY, th.GRID_COL0, th.GRID_LINE, th.GRID_MARK, th.GRID_FOCUS, th.DROP_BASE,
            th.BACKDROP_BASE, th.GHOST_BASE, th.VIEW_SEL_BASE,
        ];
        v.extend_from_slice(th.SPECTRO);
        v.extend(th.SIDE_HEADS);
        v.extend(th.SIDE_HEAD_TEXT);
        v
    }

    #[test]
    fn paleta_solo_valores_permitidos() {
        let piso = luminance(pal::NOCHE_50);
        for c in all_colors(&CREPUSCULO) {
            assert!(pal::TODOS.contains(&c), "color fuera de la paleta: {}", hex(c));
            assert!(luminance(c) >= piso, "más oscuro que el piso #142A4E: {}", hex(c));
        }
    }

    #[test]
    fn no_black_no_red() {
        for (_, th) in themes() {
            for c in all_colors(th) {
                assert!(luminance(c) > 0.01, "demasiado cerca del negro: {}", hex(c));
                // "rojo": el canal rojo domina claramente a verde y azul
                assert!(!(c.r() > c.g().saturating_add(40) && c.r() > c.b().saturating_add(40)), "rojizo: {}", hex(c));
            }
        }
    }

    #[test]
    fn crepusculo_por_defecto() {
        assert_eq!(ThemeKind::default(), ThemeKind::Crepusculo);
    }
}
