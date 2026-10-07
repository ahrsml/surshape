//! Recetas: cadenas de procesos guardadas (JSON), para aplicarlas a otra
//! fuente con un clic. `<base>/<nombre>.json`.

use std::path::Path;
use surshape_patch::Recipe;

/// Nombres de las recetas guardadas, ordenados.
pub fn list(base: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(base)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            (p.extension().and_then(|x| x.to_str()) == Some("json")).then(|| p.file_stem()?.to_str().map(String::from)).flatten()
        })
        .collect();
    v.sort_by_key(|s| s.to_lowercase());
    v
}

fn safe(name: &str) -> String {
    let s: String = name.trim().chars().map(|c| if c.is_alphanumeric() || " -_.".contains(c) { c } else { '_' }).take(60).collect();
    if s.trim().is_empty() {
        "receta".into()
    } else {
        s.trim().to_string()
    }
}

pub fn save(base: &Path, r: &Recipe) -> std::io::Result<String> {
    std::fs::create_dir_all(base)?;
    let name = safe(&r.nombre);
    let txt = serde_json::to_string_pretty(r).map_err(std::io::Error::other)?;
    std::fs::write(base.join(format!("{name}.json")), txt)?;
    Ok(name)
}

/// Archivo de una receta.
pub fn path_of(base: &Path, name: &str) -> std::path::PathBuf {
    base.join(format!("{}.json", safe(name)))
}

pub fn load(base: &Path, name: &str) -> std::io::Result<Recipe> {
    let txt = std::fs::read_to_string(base.join(format!("{}.json", safe(name))))?;
    serde_json::from_str(&txt).map_err(std::io::Error::other)
}

pub fn delete(base: &Path, name: &str) -> std::io::Result<()> {
    std::fs::remove_file(base.join(format!("{}.json", safe(name))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::tmp;
    use surshape_patch::{NodeKind, NodeTemplate};

    #[test]
    fn save_list_load_delete() {
        let base = tmp("recetas");
        let r = Recipe {
            version: 1,
            nombre: "Nube / larga".into(),
            pasos: vec![NodeTemplate {
                tipo: NodeKind::Proceso { proceso: "nat.reverse".into() },
                params: Default::default(),
                seed: 3,
                region: None,
                iteraciones: 2,
                otras_entradas: vec![],
            }],
        };
        let name = save(&base, &r).unwrap();
        assert_eq!(list(&base), vec![name.clone()]);
        assert_eq!(load(&base, &name).unwrap(), r);
        delete(&base, &name).unwrap();
        assert!(list(&base).is_empty());
        let _ = std::fs::remove_dir_all(&base);
    }
}
