//! El patch de ejemplo de la fase 6, de punta a punta con CDP:
//! sonido (.wav) -> PVOC análisis (.ana) -> desenfoque espectral (.ana) ->
//! PVOC resíntesis (.wav) -> proceso nativo (.wav). Y la auto-conversión:
//! un proceso espectral directamente sobre la fuente analiza solo.
//! Si CDP no está instalado, avisa y pasa.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32};
use std::sync::Arc;
use surshape_audio::export::{write_wav, WavFormat};
use surshape_audio::{decode, AudioBuf};
use surshape_engine::{console, FileKind, Registry, RenderOptions};
use surshape_patch::SourceInfo;
use surshape_session::runner::{build_steps, run_steps, RunResult, RunStatus};
use surshape_session::Session;

fn registry() -> Option<Registry> {
    let custom = std::env::var_os("SURSHAPE_CDP_DIR").map(std::path::PathBuf::from);
    let inst = surshape_cdp::CdpInstall::find(custom.as_deref(), None).or_else(|| {
        let local = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../third_party/cdp-bin");
        surshape_cdp::CdpInstall::discover(&local)
    });
    let Some(inst) = inst else {
        eprintln!("CDP no está instalado: se omite el patch de ejemplo");
        return None;
    };
    let mut reg = surshape_native::registry();
    surshape_cdp::procs::register(&mut reg, &Arc::new(inst));
    Some(reg)
}

fn run_all(s: &mut Session, reg: &Registry) -> RunResult {
    let opts = RenderOptions::default();
    let keys = s.patch.keys(reg, &opts);
    let plan = s.patch.render_plan(&[], &keys);
    let steps = build_steps(s, &plan, &keys, reg);
    let r = run_steps(&s.dir, &steps, &opts, HashMap::new(), &RunStatus::default(), &AtomicU32::new(0), &AtomicBool::new(false));
    for st in &r.steps {
        if let Ok(rec) = &st.result {
            s.patch.node_mut(st.node).unwrap().render = Some(rec.clone());
        }
    }
    r
}

#[test]
fn example_patch_wav_ana_spectral_resynth_native() {
    let Some(reg) = registry() else { return };
    let dir = std::env::temp_dir().join(format!("surshape_ejemplo_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut s = Session::create(&dir).unwrap();
    let src = dir.join("fuente.wav");
    let sr = 44100;
    let x: Vec<f32> = (0..sr * 2).map(|i| ((i as f32 * 0.031).sin() * 0.4 + (i as f32 * 0.0071).sin() * 0.3) * 0.8).collect();
    write_wav(&src, &AudioBuf::from_channels(sr as u32, vec![x.clone(), x]), WavFormat::Float32).unwrap();
    let (_, row) = s.patch.add_source(SourceInfo {
        ruta: src.clone(),
        hash: surshape_session::hash_file(&src, None, None).unwrap(),
        sr: sr as u32,
        canales: 2,
        frames: sr as u64 * 2,
        bytes: 0,
        modificado: 0,
        marcadores: vec![],
    });
    let p = |id: &str| reg.get(id).unwrap().clone();
    let anal = s.patch.append(row, p("cdp.pvoc_anal").as_ref(), 1).unwrap();
    let blur = s.patch.append(row, p("cdp.blur_blur").as_ref(), 1).unwrap();
    let synth = s.patch.append(row, p("cdp.pvoc_synth").as_ref(), 1).unwrap();
    let rev = s.patch.append(row, p("nat.reverse").as_ref(), 1).unwrap();
    // Rama: desenfoque directo sobre la fuente (auto-conversión a .ana).
    let (_, row2) = {
        let br = s.patch.add_branch(surshape_patch::PortRef::main(s.patch.filas[row].celdas[0])).unwrap();
        (0, br)
    };
    let auto = s.patch.append(row2, p("cdp.focus_exag").as_ref(), 1).unwrap();

    let r = run_all(&mut s, &reg);
    for st in &r.steps {
        assert!(st.result.is_ok(), "{}: {:?}", s.patch.cell_label(st.node).unwrap(), st.result);
    }
    let rec = |id| s.patch.node(id).unwrap().render.clone().unwrap();
    // .ana visibles: un archivo por canal, y su resíntesis para escuchar.
    for id in [anal, blur, auto] {
        let o = &rec(id).salidas[0];
        assert_eq!(o.kind(), FileKind::Ana, "{}", s.patch.cell_label(id).unwrap());
        assert_eq!(o.datos.len(), 2);
        assert!(o.datos.iter().all(|d| dir.join(d).is_file()));
        assert!(o.frames > 0 && dir.join(&o.archivo).is_file());
    }
    let out = decode::load(&dir.join(&rec(rev).salidas[0].archivo)).unwrap();
    assert_eq!(out.num_channels(), 2);
    assert!((out.duration_secs() - 2.0).abs() < 0.1, "{}", out.duration_secs());
    assert!(out.channels[0].iter().any(|v| v.abs() > 0.01), "la cadena da sonido");
    assert_eq!(rec(synth).salidas[0].kind(), FileKind::Wav);

    // La consola registró los programas de CDP con su celda.
    let log = console::entries();
    assert!(log.iter().any(|e| e.program.starts_with("pvoc anal") && e.cell == "A_1"), "{log:#?}");
    assert!(log.iter().any(|e| e.program.starts_with("blur") && e.cell == "A_2"));
    assert!(log.iter().any(|e| e.program == "nat.reverse" && e.cell == "A_4"));
    // La auto-conversión de la rama quedó con su celda (B_1).
    assert!(log.iter().any(|e| e.program.starts_with("pvoc anal") && e.cell == "B_1"));

    // Todo sale de la cache la segunda vez (también la auto-conversión).
    for n in &mut s.patch.nodos {
        n.render = None;
    }
    let r2 = run_all(&mut s, &reg);
    assert!(r2.steps.iter().all(|st| st.from_cache), "todo debería salir de la cache");
    let _ = std::fs::remove_dir_all(&dir);
}
