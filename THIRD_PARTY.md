# SURSHAPE · componentes de terceros

SURSHAPE se distribuye bajo **GPL-3.0-or-later** (ver `LICENSE`). Incluye o
usa los componentes de abajo, cada uno con su propia licencia. Los textos
completos están en la carpeta `licenses/`:

- `licenses/crates/<crate>-<versión>/`: archivos de licencia de cada crate de
  Rust, copiados tal cual de su paquete.
- `licenses/Apache-2.0.txt`, `licenses/MPL-2.0.txt`, `licenses/LGPL-2.1.txt`,
  `licenses/GPL-3.0.txt`: textos generales (se aplican a los crates que no
  traen archivo propio; en los que ofrecen "MIT OR Apache-2.0" se usa
  Apache-2.0).
- `licenses/fuentes/`: licencias SIL OFL 1.1 de las tipografías.
- `licenses/DEPENDENCIAS.md`: lista completa y generada de todos los crates
  (directos y transitivos) que entran en la compilación para Windows, con su
  licencia y repositorio. Se regenera con `python tools/licencias.py`, que
  además falla si aparece una licencia fuera de la lista permitida.

## Componentes principales

| Componente | Licencia | Código fuente | Uso en SURSHAPE |
|---|---|---|---|
| noisegek-dsp (proyecto hermano NOISEGEK) | GPL-3.0-or-later | mismo autor; se distribuye con el código de SURSHAPE | Módulos de distorsión, ruidos, RNG |
| CDP — Composers Desktop Project | LGPL-2.1 | https://github.com/ComposersDesktop/CDP8 | Programas externos (fase 3); nunca se linkean |
| egui / eframe | MIT OR Apache-2.0 | https://github.com/emilk/egui | Interfaz gráfica |
| symphonia | MPL-2.0 | https://github.com/pdeljanov/Symphonia | Lectura de WAV, AIFF, FLAC |
| hound | Apache-2.0 | https://github.com/ruuda/hound | Escritura de WAV |
| cpal | Apache-2.0 | https://github.com/RustAudio/cpal | Salida de audio |
| RustFFT | MIT OR Apache-2.0 | https://github.com/ejmahler/RustFFT | FFT (estiramiento, espectrales) |
| rfd | MIT | https://github.com/PolyMeilex/rfd | Diálogos de archivo |
| serde / serde_json | MIT OR Apache-2.0 | https://github.com/serde-rs/serde | Sesiones y preferencias |
| dirs | MIT OR Apache-2.0 | https://github.com/dirs-dev/dirs-rs | Carpetas del usuario |
| Anton | SIL OFL 1.1 | https://github.com/googlefonts/AntonFont | Títulos de zona |
| IBM Plex Sans | SIL OFL 1.1 | https://github.com/IBM/plex | Etiquetas y texto |
| Courier Prime | SIL OFL 1.1 | https://github.com/quoteunquoteapps/CourierPrime | Valores numéricos |

### Notas de cumplimiento

- **MPL-2.0 (symphonia)**: se usa sin modificar; su código fuente está en el
  enlace de arriba. Si algún día se modifica un archivo de symphonia, ese
  archivo modificado debe publicarse bajo MPL-2.0.
- **LGPL-2.1 (CDP)**: los programas de CDP se ejecutan como procesos
  externos, sin linkear. `tools/build_cdp.ps1` los compila sin
  modificaciones desde el repositorio oficial CDP8, fijado al commit
  `28bc42c72c1a7cb0fab933acd1c433be958a787b` (2026-06-08), con MinGW-w64 de
  MSYS2 y enlace estático. `build.ps1` los copia a `dist\cdp\` con su
  licencia (`LICENSE-CDP.txt`), el commit (`COMMIT.txt`) y un `LEEME.txt` con
  el enlace al código fuente.
- **OFL 1.1 (tipografías)**: embebidas sin modificar; no se venden por
  separado.

## Licencias aprobadas fuera de la lista base

Licencias permisivas que llegan como dependencias obligatorias de eframe en
Windows. Aprobadas por el autor el 2026-10-05; textos en `licenses/BSL-1.0.txt`
y `licenses/Unicode-3.0.txt`.

| Licencia | Crates | Vía | Para qué |
|---|---|---|---|
| BSL-1.0 (Boost) | clipboard-win, error-code | eframe → egui-winit → arboard | Copiar/pegar en campos de texto |
| Unicode-3.0 | icu_* , zerovec, yoke, tinystr, litemap, writeable, zerotrie, zerofrom, potential_utf | eframe → egui-winit → webbrowser → url → idna | Abrir enlaces (créditos) |
| (MIT OR Apache-2.0) AND Unicode-3.0 | unicode-ident | macros de compilación (no queda en el ejecutable) | — |
