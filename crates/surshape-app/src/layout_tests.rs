//! Tests de layout (egui_kittest, sin GPU): la página principal, la de
//! parámetros, Graph-Edit, la consola y Preferencias a 1024x700, 1280x800 y
//! 1920x1080, en español e inglés. Fallan si algún widget del módulo
//! `win32` queda fuera de su contenedor.
//!
//! Todo en un solo test: el idioma es global.
#![cfg(test)]


use crate::app::{App, Dialog, Page};
use crate::prefs::Prefs;
use crate::win32::audit;
use eframe::egui;
use egui_kittest::Harness;
use surshape_i18n::Lang;

fn harness(size: egui::Vec2, session: std::path::PathBuf) -> Harness<'static, App> {
    // Los tests nunca escriben las preferencias del usuario.
    crate::prefs::disable_saving();
    let ctx = egui::Context::default();
    let mut prefs = Prefs::default();
    prefs.secciones = [true, true, true, true];
    let app = App::with_ctx(&ctx, prefs, None, Vec::new(), Some(session));
    let mut first = true;
    Harness::builder().with_size(size).build_state(
        move |ctx, app: &mut App| {
            // Las fuentes se cargan en el primer cuadro y valen desde el siguiente.
            if first {
                crate::theme::apply(ctx);
                first = false;
                return;
            }
            app.frame(ctx);
        },
        app,
    )
}

#[test]
fn nothing_leaves_its_container() {
    let dir = std::env::temp_dir().join(format!("surshape_layout_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    crate::capture::example_session(&dir, None).expect("sesión de ejemplo");
    let mut fails = Vec::new();
    for lang in [Lang::Es, Lang::En] {
        surshape_i18n::set_lang(lang);
        for (w, h) in [(1024.0, 700.0), (1280.0, 800.0), (1920.0, 1080.0)] {
            let mut hn = harness(egui::vec2(w, h), dir.clone());
            hn.run_steps(3);
            let cell = hn.state().session.as_ref().and_then(|s| s.patch.at(0, 2));
            // Un sub-patch con los pasos de la fila A, en una rama.
            let sub = {
                let st = hn.state_mut();
                let s = st.session.as_mut().unwrap();
                let ids: Vec<_> = s.patch.filas[0].celdas[1..].to_vec();
                let pasos = s.patch.copy_cells(&ids);
                let br = s.patch.add_branch(surshape_patch::PortRef::main(s.patch.filas[0].celdas[0])).unwrap();
                s.patch.append_subpatch(br, "Nube", "nube.json".into(), &pasos)
            };
            let pages: Vec<(&str, Box<dyn Fn(&mut App)>)> = vec![
                ("principal", Box::new(|_a: &mut App| {})),
                ("principal_celda", Box::new(move |a: &mut App| {
                    if let Some(c) = cell {
                        a.select(c);
                    }
                })),
                ("parametros", Box::new(move |a: &mut App| {
                    if let Some(c) = cell {
                        a.select(c);
                        a.page = Page::Params(c);
                    }
                })),
                ("subpatch", Box::new(move |a: &mut App| {
                    if let Some(c) = sub {
                        a.select(c);
                        a.page = Page::Params(c);
                    }
                })),
                ("consola", Box::new(|a: &mut App| a.page = Page::Console)),
                ("preferencias", Box::new(|a: &mut App| {
                    a.page = Page::Main;
                    a.prefs_draft = Some(a.prefs.clone());
                    a.modal = Some(Dialog::Prefs);
                })),
                ("menu", Box::new(|a: &mut App| {
                    a.modal = None;
                    a.open_menu = Some(11);
                })),
            ];
            for (name, set) in &pages {
                set(hn.state_mut());
                hn.run_steps(2);
                audit::start();
                hn.run_steps(1);
                let log = audit::take();
                assert!(!log.is_empty(), "{name}: la auditoría no registró nada");
                for e in log.iter().filter(|e| e.overflows()) {
                    fails.push(format!("{lang:?} {w}x{h} {name}: «{}» {:?} fuera de {:?}", e.what, e.rect, e.bound));
                }
            }
        }
    }
    surshape_i18n::set_lang(Lang::Es);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(fails.is_empty(), "{} widgets fuera de su contenedor:\n{}", fails.len(), fails.join("\n"));
}

/// La auditoría detecta de verdad un control que no cabe.
#[test]
fn audit_catches_overflow() {
    let mut first = true;
    let mut hn = Harness::builder().with_size(egui::vec2(300.0, 100.0)).build(move |ctx| {
        if first {
            crate::theme::apply(ctx);
            first = false;
            return;
        }
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.allocate_ui(egui::vec2(100.0, 40.0), |ui| {
                ui.set_max_width(100.0);
                crate::win32::button_w(ui, "x", 250.0, true);
            });
        });
    });
    hn.run_steps(1);
    audit::start();
    hn.run_steps(1);
    assert!(audit::take().iter().any(|e| e.overflows()), "debería detectar el botón de 250 en 100");
}

/// Flujo con teclado y acciones: Alt+letra abre el menú, agregar un proceso
/// abre su página, Esc vuelve, Ctrl+Z deshace, Reemplazar y Borrar.
#[test]
fn keyboard_and_cell_operations() {
    let dir = std::env::temp_dir().join(format!("surshape_flujo_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    crate::capture::example_session(&dir, None).expect("sesión de ejemplo");
    surshape_i18n::set_lang(Lang::Es);
    let mut hn = harness(egui::vec2(1280.0, 800.0), dir.clone());
    hn.run_steps(3);
    // Alt+H abre Patch (Patc&h, igual en los dos idiomas).
    hn.press_key_modifiers(egui::Modifiers::ALT, egui::Key::H);
    hn.run_steps(2);
    assert_eq!(hn.state().open_menu, Some(11), "Alt+H abre Patch");
    hn.press_key(egui::Key::Escape);
    hn.run_steps(2);
    assert_eq!(hn.state().open_menu, None);

    let first = hn.state().session.as_ref().unwrap().patch.at(0, 1).unwrap();
    let n0 = hn.state().session.as_ref().unwrap().patch.nodos.len();
    hn.state_mut().select(first);
    let ctx = hn.ctx.clone();
    // Agregar un proceso desde la celda 1 (no es la última): va a una rama.
    hn.state_mut().do_action(&ctx, crate::shell::Action::AddProcess("nat.reverse"));
    hn.run_steps(2);
    let s = &hn.state().session.as_ref().unwrap().patch;
    assert_eq!(s.nodos.len(), n0 + 1);
    let new = hn.state().selected.unwrap();
    assert!(matches!(hn.state().page, Page::Params(id) if id == new), "abre la página de parámetros");
    let (row, col) = s.grid_pos(new).unwrap();
    assert_eq!((row, col), (1, 1), "rama nueva: fila B, después de la referencia");
    assert!(s.filas[1].origen.is_some());
    // Esc vuelve a la principal.
    hn.press_key(egui::Key::Escape);
    hn.run_steps(2);
    assert_eq!(hn.state().page, Page::Main);
    // Reemplazar: el próximo proceso elegido toma su lugar.
    hn.state_mut().do_action(&ctx, crate::shell::Action::Replace);
    hn.state_mut().do_action(&ctx, crate::shell::Action::AddProcess("nat.bitcrush"));
    let s = &hn.state().session.as_ref().unwrap().patch;
    assert_eq!(s.node(new).and_then(|n| n.process_id()), Some("nat.bitcrush"));
    // Ctrl+Z deshace el reemplazo y luego el agregado.
    hn.state_mut().page = Page::Main;
    hn.press_key_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
    hn.run_steps(2);
    assert_eq!(hn.state().session.as_ref().unwrap().patch.node(new).and_then(|n| n.process_id()), Some("nat.reverse"));
    hn.press_key_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
    hn.run_steps(2);
    assert_eq!(hn.state().session.as_ref().unwrap().patch.nodos.len(), n0);
    // Borrar una celda con dependientes pide confirmación.
    hn.state_mut().select(first);
    hn.state_mut().do_action(&ctx, crate::shell::Action::Delete);
    assert!(matches!(hn.state().modal, Some(Dialog::Confirm(_))));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Marcadores: agregar en el cursor y como región, ir al siguiente.
#[test]
fn markers_flow() {
    let dir = std::env::temp_dir().join(format!("surshape_marc_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    crate::capture::example_session(&dir, None).expect("sesión de ejemplo");
    let mut hn = harness(egui::vec2(1280.0, 800.0), dir.clone());
    hn.run_steps(3);
    let src = hn.state().session.as_ref().unwrap().patch.at(0, 0).unwrap();
    let cell = hn.state().session.as_ref().unwrap().patch.at(0, 2).unwrap();
    hn.state_mut().select(src);
    // Esperar a que el audio esté en memoria.
    for _ in 0..200 {
        hn.run_steps(1);
        if hn.state().view_loaded().is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let sr = hn.state().view_loaded().expect("audio cargado").audio.sr as usize;
    let before = hn.state().current_markers().len();
    hn.state_mut().view.cursor = sr / 2; // 0,5 s
    hn.state_mut().marker_add(false);
    hn.state_mut().view.sel = Some((2 * sr, 3 * sr));
    hn.state_mut().marker_add(true);
    // Desde otra celda de la fila se ven los mismos marcadores.
    hn.state_mut().select(cell);
    let ms = hn.state().current_markers();
    assert_eq!(ms.len(), before + 2);
    assert!(ms.iter().any(|m| m.t == 2.0 && m.fin == Some(3.0)));
    hn.state_mut().select(src);
    hn.state_mut().view.cursor = (1.5 * sr as f64) as usize;
    hn.state_mut().marker_step(true);
    assert_eq!(hn.state().view.selection(), Some((2 * sr, 3 * sr)), "la región queda elegida");
    let _ = std::fs::remove_dir_all(&dir);
}
