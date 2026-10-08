//! Visor de forma de onda (osciloscopio sin negro): regla de tiempo, un
//! carril por canal visible, selección, zoom horizontal y vertical, bucle de
//! la celda y cabezal de reproducción.
//!
//! Ratón: arrastrar = seleccionar · clic = mover el cursor (y quitar la
//! selección) · rueda = zoom alrededor del puntero · Mayús + rueda o rueda
//! horizontal = desplazarse · Ctrl + rueda = zoom vertical.

use crate::prefs::TimeUnit;
use crate::theme::{self, mono_font};
use crate::Loaded;
use eframe::egui::{self, pos2, vec2, Align2, Rect, Response, Sense, Shape, Stroke, Ui};
use surshape_i18n::t;

/// Alto de la regla de tiempo.
const RULER_H: f32 = 16.0;
/// Menor cantidad de muestras visibles (zoom máximo).
const MIN_SPAN: f64 = 32.0;

#[derive(Clone, Debug, Default)]
pub struct ViewState {
    /// Primera muestra visible.
    pub start: f64,
    /// Muestras visibles (0 = ajustar todo en el próximo dibujo).
    pub span: f64,
    /// Selección [a, b) en muestras.
    pub sel: Option<(usize, usize)>,
    /// Cursor: desde dónde se reproduce si no hay selección.
    pub cursor: usize,
    drag_anchor: Option<usize>,
}

impl ViewState {
    pub fn fit(&mut self, frames: usize) {
        self.start = 0.0;
        self.span = (frames as f64).max(MIN_SPAN);
    }

    pub fn zoom(&mut self, factor: f64, around: f64, frames: usize) {
        let total = (frames as f64).max(MIN_SPAN);
        let new_span = (self.span * factor).clamp(MIN_SPAN, total);
        let rel = if self.span > 0.0 { (around - self.start) / self.span } else { 0.5 };
        self.start = around - rel * new_span;
        self.span = new_span;
        self.clamp(frames);
    }

    fn clamp(&mut self, frames: usize) {
        let total = frames as f64;
        self.start = self.start.clamp(0.0, (total - self.span).max(0.0));
    }

    /// Selección válida (no vacía).
    pub fn selection(&self) -> Option<(usize, usize)> {
        self.sel.filter(|(a, b)| b > a)
    }
}

/// Opciones de dibujo del visor.
pub struct ViewOpts<'a> {
    /// Zoom vertical (1 = a escala completa).
    pub vzoom: &'a mut f32,
    /// Canales ocultos (bit n = canal n).
    pub hidden: u64,
    /// Bucle de la celda (muestras).
    pub loop_region: Option<(usize, usize)>,
    pub unit: TimeUnit,
    /// Marcadores de la fuente de la fila (segundos).
    pub markers: &'a [surshape_patch::Marker],
}

/// Dibuja el visor en `rect` y maneja el ratón. `playhead` = posición del
/// reproductor si está sonando este sample. Con `spectro` (una textura por
/// canal) se dibuja el espectrograma en lugar de la forma de onda.
pub fn show(
    ui: &mut Ui,
    rect: Rect,
    st: &mut ViewState,
    data: &Loaded,
    playhead: Option<usize>,
    spectro: Option<&[egui::TextureHandle]>,
    opts: ViewOpts,
) -> Response {
    let audio = &data.audio;
    let frames = audio.frames();
    if st.span <= 0.0 {
        st.fit(frames);
    }
    let resp = ui.interact(rect, ui.id().with("visor_onda"), Sense::click_and_drag()); // i18n-ok
    crate::win32::audit::record(ui, "visor", rect); // i18n-ok
    let wave_rect = Rect::from_min_max(pos2(rect.left(), rect.top() + RULER_H), rect.max);
    let px_to_frame = |x: f32| -> f64 { st.start + ((x - rect.left()) / rect.width()) as f64 * st.span };

    // --- Ratón ---
    if resp.hovered() {
        let (scroll, shift, ctrl) = ui.input(|i| (i.smooth_scroll_delta, i.modifiers.shift, i.modifiers.command));
        if let Some(pos) = resp.hover_pos() {
            let pan = if shift { scroll.y } else { scroll.x };
            if ctrl && scroll.y != 0.0 {
                *opts.vzoom = (*opts.vzoom * (scroll.y * 0.004).exp()).clamp(1.0, 64.0);
            } else if pan != 0.0 {
                st.start -= (pan / rect.width()) as f64 * st.span;
                st.clamp(frames);
            } else if scroll.y != 0.0 {
                let around = px_to_frame(pos.x);
                st.zoom((-scroll.y as f64 * 0.004).exp(), around, frames);
            }
        }
    }
    let to_frame = |x: f32, st: &ViewState| -> usize {
        (st.start + ((x - rect.left()) / rect.width()) as f64 * st.span).clamp(0.0, frames as f64) as usize
    };
    if resp.drag_started() {
        if let Some(p) = resp.interact_pointer_pos() {
            st.drag_anchor = Some(to_frame(p.x, st));
        }
    }
    if resp.dragged() {
        if let (Some(a), Some(p)) = (st.drag_anchor, resp.interact_pointer_pos()) {
            let b = to_frame(p.x, st);
            st.sel = Some((a.min(b), a.max(b)));
            st.cursor = a.min(b);
        }
    }
    if resp.drag_stopped() {
        st.drag_anchor = None;
    }
    if resp.clicked() {
        if let Some(p) = resp.interact_pointer_pos() {
            st.sel = None;
            st.cursor = to_frame(p.x, st);
        }
    }
    // El cabezal arrastra la vista si se sale de ella.
    if let Some(ph) = playhead {
        let ph = ph as f64;
        if ph > st.start + st.span || ph < st.start {
            st.start = ph;
            st.clamp(frames);
        }
    }

    // --- Dibujo ---
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 0.0, theme::t().VIEW_BG);
    let x_of = |f: f64| rect.left() + ((f - st.start) / st.span) as f32 * rect.width();

    // Bucle de la celda: franja en la regla y líneas.
    if let Some((a, b)) = opts.loop_region.filter(|(a, b)| b > a) {
        for x in [x_of(a as f64), x_of(b as f64)] {
            p.line_segment([pos2(x, rect.top()), pos2(x, rect.bottom())], Stroke::new(1.0_f32, theme::t().VIEW_LOOP));
        }
    }
    // Marcadores: línea y nombre (las regiones, con una franja en la regla).
    let srf = audio.sr as f64;
    for m in opts.markers {
        let x = x_of(m.t * srf);
        if let Some(f) = m.fin {
            let x2 = x_of(f * srf);
            p.rect_filled(Rect::from_x_y_ranges(x.max(rect.left())..=x2.min(rect.right()), rect.top() + RULER_H - 3.0..=rect.top() + RULER_H), 0.0, theme::t().VIEW_MARKER);
            p.line_segment([pos2(x2, wave_rect.top()), pos2(x2, wave_rect.bottom())], Stroke::new(1.0_f32, theme::t().VIEW_MARKER));
        }
        if (rect.left()..=rect.right()).contains(&x) {
            p.line_segment([pos2(x, wave_rect.top()), pos2(x, wave_rect.bottom())], Stroke::new(1.0_f32, theme::t().VIEW_MARKER));
            if !m.etiqueta.is_empty() {
                p.text(pos2(x + 3.0, wave_rect.bottom() - 2.0), Align2::LEFT_BOTTOM, &m.etiqueta, mono_font(11.0), theme::t().VIEW_TEXT);
            }
        }
    }
    // Selección
    if let Some((a, b)) = st.selection() {
        let r = Rect::from_x_y_ranges(x_of(a as f64).max(rect.left())..=x_of(b as f64).min(rect.right()), wave_rect.y_range());
        if r.width() > 0.0 {
            p.rect_filled(r, 0.0, theme::view_selection());
        }
    }

    // Regla de tiempo
    ruler(&p, rect, st, audio.sr, opts.unit);

    // Carriles (solo canales visibles)
    let visible: Vec<usize> = (0..audio.num_channels()).filter(|c| opts.hidden & (1 << c.min(&63)) == 0).collect();
    let nlanes = visible.len().max(1);
    let lane_h = wave_rect.height() / nlanes as f32;
    let spp = st.span / rect.width() as f64; // muestras por píxel
    let vz = *opts.vzoom;
    for (li, &ci) in visible.iter().enumerate() {
        let ch = &audio.channels[ci];
        let top = wave_rect.top() + li as f32 * lane_h;
        let mid = top + lane_h * 0.5;
        let amp = lane_h * 0.46 * vz;
        if li > 0 {
            p.line_segment([pos2(rect.left(), top), pos2(rect.right(), top)], Stroke::new(1.0_f32, theme::t().VIEW_GRID));
        }
        p.line_segment([pos2(rect.left(), mid), pos2(rect.right(), mid)], Stroke::new(1.0_f32, theme::t().VIEW_GRID));
        let lane = Rect::from_min_size(pos2(rect.left(), top), vec2(rect.width(), lane_h));
        let lp = p.with_clip_rect(lane);
        let y = |v: f32| mid - v * amp;
        if let Some(tex) = spectro.and_then(|t| t.get(ci)) {
            let total = frames.max(1) as f32;
            let uv = Rect::from_min_max(pos2(st.start as f32 / total, 0.0), pos2((st.start + st.span) as f32 / total, 1.0));
            lp.image(tex.id(), lane, uv, egui::Color32::WHITE);
            freq_axis(&lp, lane, audio.sr as f32);
        } else if spp >= 1.0 {
            let stroke = Stroke::new(1.0_f32, theme::t().VIEW_WAVE);
            let mut x = rect.left();
            while x < rect.right() {
                let s = (st.start + ((x - rect.left()) as f64) * spp) as usize;
                let e = (st.start + ((x + 1.0 - rect.left()) as f64) * spp).ceil() as usize;
                if let Some((lo, hi)) = data.peaks.channels[ci].range(ch, s, e.max(s + 1)) {
                    let (y0, y1) = (y(hi), y(lo));
                    lp.line_segment([pos2(x + 0.5, y0), pos2(x + 0.5, y1.max(y0 + 1.0))], stroke);
                }
                x += 1.0;
            }
        } else {
            // Zoom profundo: línea entre muestras.
            let s = st.start.floor() as usize;
            let e = ((st.start + st.span).ceil() as usize + 1).min(ch.len());
            let pts: Vec<_> = (s..e).map(|i| pos2(x_of(i as f64), y(ch[i]))).collect();
            lp.add(Shape::line(pts, Stroke::new(1.5_f32, theme::t().VIEW_WAVE)));
        }
        // Número de canal (lectura)
        p.text(lane.left_top() + vec2(4.0, 2.0), Align2::LEFT_TOP, t!("ui.visor.canal_corto", n = ci + 1), mono_font(11.0), theme::t().VIEW_TEXT);
    }
    if vz > 1.001 {
        p.text(pos2(rect.right() - 4.0, wave_rect.top() + 2.0), Align2::RIGHT_TOP, format!("x{vz:.1}"), mono_font(11.0), theme::t().VIEW_TEXT); // i18n-ok
    }

    // Cursor y cabezal
    let cx = x_of(st.cursor as f64);
    if (rect.left()..=rect.right()).contains(&cx) {
        p.line_segment([pos2(cx, wave_rect.top()), pos2(cx, wave_rect.bottom())], Stroke::new(1.0_f32, theme::t().VIEW_CURSOR));
    }
    if let Some(ph) = playhead {
        let x = x_of(ph as f64);
        p.line_segment([pos2(x, rect.top()), pos2(x, rect.bottom())], Stroke::new(2.0_f32, theme::t().VIEW_CURSOR));
    }
    crate::win32::bevel(&p, rect, crate::win32::Bevel::Sunken);
    resp
}

/// Frecuencia mínima del espectrograma (Hz); el máximo es Nyquist.
pub const SPECTRO_F_MIN: f32 = 30.0;

/// Marcas de frecuencia (escala log) a la izquierda de un carril del
/// espectrograma.
fn freq_axis(p: &egui::Painter, lane: Rect, sr: f32) {
    let nyq = sr / 2.0;
    for (f, label) in [(100.0, t!("ui.visor.hz_100")), (1000.0, t!("ui.visor.hz_1k")), (10000.0, t!("ui.visor.hz_10k"))] {
        if f >= nyq {
            continue;
        }
        let frac = (f / SPECTRO_F_MIN).ln() / (nyq / SPECTRO_F_MIN).ln();
        let y = lane.bottom() - frac * lane.height();
        let g = p.layout_no_wrap(label.to_string(), mono_font(11.0), theme::t().SPECTRO_AXIS_TEXT);
        let r = Rect::from_min_size(pos2(lane.left() + 20.0, y - g.size().y * 0.5), g.size() + vec2(6.0, 0.0));
        p.rect_filled(r, 0.0, theme::t().SPECTRO_AXIS_BG);
        p.galley(r.min + vec2(3.0, 0.0), g, theme::t().SPECTRO_AXIS_TEXT);
    }
}

/// Regla con marcas a intervalos "redondos" (>= 90 px entre etiquetas), en
/// las unidades de tiempo elegidas.
fn ruler(p: &egui::Painter, rect: Rect, st: &ViewState, sr: u32, unit: TimeUnit) {
    let r = Rect::from_min_size(rect.min, vec2(rect.width(), RULER_H));
    p.rect_filled(r, 0.0, theme::t().VIEW_BG);
    p.line_segment([r.left_bottom(), r.right_bottom()], Stroke::new(1.0_f32, theme::t().VIEW_GRID));
    if sr == 0 || st.span <= 0.0 {
        return;
    }
    let srf = sr as f64;
    let secs_per_px = st.span / srf / rect.width() as f64;
    const STEPS: [f64; 19] =
        [0.001, 0.002, 0.005, 0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0, 15.0, 30.0, 60.0, 120.0, 300.0, 600.0];
    let min_px = if unit == TimeUnit::Hms { 110.0 } else { 90.0 };
    let step = STEPS.iter().copied().find(|s| s / secs_per_px >= min_px).unwrap_or(1200.0);
    let t0 = st.start / srf;
    let t1 = (st.start + st.span) / srf;
    let mut t = (t0 / step).ceil() * step;
    while t <= t1 {
        let x = rect.left() + ((t - t0) / (t1 - t0)) as f32 * rect.width();
        p.line_segment([pos2(x, r.bottom() - 5.0), pos2(x, r.bottom())], Stroke::new(1.0_f32, theme::t().VIEW_TEXT));
        let label = match unit {
            TimeUnit::Segundos => {
                let s = format!("{t:.3}");
                s.trim_end_matches('0').trim_end_matches('.').to_string()
            }
            u => u.fmt(t, sr),
        };
        p.text(pos2(x + 3.0, r.top() + 1.0), Align2::LEFT_TOP, label, mono_font(11.0), theme::t().VIEW_TEXT);
        t += step;
    }
}
