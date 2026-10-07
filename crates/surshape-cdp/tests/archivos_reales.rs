//! Prueba cada proceso CDP con archivos reales del usuario (no se corre por
//! defecto). Uso:
//!   set SURSHAPE_ARCHIVOS=C:\ruta\a.wav;C:\ruta\b.wav
//!   cargo test -p surshape-cdp --release --test archivos_reales -- --ignored --nocapture
//! Para cada archivo prueba: el archivo tal cual, su estiramiento
//! (Paulstretch x4 de los primeros 20 s) y ruido del generador. Informa
//! cada fallo con el mensaje de CDP. Opcional: SURSHAPE_PROCS=cdp.a,cdp.b
//! limita los procesos y SURSHAPE_SIN_ESTIRAR=1 omite los estiramientos.

use std::sync::atomic::{AtomicBool, AtomicU32};
use std::sync::Arc;
use surshape_audio::{decode, AudioBuf};
use surshape_cdp::CdpInstall;
use surshape_engine::render::run;
use surshape_engine::{ParamSet, ParamValues, Process, RenderJob, RenderOptions};

fn render(p: &Arc<dyn Process>, inputs: Vec<Arc<AudioBuf>>) -> Result<AudioBuf, String> {
    let job = RenderJob {
        process: p.clone(),
        inputs,
        params: ParamSet { comun: ParamValues::defaults(p.params()), por_canal: None },
        seed: 7,
        options: RenderOptions::default(),
    };
    run(&job, &AtomicU32::new(0), &AtomicBool::new(false)).map(|mut o| o.outputs.remove(0)).map_err(|e| e.to_string())
}

#[test]
#[ignore]
fn every_cdp_process_on_real_files() {
    let custom = std::env::var_os("SURSHAPE_CDP_DIR").map(std::path::PathBuf::from);
    let inst = CdpInstall::find(custom.as_deref(), None).expect("CDP");
    eprintln!("CDP: {} {:?}", inst.dir.display(), inst.version);
    let mut reg = surshape_engine::Registry::new();
    surshape_cdp::procs::register(&mut reg, &Arc::new(inst));
    let nat = surshape_native::registry();
    let files = std::env::var("SURSHAPE_ARCHIVOS").expect("SURSHAPE_ARCHIVOS");
    let mut inputs: Vec<(String, Arc<AudioBuf>)> = Vec::new();
    for f in files.split(';').filter(|s| !s.is_empty()) {
        let a = Arc::new(decode::load(std::path::Path::new(f)).expect("leer"));
        let name = std::path::Path::new(f).file_name().unwrap().to_string_lossy().into_owned();
        let first = Arc::new(a.slice(0, (a.sr as usize) * 20));
        let st = render(nat.get("nat.paulstretch").unwrap(), vec![first]).expect("paulstretch");
        inputs.push((name.clone(), a));
        if std::env::var_os("SURSHAPE_SIN_ESTIRAR").is_none() {
            inputs.push((format!("{name} > estiramiento x4"), Arc::new(st)));
        }
    }
    let noise = render(nat.get("nat.noise").unwrap(), vec![]).expect("ruido");
    inputs.push(("ruido generado".into(), Arc::new(noise)));
    let only = std::env::var("SURSHAPE_PROCS").unwrap_or_default();
    let mut fails = 0;
    for p in reg.all().iter().filter(|p| only.is_empty() || only.split(',').any(|o| o == p.id())) {
        for (name, a) in &inputs {
            let ins: Vec<Arc<AudioBuf>> = (0..p.inputs().min()).map(|i| if i == 0 { a.clone() } else { inputs[0].1.clone() }).collect();
            let t = std::time::Instant::now();
            if p.works_on_files() {
                eprintln!("(archivos) {:<20} se prueba en la cadena del patch", p.id());
                continue;
            }
            match render(p, ins) {
                Ok(o) => eprintln!("OK    {:<20} {:<40} {:.1} s -> {:.1} s", p.id(), name, t.elapsed().as_secs_f64(), o.duration_secs()),
                Err(e) => {
                    fails += 1;
                    eprintln!("FALLA {:<20} {:<40} {e}", p.id(), name);
                }
            }
        }
    }
    eprintln!("fallas: {fails}");
}
