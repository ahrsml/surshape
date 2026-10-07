//! Página de parámetros (reemplaza el panel fijo NODO): ocupa el área
//! central, con la entrada visible arriba (visor compacto que se puede
//! escuchar), el modo del programa CDP si tiene, las entradas, la tabla de
//! parámetros (nombre traducido + nombre CDP, valor con spinner + slider,
//! mínimo, máximo, defecto, unidad, casilla T-V que abre el Graph-Edit y
//! rango propio para aleatorizar), pestañas (Seed, Variantes,
//! Instantáneas/Presets, Por canal), la línea de comando en vivo y los
//! botones RENDER / Previsualizar / Volver.
//!
//! También la página de la mezcla y la de una fuente.

use crate::app::{AfterRun, App, Msg, Page, ViewMode};
use crate::bpedit::BpEdit;
use crate::theme::{self, mono_font};
use crate::viewer::{self, ViewOpts, ViewState};
use crate::widgets::CellState;
use crate::win32::{self, Level, Spin};
use eframe::egui::{self, vec2, Rect, Sense, Ui};
use std::sync::Arc;
use surshape_engine::process::{desc_key, name_key};
use surshape_engine::{
    Breakpoints, Interp, ParamKind, ParamSpec, ParamValue, ParamValues, Process, Scale, TimeMode, MODE_PARAM,
};
use surshape_i18n::{t, tr};
use surshape_patch::{NodeId, NodeKind, PortRef, Region};

impl App {
    pub(crate) fn params_page(&mut self, ui: &mut Ui, id: NodeId) {
        let Some(node) = self.session.as_ref().and_then(|s| s.patch.node(id)).cloned() else {
            self.page = Page::Main;
            return;
        };
        let label = self.session.as_ref().and_then(|s| s.patch.cell_label(id)).unwrap_or_default();
        match &node.tipo {
            NodeKind::Fuente(_) => {
                let title = t!("ui.pagina.fuente", nombre = self.node_name(id), celda = label);
                if self.page_title(ui, &title) {
                    self.page = Page::Main;
                }
                self.source_page(ui, id);
            }
            NodeKind::Mezcla { .. } => self.page = Page::Mix(id),
            NodeKind::SubPatch { nombre, pasos, .. } => {
                let title = t!("ui.pagina.subpatch", nombre = nombre, celda = label);
                if self.page_title(ui, &title) {
                    self.page = Page::Main;
                    return;
                }
                let pasos = pasos.clone();
                self.subpatch_page(ui, id, &pasos);
            }
            NodeKind::Proceso { proceso } => {
                let Some(p) = self.registry.get(proceso).cloned() else {
                    if self.page_title(ui, &label) {
                        self.page = Page::Main;
                    }
                    win32::notice(ui, Level::Error, &t!("err.patch.proceso_desconocido_id", proceso = proceso));
                    return;
                };
                let engine = match p.engine() {
                    surshape_engine::Engine::Cdp => t!("ui.pagina.motor_cdp", programa = p.command(&dummy_cmd()).first().map(|c| c.split(' ').take(2).collect::<Vec<_>>().join(" ")).unwrap_or_default()),
                    surshape_engine::Engine::Nativo => t!("ui.pagina.motor_nativo").to_string(),
                };
                let title = t!("ui.pagina.parametros", proceso = tr(&name_key(p.id())), motor = engine, celda = label);
                if self.page_title(ui, &title) {
                    self.page = Page::Main;
                    return;
                }
                self.process_page(ui, id, &node, &p);
            }
        }
    }

    fn process_page(&mut self, ui: &mut Ui, id: NodeId, node: &surshape_patch::Node, p: &Arc<dyn Process>) {
        // Botones fijos abajo; el resto se desplaza.
        let bottom_h = 32.0;
        let avail = ui.available_rect_before_wrap();
        let scroll_rect = Rect::from_min_max(avail.min, egui::pos2(avail.right(), avail.bottom() - bottom_h));
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(scroll_rect).layout(egui::Layout::top_down(egui::Align::Min)));
        egui::ScrollArea::vertical().id_salt(("pagina", id)).auto_shrink([false, false]).show(&mut child, |ui| { // i18n-ok
            ui.set_width(ui.available_width());
            win32::wrap(ui, tr(&desc_key(p.id())));
            self.input_preview(ui, id, node, p.as_ref());
            let mut params = node.params.clone();
            let mut seed = node.seed;
            let mut region = node.region;
            let mut iters = node.iteraciones.max(1);
            let mut snaps = node.snapshots.clone();
            let mut random = node.aleatorio.clone();
            self.inputs_section(ui, node, p.as_ref());
            // Modo (programas CDP con modos)
            let mode = self.mode_section(ui, p.as_ref(), &mut params);
            // Parámetros
            let channel = self.channel_target(node, p.as_ref(), &params);
            win32::group(ui, tr("ui.param.titulo"), |ui| {
                if p.params().iter().all(|s| s.id == MODE_PARAM) {
                    win32::muted(ui, tr("ui.proceso.sin_parametros"));
                    return;
                }
                let target = match (channel, params.por_canal.as_mut()) {
                    (Some(c), Some(v)) if c < v.len() => &mut v[c],
                    _ => &mut params.comun,
                };
                let mut values = target.completed(p.params());
                self.param_table(ui, id, channel, p.params(), mode, &mut values, &mut random);
                let b = win32::button_w(ui, tr("ui.proceso.por_defecto"), 90.0, true);
                self.hint_key(&b, "ui.ayuda.por_defecto");
                if b.clicked() {
                    values = ParamValues::defaults(p.params());
                }
                *target = values;
            });
            // Pestañas
            let tabs = [tr("ui.tab.seed"), tr("ui.tab.variantes"), tr("ui.tab.instantaneas"), tr("ui.tab.canales")];
            ui.add_space(4.0);
            win32::tabs(ui, &mut self.param_tab, &tabs);
            win32::tab_panel(ui, |ui| match self.param_tab {
                0 => self.seed_tab(ui, node, p.as_ref(), &mut seed, &mut region, &mut iters),
                1 => self.variants_section(ui, node, p, &mut params, &mut seed, &random),
                2 => self.snapshots_section(ui, p.as_ref(), &mut params, &mut seed, &mut snaps),
                _ => self.channel_section(ui, node, p.as_ref(), &mut params),
            });
            // Guardar cambios (la celda y lo que depende quedan desactualizados)
            if params != node.params || seed != node.seed || region != node.region || iters != node.iteraciones || snaps != node.snapshots || random != node.aleatorio {
                self.before_param_edit(id);
                if let Some(n) = self.session.as_mut().and_then(|s| s.patch.node_mut(id)) {
                    n.params = params;
                    n.seed = seed;
                    n.region = region;
                    n.iteraciones = iters;
                    n.snapshots = snaps;
                    n.aleatorio = random;
                }
                self.mark_dirty();
            }
            // Línea de comando
            ui.add_space(4.0);
            self.command_box(ui, id);
            // Estado
            self.state_line(ui, id);
        });
        // Botones: RENDER, Previsualizar, Volver.
        let brect = Rect::from_min_max(egui::pos2(avail.left(), avail.bottom() - bottom_h + 4.0), avail.max);
        let mut b = ui.new_child(egui::UiBuilder::new().max_rect(brect).layout(egui::Layout::right_to_left(egui::Align::Center)));
        self.page_buttons(&mut b, id);
        ui.allocate_rect(avail, Sense::hover());
    }

    /// Página de un sub-patch: sus pasos (de solo lectura), abrirlo como fila
    /// para editarlo, la línea de comando, el estado y los botones.
    fn subpatch_page(&mut self, ui: &mut Ui, id: NodeId, pasos: &[surshape_patch::NodeTemplate]) {
        win32::wrap(ui, tr("ui.subpatch.desc"));
        win32::group(ui, tr("ui.subpatch.pasos"), |ui| {
            for (i, t) in pasos.iter().enumerate() {
                let Some(pid) = t.tipo_proceso() else { continue };
                let desc = match self.registry.get(pid) {
                    Some(p) => surshape_engine::descriptor(pid, p.params(), &t.params.completed(p.params()).comun, p.uses_seed().then_some(t.seed)),
                    None => t!("err.patch.proceso_desconocido_id", proceso = pid),
                };
                ui.horizontal(|ui| {
                    win32::bold(ui, &format!("{}. {}", i + 1, tr(&name_key(pid))));
                    win32::label(ui, egui::RichText::new(desc).font(mono_font(theme::MONO_SIZE)).color(theme::TEXT_MUTED));
                });
            }
            let b = win32::button_w(ui, tr("ui.menu.subpatch_abrir"), 140.0, true);
            self.hint_key(&b, "ui.ayuda.subpatch_abrir");
            if b.clicked() {
                self.do_action(ui.ctx(), crate::shell::Action::ExpandSubpatch);
            }
        });
        self.command_box(ui, id);
        self.state_line(ui, id);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Max), |ui| self.page_buttons(ui, id));
    }

    /// Botones de la página: RENDER (ejecuta y vuelve, según preferencias),
    /// Previsualizar (ejecuta y escucha sin salir) y Volver.
    fn page_buttons(&mut self, ui: &mut Ui, id: NodeId) {
        let b = win32::button_w(ui, tr("ui.pagina.volver"), 90.0, true);
        self.hint_key(&b, "ui.ayuda.volver");
        if b.clicked() {
            self.page = Page::Main;
        }
        let running = self.run.is_some();
        let b = win32::button_w(ui, tr("ui.pagina.previsualizar"), 100.0, !running);
        self.hint_key(&b, "ui.ayuda.previsualizar");
        if b.clicked() {
            self.start_run_then(vec![id], AfterRun::Play(id));
        }
        let state = self.cell_state(id);
        let needed = matches!(state, CellState::SinRender | CellState::Desactualizado | CellState::Error);
        let b = win32::accent_button(ui, tr("ui.proceso.render"), 110.0, !running);
        let help = match state {
            CellState::Renderizado => "ui.ayuda.render_al_dia",
            CellState::Error => "ui.ayuda.render_error",
            _ => "ui.ayuda.render",
        };
        self.hint_key(&b, help);
        if b.clicked() {
            if needed {
                self.start_run_then(vec![id], AfterRun::BackToMain);
            } else if self.prefs.volver_tras_render {
                self.page = Page::Main;
            }
        }
        if running {
            self.run_progress(ui);
        }
    }

    /// Visor compacto de la entrada principal (con su línea de origen y
    /// botón para escucharla).
    fn input_preview(&mut self, ui: &mut Ui, id: NodeId, node: &surshape_patch::Node, p: &dyn Process) {
        let Some(first) = node.entradas.first().copied() else { return };
        let file = self.cell_file(first.nodo, first.salida);
        let title = if self.auto_converts(id) && p.input_kind(0) == surshape_engine::FileKind::Ana {
            let ana = file.rsplit_once('.').map(|(b, _)| format!("{b}.ana")).unwrap_or_default(); // i18n-ok
            t!("ui.pagina.entrada_auto", archivo = ana, origen = file)
        } else {
            t!("ui.pagina.entrada", archivo = file)
        };
        win32::group(ui, &title, |ui| {
            let Some((k, path)) = self.port_target(first.nodo, first.salida) else {
                win32::muted(ui, tr("ui.visor.sin_render"));
                return;
            };
            self.ensure_loaded(&k, &path);
            let Some(d) = self.mem.get(&k).cloned() else {
                win32::muted(ui, &t!("ui.visor.cargando", archivo = file));
                return;
            };
            ui.horizontal(|ui| {
                let playing = self.player.is_playing_buf(&d.audio);
                let r = win32::tool_button(ui, if playing { win32::Icon::Stop } else { win32::Icon::Play }, tr("ui.bp.escuchar"), true, playing);
                self.hint_key(&r, "ui.ayuda.escuchar_entrada");
                if r.clicked() {
                    if playing {
                        self.player.stop();
                    } else {
                        let n = d.audio.frames();
                        self.player.play(d.audio.clone(), 0, n, false);
                    }
                }
                let w = ui.available_width();
                let (rect, _) = ui.allocate_exact_size(vec2(w, 70.0), Sense::hover());
                let mut vs: ViewState = ui.ctx().data(|m| m.get_temp(ui.id().with("vis_entrada"))).unwrap_or_default(); // i18n-ok
                let playhead = playing.then(|| self.player.position());
                let mut vz = 1.0f32;
                let opts = ViewOpts { vzoom: &mut vz, hidden: 0, loop_region: None, unit: self.prefs.unidades, markers: &[] };
                let r = viewer::show(ui, rect, &mut vs, &d, playhead, None, opts);
                self.hint_key(&r, "ui.ayuda.onda");
                ui.ctx().data_mut(|m| m.insert_temp(ui.id().with("vis_entrada"), vs)); // i18n-ok
            });
        });
    }

    /// Entradas de los procesos de dos sonidos: cuál celda va en cada una,
    /// elegible de una lista o con clic en la grilla.
    pub(crate) fn inputs_section(&mut self, ui: &mut Ui, node: &surshape_patch::Node, p: &dyn Process) {
        let n_in = p.inputs().min();
        if n_in < 2 {
            return;
        }
        let cells: Vec<(NodeId, String)> = self
            .session
            .as_ref()
            .map(|s| {
                let mut v: Vec<(NodeId, String)> = s
                    .patch
                    .nodos
                    .iter()
                    .filter(|n| n.id != node.id && !s.patch.upstream(n.id).contains(&node.id))
                    .filter_map(|n| s.patch.cell_label(n.id).map(|l| (n.id, l)))
                    .collect();
                v.sort_by(|a, b| a.1.cmp(&b.1));
                v
            })
            .unwrap_or_default();
        win32::group(ui, tr("ui.entradas.titulo"), |ui| {
            for slot in 1..n_in {
                ui.horizontal(|ui| {
                    let name = p.inputs().spec(slot).map(|s| tr(s.key)).unwrap_or("");
                    let l = win32::label(ui, name);
                    self.hint_key(&l, "ui.ayuda.entrada_secundaria");
                    let cur = node.entradas.get(slot).copied();
                    let shown = cur.map(|c| format!("{}  {}", self.cell_file(c.nodo, c.salida), self.node_name(c.nodo))).unwrap_or_else(|| t!("ui.entradas.sin_elegir").to_string());
                    let mut chosen = None;
                    let r = win32::combo(ui, ("entrada2", node.id, slot), &shown, 260.0, |ui| { // i18n-ok
                        for (cid, lab) in &cells {
                            let txt = format!("{lab}  {}", self.node_name(*cid));
                            if win32::combo_item(ui, cur.is_some_and(|c| c.nodo == *cid), &txt).clicked() {
                                chosen = Some(*cid);
                            }
                        }
                    });
                    self.hint_key(&r, "ui.ayuda.entrada_elegir");
                    if let Some(c) = chosen {
                        self.connect_input(node.id, slot, c);
                    }
                    let b = win32::button_w(ui, tr("ui.entradas.elegir"), 120.0, true);
                    self.hint_key(&b, "ui.ayuda.entrada_elegir");
                    if b.clicked() {
                        self.pick = Some((node.id, slot));
                        self.pick_return = Some(self.page);
                        self.page = Page::Main;
                    }
                });
            }
        });
    }

    fn connect_input(&mut self, node: NodeId, slot: usize, cell: NodeId) {
        self.push_undo();
        let Some(s) = self.session.as_mut() else { return };
        match s.patch.connect(node, slot, PortRef::main(cell)) {
            Ok(()) => self.save_now(),
            Err(e) => self.status = Some(Msg::new(Level::Warning, e.i18n_key())),
        }
    }

    /// Conecta la entrada que se está eligiendo a la celda `cell` (clic en la
    /// grilla) y vuelve a la página desde donde se pidió.
    pub(crate) fn finish_pick(&mut self, cell: NodeId) {
        let Some((node, slot)) = self.pick.take() else { return };
        self.push_undo();
        let Some(s) = self.session.as_mut() else { return };
        match s.patch.connect(node, slot, PortRef::main(cell)) {
            Ok(()) => {
                self.save_now();
                let label = self.session.as_ref().and_then(|s| s.patch.cell_label(cell)).unwrap_or_default();
                self.status = Some(Msg::new(Level::Info, "ui.entradas.conectada").arg("celda", label));
            }
            Err(e) => self.status = Some(Msg::new(Level::Warning, e.i18n_key())),
        }
        if let Some(p) = self.pick_return.take() {
            self.page = p;
        }
    }

    /// Selector de modo (si el proceso tiene un parámetro "modo"). Devuelve
    /// el modo elegido (0 = primero).
    fn mode_section(&mut self, ui: &mut Ui, p: &dyn Process, params: &mut surshape_engine::ParamSet) -> usize {
        let Some(spec) = p.params().iter().find(|s| s.id == MODE_PARAM) else { return 0 };
        let ParamKind::Choice(options) = spec.kind else { return 0 };
        let cur = (params.comun.get(MODE_PARAM).max(0.0) as usize).min(options.len() - 1);
        let mut chosen = cur;
        ui.horizontal(|ui| {
            let l = win32::label(ui, tr("ui.param.modo"));
            self.hint_key(&l, "ui.ayuda.modo");
            let shown = format!("{} · {}", cur + 1, tr(options[cur]));
            let r = win32::combo(ui, "modo_cdp", &shown, 320.0, |ui| { // i18n-ok
                for (i, k) in options.iter().enumerate() {
                    if win32::combo_item(ui, i == cur, &format!("{} · {}", i + 1, tr(k))).clicked() {
                        chosen = i;
                    }
                }
            });
            self.hint(&r, tr(&spec.desc_key()));
        });
        if chosen != cur {
            params.comun.set(MODE_PARAM, chosen as f64);
        }
        chosen
    }

    /// Canal que se edita si el nodo tiene valores por canal.
    fn channel_target(&self, node: &surshape_patch::Node, p: &dyn Process, params: &surshape_engine::ParamSet) -> Option<usize> {
        let nch = node.entradas.first().map_or(1, |e| self.node_channels(e.nodo));
        (p.per_channel() && nch > 1 && params.por_canal.is_some()).then_some(self.chan_tab.min(nch - 1))
    }

    /// Tabla de parámetros: Parámetro · CDP · Valor · slider · Mín · Máx ·
    /// Def · Ud · T-V · Random. Si no entra a lo ancho, se desplaza.
    #[allow(clippy::too_many_arguments)]
    fn param_table(
        &mut self,
        ui: &mut Ui,
        node: NodeId,
        channel: Option<usize>,
        specs: &[ParamSpec],
        mode: usize,
        values: &mut ParamValues,
        random: &mut std::collections::BTreeMap<String, (f64, f64)>,
    ) {
        egui::ScrollArea::horizontal().id_salt(("tabla_param", node)).auto_shrink([false, true]).show(ui, |ui| { // i18n-ok
            ui.horizontal(|ui| {
                for (k, w) in [
                    "ui.param.col_parametro",
                    "ui.param.col_cdp",
                    "ui.param.col_valor",
                    "ui.param.col_slider",
                    "ui.param.col_min",
                    "ui.param.col_max",
                    "ui.param.col_def",
                    "ui.param.col_unidad",
                    "ui.param.col_tv",
                    "ui.param.col_random",
                ]
                .into_iter()
                .zip(COLS)
                {
                    cell(ui, w, |ui| {
                        win32::head(ui, tr(k));
                    });
                }
            });
            for spec in specs.iter().filter(|s| s.id != MODE_PARAM && s.in_mode(mode)) {
                ui.horizontal(|ui| self.param_row(ui, node, channel, spec, values, random));
            }
        });
    }

    fn param_row(
        &mut self,
        ui: &mut Ui,
        node: NodeId,
        channel: Option<usize>,
        spec: &ParamSpec,
        values: &mut ParamValues,
        random: &mut std::collections::BTreeMap<String, (f64, f64)>,
    ) {
        let name = tr(spec.key);
        let desc = tr(&spec.desc_key());
        let unit = spec.unit.key().map(tr).unwrap_or("");
        let help = format!("{name}: {desc}");
        cell(ui, COLS[0], |ui| {
            let l = win32::label(ui, name).on_hover_text(desc);
            self.hint(&l, help.clone());
        });
        cell(ui, COLS[1], |ui| {
            let cdp = if spec.cdp.is_empty() { "—" } else { spec.cdp }; // i18n-ok
            win32::label(ui, egui::RichText::new(cdp).font(mono_font(theme::MONO_SIZE)).color(theme::TEXT_MUTED));
        });
        let envelope = matches!(values.value(spec.id), Some(ParamValue::Envelope(_)));
        let mut v = values.get(spec.id);
        let decimals = match spec.kind {
            ParamKind::Float { decimals } => decimals as usize,
            _ => 0,
        };
        match spec.kind {
            ParamKind::Toggle => {
                let mut on = v >= 0.5;
                cell(ui, COLS[2], |ui| {
                    let r = win32::checkbox(ui, &mut on, "");
                    self.hint(&r, help.clone());
                });
                v = if on { 1.0 } else { 0.0 };
                cell(ui, COLS[3], |_| {});
            }
            ParamKind::Choice(options) => {
                let mut idx = (v.max(0.0) as usize).min(options.len() - 1);
                let cur = idx;
                cell(ui, COLS[2] + COLS[3] + 6.0, |ui| {
                    let r = win32::combo(ui, ("opcion", node, spec.id), tr(options[cur]), COLS[2] + COLS[3], |ui| { // i18n-ok
                        for (i, k) in options.iter().enumerate() {
                            if win32::combo_item(ui, i == cur, tr(k)).clicked() {
                                idx = i;
                            }
                        }
                    });
                    self.hint(&r, help.clone());
                });
                v = idx as f64;
            }
            _ if envelope => {
                let n = match values.value(spec.id) {
                    Some(ParamValue::Envelope(bp)) => bp.puntos.len(),
                    _ => 0,
                };
                cell(ui, COLS[2], |ui| {
                    let r = win32::muted(ui, &t!("ui.param.variable", n = n));
                    self.hint(&r, help.clone());
                });
                cell(ui, COLS[3], |ui| {
                    let b = win32::button_w(ui, tr("ui.param.editar_curva"), COLS[3], true);
                    self.hint_key(&b, "ui.ayuda.param_curva");
                    if b.clicked() {
                        self.bp_edit = Some(BpEdit::new(node, spec, channel));
                        self.page = Page::Graph;
                    }
                });
            }
            _ => {
                let step = match spec.scale {
                    Scale::Log => (v.abs() * 0.05).max(10f64.powi(-(decimals as i32))),
                    Scale::Lin => ((spec.max - spec.min) / 100.0).max(10f64.powi(-(decimals as i32))),
                };
                let step = if spec.kind == ParamKind::Int { step.round().max(1.0) } else { step };
                cell(ui, COLS[2], |ui| {
                    let r = win32::spin(ui, &mut v, Spin { min: spec.min, max: spec.max, step, decimals, suffix: "", width: COLS[2] });
                    self.hint(&r, help.clone());
                });
                cell(ui, COLS[3], |ui| {
                    let r = win32::slider(ui, &mut v, spec.min, spec.max, spec.scale == Scale::Log, COLS[3]);
                    self.hint(&r, help.clone());
                });
                if spec.kind == ParamKind::Int {
                    v = v.round();
                }
            }
        }
        for (i, x) in [spec.min, spec.max, spec.default].into_iter().enumerate() {
            cell(ui, COLS[4 + i], |ui| {
                if !matches!(spec.kind, ParamKind::Toggle | ParamKind::Choice(_)) {
                    win32::label(ui, egui::RichText::new(fmt_param(spec, x)).font(mono_font(theme::MONO_SIZE)));
                }
            });
        }
        cell(ui, COLS[7], |ui| {
            win32::label(ui, if unit.is_empty() { "—" } else { unit }); // i18n-ok
        });
        // T-V: variable en el tiempo (breakpoints) -> Graph-Edit.
        if spec.automatable {
            cell(ui, COLS[8], |ui| {
                let mut tv = envelope;
                let r = win32::checkbox(ui, &mut tv, "");
                self.hint_key(&r, "ui.ayuda.tv");
                if r.changed() {
                    if tv {
                        let x = values.get(spec.id);
                        values.set_value(spec.id, ParamValue::Envelope(Breakpoints::new(TimeMode::Normalizado, Interp::Lineal, vec![(0.0, x), (1.0, x)])));
                        self.bp_edit = Some(BpEdit::new(node, spec, channel));
                        self.page = Page::Graph;
                    } else {
                        let x = values.get(spec.id);
                        values.set(spec.id, x);
                    }
                } else if !envelope {
                    values.set(spec.id, spec.clamp(v));
                }
            });
        } else {
            cell(ui, COLS[8], |_| {});
            values.set(spec.id, spec.clamp(v));
        }
        // Rango propio para aleatorizar.
        if matches!(spec.kind, ParamKind::Float { .. } | ParamKind::Int) {
            cell(ui, COLS[9], |ui| {
                let (mut lo, mut hi) = random.get(spec.id).copied().unwrap_or((spec.min, spec.max));
                let s = Spin { min: spec.min, max: spec.max, step: ((spec.max - spec.min) / 100.0).max(0.001), decimals, suffix: "", width: 78.0 };
                let r1 = win32::spin(ui, &mut lo, s);
                win32::label(ui, "–"); // i18n-ok
                let r2 = win32::spin(ui, &mut hi, s);
                self.hint_key(&r1, "ui.ayuda.rango_random");
                self.hint_key(&r2, "ui.ayuda.rango_random");
                if r1.changed() || r2.changed() {
                    let (a, b) = (lo.min(hi), lo.max(hi));
                    if (a, b) == (spec.min, spec.max) {
                        random.remove(spec.id);
                    } else {
                        random.insert(spec.id.to_string(), (a, b));
                    }
                }
            });
        }
    }

    /// Pestaña Seed: seed, iteraciones y región.
    fn seed_tab(&mut self, ui: &mut Ui, node: &surshape_patch::Node, p: &dyn Process, seed: &mut u64, region: &mut Option<Region>, iters: &mut u32) {
        ui.horizontal_wrapped(|ui| {
            if p.uses_seed() {
                let l = win32::label(ui, tr("ui.proceso.seed"));
                self.hint_key(&l, "ui.ayuda.seed");
                let mut s = *seed as f64;
                let r = win32::spin(ui, &mut s, Spin { min: 0.0, max: u32::MAX as f64, step: 1.0, decimals: 0, suffix: "", width: 100.0 });
                self.hint_key(&r, "ui.ayuda.seed");
                if r.changed() {
                    *seed = s.max(0.0) as u64;
                }
                let b = win32::button_w(ui, tr("ui.proceso.seed_nueva"), 60.0, true);
                self.hint_key(&b, "ui.ayuda.seed_nueva");
                if b.clicked() {
                    *seed = self.next_seed();
                }
                ui.add_space(10.0);
            } else {
                win32::muted(ui, tr("ui.proceso.sin_seed"));
                ui.add_space(10.0);
            }
            if p.inputs().is_generator() {
                return;
            }
            let l = win32::label(ui, tr("ui.nodo.iteraciones"));
            self.hint_key(&l, "ui.ayuda.iteraciones");
            let mut it = *iters as f64;
            let r = win32::spin(ui, &mut it, Spin { min: 1.0, max: 32.0, step: 1.0, decimals: 0, suffix: "", width: 60.0 });
            self.hint_key(&r, "ui.ayuda.iteraciones");
            if r.changed() {
                *iters = it.clamp(1.0, 32.0) as u32;
            }
        });
        if p.inputs().is_generator() {
            return;
        }
        // Región de la entrada
        let sr = node.entradas.first().and_then(|e| self.port_sr(e.nodo, e.salida)).unwrap_or(48000);
        ui.horizontal_wrapped(|ui| {
            let l = win32::label(ui, tr("ui.nodo.region_titulo"));
            self.hint_key(&l, "ui.ayuda.region");
            let r = win32::radio(ui, region.is_none(), tr("ui.nodo.region_todo"));
            self.hint_key(&r, "ui.ayuda.region");
            if r.clicked() {
                *region = None;
            }
            let txt = match region {
                Some(rg) => t!("ui.nodo.region", desde = self.prefs.unidades.fmt(rg.inicio as f64 / sr as f64, sr), hasta = self.prefs.unidades.fmt(rg.fin as f64 / sr as f64, sr)),
                None => t!("ui.nodo.region_tramo").to_string(),
            };
            let can_use = self.view_mode == ViewMode::Entrada && self.view.selection().is_some();
            let r = win32::radio(ui, region.is_some(), &txt);
            self.hint_key(&r, if can_use { "ui.ayuda.region_usar" } else { "ui.ayuda.region_como" });
            if r.clicked() && can_use {
                if let Some((a, b)) = self.view.selection() {
                    *region = Some(Region { inicio: a as u64, fin: b as u64 });
                }
            }
            let b = win32::button_w(ui, tr("ui.nodo.region_usar"), 80.0, can_use);
            self.hint_key(&b, if can_use { "ui.ayuda.region_usar" } else { "ui.ayuda.region_como" });
            if b.clicked() {
                if let Some((a, b)) = self.view.selection() {
                    *region = Some(Region { inicio: a as u64, fin: b as u64 });
                }
            }
        });
    }

    /// Frecuencia de una salida (sin cargarla).
    pub(crate) fn port_sr(&self, id: NodeId, salida: u16) -> Option<u32> {
        let n = self.session.as_ref()?.patch.node(id)?;
        match &n.tipo {
            NodeKind::Fuente(s) => Some(s.sr),
            _ => n.render.as_ref()?.salidas.get(salida as usize).map(|o| o.sr),
        }
    }

    /// Línea de comando en vivo, seleccionable y copiable.
    fn command_box(&mut self, ui: &mut Ui, id: NodeId) {
        let lines = self.cell_command(id);
        let text = lines.join("\n");
        win32::group(ui, tr("ui.pagina.comando"), |ui| {
            ui.horizontal(|ui| {
                let b = win32::button_w(ui, tr("ui.pagina.copiar"), 70.0, true);
                self.hint_key(&b, "ui.ayuda.comando_copiar");
                if b.clicked() {
                    ui.ctx().copy_text(text.clone());
                    self.status = Some(Msg::new(Level::Info, "ui.estado.copiado"));
                }
                win32::muted(ui, tr("ui.pagina.comando_nota"));
            });
            win32::sunken(ui, theme::FIELD, |ui| {
                ui.set_width(ui.available_width());
                let r = win32::mono_selectable(ui, &text);
                self.hint_key(&r, "ui.ayuda.comando");
            });
        });
    }

    /// Estado de la celda y avisos del último render.
    fn state_line(&mut self, ui: &mut Ui, id: NodeId) {
        let state = self.cell_state(id);
        let node = self.session.as_ref().and_then(|s| s.patch.node(id)).cloned();
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
            crate::widgets::paint_state_icon(ui.painter(), r.center(), 12.0, state, theme::TEXT);
            let label = match state {
                CellState::EnProceso(f) => t!("ui.celda.en_proceso", porcentaje = format!("{:.0}", f * 100.0)),
                st => t!(st.key()).to_string(),
            };
            let l = win32::bold(ui, &label);
            self.hint_key(&l, state.help_key());
            if let Some(rec) = node.as_ref().and_then(|n| n.render.as_ref()) {
                let mut info = t!("ui.nodo.ultimo_render", tiempo = format!("{:.1}", rec.segundos));
                if let Some(v) = &rec.version_motor {
                    info.push_str(&t!("ui.nodo.version_cdp", version = v));
                }
                win32::muted(ui, &info);
            }
        });
        if state == CellState::Error {
            let msg = node.as_ref().and_then(|n| n.error.as_ref()).map(|e| Msg::from_notice(Level::Error, &e.aviso)).or_else(|| {
                self.keys.get(&id).and_then(|k| k.as_ref().err()).map(|e| Msg::new(Level::Error, e.i18n_key()))
            });
            if let Some(m) = msg {
                win32::notice(ui, Level::Error, &m.text());
                let b = win32::button_w(ui, tr("ui.pagina.ver_consola"), 100.0, true);
                self.hint_key(&b, "ui.ayuda.consola");
                if b.clicked() {
                    self.page = Page::Console;
                }
            }
        }
        if let Some(rec) = node.as_ref().and_then(|n| n.render.as_ref()) {
            for a in &rec.avisos {
                win32::notice(ui, Level::Warning, &Msg::from_notice(Level::Warning, a).text());
            }
        }
    }

    // --- Fuente ---------------------------------------------------------------------------------

    fn source_page(&mut self, ui: &mut Ui, id: NodeId) {
        let Some(src) = self.session.as_ref().and_then(|s| s.patch.node(id)).and_then(|n| n.source()).cloned() else { return };
        win32::group(ui, tr("ui.celda.fuente"), |ui| {
            let unit = self.prefs.unidades;
            win32::label(ui, t!("ui.sesion.info", duracion = unit.fmt(src.frames as f64 / src.sr.max(1) as f64, src.sr), canales = src.canales, sr = src.sr));
            win32::readonly_field(ui, &src.ruta.display().to_string(), ui.available_width());
            if self.missing.contains(&id) {
                win32::notice(ui, Level::Error, tr("ui.nodo.fuente_falta"));
            }
            win32::wrap(ui, tr("ui.nodo.fuente_ayuda"));
            ui.horizontal(|ui| {
                let b = win32::button_w(ui, tr("ui.fuente.cambiar"), 120.0, true);
                self.hint_key(&b, "ui.ayuda.fuente_cambiar");
                if b.clicked() {
                    let filter = t!("ui.dialogo.filtro_audio").to_string();
                    self.open_dialog(move |d| d.add_filter(filter, surshape_audio::decode::EXTENSIONS).pick_file().map(|p| crate::app::DialogResult::ReplaceSource(id, p)));
                }
                let row = self.session.as_ref().and_then(|s| s.patch.cell_of(id)).map(|(r, _)| r);
                let b = win32::button_w(ui, tr("ui.fuente.duplicar"), 120.0, row.is_some());
                self.hint_key(&b, "ui.ayuda.fuente_duplicar");
                if b.clicked() {
                    if let Some(r) = row {
                        let filter = t!("ui.dialogo.filtro_audio").to_string();
                        self.open_dialog(move |d| d.add_filter(filter, surshape_audio::decode::EXTENSIONS).pick_file().map(|p| crate::app::DialogResult::DuplicateRow(r, p)));
                    }
                }
            });
        });
    }

    // --- Mezcla ---------------------------------------------------------------------------------

    /// Página de la mezcla: cada entrada con ganancia, paneo e inicio.
    pub(crate) fn mix_page(&mut self, ui: &mut Ui, id: NodeId) {
        let Some(node) = self.session.as_ref().and_then(|s| s.patch.node(id)).cloned() else {
            self.page = Page::Main;
            return;
        };
        let NodeKind::Mezcla { canales } = &node.tipo else {
            self.page = Page::Main;
            return;
        };
        let label = self.session.as_ref().and_then(|s| s.patch.cell_label(id)).unwrap_or_default();
        if self.page_title(ui, &t!("ui.pagina.mezcla", celda = label)) {
            self.page = Page::Main;
            return;
        }
        let mut settings = canales.clone();
        let mut remove = None;
        egui::ScrollArea::vertical().id_salt(("mezcla", id)).auto_shrink([false, false]).max_height(ui.available_height() - 36.0).show(ui, |ui| { // i18n-ok
            win32::wrap(ui, tr("ui.mezcla.desc"));
            win32::group(ui, tr("ui.mezcla.entradas"), |ui| {
                egui::ScrollArea::horizontal().id_salt(("mezcla_h", id)).auto_shrink([false, true]).show(ui, |ui| { // i18n-ok
                    egui::Grid::new(("mezcla_grid", id)).num_columns(6).spacing([6.0, 3.0]).show(ui, |ui| { // i18n-ok
                        for k in ["ui.mezcla.col_entrada", "ui.mezcla.col_ganancia", "ui.mezcla.col_paneo", "ui.mezcla.col_paneo_valor", "ui.mezcla.col_inicio", "ui.mezcla.col_quitar"] {
                            win32::head(ui, tr(k));
                        }
                        ui.end_row();
                        for (i, (port, set)) in node.entradas.iter().zip(settings.iter_mut()).enumerate() {
                            let name = format!("{}  {}", self.cell_file(port.nodo, port.salida), self.node_name(port.nodo));
                            ui.scope(|ui| {
                                ui.set_max_width(180.0);
                                let l = win32::label(ui, &name);
                                self.hint_key(&l, "ui.ayuda.mezcla_entrada");
                            });
                            let r = win32::spin(ui, &mut set.ganancia_db, Spin { min: -60.0, max: 12.0, step: 0.5, decimals: 1, suffix: tr("ui.unidad.db"), width: 90.0 });
                            self.hint_key(&r, "ui.ayuda.mezcla_ganancia");
                            let r = win32::slider(ui, &mut set.paneo, -1.0, 1.0, false, 110.0);
                            self.hint_key(&r, "ui.ayuda.mezcla_paneo");
                            let r = win32::spin(ui, &mut set.paneo, Spin { min: -1.0, max: 1.0, step: 0.05, decimals: 2, suffix: "", width: 70.0 });
                            self.hint_key(&r, "ui.ayuda.mezcla_paneo");
                            let r = win32::spin(ui, &mut set.inicio, Spin { min: 0.0, max: 3600.0, step: 0.1, decimals: 3, suffix: tr("ui.unidad.s"), width: 100.0 });
                            self.hint_key(&r, "ui.ayuda.mezcla_inicio");
                            let b = win32::button_w(ui, tr("ui.bp.quitar"), 60.0, true);
                            self.hint_key(&b, "ui.ayuda.mezcla_quitar");
                            if b.clicked() {
                                remove = Some(i);
                            }
                            ui.end_row();
                        }
                    });
                });
                let slot = node.entradas.len();
                let b = win32::button_w(ui, tr("ui.mezcla.agregar"), 140.0, true);
                self.hint_key(&b, "ui.ayuda.mezcla_agregar");
                if b.clicked() {
                    self.pick = Some((id, slot));
                    self.pick_return = Some(Page::Mix(id));
                    self.page = Page::Main;
                }
            });
            self.command_box(ui, id);
            self.state_line(ui, id);
        });
        if let Some(i) = remove {
            self.push_undo();
            if let Some(s) = self.session.as_mut() {
                s.patch.remove_input(id, i);
            }
            self.save_now();
        } else if &settings != canales {
            self.before_param_edit(id);
            if let Some(NodeKind::Mezcla { canales }) = self.session.as_mut().and_then(|s| s.patch.node_mut(id)).map(|n| &mut n.tipo) {
                *canales = settings;
            }
            self.mark_dirty();
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| self.page_buttons(ui, id));
    }
}

/// Anchos de las columnas de la tabla de parámetros.
const COLS: [f32; 10] = [140.0, 84.0, 100.0, 140.0, 56.0, 56.0, 56.0, 40.0, 34.0, 180.0];

/// Celda de ancho fijo de una fila de la tabla.
fn cell(ui: &mut Ui, w: f32, add: impl FnOnce(&mut Ui)) {
    ui.allocate_ui_with_layout(vec2(w, win32::ROW_H + 3.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_width(w);
        add(ui);
    });
}

/// Línea de comando sin entradas (para saber el programa de CDP).
fn dummy_cmd() -> surshape_engine::CmdCtx<'static> {
    static EMPTY: std::sync::OnceLock<ParamValues> = std::sync::OnceLock::new();
    surshape_engine::CmdCtx { params: EMPTY.get_or_init(ParamValues::default), seed: 0, inputs: &[], output: "", dur: 1.0, dur2: 1.0 }
}

/// Valor de un parámetro con sus decimales (sin unidad).
pub(crate) fn fmt_param(spec: &ParamSpec, v: f64) -> String {
    match spec.kind {
        ParamKind::Float { decimals } => format!("{v:.*}", decimals as usize),
        _ => format!("{v:.0}"),
    }
}
