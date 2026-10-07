//! Tema Win32 clásico y tipografía. Todos los colores de la interfaz salen
//! de aquí, con nombres semánticos. Reglas (ver CLAUDE.md):
//!   - prohibidos el negro y el rojo; blanco permitido;
//!   - la misma paleta de celestes, azules y turquesas, mapeada a los
//!     colores del sistema de Windows 2000/XP (ButtonFace, sombras, campos,
//!     selección, barras de título);
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
use std::sync::Arc;

/// Paleta base (nombres de la especificación).
#[allow(dead_code)]
pub mod pal {
    use eframe::egui::Color32;
    const fn c(hex: u32) -> Color32 {
        Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
    }
    // Azul acero
    pub const ACERO_1: Color32 = c(0x2887AC);
    pub const ACERO_2: Color32 = c(0x69A3C1);
    pub const ACERO_3: Color32 = c(0x93B9D2);
    pub const ACERO_4: Color32 = c(0xC5D8E5);
    // Celeste
    pub const CELESTE_1: Color32 = c(0x47D8F9);
    pub const CELESTE_2: Color32 = c(0x70E1FE);
    pub const CELESTE_3: Color32 = c(0x9DEAFE);
    pub const CELESTE_4: Color32 = c(0xCFF6FE);
    // Turquesa
    pub const TURQUESA_1: Color32 = c(0x2DC9C8);
    pub const TURQUESA_2: Color32 = c(0x88E1DE);
    pub const TURQUESA_3: Color32 = c(0xB4ECE9);
    pub const TURQUESA_4: Color32 = c(0xDCF6F5);
    // Azul cielo
    pub const CIELO_1: Color32 = c(0x04B5E9);
    pub const CIELO_2: Color32 = c(0x46C5EF);
    pub const CIELO_3: Color32 = c(0x83D1F5);
    pub const CIELO_4: Color32 = c(0xC1E9FC);
    // Texto: azul marino (nunca negro puro)
    pub const MARINO: Color32 = c(0x16323F);
    // Fuera de la paleta pedida, por legibilidad (aprobados):
    /// Texto secundario: marino aclarado; cumple AA en todos los fondos.
    pub const MARINO_SUAVE: Color32 = c(0x2B5468);
    /// Advertencias y selección (Highlight clásico).
    pub const AVISO: Color32 = c(0x0B4A78);
    /// Errores: aún más oscuro y saturado (distinto del aviso, sin rojo).
    pub const ERROR: Color32 = c(0x082F66);
    pub const BLANCO: Color32 = c(0xFFFFFF);
}

// --- Colores del sistema (Win32) ------------------------------------------------------

/// ButtonFace / fondo de ventanas y paneles.
pub const FACE: Color32 = pal::ACERO_4;
/// Bisel claro (borde superior izquierdo de lo que sobresale).
pub const LIGHT: Color32 = pal::BLANCO;
/// Sombra del bisel.
pub const SHADOW: Color32 = pal::ACERO_2;
/// Sombra oscura (borde exterior inferior derecho).
pub const DARK: Color32 = pal::MARINO;
/// Campos editables, listas.
pub const FIELD: Color32 = pal::BLANCO;
/// Selección (Highlight) y su texto.
pub const HIGHLIGHT: Color32 = pal::AVISO;
pub const HIGHLIGHT_TEXT: Color32 = pal::BLANCO;
/// Barras de título de páginas y diálogos: degradado y texto.
pub const TITLE_A: Color32 = pal::AVISO;
pub const TITLE_B: Color32 = pal::ACERO_1;
pub const TITLE_TEXT: Color32 = pal::BLANCO;

pub const TEXT: Color32 = pal::MARINO;
pub const TEXT_MUTED: Color32 = pal::MARINO_SUAVE;
/// Texto deshabilitado (exento de AA, como en Windows; lleva relieve blanco).
pub const TEXT_DISABLED: Color32 = pal::ACERO_2;
pub const WARNING: Color32 = pal::AVISO;
pub const ERROR: Color32 = pal::ERROR;
/// Acentos: Render y foco.
pub const ACCENT: Color32 = pal::CIELO_1;
pub const ACCENT_2: Color32 = pal::TURQUESA_1;

// --- Visor (osciloscopio, sin negro) -------------------------------------------------

pub const VIEW_BG: Color32 = pal::MARINO;
pub const VIEW_WAVE: Color32 = pal::CELESTE_1;
pub const VIEW_TEXT: Color32 = pal::CELESTE_3;
pub const VIEW_CURSOR: Color32 = pal::CELESTE_4;
/// Línea central y separación de canales.
pub const VIEW_GRID: Color32 = pal::ACERO_1;
/// Selección: #2887AC semitransparente.
pub fn view_selection() -> Color32 {
    Color32::from_rgba_unmultiplied(0x28, 0x87, 0xAC, 56)
}
/// Marcadores de la fuente.
pub const VIEW_MARKER: Color32 = pal::CIELO_2;
/// Bucle de la celda (marca turquesa).
pub const VIEW_LOOP: Color32 = pal::TURQUESA_1;

/// Gradiente del espectrograma, de silencio a máximo.
pub const SPECTRO: [Color32; 5] = [pal::MARINO, pal::ACERO_1, pal::CIELO_1, pal::CELESTE_1, pal::CELESTE_4];

/// Color del espectrograma para un valor 0..1 (gradiente [`SPECTRO`]).
pub fn spectro_color(v: f32) -> Color32 {
    let x = v.clamp(0.0, 1.0) * (SPECTRO.len() - 1) as f32;
    let i = (x.floor() as usize).min(SPECTRO.len() - 2);
    let f = x - i as f32;
    let (a, b) = (SPECTRO[i], SPECTRO[i + 1]);
    let mix = |p: u8, q: u8| (p as f32 + (q as f32 - p as f32) * f).round() as u8;
    Color32::from_rgb(mix(a.r(), b.r()), mix(a.g(), b.g()), mix(a.b(), b.b()))
}

// --- Barra lateral y grilla ------------------------------------------------------------

/// Encabezados de las secciones de la barra lateral, uno por sección.
pub const SIDE_HEADS: [Color32; 4] = [pal::ACERO_2, pal::CELESTE_2, pal::TURQUESA_2, pal::CIELO_2];

pub const GRID_HEAD: Color32 = FACE;
/// Celda con contenido.
pub const GRID_CELL: Color32 = pal::BLANCO;
/// Celda vacía.
pub const GRID_EMPTY: Color32 = pal::TURQUESA_4;
/// Columna 0 (fuentes, generadores, mezclas, referencias).
pub const GRID_COL0: Color32 = pal::TURQUESA_3;
/// Líneas de la grilla (decorativas: 2,08:1, aprobado).
pub const GRID_LINE: Color32 = pal::ACERO_3;
/// Celda que es entrada 2 de la elegida / celda bajo el puntero al elegir.
pub const GRID_MARK: Color32 = pal::CELESTE_3;

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
        o.theme_preference = egui::ThemePreference::Light;
        o.zoom_with_keyboard = false;
    });
    ctx.style_mut_of(egui::Theme::Light, |s| {
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

        let v = &mut s.visuals;
        *v = egui::Visuals::light();
        v.override_text_color = Some(TEXT);
        v.panel_fill = FACE;
        v.window_fill = FACE;
        v.window_stroke = Stroke::new(1.0_f32, DARK);
        v.window_shadow = Shadow::NONE;
        v.popup_shadow = Shadow::NONE;
        v.window_corner_radius = CornerRadius::ZERO;
        v.menu_corner_radius = CornerRadius::ZERO;
        v.extreme_bg_color = FIELD;
        v.faint_bg_color = pal::TURQUESA_4;
        v.code_bg_color = FIELD;
        v.hyperlink_color = HIGHLIGHT;
        v.warn_fg_color = WARNING;
        v.error_fg_color = ERROR;
        v.selection.bg_fill = HIGHLIGHT;
        v.selection.stroke = Stroke::new(1.0_f32, HIGHLIGHT_TEXT);
        v.text_cursor.stroke = Stroke::new(1.0_f32, TEXT);
        v.slider_trailing_fill = false;
        v.striped = false;
        v.button_frame = true;
        v.collapsing_header_frame = false;
        v.handle_shape = egui::style::HandleShape::Rect { aspect_ratio: 0.5 };
        for (w, fill, stroke) in [
            (&mut v.widgets.noninteractive, FACE, Stroke::new(1.0_f32, SHADOW)),
            (&mut v.widgets.inactive, FACE, Stroke::new(1.0_f32, DARK)),
            (&mut v.widgets.hovered, FACE, Stroke::new(1.0_f32, DARK)),
            (&mut v.widgets.active, FACE, Stroke::new(1.0_f32, DARK)),
            (&mut v.widgets.open, FACE, Stroke::new(1.0_f32, DARK)),
        ] {
            w.bg_fill = fill;
            w.weak_bg_fill = fill;
            w.bg_stroke = stroke;
            w.fg_stroke = Stroke::new(1.0_f32, TEXT);
            w.corner_radius = CornerRadius::ZERO;
            w.expansion = 0.0;
        }
        // Scrollbars: canal claro, botón con color de cara.
        v.extreme_bg_color = FIELD;
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

    /// Todos los pares texto/fondo que usa la interfaz: (uso, texto, fondo).
    pub fn text_pairs() -> Vec<(&'static str, Color32, Color32)> {
        let mut v = Vec::new();
        for (bg_name, bg) in [
            ("cara", FACE),
            ("campo", FIELD),
            ("celda", GRID_CELL),
            ("celda_vacia", GRID_EMPTY),
            ("columna0", GRID_COL0),
            ("marca", GRID_MARK),
            ("lateral1", SIDE_HEADS[0]),
            ("lateral2", SIDE_HEADS[1]),
            ("lateral3", SIDE_HEADS[2]),
            ("lateral4", SIDE_HEADS[3]),
            ("acento", ACCENT),
            ("acento2", ACCENT_2),
        ] {
            v.push((bg_name, TEXT, bg));
        }
        for (bg_name, bg) in [("cara", FACE), ("campo", FIELD), ("celda_vacia", GRID_EMPTY), ("columna0", GRID_COL0)] {
            v.push((bg_name, TEXT_MUTED, bg));
        }
        for bg in [FACE, FIELD] {
            v.push(("aviso", WARNING, bg));
            v.push(("error", ERROR, bg));
        }
        v.push(("seleccion", HIGHLIGHT_TEXT, HIGHLIGHT));
        // Barra de título: el texto va a la izquierda, sobre el primer 40 %
        // del degradado (hasta ahí lo verifica este par).
        v.push(("titulo_inicio", TITLE_TEXT, TITLE_A));
        v.push(("titulo_40", TITLE_TEXT, lerp(TITLE_A, TITLE_B, 0.4)));
        // Visor oscuro
        v.push(("visor_lectura", VIEW_TEXT, VIEW_BG));
        v.push(("visor_cursor", VIEW_CURSOR, VIEW_BG));
        v.push(("visor_sel", VIEW_TEXT, over(view_selection(), VIEW_BG)));
        v.push(("eje_espectro", pal::CELESTE_4, pal::MARINO));
        v
    }

    fn hex(c: Color32) -> String {
        format!("#{:02X}{:02X}{:02X}", c.r(), c.g(), c.b())
    }

    #[test]
    fn contraste_text_pairs_meet_wcag_aa() {
        let mut fails = Vec::new();
        println!("{:<14} {:>8} {:>8} {:>7}", "uso", "texto", "fondo", "ratio");
        for (name, fg, bg) in text_pairs() {
            let r = contrast(fg, bg);
            println!("{:<14} {:>8} {:>8} {:>7.2}", name, hex(fg), hex(bg), r);
            if r < 4.5 {
                fails.push(format!("{name}: {} sobre {} = {r:.2}", hex(fg), hex(bg)));
            }
        }
        assert!(fails.is_empty(), "pares bajo 4,5:1: {fails:?}");
    }

    #[test]
    fn no_black_no_red() {
        let all = [
            FACE, LIGHT, SHADOW, DARK, FIELD, HIGHLIGHT, TITLE_A, TITLE_B, TEXT, TEXT_MUTED, TEXT_DISABLED, WARNING, ERROR,
            ACCENT, ACCENT_2, VIEW_BG, VIEW_WAVE, VIEW_TEXT, VIEW_CURSOR, VIEW_GRID, VIEW_LOOP, GRID_CELL, GRID_EMPTY,
            GRID_COL0, GRID_LINE, GRID_MARK,
        ];
        for c in all.iter().chain(SPECTRO.iter()).chain(SIDE_HEADS.iter()) {
            assert!(luminance(*c) > 0.01, "demasiado cerca del negro: {}", hex(*c));
            // "rojo": el canal rojo domina claramente a verde y azul
            assert!(!(c.r() > c.g().saturating_add(40) && c.r() > c.b().saturating_add(40)), "rojizo: {}", hex(*c));
        }
    }

    #[test]
    fn graphics_are_visible() {
        // Elementos gráficos (no texto): WCAG 1.4.11 pide >= 3:1.
        assert!(contrast(VIEW_WAVE, VIEW_BG) >= 3.0, "{}", contrast(VIEW_WAVE, VIEW_BG));
        assert!(contrast(VIEW_CURSOR, VIEW_BG) >= 3.0);
        // El borde de los controles lo marca la sombra oscura.
        assert!(contrast(DARK, FACE) >= 3.0);
        assert!(contrast(DARK, FIELD) >= 3.0);
    }
}
