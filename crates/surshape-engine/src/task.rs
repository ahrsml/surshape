//! Trabajo en un hilo aparte con progreso y cancelación. Lo usan los renders
//! y, en la app, la carga y la escritura de archivos: la interfaz consulta
//! [`Task::poll`] en cada cuadro y nunca se bloquea.

use crate::process::ProcessError;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub struct Task<T> {
    progress: Arc<AtomicU32>,
    cancel: Arc<AtomicBool>,
    rx: mpsc::Receiver<Result<T, ProcessError>>,
    started: Instant,
    done: bool,
}

impl<T: Send + 'static> Task<T> {
    /// Lanza `f(progreso, cancelación)` en un hilo con nombre `name`. Un
    /// pánico dentro de `f` se convierte en el error `err.render.panico`.
    pub fn spawn(
        name: &str,
        f: impl FnOnce(&AtomicU32, &AtomicBool) -> Result<T, ProcessError> + Send + 'static,
    ) -> Self {
        let progress = Arc::new(AtomicU32::new(0));
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        let (p, c) = (progress.clone(), cancel.clone());
        let spawned = std::thread::Builder::new().name(name.into()).spawn(move || {
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&p, &c)))
                .unwrap_or_else(|_| Err(ProcessError::new("err.render.panico").arg("proceso", "?")));
            let _ = tx.send(r);
        });
        if let Err(e) = spawned {
            // Sin hilo no hay trabajo: el error llega en el primer poll.
            let (tx, rx2) = mpsc::channel();
            let _ = tx.send(Err(ProcessError::new("err.render.hilo").arg("detalle", e)));
            return Self { progress, cancel, rx: rx2, started: Instant::now(), done: false };
        }
        Self { progress, cancel, rx, started: Instant::now(), done: false }
    }
}

impl<T> Task<T> {
    /// Progreso 0..1.
    pub fn progress(&self) -> f32 {
        self.progress.load(Ordering::Relaxed) as f32 / 1_000_000.0
    }

    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelling(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    pub fn is_done(&self) -> bool {
        self.done
    }

    /// Resultado si ya terminó (se entrega una sola vez).
    pub fn poll(&mut self) -> Option<Result<T, ProcessError>> {
        if self.done {
            return None;
        }
        match self.rx.try_recv() {
            Ok(r) => {
                self.done = true;
                Some(r)
            }
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.done = true;
                Some(Err(ProcessError::new("err.render.panico").arg("proceso", "?")))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panic_becomes_error() {
        let mut t: Task<u32> = Task::spawn("t", |_, _| panic!("prueba"));
        let r = loop {
            if let Some(r) = t.poll() {
                break r;
            }
            std::thread::yield_now();
        };
        assert!(matches!(r, Err(ProcessError::Failed { key: "err.render.panico", .. })));
        assert!(t.poll().is_none());
    }
}
