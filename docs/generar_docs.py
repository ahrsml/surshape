# SURSHAPE · por @ahrsml
# Genera el manual (HTML, español e inglés) y el documento de revisión de
# traducciones. Compatible con Python 3.6, sin dependencias.
#
#   python docs/generar_docs.py [carpeta_salida]
#
# Entradas:
#   docs/manual/es.md, docs/manual/en.md   texto del manual (markdown simple)
#   docs/catalogo.json                     procesos (cargo run -p check_i18n --bin exportar_catalogo)
#   crates/surshape-i18n/locales/*.lang    nombres, descripciones, atajos
# Salidas:
#   <salida>/manual_es.html, manual_en.html, fonts/   (por defecto dist/manual)
#   docs/revision_traducciones.html                   (todas las claves, lado a lado)
import html
import json
import os
import re
import shutil
import sys

RAIZ = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
LOCALES = os.path.join(RAIZ, "crates", "surshape-i18n", "locales")
FUENTES = os.path.join(RAIZ, "crates", "surshape-app", "assets", "fonts")

# Textos propios del manual (no de la interfaz).
TXT = {
    "es": {"indice": "Contenido", "col_param": "Parámetro", "col_rango": "Rango", "col_defecto": "Por defecto",
           "col_desc": "Descripción", "curva": "admite curva", "si": "sí", "no": "no", "opciones": "Opciones",
           "entradas": "Entradas", "salidas": "Salidas", "varias": "varias (una fila por salida extra)",
           "generador": "Generador: no tiene entrada; empieza una fila propia.", "seed": "Usa seed (reproducible).",
           "programa": "Programa CDP", "sin_param": "Sin parámetros.", "teclas": "Teclas", "accion": "Acción",
           "version": "Versión", "rev_titulo": "Revisión de traducciones", "rev_clave": "Clave",
           "rev_termino": "término a decidir", "rev_intro": "Todas las claves de la interfaz, en el orden de los archivos. "
           "Las filas marcadas son términos técnicos pendientes de decisión ([TÉRMINO] en los archivos .lang)."},
    "en": {"indice": "Contents", "col_param": "Parameter", "col_rango": "Range", "col_defecto": "Default",
           "col_desc": "Description", "curva": "supports curve", "si": "yes", "no": "no", "opciones": "Options",
           "entradas": "Inputs", "salidas": "Outputs", "varias": "several (one row per extra output)",
           "generador": "Generator: it has no input; it starts its own row.", "seed": "Uses a seed (reproducible).",
           "programa": "CDP program", "sin_param": "No parameters.", "teclas": "Keys", "accion": "Action",
           "version": "Version", "rev_titulo": "Translation review", "rev_clave": "Key",
           "rev_termino": "term to decide", "rev_intro": "Every interface key, in file order. "
           "Highlighted rows are technical terms pending a decision ([TÉRMINO] in the .lang files)."},
}


def leer_lang(code):
    """[(seccion, clave, valor, termino)] en orden, y dict clave->valor."""
    out, d = [], {}
    seccion, termino = "", None
    with open(os.path.join(LOCALES, code + ".lang"), encoding="utf-8") as f:
        for raw in f:
            l = raw.strip()
            if not l:
                continue
            if l.startswith("#"):
                m = re.match(r"#\s*──\s*(.*?)\s*─*$", l)
                if m:
                    seccion = m.group(1)
                elif "[TÉRMINO]" in l:
                    termino = l.split("]", 1)[1].strip()
                continue
            if "=" not in l:
                continue
            k, v = l.split("=", 1)
            k, v = k.strip(), v.strip().replace("\\n", "\n")
            out.append((seccion, k, v, termino))
            d[k] = v
            termino = None
    return out, d


def inline(s):
    s = html.escape(s, quote=False)
    s = re.sub(r"`([^`]+)`", r"<code>\1</code>", s)
    s = re.sub(r"\*\*([^*]+)\*\*", r"<strong>\1</strong>", s)
    s = re.sub(r"(?<!\*)\*([^*]+)\*(?!\*)", r"<em>\1</em>", s)
    return s


def slug(s):
    s = re.sub(r"[^\w\s-]", "", s.lower(), flags=re.UNICODE)
    return re.sub(r"\s+", "-", s.strip())


def md_a_html(md, especiales):
    """Markdown simple -> (html, [(nivel, id, titulo)])."""
    out, toc, lista, parrafo = [], [], False, []

    def cerrar():
        nonlocal lista, parrafo
        if parrafo:
            out.append("<p>" + inline(" ".join(parrafo)) + "</p>")
            parrafo = []
        if lista:
            out.append("</ul>")
            lista = False

    for line in md.splitlines():
        l = line.rstrip()
        if l.strip() in especiales:
            cerrar()
            out.append(especiales[l.strip()])
            continue
        m = re.match(r"^(#{1,3})\s+(.*)$", l)
        if m:
            cerrar()
            nivel, titulo = len(m.group(1)), m.group(2)
            ident = slug(titulo)
            if nivel > 1:
                toc.append((nivel, ident, titulo))
            out.append('<h%d id="%s">%s</h%d>' % (nivel, ident, inline(titulo), nivel))
        elif l.startswith("- "):
            if parrafo:
                out.append("<p>" + inline(" ".join(parrafo)) + "</p>")
                parrafo = []
            if not lista:
                out.append("<ul>")
                lista = True
            out.append("<li>" + inline(l[2:]) + "</li>")
        elif not l.strip():
            cerrar()
        else:
            if lista:
                out.append("</ul>")
                lista = False
            parrafo.append(l.strip())
    cerrar()
    return "\n".join(out), toc


def num(v, decimales):
    if decimales == 0:
        return "%d" % round(v)
    return ("%." + str(decimales) + "f") % v


def catalogo_html(cat, t, tx):
    partes = []
    por_familia = {}
    for p in cat["processes"]:
        por_familia.setdefault(p["family_key"], []).append(p)
    for fam in cat["families"]:
        procs = por_familia.get(fam)
        if not procs:
            continue
        partes.append('<h3 id="fam-%s">%s</h3>' % (slug(fam), html.escape(t.get(fam, fam))))
        for p in procs:
            motor = "CDP" if p["engine"] == "cdp" else "NAT"
            partes.append('<div class="proc"><h4>%s <span class="badge %s">%s</span></h4>' % (
                html.escape(t.get(p["name_key"], p["id"])), motor.lower(), motor))
            partes.append("<p>%s</p>" % html.escape(t.get(p["desc_key"], "")))
            datos = []
            if p.get("program"):
                datos.append("%s: <code>%s</code>" % (tx["programa"], html.escape(p["program"])))
            if not p["inputs"]:
                datos.append(tx["generador"])
            elif len(p["inputs"]) > 1:
                datos.append("%s: %s" % (tx["entradas"], ", ".join(html.escape(t.get(k, k)) for k in p["inputs"])))
            if p["outputs"] == "varias":
                datos.append("%s: %s" % (tx["salidas"], tx["varias"]))
            if p["seed"]:
                datos.append(tx["seed"])
            if datos:
                partes.append('<p class="datos">' + " · ".join(datos) + "</p>")
            if not p["params"]:
                partes.append("<p><em>%s</em></p></div>" % tx["sin_param"])
                continue
            filas = []
            for s in p["params"]:
                unidad = t.get(s["unit_key"], "") if s["unit_key"] else ""
                if s["kind"] == "choice":
                    ops = [t.get(o, o) for o in s["options"]]
                    rango = "%s: %s" % (tx["opciones"], ", ".join(ops))
                    defecto = ops[int(s["default"])] if ops else ""
                elif s["kind"] == "toggle":
                    rango = "%s / %s" % (tx["si"], tx["no"])
                    defecto = tx["si"] if s["default"] >= 0.5 else tx["no"]
                else:
                    rango = "%s – %s %s" % (num(s["min"], s["decimals"]), num(s["max"], s["decimals"]), unidad)
                    defecto = "%s %s" % (num(s["default"], s["decimals"]), unidad)
                desc = html.escape(t.get(s["desc_key"], ""))
                if s["automatable"]:
                    desc += ' <span class="curva">(%s)</span>' % tx["curva"]
                filas.append("<tr><td>%s</td><td class=\"n\">%s</td><td class=\"n\">%s</td><td>%s</td></tr>" % (
                    html.escape(t.get(s["key"], s["key"])), html.escape(rango.strip()), html.escape(defecto.strip()), desc))
            partes.append('<table><tr><th>%s</th><th>%s</th><th>%s</th><th>%s</th></tr>%s</table></div>' % (
                tx["col_param"], tx["col_rango"], tx["col_defecto"], tx["col_desc"], "".join(filas)))
    return "\n".join(partes)


def atajos_html(entradas, t, tx):
    claves = [k for (_, k, _, _) in entradas if k.startswith("ui.atajo.") and not k.endswith(".desc")]
    filas = "".join("<tr><td class=\"n\"><kbd>%s</kbd></td><td>%s</td></tr>" % (
        html.escape(t[k]), html.escape(t.get(k + ".desc", ""))) for k in claves)
    return "<table><tr><th>%s</th><th>%s</th></tr>%s</table>" % (tx["teclas"], tx["accion"], filas)


CSS = """
@font-face { font-family: 'Anton'; src: url('fonts/Anton-Regular.ttf'); }
@font-face { font-family: 'IBM Plex Sans'; src: url('fonts/IBMPlexSans-Regular.ttf'); }
@font-face { font-family: 'IBM Plex Sans'; font-weight: 600; src: url('fonts/IBMPlexSans-SemiBold.ttf'); }
@font-face { font-family: 'Courier Prime'; src: url('fonts/CourierPrime-Regular.ttf'); }
:root { --fondo:#DCF6F5; --panel:#CFF6FE; --panel2:#C1E9FC; --texto:#16323F; --suave:#2B5468;
        --acento:#04B5E9; --acento2:#2DC9C8; --borde:#69A3C1; --nat:#88E1DE; --cdp:#83D1F5; }
* { box-sizing: border-box; }
body { margin:0; background:var(--fondo); color:var(--texto); font:16px/1.55 'IBM Plex Sans','Segoe UI',sans-serif; }
nav { position:fixed; top:0; left:0; bottom:0; width:270px; overflow:auto; background:var(--panel2); padding:20px 16px; }
nav h2 { font-family:'Anton',sans-serif; font-weight:normal; letter-spacing:.5px; margin:0 0 10px; }
nav a { display:block; color:var(--texto); text-decoration:none; padding:3px 0; }
nav a.l3 { padding-left:14px; font-size:14px; color:var(--suave); }
nav a:hover { text-decoration:underline; }
main { margin-left:270px; max-width:980px; padding:24px 40px 80px; }
h1 { font-family:'Anton',sans-serif; font-weight:normal; font-size:42px; margin:0 0 8px; }
h2 { font-family:'Anton',sans-serif; font-weight:normal; font-size:28px; margin:42px 0 10px; border-bottom:2px solid var(--acento2); }
h3 { font-weight:600; font-size:20px; margin:28px 0 8px; }
h4 { font-weight:600; margin:0 0 4px; font-size:17px; }
code, kbd, td.n { font-family:'Courier Prime','Consolas',monospace; }
code, kbd { background:var(--panel); padding:0 4px; }
.proc { background:var(--panel); border-left:4px solid var(--acento); padding:12px 16px; margin:12px 0; }
.datos { color:var(--suave); font-size:15px; }
.badge { font:12px 'Courier Prime',monospace; padding:1px 6px; vertical-align:middle; }
.badge.nat { background:var(--nat); } .badge.cdp { background:var(--cdp); }
.curva { color:var(--suave); font-size:14px; }
table { border-collapse:collapse; width:100%; margin:8px 0; background:var(--fondo); }
th, td { text-align:left; padding:5px 8px; border-bottom:1px solid var(--borde); vertical-align:top; }
td.n { white-space:nowrap; }
th { background:var(--panel2); font-weight:600; }
tr.term td { background:#B4ECE9; }
.version { color:var(--suave); }
.captura img { width:100%; height:auto; border:1px solid #69A3C1; }
@media (max-width: 900px) { nav { position:static; width:auto; } main { margin-left:0; padding:16px; } }
"""


def pagina(titulo, toc, cuerpo, tx, version):
    enlaces = "".join('<a class="l%d" href="#%s">%s</a>' % (n, i, inline(t)) for (n, i, t) in toc)
    return ("<!doctype html><html><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">"
            "<title>%s</title><style>%s</style></head><body><nav><h2>%s</h2>%s</nav><main>"
            "<p class=\"version\">%s %s</p>%s</main></body></html>") % (
        html.escape(titulo), CSS, tx["indice"], enlaces, tx["version"], version, cuerpo)


def main():
    salida = os.path.abspath(sys.argv[1]) if len(sys.argv) > 1 else os.path.join(RAIZ, "dist", "manual")
    os.makedirs(os.path.join(salida, "fonts"), exist_ok=True)
    for f in os.listdir(FUENTES):
        if f.endswith(".ttf") or f.endswith(".txt"):
            shutil.copy(os.path.join(FUENTES, f), os.path.join(salida, "fonts", f))
    with open(os.path.join(RAIZ, "docs", "catalogo.json"), encoding="utf-8") as f:
        cat = json.load(f)
    lenguas = {}
    for code in ("es", "en"):
        entradas, t = leer_lang(code)
        lenguas[code] = (entradas, t)
        tx = TXT[code]
        with open(os.path.join(RAIZ, "docs", "manual", code + ".md"), encoding="utf-8") as f:
            md = f.read()
        especiales = {"{{ATAJOS}}": atajos_html(entradas, t, tx), "{{CATALOGO}}": catalogo_html(cat, t, tx)}
        # Capturas de la interfaz (SURSHAPE.exe --captura docs/capturas).
        capturas = os.path.join(RAIZ, "docs", "capturas")
        for nombre in ("principal", "parametros", "consola", "preferencias"):
            archivo = "%s_1280x800_%s.png" % (nombre, code)
            if os.path.isfile(os.path.join(capturas, archivo)):
                os.makedirs(os.path.join(salida, "capturas"), exist_ok=True)
                shutil.copy(os.path.join(capturas, archivo), os.path.join(salida, "capturas", archivo))
                especiales["{{CAPTURA:%s}}" % nombre] = '<p class="captura"><img src="capturas/%s" alt="%s"></p>' % (archivo, nombre)
            else:
                especiales["{{CAPTURA:%s}}" % nombre] = ""
        cuerpo, toc = md_a_html(md, especiales)
        # El catálogo aporta sus familias al índice.
        toc2 = []
        for item in toc:
            toc2.append(item)
            if item[1] in (slug("Catálogo de procesos"), slug("Process catalog"), slug("Process catalogue")):
                for fam in cat["families"]:
                    if any(p["family_key"] == fam for p in cat["processes"]):
                        toc2.append((3, "fam-" + slug(fam), t.get(fam, fam)))
        titulo = md.splitlines()[0].lstrip("# ").strip()
        with open(os.path.join(salida, "manual_%s.html" % code), "w", encoding="utf-8") as f:
            f.write(pagina(titulo, toc2, cuerpo, tx, cat["version"]))

    # Revisión de traducciones (para el traductor)
    es_e, _ = lenguas["es"]
    _, en_t = lenguas["en"]
    tx = TXT["es"]
    filas, seccion, n_term = [], None, 0
    for sec, k, v, term in es_e:
        if sec != seccion:
            filas.append('<tr><th colspan="4">%s</th></tr>' % html.escape(sec))
            seccion = sec
        clase = ' class="term"' if term else ""
        if term:
            n_term += 1
        marca = "<br><em>%s: %s</em>" % (tx["rev_termino"], html.escape(term)) if term else ""
        filas.append("<tr%s><td class=\"n\">%s%s</td><td>%s</td><td>%s</td></tr>" % (
            clase, html.escape(k), marca, html.escape(v).replace("\n", "<br>"), html.escape(en_t.get(k, "")).replace("\n", "<br>")))
    cuerpo = ("<h1>%s</h1><p>%s</p><p>%d claves · %d términos marcados</p>"
              "<table><tr><th>%s</th><th>Español</th><th>English</th></tr>%s</table>") % (
        tx["rev_titulo"], tx["rev_intro"], len(es_e), n_term, tx["rev_clave"], "".join(filas))
    with open(os.path.join(RAIZ, "docs", "revision_traducciones.html"), "w", encoding="utf-8") as f:
        f.write(pagina(tx["rev_titulo"], [], cuerpo, tx, cat["version"]).replace('href="fonts/', 'href="../crates/surshape-app/assets/fonts/')
                .replace("url('fonts/", "url('../crates/surshape-app/assets/fonts/"))
    print("Manual: %s (es, en) · revisión: %d claves, %d términos" % (salida, len(es_e), n_term))


if __name__ == "__main__":
    main()
