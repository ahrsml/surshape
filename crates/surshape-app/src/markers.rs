//! Marcadores por fuente (fase 8): importar y exportar etiquetas de
//! Audacity, agregar en el cursor o como región, ir al anterior / siguiente,
//! usar una región para procesar un fragmento y pasar tiempos al Graph-Edit.
//!
//! Los marcadores son de la fuente de la fila (los de una rama, de la fuente
//! de la fila de donde nace). No cambian el sonido: no entran en la clave.

use crate::app::{App, DialogResult, Msg};
use crate::win32::Level;
use surshape_i18n::t;
use surshape_patch::{Marker, NodeId, NodeKind};

/// Lee etiquetas de Audacity ("inicio<TAB>fin<TAB>texto" por línea, en
/// segundos). Las líneas de selección espectral (empiezan con "\") se
/// ignoran. Una etiqueta de punto (inicio = fin) es un marcador; si no, una
/// región.
pub(crate) fn parse_labels(txt: &str) -> Result<Vec<Marker>, usize> {
    let mut out = Vec::new();
    for (i, raw) in txt.lines().enumerate() {
        let l = raw.trim_end_matches('\r');
        if l.trim().is_empty() || l.starts_with('\\') {
            continue;
        }
        let mut parts = l.splitn(3, '\t');
        let num = |s: Option<&str>| s.and_then(|x| x.trim().replace(',', ".").parse::<f64>().ok()).filter(|v| v.is_finite() && *v >= 0.0);
        let a = num(parts.next()).ok_or(i + 1)?;
        let b = num(parts.next()).ok_or(i + 1)?;
        let label = parts.next().unwrap_or("").trim().to_string();
        let (a, b) = (a.min(b), a.max(b));
        out.push(Marker { t: a, fin: (b - a > 1e-6).then_some(b), etiqueta: label });
    }
    out.sort_by(|x, y| x.t.total_cmp(&y.t));
    Ok(out)
}

/// Etiquetas de Audacity a partir de marcadores.
pub(crate) fn labels_text(m: &[Marker]) -> String {
    m.iter().map(|m| format!("{:.6}\t{:.6}\t{}\n", m.t, m.fin.unwrap_or(m.t), m.etiqueta)).collect()
}

impl App {
    /// Fuente de la fila de una celda (siguiendo las ramas hasta su origen).
    pub(crate) fn row_source(&self, id: NodeId) -> Option<NodeId> {
        let p = &self.session.as_ref()?.patch;
        let mut cur = id;
        for _ in 0..64 {
            let (r, _) = p.cell_of(cur)?;
            let row = &p.filas[r];
            match row.origen {
                Some(o) => cur = o.nodo,
                None => {
                    let first = *row.celdas.first()?;
                    return p.node(first).filter(|n| n.source().is_some()).map(|n| n.id);
                }
            }
        }
        None
    }

    /// Marcadores de la fuente de la fila de la celda elegida.
    pub(crate) fn current_markers(&self) -> Vec<Marker> {
        self.selected
            .and_then(|id| self.row_source(id))
            .and_then(|s| self.session.as_ref()?.patch.node(s)?.source().map(|x| x.marcadores.clone()))
            .unwrap_or_default()
    }

    fn edit_markers(&mut self, f: impl FnOnce(&mut Vec<Marker>)) -> bool {
        let Some(src) = self.selected.and_then(|id| self.row_source(id)) else {
            self.status = Some(Msg::new(Level::Warning, "ui.marcadores.sin_fuente"));
            return false;
        };
        self.push_undo();
        if let Some(NodeKind::Fuente(s)) = self.session.as_mut().and_then(|s| s.patch.node_mut(src)).map(|n| &mut n.tipo) {
            f(&mut s.marcadores);
            s.marcadores.sort_by(|a, b| a.t.total_cmp(&b.t));
        }
        self.save_now();
        true
    }

    /// Segundos del cursor y de la selección del visor.
    fn view_secs(&self) -> (f64, Option<(f64, f64)>) {
        let sr = self.view_loaded().map_or(48000, |l| l.audio.sr) as f64;
        (self.view.cursor as f64 / sr, self.view.selection().map(|(a, b)| (a as f64 / sr, b as f64 / sr)))
    }

    pub(crate) fn marker_add(&mut self, region: bool) {
        let (cur, sel) = self.view_secs();
        let n = self.current_markers().len() + 1;
        let label = t!("ui.marcadores.nombre", n = n);
        let m = match (region, sel) {
            (true, Some((a, b))) => Marker { t: a, fin: Some(b), etiqueta: label },
            (true, None) => {
                self.status = Some(Msg::new(Level::Info, "ui.marcadores.sin_seleccion"));
                return;
            }
            _ => Marker { t: cur, fin: None, etiqueta: label },
        };
        if self.edit_markers(|v| v.push(m)) {
            self.status = Some(Msg::new(Level::Info, "ui.marcadores.agregado"));
        }
    }

    pub(crate) fn markers_clear(&mut self) {
        if self.edit_markers(|v| v.clear()) {
            self.status = Some(Msg::new(Level::Info, "ui.marcadores.quitados"));
        }
    }

    /// Va al marcador anterior o siguiente; si es una región, la elige.
    pub(crate) fn marker_step(&mut self, forward: bool) {
        let ms = self.current_markers();
        let Some(l) = self.view_loaded() else { return };
        let (sr, frames) = (l.audio.sr as f64, l.audio.frames());
        let cur = self.view.cursor as f64 / sr;
        let pick = if forward {
            ms.iter().find(|m| m.t > cur + 1e-4)
        } else {
            ms.iter().rev().find(|m| m.t < cur - 1e-4)
        };
        let Some(m) = pick.or(if forward { ms.first() } else { ms.last() }).cloned() else {
            self.status = Some(Msg::new(Level::Info, "ui.marcadores.ninguno"));
            return;
        };
        let to = |s: f64| ((s * sr) as usize).min(frames);
        self.view.cursor = to(m.t);
        self.view.sel = m.fin.map(|f| (to(m.t), to(f)));
        if self.view.cursor as f64 > self.view.start + self.view.span || (self.view.cursor as f64) < self.view.start {
            self.view.start = (self.view.cursor as f64 - self.view.span * 0.1).max(0.0);
        }
        let i = ms.iter().position(|x| x.t == m.t).map_or(0, |i| i + 1);
        self.status = Some(Msg::new(Level::Info, "ui.marcadores.en").arg("n", i).arg("total", ms.len()).arg("nombre", m.etiqueta));
    }

    pub(crate) fn dialog_import_labels(&mut self) {
        let Some(src) = self.selected.and_then(|id| self.row_source(id)) else {
            self.status = Some(Msg::new(Level::Warning, "ui.marcadores.sin_fuente"));
            return;
        };
        let filter = t!("ui.dialogo.filtro_etiquetas").to_string();
        self.open_dialog(move |d| d.add_filter(filter, &["txt"]).pick_file().map(|p| DialogResult::ImportLabels(src, p))); // i18n-ok
    }

    pub(crate) fn dialog_export_labels(&mut self) {
        let txt = labels_text(&self.current_markers());
        let filter = t!("ui.dialogo.filtro_etiquetas").to_string();
        self.open_dialog(move |d| d.add_filter(filter, &["txt"]).set_file_name("etiquetas.txt").save_file().map(|p| DialogResult::SaveText(p, txt))); // i18n-ok
    }

    pub(crate) fn import_labels(&mut self, src: NodeId, path: &std::path::Path) {
        let txt = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => {
                self.status = Some(Msg::new(Level::Error, "err.audio.abrir").arg("archivo", path.display()).arg("detalle", e));
                return;
            }
        };
        match parse_labels(&txt) {
            Ok(ms) => {
                let n = ms.len();
                self.push_undo();
                if let Some(NodeKind::Fuente(s)) = self.session.as_mut().and_then(|s| s.patch.node_mut(src)).map(|n| &mut n.tipo) {
                    s.marcadores = ms;
                }
                self.save_now();
                self.status = Some(Msg::new(Level::Info, "ui.marcadores.importados").arg("n", n));
            }
            Err(line) => self.status = Some(Msg::new(Level::Error, "err.marcadores.linea").arg("linea", line)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audacity_labels_roundtrip() {
        let txt = "1.500000\t1.500000\tgolpe\n\\\t100.0\t2000.0\n0,250000\t0,750000\tzona uno\n";
        let m = parse_labels(txt).unwrap();
        assert_eq!(m.len(), 2);
        assert_eq!((m[0].t, m[0].fin, m[0].etiqueta.as_str()), (0.25, Some(0.75), "zona uno"));
        assert_eq!((m[1].t, m[1].fin), (1.5, None));
        assert_eq!(parse_labels(&labels_text(&m)).unwrap(), m);
        assert_eq!(parse_labels("x\ty\n"), Err(1));
    }
}
