//! Catálogo de procesos disponibles.

use crate::process::{Family, Process};
use std::sync::Arc;

#[derive(Default, Clone)]
pub struct Registry {
    procs: Vec<Arc<dyn Process>>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Agrega un proceso. Un id repetido es un error de programación.
    pub fn add(&mut self, p: Arc<dyn Process>) {
        assert!(self.get(p.id()).is_none(), "id de proceso repetido: {}", p.id());
        self.procs.push(p);
    }

    pub fn all(&self) -> &[Arc<dyn Process>] {
        &self.procs
    }

    pub fn get(&self, id: &str) -> Option<&Arc<dyn Process>> {
        self.procs.iter().find(|p| p.id() == id)
    }

    /// Procesos de una familia, en orden de registro.
    pub fn family(&self, f: Family) -> impl Iterator<Item = &Arc<dyn Process>> {
        self.procs.iter().filter(move |p| p.family() == f)
    }
}
