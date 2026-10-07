//! Incrusta el ícono en el ejecutable de Windows con `windres` (MinGW de
//! MSYS2). Si no está, el programa compila igual, sin ícono, y lo avisa.

fn main() {
    println!("cargo:rerun-if-changed=assets/surshape.rc");
    println!("cargo:rerun-if-changed=assets/surshape.ico");
    let target = std::env::var("TARGET").unwrap_or_default();
    if !target.contains("windows") {
        return;
    }
    let out = std::env::var("OUT_DIR").expect("OUT_DIR");
    let obj = std::path::Path::new(&out).join("surshape_icono.o");
    let msys = r"C:\msys64\mingw64\bin";
    let path = format!("{msys};{}", std::env::var("PATH").unwrap_or_default());
    let ok = ["windres", &format!(r"{msys}\windres.exe")].iter().any(|w| {
        std::process::Command::new(w)
            .args(["assets/surshape.rc", "-O", "coff", "-o"])
            .arg(&obj)
            .env("PATH", &path)
            .status()
            .is_ok_and(|s| s.success())
    });
    if ok {
        println!("cargo:rustc-link-arg-bins={}", obj.display());
    } else {
        println!("cargo:warning=windres no disponible: el ejecutable no tendrá ícono");
    }
}
