//! Pestañas de la página de parámetros:
//!
//! - Variantes: aleatorizar parámetros (dentro del rango propio de cada uno)
//!   y previews: varias versiones de los primeros segundos para escuchar,
//!   comparar con la entrada y elegir una.
//! - Instantáneas (varios juegos de parámetros guardados en el nodo) y
//!   presets por proceso (archivos de texto, con uno por defecto).
//! - Valores por canal (estéreo).

use crate::app::{App, Msg};
use crate::theme;
use crate::win32::{self, Level, Spin};
use crate::Loaded;
use eframe::egui::{self, Ui};
use std::collections::BTreeMap;
use std::sync::atomic::AtomicU32;
use std::sync::Arc;
use surshape_audio::AudioBuf;
use surshape_engine::render::{run, RenderJob};
use surshape_engine::{randomize, ParamSet, ParamSpec, ParamValues, Process, ProcessError, Task, MODE_PARAM};
use surshape_i18n::{t, tr};
use surshape_patch::{Node, NodeId, NodeKind, PortRef, Snapshot};
use surshape_session::presets;
use surshape_session::runner::port_key;

/// Una versión de prueba de un nodo.
#[derive(Clone)]
pub(crate) struct Variant {
    pub params: ParamValues,
    pub seed: u64,
    pub audio: Loaded,
}

/// Previews en curso o listas para un nodo.
pub(crate) struct Previews {
    pub node: NodeId,
    pub task: Option<Task<Vec<Variant>>>,
    pub variants: Vec<Variant>,
    /// Entrada recortada, para comparar (A).
    pub source: Option<Arc<AudioBuf>>,
}

/// Ajustes de las previews y de "aleatorizar".
#[derive(Clone, Copy)]
pub(crate) struct VariantCfg {
    pub cantidad: usize,
    pub segundos: f64,
    pub variacion: f64,
}

impl Default for VariantCfg {
    fn default() -> Self {
        Self { cantidad: 4, segundos: 8.0, variacion: 30.0 }
    }
}

/// Aleatoriza respetando el rango propio de cada parámetro y sin tocar el
/// modo.
pub(crate) fn randomize_in_ranges(specs: &[ParamSpec], values: &ParamValues, amount: f64, seed: u64, ranges: &BTreeMap<String, (f64, f64)>) -> ParamValues {
    let ranged: Vec<ParamSpec> = specs
        .iter()
        .filter(|s| s.id != MODE_PARAM)
        .map(|s| match ranges.get(s.id) {
            Some(&(lo, hi)) => ParamSpec { min: lo.max(s.min), max: hi.min(s.max), ..*s },
            None => *s,
        })
        .collect();
    let mut out = randomize(&ranged, values, amount, seed);
    if let Some(m) = values.value(MODE_PARAM) {
        out.set_value(MODE_PARAM, m.clone());
    }
    out.completed(specs)
}

impl App {
    /// (clave, archivo) de una salida de un nodo, si existe.
    pub(crate) fn port_target(&self, id: NodeId, salida: u16) -> Option<(String, std::path::PathBuf)> {
        let s = self.session.as_ref()?;
        let n = s.patch.node(id)?;
        match &n.tipo {
            NodeKind::Fuente(src) => {
                let k = self.keys.get(&id)?.as_ref().ok()?;
                Some((port_key(k, 0), src.ruta.clone()))
            }
            _ => {
                let rec = n.render.as_ref()?;
                let o = rec.salidas.get(salida as usize)?;
                Some((port_key(&rec.clave, salida), s.abs(&o.archivo)))
            }
        }
    }

    /// Duración en segundos de una salida (sin cargarla).
    pub(crate) fn port_duration(&self, id: NodeId, salida: u16) -> Option<f64> {
        let n = self.session.as_ref()?.patch.node(id)?;
        match &n.tipo {
            NodeKind::Fuente(s) => Some(s.frames as f64 / s.sr.max(1) as f64),
            _ => n.render.as_ref()?.salidas.get(salida as usize).map(|o| o.frames as f64 / o.sr.max(1) as f64),
        }
    }

    /// Audio de la entrada principal de un nodo, si ya está en memoria.
    pub(crate) fn node_input_loaded(&self, id: NodeId) -> Option<Loaded> {
        let p = *self.session.as_ref()?.patch.node(id)?.entradas.first()?;
        let (k, _) = self.port_target(p.nodo, p.salida)?;
        self.mem.get(&k).cloned()
    }

    pub(crate) fn node_input_audio(&self, id: NodeId) -> Option<Arc<AudioBuf>> {
        self.node_input_loaded(id).map(|l| l.audio)
    }

    /// Pide cargar todas las entradas de un nodo. Devuelve su audio si ya
    /// están todas en memoria.
    pub(crate) fn ensure_node_inputs(&mut self, id: NodeId) -> Option<Vec<Arc<AudioBuf>>> {
        let ports: Vec<PortRef> = self.session.as_ref()?.patch.node(id)?.entradas.clone();
        let mut out = Vec::new();
        let mut missing = false;
        for p in ports {
            match self.port_target(p.nodo, p.salida) {
                Some((k, path)) => match self.mem.get(&k) {
                    Some(l) => out.push(l.audio.clone()),
                    None => {
                        self.ensure_loaded(&k, &path);
                        missing = true;
                    }
                },
                None => missing = true,
            }
        }
        (!missing).then_some(out)
    }

    // --- Valores por canal ---------------------------------------------------------------------

    /// Interruptor y pestañas de canal.
    pub(crate) fn channel_section(&mut self, ui: &mut Ui, node: &Node, p: &dyn Process, params: &mut ParamSet) {
        let nch = node.entradas.first().map_or(1, |e| self.node_channels(e.nodo));
        if !p.per_channel() || nch < 2 || p.params().is_empty() {
            win32::muted(ui, tr("ui.canales.no_aplica"));
            return;
        }
        let mut on = params.por_canal.is_some();
        let r = win32::checkbox(ui, &mut on, tr("ui.canales.distintos"));
        self.hint_key(&r, "ui.ayuda.canales");
        if on != params.por_canal.is_some() {
            if on {
                params.por_canal = Some(vec![params.comun.clone(); nch]);
            } else if let Some(v) = params.por_canal.take() {
                params.comun = v.into_iter().next().unwrap_or_default();
            }
        }
        if params.por_canal.is_none() {
            return;
        }
        self.chan_tab = self.chan_tab.min(nch - 1);
        ui.horizontal(|ui| {
            win32::label(ui, tr("ui.canales.editando"));
            for c in 0..nch {
                let r = win32::radio(ui, self.chan_tab == c, &t!("ui.canales.canal", n = c + 1));
                self.hint_key(&r, "ui.ayuda.canales");
                if r.clicked() {
                    self.chan_tab = c;
                }
            }
        });
        win32::muted(ui, tr("ui.canales.nota"));
    }

    // --- Variantes: aleatorizar y previews -----------------------------------------------------

    pub(crate) fn variants_section(&mut self, ui: &mut Ui, node: &Node, p: &Arc<dyn Process>, params: &mut ParamSet, seed: &mut u64, ranges: &BTreeMap<String, (f64, f64)>) {
        ui.horizontal_wrapped(|ui| {
            let l = win32::label(ui, tr("ui.variantes.variacion"));
            self.hint_key(&l, "ui.ayuda.variacion");
            let r = win32::spin(ui, &mut self.variant_cfg.variacion, Spin { min: 0.0, max: 100.0, step: 5.0, decimals: 0, suffix: tr("ui.unidad.porcentaje"), width: 80.0 });
            self.hint_key(&r, "ui.ayuda.variacion");
            let b = win32::button_w(ui, tr("ui.variantes.aleatorizar"), 90.0, true);
            self.hint_key(&b, "ui.ayuda.aleatorizar");
            if b.clicked() {
                let s = self.next_seed();
                params.comun = randomize_in_ranges(p.params(), &params.comun, self.variant_cfg.variacion / 100.0, s, ranges);
                if p.uses_seed() {
                    *seed = s;
                }
            }
        });
        if p.works_on_files() {
            win32::muted(ui, tr("ui.previews.no_archivos"));
            return;
        }
        ui.horizontal_wrapped(|ui| {
            let l = win32::label(ui, tr("ui.previews.cantidad"));
            self.hint_key(&l, "ui.ayuda.previews");
            let mut n = self.variant_cfg.cantidad as f64;
            if win32::spin(ui, &mut n, Spin { min: 2.0, max: 8.0, step: 1.0, decimals: 0, suffix: "", width: 50.0 }).changed() {
                self.variant_cfg.cantidad = n as usize;
            }
            let l = win32::label(ui, tr("ui.previews.segundos"));
            self.hint_key(&l, "ui.ayuda.previews_segundos");
            win32::spin(ui, &mut self.variant_cfg.segundos, Spin { min: 1.0, max: 60.0, step: 1.0, decimals: 1, suffix: tr("ui.unidad.s"), width: 76.0 });
            let busy = self.previews.as_ref().is_some_and(|pv| pv.task.is_some());
            let b = win32::button_w(ui, tr("ui.previews.generar"), 80.0, !busy);
            self.hint_key(&b, "ui.ayuda.previews");
            if b.clicked() {
                self.start_previews(node, p.clone(), params, *seed, ranges.clone());
            }
        });
        self.previews_list(ui, node.id, params, seed);
    }

    fn start_previews(&mut self, node: &Node, p: Arc<dyn Process>, params: &ParamSet, seed: u64, ranges: BTreeMap<String, (f64, f64)>) {
        let Some(mut inputs) = self.ensure_node_inputs(node.id) else {
            self.status = Some(Msg::new(Level::Info, "ui.previews.cargando"));
            return;
        };
        if inputs.len() < p.inputs().min() {
            self.status = Some(Msg::new(Level::Warning, "err.patch.falta_entrada"));
            return;
        }
        // Región del nodo y luego los primeros N segundos.
        if let Some(first) = inputs.first_mut() {
            let mut a = (**first).clone();
            if let Some(r) = node.region {
                a = a.slice(r.inicio as usize, r.fin as usize);
            }
            let n = (self.variant_cfg.segundos * a.sr as f64) as usize;
            *first = Arc::new(a.slice(0, n));
        }
        let cfg = self.variant_cfg;
        let opts = self.render_options();
        let base = params.clone();
        let source = inputs.first().cloned();
        let task = Task::spawn("surshape-previews", move |prog, cancel| { // i18n-ok
            let mut out = Vec::with_capacity(cfg.cantidad);
            for i in 0..cfg.cantidad {
                let s = seed.wrapping_add(i as u64 * 7919);
                // La variante 1 es la actual; las demás, aleatorizadas.
                let comun = if i == 0 { base.comun.clone() } else { randomize_in_ranges(p.params(), &base.comun, cfg.variacion / 100.0, s, &ranges) };
                let mut set = base.clone();
                set.comun = comun.clone();
                let job = RenderJob { process: p.clone(), inputs: inputs.clone(), params: set, seed: if i == 0 { seed } else { s }, options: opts };
                let sub = AtomicU32::new(0);
                let r = run(&job, &sub, cancel)?;
                prog.store(((i + 1) as f64 / cfg.cantidad as f64 * 1_000_000.0) as u32, std::sync::atomic::Ordering::Relaxed);
                out.push(Variant { params: comun, seed: job.seed, audio: Loaded::new(r.outputs.into_iter().next().unwrap_or_default()) });
            }
            Ok(out)
        });
        self.previews = Some(Previews { node: node.id, task: Some(task), variants: Vec::new(), source });
    }

    /// Consulta la tarea de previews.
    pub(crate) fn poll_previews(&mut self) {
        let Some(pv) = self.previews.as_mut() else { return };
        let Some(res) = pv.task.as_mut().and_then(|t| t.poll()) else { return };
        pv.task = None;
        match res {
            Ok(v) => pv.variants = v,
            Err(ProcessError::Cancelled) => {}
            Err(e) => self.status = Some(Msg::from_err(&e)),
        }
    }

    fn previews_list(&mut self, ui: &mut Ui, id: NodeId, params: &mut ParamSet, seed: &mut u64) {
        let Some(pv) = self.previews.as_ref().filter(|pv| pv.node == id) else { return };
        if let Some(t) = &pv.task {
            win32::muted(ui, &format!("{} {:.0} {}", tr("ui.previews.generando"), t.progress() * 100.0, tr("ui.unidad.porcentaje")));
            return;
        }
        let source = pv.source.clone();
        let variants = pv.variants.clone();
        let mut choose = None;
        egui::Grid::new("previews").num_columns(3).spacing([8.0, 3.0]).show(ui, |ui| { // i18n-ok
            if let Some(src) = source {
                win32::head(ui, tr("ui.previews.fuente"));
                self.play_button(ui, &src);
                ui.label("");
                ui.end_row();
            }
            for (i, v) in variants.iter().enumerate() {
                let label = if i == 0 { t!("ui.previews.actual").to_string() } else { t!("ui.previews.variante", n = i + 1) };
                win32::label(ui, &label);
                self.play_button(ui, &v.audio.audio);
                let b = win32::button_w(ui, tr("ui.previews.elegir"), 60.0, true);
                self.hint_key(&b, "ui.ayuda.previews_elegir");
                if b.clicked() {
                    choose = Some(i);
                }
                ui.end_row();
            }
        });
        if let Some(i) = choose {
            let v = &variants[i];
            params.comun = v.params.clone();
            *seed = v.seed;
            self.status = Some(Msg::new(Level::Info, "ui.previews.elegida").arg("n", i + 1));
        }
    }

    fn play_button(&mut self, ui: &mut Ui, audio: &Arc<AudioBuf>) {
        let playing = self.player.is_playing_buf(audio);
        let b = win32::tool_button(ui, if playing { win32::Icon::Stop } else { win32::Icon::Play }, tr("ui.ayuda.previews_escuchar"), true, playing);
        self.hint_key(&b, "ui.ayuda.previews_escuchar");
        if b.clicked() {
            if playing {
                self.player.stop();
            } else {
                self.player.play(audio.clone(), 0, audio.frames(), false);
            }
        }
    }

    // --- Instantáneas y presets ------------------------------------------------------------

    pub(crate) fn snapshots_section(&mut self, ui: &mut Ui, p: &dyn Process, params: &mut ParamSet, seed: &mut u64, snaps: &mut Vec<Snapshot>) {
        // Instantáneas del nodo
        let b = win32::button_w(ui, tr("ui.instantaneas.guardar"), 140.0, true);
        self.hint_key(&b, "ui.ayuda.instantaneas");
        if b.clicked() {
            let n = snaps.len() + 1;
            snaps.push(Snapshot { nombre: t!("ui.instantaneas.nombre", n = n), params: params.clone(), seed: *seed });
        }
        let mut action = None;
        for (i, s) in snaps.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(&s.nombre).font(theme::mono_font(theme::MONO_SIZE)));
                let b = win32::button_w(ui, tr("ui.instantaneas.cargar"), 60.0, true);
                self.hint_key(&b, "ui.ayuda.instantaneas");
                if b.clicked() {
                    action = Some((i, true));
                }
                let b = win32::button_w(ui, tr("ui.bp.quitar"), 60.0, true);
                if b.clicked() {
                    action = Some((i, false));
                }
            });
        }
        match action {
            Some((i, true)) => {
                *params = snaps[i].params.clone();
                *seed = snaps[i].seed;
            }
            Some((i, false)) => {
                snaps.remove(i);
            }
            None => {}
        }

        // Presets del proceso (archivos)
        let Some(base) = crate::prefs::presets_dir() else { return };
        let pid = p.id();
        let list = presets::list(&base, pid);
        let default = presets::default_name(&base, pid);
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            let l = win32::label(ui, tr("ui.presets.titulo"));
            self.hint_key(&l, "ui.ayuda.presets");
            let shown = if self.preset_sel.is_empty() { t!("ui.presets.ninguno").to_string() } else { self.preset_sel.clone() };
            let mut chosen = None;
            win32::combo(ui, "presets", &shown, 180.0, |ui| { // i18n-ok
                for name in &list {
                    let label = if default.as_deref() == Some(name.as_str()) { t!("ui.presets.con_defecto", nombre = name) } else { name.clone() };
                    if win32::combo_item(ui, *name == self.preset_sel, &label).clicked() {
                        chosen = Some(name.clone());
                    }
                }
            });
            if let Some(c) = chosen {
                self.preset_sel = c;
            }
            let has = list.contains(&self.preset_sel);
            let b = win32::button_w(ui, tr("ui.presets.cargar"), 60.0, has);
            self.hint_key(&b, "ui.ayuda.presets");
            if b.clicked() {
                match presets::load(&base, pid, &self.preset_sel) {
                    Ok(v) => params.comun = v.completed(p.params()),
                    Err(e) => self.status = Some(Msg::new(Level::Error, "err.presets.leer").arg("detalle", e)),
                }
            }
            let b = win32::button_w(ui, tr("ui.presets.defecto"), 60.0, has);
            self.hint_key(&b, "ui.ayuda.presets_defecto");
            if b.clicked() {
                let name = (default.as_deref() != Some(self.preset_sel.as_str())).then_some(self.preset_sel.as_str());
                if let Err(e) = presets::set_default(&base, pid, name) {
                    self.status = Some(Msg::new(Level::Error, "err.presets.guardar").arg("detalle", e));
                }
            }
            let b = win32::button_w(ui, tr("ui.bp.quitar"), 60.0, has);
            self.hint_key(&b, "ui.ayuda.presets_borrar");
            if b.clicked() {
                let _ = presets::delete(&base, pid, &self.preset_sel);
                self.preset_sel.clear();
            }
        });
        ui.horizontal(|ui| {
            let r = win32::text_field(ui, &mut self.preset_name, 180.0, tr("ui.presets.nombre"));
            self.hint_key(&r, "ui.ayuda.presets_guardar");
            let b = win32::button_w(ui, tr("ui.presets.guardar"), 60.0, !self.preset_name.trim().is_empty());
            self.hint_key(&b, "ui.ayuda.presets_guardar");
            if b.clicked() {
                match presets::save(&base, pid, &self.preset_name, &params.comun) {
                    Ok(name) => {
                        self.preset_sel = name.clone();
                        self.preset_name.clear();
                        self.status = Some(Msg::new(Level::Info, "ui.presets.guardado").arg("nombre", name));
                    }
                    Err(e) => self.status = Some(Msg::new(Level::Error, "err.presets.guardar").arg("detalle", e)),
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_ranges_are_respected_and_mode_kept() {
        let specs = [
            ParamSpec::choice(MODE_PARAM, "x.m", &["a", "b", "c"], 0),
            ParamSpec::float("f", "x.f", 0.0, 100.0, 50.0),
        ];
        let mut v = ParamValues::defaults(&specs);
        v.set(MODE_PARAM, 2.0);
        let ranges = BTreeMap::from([("f".to_string(), (40.0, 45.0))]);
        for s in 0..50 {
            let r = randomize_in_ranges(&specs, &v, 1.0, s, &ranges);
            assert_eq!(r.get(MODE_PARAM), 2.0);
            assert!((40.0..=45.0).contains(&r.get("f")), "{}", r.get("f"));
        }
    }
}
