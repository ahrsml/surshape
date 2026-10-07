//! Componentes de terceros para "Acerca de / créditos", desde
//! `assets/creditos.tsv` (nombre, licencia, enlace al código, clave i18n de
//! la descripción). Los nombres propios y licencias no se traducen; la
//! descripción sí.

const SRC: &str = include_str!("../assets/creditos.tsv"); // i18n-ok

#[derive(Clone, Debug, PartialEq)]
pub struct Credit {
    pub name: &'static str,
    pub license: &'static str,
    /// Enlace al código fuente; None si todavía no hay uno público.
    pub url: Option<&'static str>,
    pub desc_key: &'static str,
}

pub fn all() -> Vec<Credit> {
    SRC.lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            let mut it = l.split('\t');
            let (name, license, url) = (it.next()?, it.next()?, it.next()?);
            let url = (url != "-").then_some(url); // i18n-ok
            Some(Credit { name, license, url, desc_key: it.next()?.trim() })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_line_parses_and_has_a_known_key() {
        let raw = super::SRC.lines().filter(|l| !l.trim().is_empty() && !l.starts_with('#')).count();
        let all = super::all();
        assert_eq!(all.len(), raw, "alguna línea de creditos.tsv no tiene 4 columnas");
        for c in all {
            assert!(c.url.is_none_or(|u| u.starts_with("https://")), "{}", c.name);
            assert!(surshape_i18n::has(c.desc_key), "falta la clave {}", c.desc_key);
        }
    }
}
