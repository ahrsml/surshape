//! Exporta el catálogo de procesos (nativos y CDP curados) a JSON, para que
//! `docs/generar_docs.py` arme el manual desde el código (rangos, unidades,
//! valores por defecto) y no quede desactualizado.
//!
//!   cargo run -p check_i18n --bin exportar_catalogo -- docs/catalogo.json

use serde_json::{json, Value};
use surshape_engine::process::{desc_key, name_key};
use surshape_engine::{Engine, Inputs, Outputs, ParamKind, ParamSpec, Scale};

fn param(s: &ParamSpec) -> Value {
    let (kind, decimals, options): (&str, u8, Vec<&str>) = match s.kind {
        ParamKind::Float { decimals } => ("float", decimals, vec![]),
        ParamKind::Int => ("int", 0, vec![]),
        ParamKind::Toggle => ("toggle", 0, vec![]),
        ParamKind::Choice(o) => ("choice", 0, o.to_vec()),
    };
    json!({
        "key": s.key,
        "desc_key": s.desc_key(),
        "min": s.min,
        "max": s.max,
        "default": s.default,
        "unit_key": s.unit.key(),
        "kind": kind,
        "decimals": decimals,
        "options": options,
        "log": s.scale == Scale::Log,
        "automatable": s.automatable,
    })
}

fn inputs(i: Inputs) -> Vec<&'static str> {
    match i {
        Inputs::Fixed(s) => s.iter().map(|x| x.key).collect(),
        Inputs::Variadic => vec!["ui.entrada.principal", "ui.entrada.segunda"],
    }
}

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| "catalogo.json".into());
    let mut procs = Vec::new();
    for p in surshape_native::registry().all() {
        procs.push(json!({
            "id": p.id(),
            "engine": Engine::Nativo.id(),
            "family_key": p.family().key(),
            "name_key": name_key(p.id()),
            "desc_key": desc_key(p.id()),
            "inputs": inputs(p.inputs()),
            "outputs": match p.outputs() { Outputs::Fixed(n) => json!(n), Outputs::Dynamic => json!("varias") },
            "seed": p.uses_seed(),
            "per_channel": p.per_channel(),
            "params": p.params().iter().map(param).collect::<Vec<_>>(),
        }));
    }
    for d in &surshape_cdp::procs::DEFS {
        procs.push(json!({
            "id": d.id,
            "engine": Engine::Cdp.id(),
            "program": d.program,
            "family_key": d.family.key(),
            "name_key": name_key(d.id),
            "desc_key": desc_key(d.id),
            "inputs": inputs(d.inputs),
            "outputs": 1,
            "seed": d.seed,
            "per_channel": true,
            "params": d.params.iter().map(param).collect::<Vec<_>>(),
        }));
    }
    let families: Vec<&str> = surshape_engine::Family::ALL.iter().map(|f| f.key()).collect();
    let doc = json!({ "version": env!("CARGO_PKG_VERSION"), "families": families, "processes": procs });
    std::fs::write(&out, serde_json::to_string_pretty(&doc).expect("json")).expect("escribir catálogo");
    println!("{} procesos -> {out}", procs.len());
}
