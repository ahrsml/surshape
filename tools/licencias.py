# SURSHAPE · por @ahrsml
# Revisa las licencias de TODAS las dependencias de terceros (directas y
# transitivas) y copia sus textos de licencia a licenses/crates/.
#
#   python tools/licencias.py            (con el Rust local activo: . .\env.ps1)
#
# - Lista permitida (CLAUDE.md): MIT, BSD, ISC, zlib, Apache-2.0, MPL-2.0, LGPL.
# - Expresiones SPDX: "A OR B" pasa si alguna alternativa pasa; "A AND B"
#   exige todas. Lo que no pasa se informa y el script termina con error:
#   hay que preguntarle al usuario antes de aceptarlo.
# - Genera licenses/DEPENDENCIAS.md (tabla completa) y copia los archivos
#   LICENSE*/COPYING*/NOTICE* de cada crate a licenses/crates/<crate>-<versión>/.
import json
import os
import re
import shutil
import subprocess
import sys

RAIZ = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
PERMITIDAS = re.compile(r"^(MIT|MIT-0|Apache-2\.0|Apache-2\.0 WITH LLVM-exception|BSD-[0-9]-Clause|0BSD|ISC|Zlib|MPL-2\.0|LGPL-[0-9.]+(-or-later|-only)?)$")
# Excepciones ya aprobadas por el usuario (licencia exacta -> motivo).
APROBADAS = {
    # 2026-10-05, aprobadas por el usuario ("haz lo que sea mejor"): permisivas,
    # compatibles con GPL-3, obligatorias con eframe en Windows.
    "BSL-1.0": "clipboard-win / error-code (portapapeles de egui-winit)",
    "Unicode-3.0": "ICU4X vía webbrowser/url (enlaces) y unicode-ident (macros)",
}


def evalua(expr):
    """True si la expresión SPDX es aceptable."""
    expr = expr.replace("/", " OR ").strip()
    # Paréntesis: se resuelven de adentro hacia afuera.
    while "(" in expr:
        expr = re.sub(r"\(([^()]*)\)", lambda m: "MIT" if evalua(m.group(1)) else "NO_PERMITIDA", expr)
    for alternativa in re.split(r"\s+OR\s+", expr):
        partes = [p.strip() for p in re.split(r"\s+AND\s+", alternativa)]
        if all(PERMITIDAS.match(p) or p in APROBADAS for p in partes):
            return True
    return False


def main():
    meta = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--format-version", "1", "--locked"], cwd=RAIZ).decode("utf-8"))
    # Solo paquetes que realmente entran en la compilación para Windows
    # (dependencias normales y de compilación, según `cargo tree`).
    arbol = subprocess.check_output(
        ["cargo", "tree", "--workspace", "--target", "x86_64-pc-windows-gnu", "-e", "normal,build",
         "--prefix", "none", "--format", "{p}"], cwd=RAIZ).decode("utf-8")
    usados = set()
    for linea in arbol.splitlines():
        partes = linea.split()
        if len(partes) >= 2 and partes[1].startswith("v"):
            usados.add((partes[0], partes[1][1:]))
    usados_ids = {p["id"] for p in meta["packages"] if (p["name"], p["version"]) in usados}

    terceros, problemas = [], []
    for p in meta["packages"]:
        if p["id"] not in usados_ids or p["source"] is None:
            continue  # crates propios (ruta local): SURSHAPE y noisegek-dsp
        lic = p.get("license") or ("ARCHIVO: " + (p.get("license_file") or "?"))
        terceros.append((p["name"], p["version"], lic, p.get("repository") or ""))
        if not evalua(lic):
            problemas.append((p["name"], p["version"], lic))
        # Copia de textos de licencia
        src_dir = os.path.dirname(p["manifest_path"])
        dst = os.path.join(RAIZ, "licenses", "crates", "%s-%s" % (p["name"], p["version"]))
        archivos = [f for f in os.listdir(src_dir) if re.match(r"(?i)^(licen[cs]e|copying|notice|unlicense)", f)]
        if archivos:
            os.makedirs(dst, exist_ok=True)
            for f in archivos:
                origen = os.path.join(src_dir, f)
                if os.path.isfile(origen):
                    shutil.copy(origen, os.path.join(dst, f))
                elif os.path.isdir(origen):
                    shutil.copytree(origen, os.path.join(dst, f), dirs_exist_ok=True) if sys.version_info >= (3, 8) else None

    terceros.sort()
    with open(os.path.join(RAIZ, "licenses", "DEPENDENCIAS.md"), "w", encoding="utf-8", newline="\n") as f:
        f.write("# Dependencias de terceros (generado por tools/licencias.py)\n\n")
        f.write("No editar a mano: volver a correr el script tras cambiar dependencias.\n\n")
        f.write("| Crate | Versión | Licencia | Repositorio |\n|---|---|---|---|\n")
        for n, v, l, r in terceros:
            f.write("| %s | %s | %s | %s |\n" % (n, v, l, r))
    print("%d crates de terceros revisados." % len(terceros))
    if problemas:
        print("\nLICENCIAS FUERA DE LA LISTA PERMITIDA (preguntar al usuario):")
        for n, v, l in problemas:
            print("  %s %s: %s" % (n, v, l))
        sys.exit(1)
    print("Todas las licencias están en la lista permitida.")


if __name__ == "__main__":
    main()
