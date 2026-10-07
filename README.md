# SURSHAPE

Diseño sonoro **offline** para ambient y noise, en el espíritu de Soundshaper /
Composers Desktop Project: sample → proceso → archivo nuevo. Nada se
sobrescribe; cada render guarda su linaje (origen, proceso, parámetros, seed).

Por Adolfo Rosas (@ahrsml). Licencia GPL-3.0-or-later.

## Compilar y abrir (Windows)

Usa el Rust local (sin tocar PATH ni registro). Si no hay `.\.toolchain`, se
usa el de NOISEGEK (`..\NOISEGEK\vst\.toolchain`).

SURSHAPE usa el DSP de NOISEGEK (`noisegek-dsp`) desde `..\NOISEGEK\vst\`.
En un clon nuevo, clona [NOISEGEK](https://github.com/ahrsml/noisegek) en esa
carpeta, junto a SURSHAPE:

```powershell
git clone https://github.com/ahrsml/noisegek.git ..\NOISEGEK\vst
```

Los programas de CDP se compilan con `tools\build_cdp.ps1` (no van en el
repositorio).

```powershell
powershell -ExecutionPolicy Bypass -File .\ejecutar.ps1        # compila y abre
powershell -ExecutionPolicy Bypass -File .\build.ps1 -Test     # tests + dist\SURSHAPE.exe
powershell -ExecutionPolicy Bypass -File .\limpiar.ps1         # borra target\
```

Tests a mano (con `. .\env.ps1` activo):

```powershell
cargo test --workspace          # SURSHAPE (incluye i18n y contraste de color)
cargo test -p noisegek-dsp      # el DSP compartido con NOISEGEK
python tools\licencias.py       # auditoría de licencias de dependencias
```

## Uso rápido

1. Arrastra WAV, AIFF o FLAC a la ventana (o *Importar…*): cada archivo
   empieza una fila en la grilla **PATCH**.
2. **+ Proceso** al final de una fila agrega un proceso que toma como
   entrada la celda anterior. Clic en una celda abre su página en **NODO**.
3. Ajusta parámetros y pulsa **Render**. Si cambias algo después, la celda
   (y lo que sigue) queda *desactualizada*; **Render de todo lo
   desactualizado** lo pone al día en orden. Lo que no cambió sale de la
   cache sin recalcular.
4. En el **VISOR**: *Resultado* o *Entrada* de la celda; arrastra para
   seleccionar (en *Entrada*, "Usar la selección" fija la región que se
   procesa), rueda para zoom, barra espaciadora para escuchar.
5. *Exportar…* guarda lo que muestra el visor como WAV 24 bit o 32 float.

Las sesiones viven en `Documentos\SURSHAPE\Sesiones\` (configurable). Cada
una es una carpeta con `sesion.json` (el patch) y `cache\` (los renders).

## Manual

`docs\manual\es.md` y `en.md` (texto editable) + catálogo exportado del código
→ `python docs\generar_docs.py` → `dist\manual\manual_es.html` / `manual_en.html`.
`build.ps1` lo hace solo. En la app: **Ayuda → Manual**; atajos con **F1**.

## Traducciones

`crates\surshape-i18n\locales\es.lang` y `en.lang` (`clave = texto`). Para
probar cambios sin recompilar, copia los archivos a una carpeta `locales\`
junto a `SURSHAPE.exe`. `cargo test -p check_i18n` verifica que no falten
claves, que no sobren, que ambos idiomas usen los mismos argumentos `{…}` y que
no haya textos visibles fuera de esos archivos. `docsevision_traducciones.html`
muestra todas las claves lado a lado, con los términos a decidir resaltados.

Ver `CLAUDE.md` para las decisiones del proyecto y `THIRD_PARTY.md` para
licencias de terceros.
