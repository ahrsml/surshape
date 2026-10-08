//! Graph-Edit (página propia): breakpoints de un parámetro dibujados sobre
//! la onda de la entrada, con reproducción.
//!
//! Lienzo: clic en un lugar vacío = agregar punto · arrastrar un punto =
//! moverlo (y elegirlo) · clic derecho sobre un punto = quitarlo. El punto
//! elegido también se mueve con los sliders X (abajo) e Y (a la derecha).
//! Debajo: tabla de puntos editable, formas predefinidas (rampas,
//! exponencial, logarítmica, azar con seed), importar/exportar texto.
//! Los cambios se ven en vivo; Cancelar vuelve al valor de antes de abrir.

use crate::app::{App, DialogResult, Msg, Page};
use crate::theme::{self, mono_font};
use crate::win32::{self, Bevel, Icon, Level, Spin};
use eframe::egui::{self, pos2, vec2, Align2, Rect, Sense, Shape, Stroke, Ui};
use surshape_engine::{Breakpoints, Interp, ParamSpec, ParamValue, Scale, TimeMode};
use surshape_i18n::{t, tr};
use surshape_patch::NodeId;

/// Qué se está editando.
#[derive(Clone, Debug)]
pub(crate) struct BpEdit {
    pub node: NodeId,
    pub param: &'static str,
    /// Canal (si el nodo usa valores por canal).
    pub channel: Option<usize>,
    drag: Option<usize>,
    /// Punto elegido (para los sliders X/Y).
    pub sel: Option<usize>,
    pub desde: f64,
    pub hasta: f64,
    pub puntos_azar: usize,
    pub seed_azar: u64,
    /// Valor antes de abrir (Cancelar).
    pub original: Option<ParamValue>,
}

impl BpEdit {
    pub fn new(node: NodeId, spec: &ParamSpec, channel: Option<usize>) -> Self {
        Self {
            node,
            param: spec.id,
            channel,
            drag: None,
            sel: None,
            desde: spec.min,
            hasta: spec.max,
            puntos_azar: 8,
            seed_azar: 1,
            original: None,
        }
    }
}

/// Valor -> posición vertical 0..1 (log para parámetros logarítmicos).
fn v_to_y(spec: &ParamSpec, v: f64) -> f32 {
    let f = if spec.scale == Scale::Log && spec.min > 0.0 {
        (v.max(spec.min).ln() - spec.min.ln()) / (spec.max.ln() - spec.min.ln())
    } else {
        (v - spec.min) / (spec.max - spec.min)
    };
    f.clamp(0.0, 1.0) as f32
}

fn y_to_v(spec: &ParamSpec, f: f32) -> f64 {
    let f = f.clamp(0.0, 1.0) as f64;
    let v = if spec.scale == Scale::Log && spec.min > 0.0 {
        (spec.min.ln() + f * (spec.max.ln() - spec.min.ln())).exp()
    } else {
        spec.min + f * (spec.max - spec.min)
    };
    spec.clamp(v)
}

/// Formas predefinidas, en tiempo normalizado.
fn shape(kind: usize, a: f64, b: f64, n: usize, seed: u64) -> (Interp, Vec<(f64, f64)>) {
    match kind {
        0 => (Interp::Lineal, vec![(0.0, a), (1.0, b)]),
        1 => (Interp::Lineal, vec![(0.0, b), (1.0, a)]),
        2 => (Interp::Exponencial, vec![(0.0, a), (1.0, b)]),
        3 => (
            Interp::Lineal,
            (0..=8)
                .map(|i| {
                    let x = i as f64 / 8.0;
                    (x, a + (b - a) * (1.0 + 9.0 * x).log10())
                })
                .collect(),
        ),
        _ => {
            let mut s = surshape_engine::seed32(seed) as u64 | 1;
            let n = n.max(2);
            (
                Interp::Lineal,
                (0..n)
                    .map(|i| {
                        s ^= s << 13;
                        s ^= s >> 7;
                        s ^= s << 17;
                        let r = (s >> 11) as f64 / (1u64 << 53) as f64;
                        (i as f64 / (n - 1) as f64, a + (b - a) * r)
                    })
                    .collect(),
            )
        }
    }
}

const SHAPES: [&str; 5] = ["ui.bp.forma.subida", "ui.bp.forma.bajada", "ui.bp.forma.exponencial", "ui.bp.forma.logaritmica", "ui.bp.forma.azar"];

fn interp_key(i: Interp) -> &'static str {
    match i {
        Interp::Lineal => "ui.bp.lineal",
        Interp::Escalon => "ui.bp.escalon",
        Interp::Exponencial => "ui.bp.exponencial",
    }
}

fn decimals(spec: &ParamSpec) -> usize {
    match spec.kind {
        surshape_engine::ParamKind::Float { decimals } => decimals as usize,
        _ => 0,
    }
}

impl App {
    /// Duración de referencia del nodo (su entrada principal o, si es un
    /// generador, su parámetro "duracion").
    pub(crate) fn node_ref_duration(&self, id: NodeId) -> f64 {
        let Some(n) = self.session.as_ref().and_then(|s| s.patch.node(id)) else { return 1.0 };
        match n.entradas.first() {
            Some(p) => self.port_duration(p.nodo, p.salida).unwrap_or(1.0),
            None => n.params.comun.get("duracion").max(0.001), // i18n-ok
        }
    }

    pub(crate) fn graph_page(&mut self, ui: &mut Ui) {
        let Some(mut ed) = self.bp_edit.clone() else {
            self.page = Page::Main;
            return;
        };
        let Some(node) = self.session.as_ref().and_then(|s| s.patch.node(ed.node)).cloned() else {
            self.bp_edit = None;
            self.page = Page::Main;
            return;
        };
        let Some(proc_) = node.process_id().and_then(|p| self.registry.get(p)).cloned() else { return };
        let Some(spec) = proc_.params().iter().find(|s| s.id == ed.param).copied() else { return };
        let dur = self.node_ref_duration(ed.node);
        // La onda de la entrada se dibuja de fondo: pedir que se cargue.
        let _ = self.ensure_node_inputs(ed.node);
        let mut params = node.params.clone();
        let target = match (ed.channel, params.por_canal.as_mut()) {
            (Some(c), Some(v)) if c < v.len() => &mut v[c],
            _ => &mut params.comun,
        };
        let current = target.value(spec.id).cloned().unwrap_or(ParamValue::Fixed(spec.default));
        if ed.original.is_none() {
            ed.original = Some(current.clone());
        }
        let mut bp = match &current {
            ParamValue::Envelope(bp) if !bp.puntos.is_empty() => bp.clone(),
            v => Breakpoints::new(TimeMode::Normalizado, Interp::Lineal, vec![(0.0, v.initial()), (1.0, v.initial())]),
        };
        let label = self.session.as_ref().and_then(|s| s.patch.cell_label(ed.node)).unwrap_or_default();
        let tmode = if bp.tiempo == TimeMode::Absoluto { tr("ui.bp.absoluto") } else { tr("ui.bp.normalizado") };
        let title = t!("ui.bp.titulo", parametro = tr(spec.key), celda = label, tiempo = tmode);
        let mut close: Option<bool> = None; // Some(true) = aceptar, Some(false) = cancelar
        if self.page_title(ui, &title) {
            close = Some(true);
        }
        let mut to_fixed = false;
        let avail_h = ui.available_height();
        egui::ScrollArea::vertical().id_salt("graph_edit").auto_shrink([false, false]).max_height(avail_h - 30.0).show(ui, |ui| { // i18n-ok
            ui.set_width(ui.available_width());
            // --- Lienzo + slider Y ---
            let canvas_h = (avail_h * 0.42).clamp(150.0, 360.0);
            ui.horizontal(|ui| {
                let w = ui.available_width() - 30.0;
                let (rect, resp) = ui.allocate_exact_size(vec2(w.max(100.0), canvas_h), Sense::click_and_drag());
                win32::audit::record(ui, "graph", rect); // i18n-ok
                self.bp_canvas(ui, rect, &resp, &spec, &mut bp, &mut ed, dur);
                self.hint_key(&resp, "ui.ayuda.bp_lienzo");
                // Slider Y vertical del punto elegido.
                let (yr, yresp) = ui.allocate_exact_size(vec2(24.0, canvas_h), Sense::click_and_drag());
                let p = ui.painter();
                let track = Rect::from_center_size(yr.center(), vec2(4.0, yr.height() - 12.0));
                win32::bevel(p, track, Bevel::Sunken);
                if let Some(i) = ed.sel.filter(|&i| i < bp.puntos.len()) {
                    if let Some(pos) = yresp.interact_pointer_pos().filter(|_| yresp.dragged() || yresp.clicked()) {
                        let f = (track.bottom() - pos.y) / track.height();
                        bp.puntos[i].1 = y_to_v(&spec, f);
                    }
                    let y = track.bottom() - v_to_y(&spec, bp.puntos[i].1) * track.height();
                    let thumb = Rect::from_center_size(pos2(yr.center().x, y), vec2(18.0, 10.0));
                    p.rect_filled(thumb, 0.0, theme::t().FACE);
                    win32::bevel(p, thumb, Bevel::Raised);
                }
                self.hint_key(&yresp, "ui.ayuda.bp_slider");
            });
            // --- Slider X del punto elegido ---
            ui.horizontal(|ui| {
                win32::label(ui, tr("ui.bp.x"));
                let tmax = if bp.tiempo == TimeMode::Absoluto { dur.max(0.001) } else { 1.0 };
                match ed.sel.filter(|&i| i < bp.puntos.len()) {
                    Some(i) => {
                        let mut x = bp.puntos[i].0;
                        let r = win32::slider(ui, &mut x, 0.0, tmax, false, ui.available_width() - 4.0);
                        self.hint_key(&r, "ui.ayuda.bp_slider");
                        if r.changed() {
                            bp.puntos[i].0 = x;
                        }
                    }
                    None => {
                        win32::muted(ui, tr("ui.bp.elegir_punto"));
                    }
                }
            });
            // --- Reproducción y opciones ---
            ui.horizontal_wrapped(|ui| {
                if let Some(audio) = self.node_input_audio(ed.node) {
                    let playing = self.player.is_playing_buf(&audio);
                    let r = win32::tool_button(ui, Icon::Play, tr("ui.bp.escuchar"), !playing, playing);
                    self.hint_key(&r, "ui.ayuda.bp_escuchar");
                    if r.clicked() {
                        let n = audio.frames();
                        self.player.play(audio.clone(), 0, n, false);
                    }
                    let r = win32::tool_button(ui, Icon::Stop, tr("ui.visor.detener"), playing, false);
                    if r.clicked() {
                        self.player.stop();
                    }
                }
                ui.add_space(6.0);
                let l = win32::label(ui, tr("ui.bp.tiempo"));
                self.hint_key(&l, "ui.ayuda.bp_tiempo");
                let before = bp.tiempo;
                if win32::radio(ui, bp.tiempo == TimeMode::Absoluto, tr("ui.bp.absoluto")).clicked() {
                    bp.tiempo = TimeMode::Absoluto;
                }
                if win32::radio(ui, bp.tiempo == TimeMode::Normalizado, tr("ui.bp.normalizado")).clicked() {
                    bp.tiempo = TimeMode::Normalizado;
                }
                if bp.tiempo != before && dur > 0.0 {
                    // Mismo lugar en el tiempo, distinta escala.
                    let f = if bp.tiempo == TimeMode::Normalizado { 1.0 / dur } else { dur };
                    bp.puntos.iter_mut().for_each(|p| p.0 *= f);
                }
                ui.add_space(6.0);
                let l = win32::label(ui, tr("ui.bp.forma_curva"));
                self.hint_key(&l, "ui.ayuda.bp_curva");
                let cur = bp.curva;
                let mut chosen = cur;
                win32::combo(ui, "bp_interp", tr(interp_key(cur)), 130.0, |ui| { // i18n-ok
                    for i in [Interp::Lineal, Interp::Escalon, Interp::Exponencial] {
                        if win32::combo_item(ui, i == cur, tr(interp_key(i))).clicked() {
                            chosen = i;
                        }
                    }
                });
                bp.curva = chosen;
            });
            // --- Formas ---
            ui.horizontal_wrapped(|ui| {
                let l = win32::label(ui, tr("ui.bp.formas"));
                self.hint_key(&l, "ui.ayuda.bp_formas");
                let s = Spin { min: spec.min, max: spec.max, step: ((spec.max - spec.min) / 100.0).max(0.001), decimals: decimals(&spec), suffix: "", width: 80.0 };
                win32::label(ui, tr("ui.bp.desde"));
                win32::spin(ui, &mut ed.desde, s);
                win32::label(ui, tr("ui.bp.hasta"));
                win32::spin(ui, &mut ed.hasta, s);
                for (k, key) in SHAPES.iter().enumerate() {
                    let b = win32::button_w(ui, tr(key), 50.0, true);
                    self.hint_key(&b, "ui.ayuda.bp_formas");
                    if b.clicked() {
                        let (interp, pts) = shape(k, ed.desde, ed.hasta, ed.puntos_azar, ed.seed_azar);
                        bp.curva = interp;
                        bp.puntos = match bp.tiempo {
                            TimeMode::Normalizado => pts,
                            TimeMode::Absoluto => pts.into_iter().map(|(x, v)| (x * dur, v)).collect(),
                        };
                        ed.sel = None;
                        if k == 4 {
                            ed.seed_azar += 1;
                        }
                    }
                }
                win32::label(ui, tr("ui.bp.puntos_azar"));
                let mut n = ed.puntos_azar as f64;
                if win32::spin(ui, &mut n, Spin { min: 2.0, max: 64.0, step: 1.0, decimals: 0, suffix: "", width: 50.0 }).changed() {
                    ed.puntos_azar = n as usize;
                }
                win32::label(ui, tr("ui.proceso.seed"));
                let mut sd = ed.seed_azar as f64;
                if win32::spin(ui, &mut sd, Spin { min: 0.0, max: 1e9, step: 1.0, decimals: 0, suffix: "", width: 70.0 }).changed() {
                    ed.seed_azar = sd as u64;
                }
            });
            // --- Tabla de puntos + importar/exportar ---
            ui.horizontal_top(|ui| {
                let half = (ui.available_width() * 0.55).max(260.0);
                ui.allocate_ui(vec2(half, 220.0), |ui| {
                    win32::group(ui, tr("ui.bp.puntos"), |ui| self.bp_table(ui, &spec, &mut bp, &mut ed, dur));
                });
                ui.vertical(|ui| {
                    let b = win32::button_w(ui, tr("ui.bp.importar"), 120.0, true);
                    self.hint_key(&b, "ui.ayuda.bp_importar");
                    if b.clicked() {
                        self.open_dialog(|d| d.add_filter("brk", &["brk", "txt"]).pick_file().map(DialogResult::BpImport)); // i18n-ok
                    }
                    let b = win32::button_w(ui, tr("ui.bp.exportar"), 120.0, true);
                    self.hint_key(&b, "ui.ayuda.bp_exportar");
                    if b.clicked() {
                        let txt = bp.to_text();
                        self.open_dialog(move |d| d.add_filter("brk", &["brk", "txt"]).save_file().map(|p| DialogResult::BpExport(p, txt))); // i18n-ok
                    }
                    let markers = self.row_source(ed.node).and_then(|s| self.session.as_ref()?.patch.node(s)?.source().map(|x| x.marcadores.clone())).unwrap_or_default();
                    let b = win32::button_w(ui, tr("ui.bp.desde_marcadores"), 120.0, !markers.is_empty());
                    self.hint_key(&b, "ui.ayuda.bp_desde_marcadores");
                    if b.clicked() {
                        // Un punto en cada marcador, con el valor que la curva tiene ahí.
                        let scale = if bp.tiempo == TimeMode::Normalizado { 1.0 / dur.max(1e-9) } else { 1.0 };
                        for m in &markers {
                            let v = bp.value_at(m.t, dur);
                            bp.puntos.push((m.t * scale, v));
                        }
                        bp.puntos.sort_by(|a, b| a.0.total_cmp(&b.0));
                        bp.puntos.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-9);
                    }
                    let b = win32::button_w(ui, tr("ui.bp.a_marcadores"), 120.0, self.row_source(ed.node).is_some());
                    self.hint_key(&b, "ui.ayuda.bp_a_marcadores");
                    if b.clicked() {
                        let tscale = if bp.tiempo == TimeMode::Normalizado { dur } else { 1.0 };
                        let pts: Vec<f64> = bp.puntos.iter().map(|p| p.0 * tscale).collect();
                        if let Some(src) = self.row_source(ed.node) {
                            self.push_undo();
                            if let Some(surshape_patch::NodeKind::Fuente(s)) = self.session.as_mut().and_then(|s| s.patch.node_mut(src)).map(|n| &mut n.tipo) {
                                for (i, t) in pts.into_iter().enumerate() {
                                    if !s.marcadores.iter().any(|m| (m.t - t).abs() < 1e-6) {
                                        s.marcadores.push(surshape_patch::Marker { t, fin: None, etiqueta: format!("bp{}", i + 1) }); // i18n-ok
                                    }
                                }
                                s.marcadores.sort_by(|a, b| a.t.total_cmp(&b.t));
                            }
                            self.save_now();
                        }
                    }
                    ui.add_space(6.0);
                    let b = win32::button_w(ui, tr("ui.bp.fijar"), 120.0, true);
                    self.hint_key(&b, "ui.ayuda.param_fijar");
                    if b.clicked() {
                        to_fixed = true;
                    }
                });
            });
        });
        // --- Aceptar / Cancelar / Aplicar ---
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let b = win32::button(ui, tr("ui.dialogo.aplicar"));
            self.hint_key(&b, "ui.ayuda.aplicar");
            let b = win32::button(ui, tr("ui.dialogo.cancelar"));
            self.hint_key(&b, "ui.ayuda.bp_cancelar");
            if b.clicked() {
                close = Some(false);
            }
            let b = win32::button(ui, tr("ui.dialogo.aceptar"));
            self.hint_key(&b, "ui.ayuda.aceptar");
            if b.clicked() {
                close = Some(true);
            }
        });

        // Aplicar (en vivo)
        bp.puntos.sort_by(|a, b| a.0.total_cmp(&b.0));
        let new_value = match close {
            Some(false) => ed.original.clone().unwrap_or(ParamValue::Fixed(spec.default)),
            _ if to_fixed => ParamValue::Fixed(bp.puntos.first().map_or(spec.default, |p| p.1)),
            _ => ParamValue::Envelope(bp),
        };
        if new_value != current {
            self.before_param_edit(ed.node);
            target.set_value(spec.id, new_value);
            if let Some(n) = self.session.as_mut().and_then(|s| s.patch.node_mut(ed.node)) {
                n.params = params;
            }
            self.mark_dirty();
        }
        if close.is_some() || to_fixed {
            self.bp_edit = None;
            self.page = Page::Params(ed.node);
        } else {
            self.bp_edit = Some(ed);
        }
    }

    fn bp_table(&mut self, ui: &mut Ui, spec: &ParamSpec, bp: &mut Breakpoints, ed: &mut BpEdit, dur: f64) {
        let unit_t = if bp.tiempo == TimeMode::Absoluto { tr("ui.unidad.s") } else { "" };
        let mut remove = None;
        egui::ScrollArea::vertical().id_salt("bp_tabla").max_height(150.0).auto_shrink([false, true]).show(ui, |ui| { // i18n-ok
            egui::Grid::new("bp_grid").num_columns(4).spacing([6.0, 2.0]).show(ui, |ui| { // i18n-ok
                win32::head(ui, tr("ui.bp.col_n"));
                win32::head(ui, tr("ui.bp.col_tiempo"));
                win32::head(ui, tr("ui.bp.col_valor"));
                ui.label("");
                ui.end_row();
                let tmax = if bp.tiempo == TimeMode::Absoluto { dur.max(0.001) * 10.0 } else { 1.0 };
                for (i, p) in bp.puntos.iter_mut().enumerate() {
                    let r = win32::list_row(ui, ed.sel == Some(i), &format!("{}", i + 1), "", true);
                    if r.clicked() {
                        ed.sel = Some(i);
                    }
                    win32::spin(ui, &mut p.0, Spin { min: 0.0, max: tmax, step: tmax / 100.0, decimals: 4, suffix: unit_t, width: 100.0 });
                    let s = Spin { min: spec.min, max: spec.max, step: ((spec.max - spec.min) / 100.0).max(0.001), decimals: decimals(spec), suffix: "", width: 90.0 };
                    win32::spin(ui, &mut p.1, s);
                    let b = win32::button_w(ui, tr("ui.bp.quitar"), 54.0, true);
                    self.hint_key(&b, "ui.ayuda.bp_quitar");
                    if b.clicked() {
                        remove = Some(i);
                    }
                    ui.end_row();
                }
            });
        });
        if let Some(i) = remove {
            if bp.puntos.len() > 1 {
                bp.puntos.remove(i);
                ed.sel = None;
            }
        }
        let b = win32::button_w(ui, tr("ui.bp.agregar"), 90.0, true);
        self.hint_key(&b, "ui.ayuda.bp_agregar");
        if b.clicked() {
            let end = if bp.tiempo == TimeMode::Absoluto { dur } else { 1.0 };
            let last = bp.puntos.last().copied().unwrap_or((0.0, spec.default));
            let tnew = ((last.0 + end) * 0.5).min(end);
            bp.puntos.push((if tnew <= last.0 { end } else { tnew }, last.1));
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn bp_canvas(&mut self, ui: &Ui, rect: Rect, resp: &egui::Response, spec: &ParamSpec, bp: &mut Breakpoints, ed: &mut BpEdit, dur: f64) {
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 0.0, theme::t().VIEW_BG);
        let tscale = if bp.tiempo == TimeMode::Absoluto { 1.0 } else { dur };
        let x_of = |t: f64| rect.left() + ((t * tscale / dur.max(1e-9)) as f32).clamp(0.0, 1.0) * rect.width();
        let y_of = |v: f64| rect.bottom() - v_to_y(spec, v) * rect.height();
        let t_of = |x: f32| ((x - rect.left()) / rect.width()).clamp(0.0, 1.0) as f64 * dur / tscale;

        // Onda de la entrada, tenue, como referencia.
        if let Some(loaded) = self.node_input_loaded(ed.node) {
            let a = &loaded.audio;
            let n = a.frames();
            let mid = rect.center().y;
            let amp = rect.height() * 0.45;
            let col = theme::bp_ghost();
            let mut x = rect.left();
            while x < rect.right() {
                let s = (((x - rect.left()) / rect.width()) as f64 * n as f64) as usize;
                let e = ((((x + 1.0 - rect.left()) / rect.width()) as f64 * n as f64) as usize).max(s + 1);
                if let Some((lo, hi)) = loaded.peaks.channels[0].range(&a.channels[0], s, e) {
                    p.line_segment([pos2(x, mid - hi * amp), pos2(x, mid - lo * amp)], Stroke::new(1.0_f32, col));
                }
                x += 1.0;
            }
            // Cabezal
            if self.player.is_playing_buf(&loaded.audio) {
                let px = rect.left() + (self.player.position() as f32 / n.max(1) as f32) * rect.width();
                p.line_segment([pos2(px, rect.top()), pos2(px, rect.bottom())], Stroke::new(2.0_f32, theme::t().VIEW_CURSOR));
            }
        }
        // Grilla y rótulos de valor
        for f in [0.0f32, 0.5, 1.0] {
            let y = rect.bottom() - f * rect.height();
            p.line_segment([pos2(rect.left(), y), pos2(rect.right(), y)], Stroke::new(1.0_f32, theme::t().VIEW_GRID));
            let label = crate::page::fmt_param(spec, y_to_v(spec, f));
            let g = p.layout_no_wrap(label, mono_font(11.0), theme::t().VIEW_TEXT);
            let r = Rect::from_min_size(pos2(rect.left() + 2.0, (y - g.size().y).max(rect.top())), g.size() + vec2(4.0, 0.0));
            p.rect_filled(r, 0.0, theme::t().VIEW_BG);
            p.galley(r.min + vec2(2.0, 0.0), g, theme::t().VIEW_TEXT);
        }

        // Interacción
        let hit = |pos: egui::Pos2, pts: &[(f64, f64)]| pts.iter().position(|&(t, v)| (pos2(x_of(t), y_of(v)) - pos).length() < 9.0);
        if resp.drag_started() {
            if let Some(pos) = resp.interact_pointer_pos() {
                ed.drag = hit(pos, &bp.puntos);
                ed.sel = ed.drag.or(ed.sel);
            }
        }
        if let (Some(i), Some(pos)) = (ed.drag, resp.interact_pointer_pos()) {
            if resp.dragged() && i < bp.puntos.len() {
                bp.puntos[i] = (t_of(pos.x), y_to_v(spec, (rect.bottom() - pos.y) / rect.height()));
                // Mantener el orden siguiendo al punto arrastrado.
                let moved = bp.puntos[i];
                bp.puntos.sort_by(|a, b| a.0.total_cmp(&b.0));
                ed.drag = bp.puntos.iter().position(|p| *p == moved);
                ed.sel = ed.drag;
            }
        }
        if resp.drag_stopped() {
            ed.drag = None;
        }
        if resp.clicked() {
            if let Some(pos) = resp.interact_pointer_pos() {
                match hit(pos, &bp.puntos) {
                    Some(i) => ed.sel = Some(i),
                    None => {
                        let np = (t_of(pos.x), y_to_v(spec, (rect.bottom() - pos.y) / rect.height()));
                        bp.puntos.push(np);
                        bp.puntos.sort_by(|a, b| a.0.total_cmp(&b.0));
                        ed.sel = bp.puntos.iter().position(|p| *p == np);
                    }
                }
            }
        }
        if resp.secondary_clicked() {
            if let Some(i) = resp.interact_pointer_pos().and_then(|pos| hit(pos, &bp.puntos)) {
                if bp.puntos.len() > 1 {
                    bp.puntos.remove(i);
                    ed.sel = None;
                }
            }
        }

        // Curva y puntos
        let mut pts = Vec::with_capacity(rect.width() as usize + 1);
        let mut x = rect.left();
        while x <= rect.right() {
            let t = t_of(x) * tscale;
            pts.push(pos2(x, y_of(bp.value_at(t, dur))));
            x += 2.0;
        }
        p.add(Shape::line(pts, Stroke::new(2.0_f32, theme::t().ACCENT)));
        for (i, &(t, v)) in bp.puntos.iter().enumerate() {
            let c = pos2(x_of(t), y_of(v));
            let active = ed.drag == Some(i) || ed.sel == Some(i);
            let r = Rect::from_center_size(c, vec2(if active { 9.0 } else { 7.0 }, if active { 9.0 } else { 7.0 }));
            p.rect_filled(r, 0.0, if active { theme::t().VIEW_CURSOR } else { theme::t().ACCENT_2 });
            p.rect_stroke(r, 0.0, Stroke::new(1.0_f32, theme::t().DARK), egui::StrokeKind::Inside);
        }
        win32::bevel(&p, rect, Bevel::Sunken);
        if let Some(pos) = resp.hover_pos() {
            let v = y_to_v(spec, (rect.bottom() - pos.y) / rect.height());
            let tt = t_of(pos.x) * tscale;
            let txt = format!("{} · {}", self.prefs.unidades.fmt(tt, 48000), crate::page::fmt_param(spec, v));
            p.text(pos2(rect.right() - 6.0, rect.top() + 4.0), Align2::RIGHT_TOP, txt, mono_font(12.0), theme::t().VIEW_TEXT);
        }
    }

    /// Aplica breakpoints importados de un archivo de texto al editor abierto.
    pub(crate) fn bp_import(&mut self, path: &std::path::Path) {
        let Some(ed) = self.bp_edit.clone() else { return };
        let txt = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => {
                self.status = Some(Msg::new(Level::Error, "err.audio.abrir").arg("archivo", path.display()).arg("detalle", e));
                return;
            }
        };
        let Some(n) = self.session.as_ref().and_then(|s| s.patch.node(ed.node)).cloned() else { return };
        let mode = match n.params.comun.value(ed.param) {
            Some(ParamValue::Envelope(bp)) => bp.tiempo,
            _ => TimeMode::Absoluto,
        };
        match Breakpoints::from_text(&txt, mode, Interp::Lineal) {
            Ok(bp) => {
                self.before_param_edit(ed.node);
                if let Some(node) = self.session.as_mut().and_then(|s| s.patch.node_mut(ed.node)) {
                    let target = match (ed.channel, node.params.por_canal.as_mut()) {
                        (Some(c), Some(v)) if c < v.len() => &mut v[c],
                        _ => &mut node.params.comun,
                    };
                    target.set_value(ed.param, ParamValue::Envelope(bp));
                }
                self.mark_dirty();
            }
            Err(e) => {
                self.status = Some(Msg::new(Level::Error, e.key).arg("linea", e.line));
            }
        }
    }
}
