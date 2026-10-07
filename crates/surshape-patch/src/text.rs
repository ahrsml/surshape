//! Patch como texto legible y editable a mano (`.sspatch`).
//!
//! Una línea por celda, con su nombre de planilla, su tipo y sus valores:
//!
//! ```text
//! # SURSHAPE patch
//! version 1
//! A_0  fuente  ruta="C:\Sonidos\koto.wav"
//! A_1  cdp.pvoc_anal  modo=0 puntos=1024 superposicion=3
//! A_2  cdp.blur_blur  ventanas=bp(normalizado,lineal;0:2;1:40)
//! A_3  nat.paulstretch  factor=8 ventana=0.25 seed=922299 iter=2
//! B_0  ->A_1
//! B_1  cdp.combine_cross  cantidad=1 entrada2=A_2
//! C_0  mezcla  entrada1=A_3 gan1=-3 pan1=0 ini1=0 entrada2=B_1 gan2=0 pan2=0.5 ini2=1.2
//! ```
//!
//! - La entrada principal de una celda es la celda anterior de su fila (o
//!   la referencia de la columna 0, `->A_1`); otras entradas van como
//!   `entrada2=B_1` (o `B_1:2` para la salida 2).
//! - Breakpoints: `bp(tiempo,curva;t:v;t:v...)` con tiempo `absoluto` o
//!   `normalizado` y curva `lineal`, `escalon` o `exponencial`.
//! - Valores por canal: `c2.ventanas=14` (canal 2).
//! - `region=1000-3000` (muestras de la entrada), `iter=2`, `seed=N`.
//!
//! - Marcadores de una fuente: líneas `marcador t=1.5 fin=3 etiqueta="x"`
//!   debajo de la fuente.
//! - Sub-patch: `B_2  subpatch  nombre="Nube"` y debajo una línea `paso`
//!   por proceso (`paso nat.paulstretch factor=8`; entradas internas como
//!   `entrada2=@0`).
//!
//! Los renders no viajan: al abrir el patch se recuperan de la cache por su
//! clave o se vuelven a calcular.

use crate::{row_index, row_name, Marker, MixInput, Node, NodeKind, NodeTemplate, Patch, PortRef, Region, Row, SourceInfo, TemplateInput};
use std::collections::HashMap;
use std::path::PathBuf;
use surshape_engine::{Breakpoints, Interp, ParamSet, ParamValue, ParamValues, TimeMode};

pub const HEADER: &str = "# SURSHAPE patch";
pub const VERSION: u32 = 1;

/// Error al leer: línea (1..) y clave i18n.
#[derive(Clone, Debug, PartialEq)]
pub struct TextError {
    pub line: usize,
    pub key: &'static str,
    pub detail: String,
}

fn terr(line: usize, key: &'static str, detail: impl ToString) -> TextError {
    TextError { line, key, detail: detail.to_string() }
}

fn num(x: f64) -> String {
    if x.fract() == 0.0 && x.abs() < 1e15 {
        format!("{}", x as i64)
    } else {
        let s = format!("{x:.9}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

fn value_text(v: &ParamValue) -> String {
    match v {
        ParamValue::Fixed(x) => num(*x),
        ParamValue::Envelope(bp) => {
            let t = match bp.tiempo {
                TimeMode::Absoluto => "absoluto",
                TimeMode::Normalizado => "normalizado",
            };
            let c = match bp.curva {
                Interp::Lineal => "lineal",
                Interp::Escalon => "escalon",
                Interp::Exponencial => "exponencial",
            };
            let pts: Vec<String> = bp.puntos.iter().map(|(a, b)| format!("{}:{}", num(*a), num(*b))).collect();
            format!("bp({t},{c};{})", pts.join(";"))
        }
    }
}

fn parse_value(s: &str) -> Option<ParamValue> {
    if let Some(inner) = s.strip_prefix("bp(").and_then(|r| r.strip_suffix(')')) {
        let mut parts = inner.split(';');
        let head = parts.next()?;
        let (t, c) = head.split_once(',')?;
        let tiempo = match t.trim() {
            "absoluto" => TimeMode::Absoluto,
            "normalizado" => TimeMode::Normalizado,
            _ => return None,
        };
        let curva = match c.trim() {
            "lineal" => Interp::Lineal,
            "escalon" => Interp::Escalon,
            "exponencial" => Interp::Exponencial,
            _ => return None,
        };
        let mut pts = Vec::new();
        for p in parts.filter(|p| !p.trim().is_empty()) {
            let (a, b) = p.split_once(':')?;
            pts.push((a.trim().parse().ok()?, b.trim().parse().ok()?));
        }
        return Some(ParamValue::Envelope(Breakpoints::new(tiempo, curva, pts)));
    }
    s.parse::<f64>().ok().filter(|x| x.is_finite()).map(ParamValue::Fixed)
}

fn port_text(p: &Patch, port: PortRef) -> String {
    let l = p.cell_label(port.nodo).unwrap_or_else(|| "?".into());
    if port.salida > 0 {
        format!("{l}:{}", port.salida)
    } else {
        l
    }
}

fn params_text(out: &mut String, set: &ParamSet) {
    for (k, v) in &set.comun.0 {
        out.push_str(&format!(" {k}={}", value_text(v)));
    }
    if let Some(chs) = &set.por_canal {
        for (i, vals) in chs.iter().enumerate() {
            for (k, v) in &vals.0 {
                out.push_str(&format!(" c{}.{k}={}", i + 1, value_text(v)));
            }
        }
    }
}

/// Texto del patch.
pub fn to_text(p: &Patch) -> String {
    let mut s = format!("{HEADER}\nversion {VERSION}\n");
    for (r, row) in p.filas.iter().enumerate() {
        let letter = row_name(r);
        if let Some(o) = row.origen {
            s.push_str(&format!("{letter}_0  ->{}\n", port_text(p, o)));
        }
        let off = usize::from(row.origen.is_some());
        for (c, &id) in row.celdas.iter().enumerate() {
            let Some(n) = p.node(id) else { continue };
            let mut line = format!("{letter}_{}  ", c + off);
            // Entrada principal implícita: la celda anterior o el origen.
            let implicit = if c > 0 { Some(PortRef::main(row.celdas[c - 1])) } else { row.origen };
            match &n.tipo {
                NodeKind::Fuente(src) => {
                    line.push_str(&format!(
                        "fuente  ruta=\"{}\" hash={} sr={} canales={} frames={}",
                        src.ruta.display(),
                        src.hash,
                        src.sr,
                        src.canales,
                        src.frames
                    ));
                }
                NodeKind::Mezcla { canales } => {
                    line.push_str("mezcla ");
                    for (i, (port, m)) in n.entradas.iter().zip(canales).enumerate() {
                        let k = i + 1;
                        line.push_str(&format!(
                            " entrada{k}={} gan{k}={} pan{k}={} ini{k}={}",
                            port_text(p, *port),
                            num(m.ganancia_db),
                            num(m.paneo),
                            num(m.inicio)
                        ));
                    }
                }
                NodeKind::Proceso { proceso } => {
                    line.push_str(proceso);
                    line.push(' ');
                    params_text(&mut line, &n.params);
                    if n.seed != 0 {
                        line.push_str(&format!(" seed={}", n.seed));
                    }
                    if let Some(rg) = n.region {
                        line.push_str(&format!(" region={}-{}", rg.inicio, rg.fin));
                    }
                    if n.iteraciones > 1 {
                        line.push_str(&format!(" iter={}", n.iteraciones));
                    }
                    for (i, port) in n.entradas.iter().enumerate() {
                        if i == 0 && Some(*port) == implicit {
                            continue;
                        }
                        line.push_str(&format!(" entrada{}={}", i + 1, port_text(p, *port)));
                    }
                }
                NodeKind::SubPatch { plantilla, nombre, pasos } => {
                    line.push_str(&format!("subpatch  nombre=\"{nombre}\" plantilla=\"{}\"", plantilla.display()));
                    if n.entradas.first().copied() != implicit {
                        if let Some(port) = n.entradas.first() {
                            line.push_str(&format!(" entrada1={}", port_text(p, *port)));
                        }
                    }
                    for t in pasos {
                        line.push('\n');
                        line.push_str(&step_text(t));
                    }
                }
            }
            s.push_str(line.trim_end());
            s.push('\n');
            if let NodeKind::Fuente(src) = &n.tipo {
                for m in &src.marcadores {
                    let mut l = format!("    marcador t={}", num(m.t));
                    if let Some(f) = m.fin {
                        l.push_str(&format!(" fin={}", num(f)));
                    }
                    if !m.etiqueta.is_empty() {
                        l.push_str(&format!(" etiqueta=\"{}\"", m.etiqueta.replace('"', "'")));
                    }
                    s.push_str(&l);
                    s.push('\n');
                }
            }
        }
    }
    s
}

/// Línea de un paso de sub-patch.
fn step_text(t: &NodeTemplate) -> String {
    let NodeKind::Proceso { proceso } = &t.tipo else { return String::new() };
    let mut l = format!("    paso {proceso}");
    let mut tmp = String::new();
    params_text(&mut tmp, &t.params);
    l.push_str(&tmp);
    if t.seed != 0 {
        l.push_str(&format!(" seed={}", t.seed));
    }
    if let Some(rg) = t.region {
        l.push_str(&format!(" region={}-{}", rg.inicio, rg.fin));
    }
    if t.iteraciones > 1 {
        l.push_str(&format!(" iter={}", t.iteraciones));
    }
    for (i, e) in t.otras_entradas.iter().enumerate() {
        if let TemplateInput::Interna { indice, salida } = e {
            let s = if *salida > 0 { format!(":{salida}") } else { String::new() };
            l.push_str(&format!(" entrada{}=@{indice}{s}", i + 2));
        }
    }
    l
}

/// Valores de un paso o una celda: (parámetros, seed, región, iteraciones,
/// entradas `entradaN`).
#[allow(clippy::type_complexity)]
fn parse_values(ln: usize, kv: &[(String, String)]) -> Result<(ParamSet, u64, Option<Region>, u32, Vec<(usize, String)>), TextError> {
    let mut params = ParamSet::default();
    let (mut seed, mut region, mut iter, mut ins) = (0, None, 1, Vec::new());
    for (k, v) in kv {
        if let Some(n) = k.strip_prefix("entrada").and_then(|x| x.parse::<usize>().ok()) {
            ins.push((n, v.clone()));
            continue;
        }
        match k.as_str() {
            "seed" => seed = v.parse().map_err(|_| terr(ln, "err.patch_texto.valor", k))?,
            "iter" => iter = v.parse::<u32>().map_err(|_| terr(ln, "err.patch_texto.valor", k))?.max(1),
            "region" => {
                let (a, b) = v.split_once('-').ok_or_else(|| terr(ln, "err.patch_texto.valor", k))?;
                let a = a.parse().map_err(|_| terr(ln, "err.patch_texto.valor", k))?;
                let b = b.parse().map_err(|_| terr(ln, "err.patch_texto.valor", k))?;
                region = Some(Region { inicio: a, fin: b });
            }
            _ => {
                let val = parse_value(v).ok_or_else(|| terr(ln, "err.patch_texto.valor", format!("{k}={v}")))?;
                let per = k.strip_prefix('c').and_then(|r| r.split_once('.')).and_then(|(n, id)| Some((n.parse::<usize>().ok()?, id)));
                match per {
                    Some((ch, pid)) if ch >= 1 => {
                        let v = params.por_canal.get_or_insert_with(Vec::new);
                        if v.len() < ch {
                            v.resize(ch, ParamValues::default());
                        }
                        v[ch - 1].set_value(pid, val);
                    }
                    _ => params.comun.set_value(k, val),
                }
            }
        }
    }
    Ok((params, seed, region, iter, ins))
}

/// Separa una línea en palabras, respetando comillas.
fn tokens(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for ch in line.chars() {
        match ch {
            '"' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn parse_label(s: &str) -> Option<(usize, usize)> {
    let (r, c) = s.split_once('_')?;
    Some((row_index(r)?, c.parse().ok()?))
}

struct Pending {
    line: usize,
    id: crate::NodeId,
    /// (número de entrada desde 1, referencia).
    inputs: Vec<(usize, String)>,
}

/// Lee un patch de texto.
pub fn from_text(src: &str) -> Result<Patch, TextError> {
    let mut p = Patch::default();
    let mut labels: HashMap<(usize, usize), crate::NodeId> = HashMap::new();
    let mut origins: Vec<(usize, usize, String)> = Vec::new(); // (línea, fila, ref)
    let mut pending: Vec<Pending> = Vec::new();
    let mut version_seen = false;
    for (i, raw) in src.lines().enumerate() {
        let ln = i + 1;
        let l = raw.trim();
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        let tk = tokens(l);
        // Líneas que completan la celda anterior: marcadores y pasos.
        if tk[0] == "marcador" || tk[0] == "paso" {
            let last = pending.last().map(|x| x.id).ok_or_else(|| terr(ln, "err.patch_texto.tipo", &tk[0]))?;
            let kv: Vec<(String, String)> = tk[if tk[0] == "paso" { 2 } else { 1 }..]
                .iter()
                .map(|t| t.split_once('=').map(|(k, v)| (k.to_string(), v.to_string())).ok_or_else(|| terr(ln, "err.patch_texto.valor", t)))
                .collect::<Result<_, _>>()?;
            let get = |k: &str| kv.iter().find(|(a, _)| a == k).map(|(_, v)| v.clone());
            let node = p.node_mut(last).expect("celda anterior");
            match (&mut node.tipo, tk[0].as_str()) {
                (NodeKind::Fuente(src), "marcador") => {
                    let t = get("t").and_then(|v| v.parse().ok()).ok_or_else(|| terr(ln, "err.patch_texto.valor", "t"))?;
                    let fin = get("fin").and_then(|v| v.parse().ok());
                    src.marcadores.push(Marker { t, fin, etiqueta: get("etiqueta").unwrap_or_default() });
                }
                (NodeKind::SubPatch { pasos, .. }, "paso") => {
                    let id = tk.get(1).filter(|x| x.contains('.')).ok_or_else(|| terr(ln, "err.patch_texto.tipo", l))?;
                    let (params, seed, region, iteraciones, ins) = parse_values(ln, &kv)?;
                    let mut otras = Vec::new();
                    for (_, r) in ins {
                        let r = r.strip_prefix('@').ok_or_else(|| terr(ln, "err.patch_texto.referencia", &r))?;
                        let (i, s) = r.split_once(':').unwrap_or((r, "0"));
                        let indice = i.parse().map_err(|_| terr(ln, "err.patch_texto.referencia", r))?;
                        let salida = s.parse().map_err(|_| terr(ln, "err.patch_texto.referencia", r))?;
                        otras.push(TemplateInput::Interna { indice, salida });
                    }
                    pasos.push(NodeTemplate { tipo: NodeKind::Proceso { proceso: id.clone() }, params, seed, region, iteraciones, otras_entradas: otras });
                }
                _ => return Err(terr(ln, "err.patch_texto.tipo", l)),
            }
            continue;
        }
        if tk[0] == "version" {
            let v: u32 = tk.get(1).and_then(|x| x.parse().ok()).ok_or_else(|| terr(ln, "err.patch_texto.version", l))?;
            if v > VERSION {
                return Err(terr(ln, "err.patch_texto.version", v));
            }
            version_seen = true;
            continue;
        }
        let (row, col) = parse_label(&tk[0]).ok_or_else(|| terr(ln, "err.patch_texto.celda", &tk[0]))?;
        if tk.len() < 2 {
            return Err(terr(ln, "err.patch_texto.tipo", l));
        }
        while p.filas.len() <= row {
            p.filas.push(Row::default());
        }
        if let Some(r) = tk[1].strip_prefix("->") {
            if col != 0 {
                return Err(terr(ln, "err.patch_texto.celda", &tk[0]));
            }
            origins.push((ln, row, r.to_string()));
            labels.insert((row, 0), u32::MAX); // marca: columna ocupada por la referencia
            continue;
        }
        if labels.contains_key(&(row, col)) {
            return Err(terr(ln, "err.patch_texto.repetida", &tk[0]));
        }
        let kv: Vec<(String, String)> = tk[2..]
            .iter()
            .map(|t| t.split_once('=').map(|(k, v)| (k.to_string(), v.to_string())).ok_or_else(|| terr(ln, "err.patch_texto.valor", t)))
            .collect::<Result<_, _>>()?;
        let get = |k: &str| kv.iter().find(|(a, _)| a == k).map(|(_, v)| v.as_str());
        let tipo = match tk[1].as_str() {
            "fuente" => {
                let ruta = get("ruta").ok_or_else(|| terr(ln, "err.patch_texto.valor", "ruta"))?;
                let n = |k: &str| get(k).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
                NodeKind::Fuente(SourceInfo {
                    ruta: PathBuf::from(ruta),
                    hash: get("hash").unwrap_or_default().to_string(),
                    sr: n("sr") as u32,
                    canales: n("canales") as u16,
                    frames: n("frames"),
                    bytes: 0,
                    modificado: 0,
                    marcadores: Vec::new(),
                })
            }
            "mezcla" => NodeKind::Mezcla { canales: Vec::new() },
            "subpatch" => NodeKind::SubPatch {
                plantilla: PathBuf::from(get("plantilla").unwrap_or_default()),
                nombre: get("nombre").unwrap_or_default().to_string(),
                pasos: Vec::new(),
            },
            id if id.contains('.') => NodeKind::Proceso { proceso: id.to_string() },
            other => return Err(terr(ln, "err.patch_texto.tipo", other)),
        };
        let is_mix = matches!(tipo, NodeKind::Mezcla { .. });
        let id = p.new_node(tipo, Vec::new());
        labels.insert((row, col), id);
        let mut pend = Pending { line: ln, id, inputs: Vec::new() };
        let mut params = ParamSet::default();
        let mut mix: Vec<MixInput> = Vec::new();
        for (k, v) in &kv {
            let node = p.node_mut(id).expect("nodo nuevo");
            if let Some(n) = k.strip_prefix("entrada").and_then(|x| x.parse::<usize>().ok()) {
                pend.inputs.push((n, v.clone()));
                continue;
            }
            if is_mix {
                let field = |pre: &str| k.strip_prefix(pre).and_then(|x| x.parse::<usize>().ok());
                let x: f64 = v.parse().map_err(|_| terr(ln, "err.patch_texto.valor", k))?;
                for (pre, f) in [("gan", 0), ("pan", 1), ("ini", 2)] {
                    if let Some(n) = field(pre) {
                        if mix.len() < n {
                            mix.resize(n, MixInput::default());
                        }
                        let m = &mut mix[n - 1];
                        match f {
                            0 => m.ganancia_db = x,
                            1 => m.paneo = x,
                            _ => m.inicio = x,
                        }
                    }
                }
                continue;
            }
            match k.as_str() {
                "ruta" | "hash" | "sr" | "canales" | "frames" | "plantilla" | "nombre" => {}
                "seed" => node.seed = v.parse().map_err(|_| terr(ln, "err.patch_texto.valor", k))?,
                "iter" => node.iteraciones = v.parse::<u32>().map_err(|_| terr(ln, "err.patch_texto.valor", k))?.max(1),
                "region" => {
                    let (a, b) = v.split_once('-').ok_or_else(|| terr(ln, "err.patch_texto.valor", k))?;
                    let a = a.parse().map_err(|_| terr(ln, "err.patch_texto.valor", k))?;
                    let b = b.parse().map_err(|_| terr(ln, "err.patch_texto.valor", k))?;
                    node.region = Some(Region { inicio: a, fin: b });
                }
                _ => {
                    let val = parse_value(v).ok_or_else(|| terr(ln, "err.patch_texto.valor", format!("{k}={v}")))?;
                    // "c2.ventanas": valor del canal 2.
                    let per = k.strip_prefix('c').and_then(|r| r.split_once('.')).and_then(|(n, id)| Some((n.parse::<usize>().ok()?, id)));
                    match per {
                        Some((ch, pid)) if ch >= 1 => {
                            let v = params.por_canal.get_or_insert_with(Vec::new);
                            if v.len() < ch {
                                v.resize(ch, ParamValues::default());
                            }
                            v[ch - 1].set_value(pid, val);
                        }
                        _ => params.comun.set_value(k, val),
                    }
                }
            }
        }
        if let Some(node) = p.node_mut(id) {
            node.params = params;
            if let NodeKind::Mezcla { canales } = &mut node.tipo {
                *canales = mix;
            }
        }
        pending.push(pend);
    }
    if !version_seen {
        return Err(terr(0, "err.patch_texto.version", ""));
    }

    // Filas: celdas en orden de columna.
    let mut by_row: Vec<Vec<(usize, crate::NodeId)>> = vec![Vec::new(); p.filas.len()];
    for (&(r, c), &id) in &labels {
        if id != u32::MAX {
            by_row[r].push((c, id));
        }
    }
    for (r, cells) in by_row.iter_mut().enumerate() {
        cells.sort();
        p.filas[r].celdas = cells.iter().map(|(_, id)| *id).collect();
    }
    let resolve = |p: &Patch, labels: &HashMap<(usize, usize), crate::NodeId>, s: &str, ln: usize| -> Result<PortRef, TextError> {
        let (lab, salida) = match s.split_once(':') {
            Some((a, b)) => (a, b.parse::<u16>().map_err(|_| terr(ln, "err.patch_texto.referencia", s))?),
            None => (s, 0),
        };
        let pos = parse_label(lab).ok_or_else(|| terr(ln, "err.patch_texto.referencia", s))?;
        let id = labels.get(&pos).copied().filter(|&i| i != u32::MAX).ok_or_else(|| terr(ln, "err.patch_texto.referencia", s))?;
        p.node(id).ok_or_else(|| terr(ln, "err.patch_texto.referencia", s))?;
        Ok(PortRef { nodo: id, salida })
    };
    for (ln, row, r) in &origins {
        let port = resolve(&p, &labels, r, *ln)?;
        p.filas[*row].origen = Some(port);
    }
    // Entradas: la principal implícita y las explícitas.
    for pend in &pending {
        let (row, idx) = p.cell_of(pend.id).expect("celda");
        let is_mix = matches!(p.node(pend.id).map(|n| &n.tipo), Some(NodeKind::Mezcla { .. }));
        let is_source = matches!(p.node(pend.id).map(|n| &n.tipo), Some(NodeKind::Fuente(_)));
        let mut ins: Vec<Option<PortRef>> = Vec::new();
        if !is_mix && !is_source {
            let implicit = if idx > 0 { Some(PortRef::main(p.filas[row].celdas[idx - 1])) } else { p.filas[row].origen };
            ins.push(implicit);
        }
        for (n, s) in &pend.inputs {
            let port = resolve(&p, &labels, s, pend.line)?;
            if ins.len() < *n {
                ins.resize(*n, None);
            }
            ins[n - 1] = Some(port);
        }
        // Un generador no tiene entrada implícita: si la primera quedó
        // vacía y no hay otras, no lleva entradas.
        let ins: Vec<PortRef> = ins.into_iter().flatten().collect();
        if let Some(node) = p.node_mut(pend.id) {
            node.entradas = ins;
            if let NodeKind::Mezcla { canales } = &mut node.tipo {
                let len = node.entradas.len();
                canales.resize(len, MixInput::default());
            }
        }
    }
    // Las filas vacías (huecos en las letras) se quitan.
    p.filas.retain(|r| !r.celdas.is_empty() || r.origen.is_some());
    p.topo_order().map_err(|_| terr(0, "err.patch.ciclo", ""))?;
    Ok(p)
}

/// Quita la entrada implícita de los generadores (procesos que no tienen
/// entradas): `from_text` no conoce el catálogo, la app lo corrige con esto.
pub fn fix_generators(p: &mut Patch, is_generator: impl Fn(&str) -> bool) {
    let ids: Vec<crate::NodeId> = p.nodos.iter().filter(|n| n.process_id().is_some_and(&is_generator)).map(|n| n.id).collect();
    for id in ids {
        let first_cell = p.cell_of(id).is_some_and(|(_, c)| c == 0);
        if let Some(n) = p.node_mut(id) {
            if first_cell {
                n.entradas.clear();
            }
        }
    }
}

impl Node {
    /// Nombre corto del tipo de nodo (para mensajes).
    pub fn kind_name(&self) -> &str {
        match &self.tipo {
            NodeKind::Fuente(_) => "fuente",
            NodeKind::Proceso { proceso } => proceso,
            NodeKind::Mezcla { .. } => "mezcla",
            NodeKind::SubPatch { .. } => "subpatch",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use surshape_engine::RenderOptions;

    fn src(h: &str) -> SourceInfo {
        SourceInfo {
            ruta: PathBuf::from(format!("C:\\Sonidos con espacio\\{h}.wav")),
            hash: h.into(),
            sr: 48000,
            canales: 2,
            frames: 48000,
            bytes: 0,
            modificado: 0,
            marcadores: vec![],
        }
    }

    #[test]
    fn text_roundtrip_keeps_keys() {
        let reg = surshape_native::registry();
        let opts = RenderOptions::default();
        let mut p = Patch::default();
        let (ir, _) = p.add_source(src("ir"));
        let (_, row) = p.add_source(src("a"));
        let a = p.append(row, reg.get("nat.reverse").unwrap().as_ref(), 1).unwrap();
        let b = p.append(row, reg.get("nat.bitcrush").unwrap().as_ref(), 7).unwrap();
        p.node_mut(b).unwrap().params.comun.set_value(
            "bits",
            ParamValue::Envelope(Breakpoints::new(TimeMode::Normalizado, Interp::Exponencial, vec![(0.0, 2.0), (1.0, 12.0)])),
        );
        p.node_mut(b).unwrap().region = Some(Region { inicio: 10, fin: 900 });
        p.node_mut(b).unwrap().iteraciones = 3;
        let c = p.append(row, reg.get("nat.convolve").unwrap().as_ref(), 1).unwrap();
        p.connect(c, 1, PortRef::main(ir)).unwrap();
        let br = p.add_branch(PortRef::main(a)).unwrap();
        p.append(br, reg.get("nat.reverse").unwrap().as_ref(), 1).unwrap();
        let (m, _) = p.add_mix();
        p.connect(m, 0, PortRef::main(c)).unwrap();
        p.connect(m, 1, PortRef::main(b)).unwrap();
        if let NodeKind::Mezcla { canales } = &mut p.node_mut(m).unwrap().tipo {
            canales[1].paneo = 0.5;
        }
        if let NodeKind::Fuente(s) = &mut p.node_mut(ir).unwrap().tipo {
            s.marcadores.push(Marker { t: 0.5, fin: Some(1.25), etiqueta: "golpe uno".into() });
            s.marcadores.push(Marker { t: 2.0, fin: None, etiqueta: String::new() });
        }
        let pasos = p.copy_cells(&[a, b]);
        let sp = p.append_subpatch(br, "Nube", PathBuf::from("nube.receta"), &pasos).unwrap();
        let txt = to_text(&p);
        assert!(txt.contains("B_1  nat.reverse"), "{txt}");
        assert!(txt.contains("marcador t=0.5 fin=1.25 etiqueta=\"golpe uno\""), "{txt}");
        assert!(txt.contains("subpatch  nombre=\"Nube\""), "{txt}");
        assert!(txt.contains("    paso nat.bitcrush"), "{txt}");
        let _ = sp;
        assert!(txt.contains("C_0  ->B_1"), "{txt}");
        assert!(txt.contains("bp(normalizado,exponencial;0:2;1:12)"), "{txt}");
        let back = from_text(&txt).unwrap();
        assert_eq!(to_text(&back), txt);
        // Mismas claves celda por celda.
        let (k1, k2) = (p.keys(&reg, &opts), back.keys(&reg, &opts));
        for n in &p.nodos {
            let l = p.cell_label(n.id).unwrap();
            let id2 = back.nodos.iter().find(|x| back.cell_label(x.id).as_deref() == Some(l.as_str())).unwrap().id;
            assert_eq!(k1[&n.id], k2[&id2], "celda {l}");
        }
    }

    #[test]
    fn errors_point_to_the_line() {
        let e = from_text("version 1\nA_0 fuente ruta=\"x.wav\"\nA_1 nat.reverse entrada2=Z_9\n").unwrap_err();
        assert_eq!((e.line, e.key), (3, "err.patch_texto.referencia"));
        assert_eq!(from_text("A_0 fuente ruta=x").unwrap_err().key, "err.patch_texto.version");
        assert_eq!(from_text("version 1\nA_0 fuente ruta=x\nA_1 nat.x v=abc").unwrap_err().key, "err.patch_texto.valor");
    }
}
