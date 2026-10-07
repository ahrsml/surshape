//! Modo captura (`--captura <carpeta>`): arma una sesión de ejemplo (un
//! sonido generado y el patch .wav -> .ana -> espectral -> resíntesis ->
//! nativo), la renderiza y guarda capturas de la página principal y de la
//! de parámetros a 1280x800 y 1920x1080, en español e inglés. PNG propio,
//! sin dependencias.

use crate::app::{App, Page};
use crate::prefs::Prefs;
use eframe::egui;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use surshape_audio::export::{write_wav, WavFormat};
use surshape_audio::AudioBuf;
use surshape_i18n::Lang;
use surshape_patch::SourceInfo;
use surshape_session::Session;

/// Una captura pendiente.
#[derive(Clone, Copy)]
struct Shot {
    w: f32,
    h: f32,
    lang: Lang,
    /// 0 principal, 1 parámetros, 2 consola, 3 preferencias.
    page: u8,
}

pub(crate) struct Capture {
    dir: PathBuf,
    shots: Vec<Shot>,
    idx: usize,
    /// Cuadros esperados desde el último cambio.
    wait: u32,
    rendered: bool,
    requested: bool,
    pub done: bool,
}

/// Crea la sesión de ejemplo y la app en modo captura.
pub(crate) fn start(ctx: &egui::Context, mut prefs: Prefs, exe_dir: Option<PathBuf>, overrides: Vec<PathBuf>, dir: PathBuf) -> App {
    crate::prefs::disable_saving();
    let ses = std::env::temp_dir().join(format!("surshape_captura_{}", std::process::id())); // i18n-ok
    let _ = std::fs::remove_dir_all(&ses);
    prefs.secciones = [true, true, true, false];
    let ok = example_session(&ses, exe_dir.as_deref()).is_ok();
    let mut app = App::with_ctx(ctx, prefs, exe_dir, overrides, Some(ses));
    let mut shots = Vec::new();
    for (w, h) in [(1280.0, 800.0), (1920.0, 1080.0)] {
        for lang in [Lang::Es, Lang::En] {
            for page in 0..4 {
                shots.push(Shot { w, h, lang, page });
            }
        }
    }
    app.capture = Some(Capture { dir, shots, idx: 0, wait: 0, rendered: !ok, requested: false, done: false });
    app
}

/// Sesión de ejemplo: fuente generada y el patch de 4 celdas + la fuente.
pub(crate) fn example_session(dir: &Path, exe_dir: Option<&Path>) -> Result<(), String> {
    let mut s = Session::create(dir).map_err(|e| e.detalle)?;
    let src = dir.join("ejemplo.wav"); // i18n-ok
    let sr = 44100usize;
    let mk = |ph: f32| -> Vec<f32> {
        (0..sr * 6)
            .map(|i| {
                let t = i as f32 / sr as f32;
                let env = (t * 0.9).sin().abs() * 0.6 + 0.2;
                env * ((t * 220.0 * std::f32::consts::TAU + ph).sin() * 0.5 + (t * 331.0 * std::f32::consts::TAU).sin() * 0.25 + (t * 57.0).sin() * 0.1)
            })
            .collect()
    };
    write_wav(&src, &AudioBuf::from_channels(sr as u32, vec![mk(0.0), mk(0.7)]), WavFormat::Float32).map_err(|e| e.to_string())?;
    let (bytes, modificado) = surshape_session::file_stamp(&src).unwrap_or_default();
    let hash = surshape_session::hash_file(&src, None, None).map_err(|e| e.to_string())?;
    let (_, row) = s.patch.add_source(SourceInfo { ruta: src, hash, sr: sr as u32, canales: 2, frames: (sr * 6) as u64, bytes, modificado, marcadores: vec![
        surshape_patch::Marker { t: 1.0, fin: None, etiqueta: "1".into() }, // i18n-ok
        surshape_patch::Marker { t: 2.5, fin: Some(4.0), etiqueta: "2".into() }, // i18n-ok
    ] });
    let mut reg = surshape_engine::Registry::new();
    surshape_native::register(&mut reg);
    let local = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("third_party").join("cdp-bin"); // i18n-ok
    let cdp = surshape_cdp::CdpInstall::find(None, exe_dir).or_else(|| surshape_cdp::CdpInstall::discover(&local));
    let chain: &[&str] = if let Some(c) = cdp {
        surshape_cdp::procs::register(&mut reg, &Arc::new(c));
        &["cdp.pvoc_anal", "cdp.blur_blur", "cdp.pvoc_synth", "nat.paulstretch"] // i18n-ok
    } else {
        &["nat.reverse", "nat.paulstretch"] // i18n-ok
    };
    for (i, id) in chain.iter().enumerate() {
        if let Some(p) = reg.get(id) {
            s.patch.append(row, p.as_ref(), 1000 + i as u64);
        }
    }
    s.save().map_err(|e| e.detalle)
}

impl Capture {
    /// Un paso por cuadro.
    pub(crate) fn step(&mut self, app: &mut App, ctx: &egui::Context) {
        // 1. Renderizar el patch.
        if !self.rendered {
            if app.run.is_none() && self.wait == 0 {
                app.start_run(Vec::new());
            }
            self.wait += 1;
            if app.run.is_none() && self.wait > 5 && app.loads.is_empty() {
                self.rendered = true;
                self.wait = 0;
                // Celda elegida: la espectral (A_2).
                let id = app.session.as_ref().and_then(|s| s.patch.at(0, 2).or_else(|| s.patch.at(0, 1)));
                if let Some(id) = id {
                    app.select(id);
                }
            }
            return;
        }
        // 2. Capturas.
        if self.requested {
            let img = ctx.input(|i| {
                i.raw.events.iter().find_map(|e| match e {
                    egui::Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
            });
            if let Some(img) = img {
                let s = self.shots[self.idx];
                let page = ["principal", "parametros", "consola", "preferencias"][s.page as usize]; // i18n-ok
                let name = format!("{}_{}x{}_{}.png", page, s.w as u32, s.h as u32, s.lang.code()); // i18n-ok
                let _ = std::fs::create_dir_all(&self.dir);
                let _ = write_png(&self.dir.join(name), img.size[0], img.size[1], img.as_raw());
                self.requested = false;
                self.idx += 1;
                self.wait = 0;
            }
            return;
        }
        let Some(&s) = self.shots.get(self.idx) else {
            self.done = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        };
        if self.wait == 0 {
            surshape_i18n::set_lang(s.lang);
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(s.w, s.h)));
            app.modal = None;
            app.page = match (s.page, app.selected) {
                (1, Some(id)) => Page::Params(id),
                (2, _) => Page::Console,
                _ => Page::Main,
            };
            if s.page == 3 {
                app.prefs_draft = Some(app.prefs.clone());
                app.modal = Some(crate::app::Dialog::Prefs);
            }
        }
        self.wait += 1;
        ctx.request_repaint();
        if self.wait == 20 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            self.requested = true;
        }
    }
}

// --- PNG mínimo (sin compresión real: bloques "stored" de zlib) -----------------------

fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (n, t) in table.iter_mut().enumerate() {
        let mut c = n as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
        *t = c;
    }
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        c = table[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
    }
    c ^ 0xFFFF_FFFF
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &x in data {
        a = (a + x as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let mut c = kind.to_vec();
    c.extend_from_slice(data);
    out.extend_from_slice(&c);
    out.extend_from_slice(&crc32(&c).to_be_bytes());
}

/// Codifica RGBA 8 bit como PNG.
pub(crate) fn encode_png(w: usize, h: usize, rgba: &[u8]) -> Vec<u8> {
    let mut raw = Vec::with_capacity((w * 4 + 1) * h);
    for y in 0..h {
        raw.push(0);
        raw.extend_from_slice(&rgba[y * w * 4..(y + 1) * w * 4]);
    }
    let mut z = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(65535).collect();
    for (i, b) in blocks.iter().enumerate() {
        z.push(u8::from(i + 1 == blocks.len()));
        let n = b.len() as u16;
        z.extend_from_slice(&n.to_le_bytes());
        z.extend_from_slice(&(!n).to_le_bytes());
        z.extend_from_slice(b);
    }
    z.extend_from_slice(&adler32(&raw).to_be_bytes());
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&(w as u32).to_be_bytes());
    ihdr.extend_from_slice(&(h as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr); // i18n-ok
    chunk(&mut out, b"IDAT", &z); // i18n-ok
    chunk(&mut out, b"IEND", &[]); // i18n-ok
    out
}

fn write_png(path: &Path, w: usize, h: usize, rgba: &[u8]) -> std::io::Result<()> {
    std::fs::write(path, encode_png(w, h, rgba))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_header_and_crc() {
        assert_eq!(crc32(b"IEND"), 0xAE42_6082);
        let p = encode_png(2, 1, &[1, 2, 3, 255, 4, 5, 6, 255]);
        assert_eq!(&p[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
        assert_eq!(&p[p.len() - 8..p.len() - 4], b"IEND");
    }
}
