//! Grilla de patch tipo planilla (como Soundshaper): filas A..P, columnas
//! 0..98. La columna 0 es la fuente de la fila (archivo, generador, mezcla)
//! o una referencia a otra celda (`→A_1`), que es como se ve una rama; las
//! celdas siguientes son los procesos, de izquierda a derecha.
//!
//! Cada celda muestra el nombre del proceso, su ícono de estado (con texto
//! en la línea de ayuda), el tipo de archivo que produce (wav, ana...) y
//! "auto" si convierte su entrada sola. Clic: elige la celda y la carga en
//! el visor · doble clic: página de parámetros · clic derecho: operaciones
//! de celda · Mayús+clic: tramo de celdas de la fila.
//!
//! Se dibuja solo lo visible; las barras de desplazamiento tienen flechas.

use crate::app::{App, Page};
use crate::shell::Action;
use crate::theme::{self, body_font, bold_font, ui_font};
use crate::widgets::{paint_state_icon, CellState};
use crate::win32::{self, Bevel};
use eframe::egui::{pos2, vec2, Align2, Rect, Sense, Ui};
use std::sync::atomic::Ordering;
use surshape_engine::process::name_key;
use surshape_engine::FileKind;
use surshape_i18n::{t, tr};
use surshape_patch::{row_name, NodeId, NodeKind, PortRef, Status, MAX_COLS, MAX_ROWS};

const HDR_W: f32 = 26.0;
const HDR_H: f32 = 18.0;
const CELL_W: f32 = 118.0;
const CELL_H: f32 = 36.0;
const SB: f32 = 16.0;

/// Lo que hay en una posición de la planilla.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Slot {
    Empty,
    Node(NodeId),
    /// Columna 0 de una fila que nace de otra celda.
    Origin(PortRef),
}

impl App {
    /// Estado de una celda para mostrar (agrega "en proceso" / "en cola").
    pub(crate) fn cell_state(&self, id: NodeId) -> CellState {
        if let Some(run) = &self.run {
            if run.status.node.load(Ordering::Relaxed) == id && run.task.progress() < 1.0 {
                return CellState::EnProceso(run.task.progress());
            }
            if run.queue.contains(&id) && !run.done_nodes().contains(&id) {
                return CellState::EnCola;
            }
        }
        if self.missing.contains(&id) {
            return CellState::FuenteFalta;
        }
        let Some(s) = &self.session else { return CellState::Error };
        match s.patch.status(id, &self.keys) {
            Status::Fuente => CellState::Fuente,
            Status::SinRender => CellState::SinRender,
            Status::Renderizado => CellState::Renderizado,
            Status::Desactualizado => CellState::Desactualizado,
            Status::Error => CellState::Error,
        }
    }

    /// Nombre visible de un nodo.
    pub(crate) fn node_name(&self, id: NodeId) -> String {
        let Some(n) = self.session.as_ref().and_then(|s| s.patch.node(id)) else { return String::new() };
        match &n.tipo {
            NodeKind::Fuente(src) => src.ruta.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
            NodeKind::Proceso { proceso } => tr(&name_key(proceso)).to_string(),
            NodeKind::Mezcla { .. } => t!("ui.nodo.mezcla").to_string(),
            NodeKind::SubPatch { nombre, .. } if !nombre.is_empty() => nombre.clone(),
            NodeKind::SubPatch { .. } => t!("ui.nodo.subpatch").to_string(),
        }
    }

    /// Tipo de archivo que produce un nodo.
    pub(crate) fn node_kind(&self, id: NodeId) -> FileKind {
        let n = self.session.as_ref().and_then(|s| s.patch.node(id));
        match n.map(|n| &n.tipo) {
            Some(NodeKind::Proceso { proceso }) => self.registry.get(proceso).map_or(FileKind::Wav, |p| p.output_kind()),
            Some(NodeKind::SubPatch { pasos, .. }) => pasos
                .last()
                .and_then(|t| t.tipo_proceso())
                .and_then(|p| self.registry.get(p))
                .map_or(FileKind::Wav, |p| p.output_kind()),
            _ => FileKind::Wav,
        }
    }

    /// ¿La celda convierte su entrada principal sola (sonido -> .ana)?
    pub(crate) fn auto_converts(&self, id: NodeId) -> bool {
        let Some(n) = self.session.as_ref().and_then(|s| s.patch.node(id)) else { return false };
        let Some(p) = n.process_id().and_then(|p| self.registry.get(p)) else { return false };
        n.entradas.iter().enumerate().any(|(i, port)| p.input_kind(i) == FileKind::Ana && self.node_kind(port.nodo) != FileKind::Ana)
    }

    /// Qué hay en (fila, columna) de la planilla.
    pub(crate) fn slot(&self, row: usize, col: usize) -> Slot {
        let Some(s) = &self.session else { return Slot::Empty };
        let Some(r) = s.patch.filas.get(row) else { return Slot::Empty };
        if col == 0 {
            if let Some(o) = r.origen {
                return Slot::Origin(o);
            }
        }
        s.patch.at(row, col).map_or(Slot::Empty, Slot::Node)
    }

    pub(crate) fn grid_zone(&mut self, ui: &mut Ui) {
        // Encabezado: nombre del patch y aviso de "eligiendo entrada".
        ui.horizontal(|ui| {
            let name = self.session.as_ref().and_then(|s| s.dir.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            win32::bold(ui, &t!("ui.grilla.titulo", nombre = name));
            if let Some((node, slot)) = self.pick {
                let input = self
                    .session
                    .as_ref()
                    .and_then(|s| s.patch.node(node))
                    .and_then(|n| n.process_id())
                    .and_then(|p| self.registry.get(p))
                    .and_then(|p| p.inputs().spec(slot))
                    .map(|s| tr(s.key).to_string())
                    .unwrap_or_else(|| t!("ui.mezcla.entrada", n = slot + 1));
                ui.add_space(8.0);
                win32::notice(ui, crate::win32::Level::Info, &t!("ui.entradas.banner", entrada = input));
            } else if let Some(id) = self.replace {
                ui.add_space(8.0);
                let label = self.session.as_ref().and_then(|s| s.patch.cell_label(id)).unwrap_or_default();
                win32::notice(ui, crate::win32::Level::Info, &t!("ui.grilla.reemplazando", celda = label));
            }
        });
        let rect = ui.available_rect_before_wrap();
        ui.allocate_rect(rect, Sense::hover());
        win32::audit::record(ui, "planilla", rect); // i18n-ok
        self.sheet(ui, rect);
    }

    /// La planilla en `rect` (con sus barras de desplazamiento).
    fn sheet(&mut self, ui: &mut Ui, rect: Rect) {
        let p = ui.painter().clone();
        p.rect_filled(rect, 0.0, theme::t().FACE);
        win32::bevel(&p, rect, Bevel::Sunken);
        let inner = rect.shrink(2.0);
        let view = Rect::from_min_max(inner.min + vec2(HDR_W, HDR_H), inner.max - vec2(SB, SB));
        let content = vec2(MAX_COLS as f32 * CELL_W, MAX_ROWS as f32 * CELL_H);
        let (mut sx, mut sy) = self.grid_scroll;

        // Rueda del mouse: vertical (Mayús: horizontal).
        let resp_bg = ui.interact(inner, ui.id().with("planilla_fondo"), Sense::hover()); // i18n-ok
        if resp_bg.hovered() {
            let (d, shift) = ui.input(|i| (i.smooth_scroll_delta, i.modifiers.shift));
            if shift {
                sx -= d.y + d.x;
            } else {
                sx -= d.x;
                sy -= d.y;
            }
        }
        // Barras
        let vbar = Rect::from_min_max(pos2(inner.right() - SB, inner.top()), pos2(inner.right(), inner.bottom() - SB));
        let hbar = Rect::from_min_max(pos2(inner.left(), inner.bottom() - SB), pos2(inner.right() - SB, inner.bottom()));
        win32::scrollbar(ui, vbar, true, &mut sy, content.y + HDR_H, view.height() + HDR_H, CELL_H);
        win32::scrollbar(ui, hbar, false, &mut sx, content.x + HDR_W, view.width() + HDR_W, CELL_W);
        let corner = Rect::from_min_max(pos2(vbar.left(), hbar.top()), inner.max);
        p.rect_filled(corner, 0.0, theme::t().FACE);
        sx = sx.clamp(0.0, (content.x - view.width()).max(0.0));
        sy = sy.clamp(0.0, (content.y - view.height()).max(0.0));
        self.grid_scroll = (sx, sy);

        // Mantener a la vista la celda elegida con el teclado.
        if self.grid_follow {
            self.grid_follow = false;
            let (r, c) = self.cursor_cell;
            let (x0, y0) = (c as f32 * CELL_W, r as f32 * CELL_H);
            if x0 < sx {
                sx = x0;
            } else if x0 + CELL_W > sx + view.width() {
                sx = x0 + CELL_W - view.width();
            }
            if y0 < sy {
                sy = y0;
            } else if y0 + CELL_H > sy + view.height() {
                sy = y0 + CELL_H - view.height();
            }
            self.grid_scroll = (sx.max(0.0), sy.max(0.0));
        }
        let (sx, sy) = self.grid_scroll;
        let c0 = (sx / CELL_W).floor() as usize;
        let c1 = (((sx + view.width()) / CELL_W).ceil() as usize).min(MAX_COLS);
        let r0 = (sy / CELL_H).floor() as usize;
        let r1 = (((sy + view.height()) / CELL_H).ceil() as usize).min(MAX_ROWS);
        let cell_rect = |r: usize, c: usize| {
            Rect::from_min_size(pos2(view.left() + c as f32 * CELL_W - sx, view.top() + r as f32 * CELL_H - sy), vec2(CELL_W, CELL_H))
        };

        // Encabezados de columna
        let hp = p.with_clip_rect(Rect::from_min_max(pos2(view.left(), inner.top()), pos2(view.right(), view.top())));
        for c in c0..c1 {
            let r = cell_rect(0, c);
            let h = Rect::from_min_max(pos2(r.left(), inner.top()), pos2(r.right(), view.top()));
            hp.rect_filled(h, 0.0, theme::t().GRID_HEAD);
            win32::bevel(&hp, h, Bevel::ThinRaised);
            let col_sel = self.cursor_cell.1 == c;
            hp.text(h.center(), Align2::CENTER_CENTER, format!("{c}"), if col_sel { bold_font(theme::SMALL_SIZE) } else { body_font(theme::SMALL_SIZE) }, theme::t().TEXT);
        }
        // Encabezados de fila
        let vp = p.with_clip_rect(Rect::from_min_max(pos2(inner.left(), view.top()), pos2(view.left(), view.bottom())));
        for r in r0..r1 {
            let cr = cell_rect(r, 0);
            let h = Rect::from_min_max(pos2(inner.left(), cr.top()), pos2(view.left(), cr.bottom()));
            vp.rect_filled(h, 0.0, theme::t().GRID_HEAD);
            win32::bevel(&vp, h, Bevel::ThinRaised);
            let row_sel = self.cursor_cell.0 == r;
            vp.text(h.center(), Align2::CENTER_CENTER, row_name(r), if row_sel { bold_font(theme::UI_SIZE) } else { ui_font() }, theme::t().TEXT);
        }
        let corner_tl = Rect::from_min_max(inner.min, pos2(view.left(), view.top()));
        p.rect_filled(corner_tl, 0.0, theme::t().GRID_HEAD);
        win32::bevel(&p, corner_tl, Bevel::ThinRaised);

        // Celdas
        let cp = p.with_clip_rect(view);
        let secondary: Vec<NodeId> = self
            .selected
            .and_then(|sel| self.session.as_ref().and_then(|s| s.patch.node(sel)))
            .map(|n| {
                let skip = if matches!(n.tipo, NodeKind::Mezcla { .. }) { 0 } else { 1 };
                n.entradas.iter().skip(skip).map(|p| p.nodo).collect()
            })
            .unwrap_or_default();
        let mut clicked: Option<(usize, usize, bool, bool)> = None; // (fila, col, doble, mayús)
        for r in r0..r1 {
            for c in c0..c1 {
                let cr = cell_rect(r, c);
                let slot = self.slot(r, c);
                let selected = self.cursor_cell == (r, c);
                let in_range = self.range_sel.is_some_and(|(rr, a, b)| rr == r && (a.min(b)..=a.max(b)).contains(&c));
                let is_secondary = matches!(slot, Slot::Node(id) if secondary.contains(&id));
                let fill = if selected {
                    theme::t().HIGHLIGHT
                } else if in_range || is_secondary {
                    theme::t().GRID_MARK
                } else if c == 0 {
                    theme::t().GRID_COL0
                } else if slot == Slot::Empty {
                    theme::t().GRID_EMPTY
                } else {
                    theme::t().GRID_CELL
                };
                cp.rect_filled(cr, 0.0, fill);
                if selected {
                    cp.rect_stroke(cr.shrink(1.0), 0.0, eframe::egui::Stroke::new(1.0_f32, theme::t().GRID_FOCUS), eframe::egui::StrokeKind::Inside);
                }
                // Líneas de la grilla (abajo y derecha).
                cp.rect_filled(Rect::from_min_max(pos2(cr.left(), cr.bottom() - 1.0), cr.max), 0.0, theme::t().GRID_LINE);
                cp.rect_filled(Rect::from_min_max(pos2(cr.right() - 1.0, cr.top()), cr.max), 0.0, theme::t().GRID_LINE);
                let ink = if selected { theme::t().HIGHLIGHT_TEXT } else { theme::t().TEXT };
                let muted = if selected { theme::t().HIGHLIGHT_TEXT } else { theme::t().TEXT_MUTED };
                let tp = cp.with_clip_rect(cr.shrink(2.0).intersect(view));
                let help = match slot {
                    Slot::Empty => {
                        if r == 0 && c == 0 && self.session.as_ref().is_some_and(|s| s.patch.filas.is_empty()) {
                            tp.text(cr.left_center() + vec2(5.0, 0.0), Align2::LEFT_CENTER, tr("ui.grilla.vacia_corto"), ui_font(), muted);
                        }
                        format!("{}_{c} · {}", row_name(r), tr("ui.ayuda.celda_vacia"))
                    }
                    Slot::Origin(o) => {
                        let lab = self.session.as_ref().and_then(|s| s.patch.cell_label(o.nodo)).unwrap_or_default();
                        let txt = if o.salida > 0 { format!("→{lab}:{}", o.salida + 1) } else { format!("→{lab}") };
                        tp.text(cr.left_top() + vec2(5.0, 3.0), Align2::LEFT_TOP, &txt, bold_font(theme::UI_SIZE), ink);
                        tp.text(cr.left_bottom() + vec2(5.0, -3.0), Align2::LEFT_BOTTOM, self.node_kind(o.nodo).ext(), body_font(theme::SMALL_SIZE), muted);
                        t!("ui.ayuda.referencia", celda = lab)
                    }
                    Slot::Node(id) => {
                        let state = self.cell_state(id);
                        let name = self.node_name(id);
                        tp.text(cr.left_top() + vec2(5.0, 3.0), Align2::LEFT_TOP, &name, ui_font(), ink);
                        let ic = pos2(cr.left() + 11.0, cr.bottom() - 10.0);
                        paint_state_icon(&tp, ic, 11.0, state, ink);
                        let mut line2 = self.node_kind(id).ext().to_string();
                        if self.auto_converts(id) {
                            line2.push_str(&format!("  {}", tr("ui.celda.auto")));
                        }
                        if let CellState::EnProceso(f) = state {
                            line2 = format!("{line2}  {:.0}%", f * 100.0);
                        }
                        if is_secondary {
                            line2.push_str(&format!("  {}", tr("ui.entradas.distintivo_corto")));
                        }
                        tp.text(pos2(cr.left() + 20.0, ic.y), Align2::LEFT_CENTER, &line2, body_font(theme::SMALL_SIZE), muted);
                        if let CellState::EnProceso(f) = state {
                            let bar = Rect::from_min_size(pos2(cr.left(), cr.bottom() - 3.0), vec2((cr.width() - 1.0) * f, 2.0));
                            tp.rect_filled(bar, 0.0, theme::t().ACCENT);
                        }
                        let label = format!("{}_{c}", row_name(r));
                        format!("{label} · {name} · {}", tr(state.help_key()))
                    }
                };
                let resp = ui.interact(cr.intersect(view), ui.id().with(("celda", r, c)), Sense::click()); // i18n-ok
                if resp.hovered() {
                    self.help = help;
                }
                if resp.clicked() || resp.double_clicked() {
                    clicked = Some((r, c, resp.double_clicked(), ui.input(|i| i.modifiers.shift)));
                }
                if resp.secondary_clicked() {
                    clicked = Some((r, c, false, false));
                }
                if matches!(slot, Slot::Node(_)) || !self.clipboard.is_empty() {
                    resp.context_menu(|ui| self.cell_menu(ui, r, c));
                }
            }
        }
        if let Some((r, c, double, shift)) = clicked {
            self.click_cell(r, c, double, shift);
        }
    }

    /// Clic en una posición de la planilla.
    pub(crate) fn click_cell(&mut self, r: usize, c: usize, double: bool, shift: bool) {
        let slot = self.slot(r, c);
        if let Some((_, _)) = self.pick {
            let target = match slot {
                Slot::Node(id) => Some(id),
                Slot::Origin(o) => Some(o.nodo),
                Slot::Empty => None,
            };
            if let Some(id) = target {
                self.finish_pick(id);
            }
            return;
        }
        if shift {
            let anchor = match self.range_sel {
                Some((rr, a, _)) if rr == r => a,
                _ if self.cursor_cell.0 == r => self.cursor_cell.1,
                _ => c,
            };
            self.range_sel = Some((r, anchor, c));
            return;
        }
        self.range_sel = None;
        self.cursor_cell = (r, c);
        match slot {
            Slot::Node(id) => {
                self.select(id);
                if double {
                    self.open_page(id);
                }
            }
            Slot::Origin(o) => {
                self.select(o.nodo);
                self.cursor_cell = (r, c);
                self.view_salida = o.salida;
                self.view_mode = crate::app::ViewMode::Resultado;
                self.view = Default::default();
            }
            Slot::Empty => {
                self.selected = None;
                self.player.stop();
            }
        }
    }

    /// Abre la página que corresponde a la celda (doble clic / Re-editar).
    pub(crate) fn open_page(&mut self, id: NodeId) {
        let Some(n) = self.session.as_ref().and_then(|s| s.patch.node(id)) else { return };
        self.page = match n.tipo {
            NodeKind::Mezcla { .. } => Page::Mix(id),
            _ => Page::Params(id),
        };
        self.param_tab = 0;
    }

    /// Menú contextual de una celda (operaciones de Soundshaper).
    fn cell_menu(&mut self, ui: &mut Ui, r: usize, c: usize) {
        use Action as A;
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        ui.set_min_width(190.0);
        let slot = self.slot(r, c);
        let node = matches!(slot, Slot::Node(_));
        let mut act = None;
        let items: [(&str, &str, Action, bool); 10] = [
            ("ui.menu.reeditar", "ui.ayuda.reeditar", A::ReEdit, node),
            ("ui.menu.reemplazar", "ui.ayuda.reemplazar", A::Replace, node),
            ("ui.menu.copiar", "ui.ayuda.copiar", A::Copy, node),
            ("ui.menu.pegar", "ui.ayuda.pegar", A::Paste, !self.clipboard.is_empty()),
            ("ui.menu.ramificar", "ui.ayuda.ramificar", A::Branch, node),
            ("ui.menu.ejecutar_celda", "ui.ayuda.render", A::RunCell, node),
            ("ui.menu.ejecutar_desde", "ui.ayuda.cascada", A::RunFrom, node),
            ("ui.menu.deshacer_celda", "ui.ayuda.deshacer_celda", A::UndoCell, node && !self.undo.is_empty()),
            ("ui.menu.info_celda", "ui.ayuda.info_celda", A::CellInfo, node),
            ("ui.menu.borrar", "ui.ayuda.quitar", A::Delete, node),
        ];
        for (i, (key, help, a, en)) in items.into_iter().enumerate() {
            if i == 5 || i == 8 {
                win32::menu_sep(ui);
            }
            let resp = win32::list_row(ui, false, tr(key), "", en);
            self.hint_key(&resp, help);
            if resp.clicked() {
                act = Some(a);
            }
        }
        if let Some(a) = act {
            ui.close_menu();
            self.cursor_cell = (r, c);
            if let Slot::Node(id) = slot {
                self.select(id);
            }
            self.do_action(ui.ctx(), a);
        }
    }

    /// Mueve la celda elegida con las flechas.
    pub(crate) fn navigate(&mut self, dr: i32, dc: i32) {
        let (r, c) = self.cursor_cell;
        let nr = (r as i32 + dr).clamp(0, MAX_ROWS as i32 - 1) as usize;
        let nc = (c as i32 + dc).clamp(0, MAX_COLS as i32 - 1) as usize;
        self.grid_follow = true;
        self.click_cell(nr, nc, false, false);
    }
}
