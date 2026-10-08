# SURSHAPE · guía del proyecto

Aplicación standalone de diseño sonoro **offline** (espíritu Soundshaper /
Composers Desktop Project), cercana a Soundshaper también en lo estructural:
la sesión es un **patch vivo** (grafo de nodos editable y re-renderizable).
No es plugin ni efecto en tiempo real. Este archivo fija las decisiones del
proyecto; mantenerlo al día cuando algo cambie.

## Decisiones fijas

- **Stack**: Rust 2021 + egui/eframe (0.31, igual que NOISEGEK). Comentarios,
  documentación y mensajes de commit en **español**.
- **Licencia**: GPL-3.0-or-later.
- **Dependencias permitidas**: MIT, BSD, ISC, zlib, Apache-2.0, MPL-2.0, LGPL.
  Fuentes: SIL OFL. **Nada GPL de terceros sin preguntar.** Cualquier otra
  licencia, preguntar. `python tools/licencias.py` audita todo el árbol de
  Windows y falla ante licencias fuera de la lista (las aprobadas se anotan
  en `APROBADAS` dentro del script). Cada dependencia directa nueva va a
  `THIRD_PARTY.md` y a `crates/surshape-app/assets/creditos.tsv`.
- **Toolchain**: Rust local (x86_64-pc-windows-gnu), sin tocar PATH ni
  registro. `env.ps1` usa `.\.toolchain` si existe; si no, el de NOISEGEK.
  Windows x64, MSYS2 MinGW, CMake, Ninja. **Sin MSVC.** Los `.ps1` se guardan
  en UTF-8 **con BOM** (PowerShell 5.1).
- **Proyecto hermano**: `noisegek-dsp` (`../NOISEGEK/vst/noisegek-dsp`) se usa
  **tal cual** por ruta relativa; no se modifica. Sus tests (34) deben seguir
  pasando: `cargo test -p noisegek-dsp`.
- **CDP**: Release 8 en `C:\CDPR8\_cdp\_cdprogs` (265 programas). Siempre como
  procesos externos, **nunca linkeado**. Fase 3: build desde el repo oficial
  CDP8 (commit fijo, MinGW) con `tools/build_cdp.ps1`.

## Concepto central: patch vivo

- Offline y **no destructivo**: nunca se sobrescribe un original.
- La sesión contiene un **patch**: grafo dirigido acíclico de nodos. Cada
  nodo es una fuente, un proceso, una mezcla o (más adelante) un sub-patch,
  con parámetros, seed y referencias a sus entradas (`nodo:salida`).
- Los parámetros de cualquier nodo se editan después. Un nodo está
  **desactualizado** si su hash actual no coincide con el de su último render;
  el estado se *deriva*, no se guarda a mano. Re-render en cascada (uno o
  todos, en orden topológico).
- **Cache por hash** de (id y versión del proceso + parámetros, incluidos
  breakpoints + seed + hashes de las entradas + región). Mismo hash = no se
  recalcula. Los renders viven en `sesion/cache/<hash>_<salida>.wav`.
- Un patch se guarda como **plantilla** (las fuentes pasan a ser entradas con
  nombre) y se re-ejecuta con otras fuentes. Las "recetas" son plantillas.
  **Sub-patch** = plantilla usada como un solo nodo (modelo listo; implementar
  al final). **Bulk** = la misma plantilla, una fila por fuente.
- Seed explícita en todo proceso aleatorio: reproducible.
- Todo el trabajo pesado en hilos aparte (`Task`); la UI nunca se congela.

## Workspace

```
crates/
  surshape-audio/    AudioBuf f32 planar, decodificación (symphonia),
                     export WAV 24/32f (hound), análisis, limitador, picos
  surshape-engine/   trait Process, ParamSpec/ParamValue (fijo o
                     breakpoints), RenderCtx, Task, render + validación,
                     hash estable para la cache
  surshape-patch/    MODELO: grafo de nodos, puertos, grilla (vista), estado
                     derivado, orden topológico, planificador de cascada,
                     plantillas/sub-patches, snapshots, presets
  surshape-native/   procesos nativos; depende de noisegek-dsp
  surshape-cdp/      wrapper CDP (fase 3; hoy solo descubrimiento) + escritura
                     de archivos breakpoint de texto
  surshape-i18n/     t!("clave"), locales/es.lang y locales/en.lang
  surshape-session/  persistencia: carpeta, sesion.json, autosave, cache en
                     disco, marcadores
  surshape-app/      eframe: grilla de patch, visor, página de parámetros,
                     preferencias, créditos
tools/check_i18n/    tests de i18n · tools/licencias.py · tools/build_cdp.ps1
licenses/            textos de licencia (generados + generales)
```

### Interfaz de procesos (`surshape-engine::Process`)

- `id()` estable (`nat.*`, `cdp.*`) y `version()`: subirla cuando cambia el
  algoritmo, para invalidar la cache.
- `family()`, `engine()` (Nativo | Cdp).
- `params() -> &[ParamSpec]`: clave i18n, rango, unidad, default, escala
  lin/log, `automatable` (admite breakpoints).
- `inputs() -> &[InputSpec]`: entradas con nombre (clave i18n); la 2.ª entrada
  puede ser cualquier celda. `Variadic` para la mezcla.
- `outputs() -> OutputSpec`: `Fixed(n)` o `Dynamic` (p. ej. slice espectral):
  `process()` devuelve `Vec<AudioBuf>`; las salidas extra van a filas nuevas.
- `kind`: audio, o **info** (reporte de texto, sin audio; fase 7).
- `per_channel`: si admite parámetros distintos por canal (estéreo).
- `uses_seed()`, `expected_len(inputs, params)`, `process(&mut RenderCtx)`.
- `RenderCtx`: entradas, parámetros resueltos (`ctx.value_at(id, t)` para
  breakpoints), seed, canal, progreso (`&AtomicU32`), cancelación
  (`&AtomicBool`), carpeta temporal.

### Parámetros

- `ParamValue = Fixed(f64) | Envelope(Breakpoints)`. `Breakpoints`: pares
  (tiempo, valor), tiempo **absoluto o normalizado** a la duración de la
  entrada, interpolación lineal/exp/log/escalón. Formato de texto propio para
  importar/exportar (y el `.brk` de CDP se genera de ahí). Formas
  predefinidas: rampas, curvas exp/log, aleatorio con seed.
- `ParamSet`: valores comunes + opcionalmente por canal.
- **Snapshots** (varios ParamSet con nombre por nodo) y **presets** por
  proceso en archivos de texto `presets/<proceso>/<nombre>.preset`
  (`clave = valor`), con preset por defecto cargado automáticamente.
- **Previews**: N variantes de un mismo nodo renderizadas a la cache sin
  entrar al grafo hasta que se elige una.

### Modelo del patch (`surshape-patch`)

- `Node { id, kind, params: ParamSet, seed, inputs: Vec<PortRef>, region,
  snapshots, render: Option<RenderRecord { hash, salidas, informe }> }`.
- `NodeKind = Source { archivo } | Process { proceso } | Mix { entradas con
  ganancia, paneo e inicio } | SubPatch { plantilla, enlaces }`.
- `PortRef { nodo, salida }`. Estado derivado: renderizado / desactualizado
  / error / en proceso.
- **Grilla** = vista, separada del grafo: filas (cadenas paralelas, cada una
  con su fuente) y celdas (nodos de izquierda a derecha). Copiar celdas o
  tramos para ramificar; elegir 2.ª entrada con clic en la grilla.
- **Marcadores** por fuente (importables desde etiquetas de Audacity) para
  procesar fragmentos y crear tiempos de breakpoint.

Límite actual: cada render vive entero en memoria (`MAX_RENDER_SAMPLES`). Si
se pasa a streaming, el export necesitará RF64 (> 4 GB).

## i18n

- `crates/surshape-i18n/locales/{es,en}.lang`: `clave = valor`, `#` comenta,
  `\n` salto, `{nombre}` argumento. Mismo orden de claves en ambos. Secciones
  `ui.*`, `proc.<id>.nombre/desc`, `param.<proc>.<param>` y `.desc`, `err.*`,
  `creditos.*`.
- Embebidos; una carpeta `locales/` junto al exe tiene prioridad (revisión
  de traducciones sin recompilar).
- Términos técnicos marcados con `# [TÉRMINO]`: **los decide el usuario (es
  traductor)**. No cambiar traducciones que él haya revisado.
- `cargo test -p check_i18n` falla si falta una clave, hay huérfanas o hay un
  literal visible en `surshape-app` fuera de i18n (los no visibles llevan
  `// i18n-ok`; los bloques `#[cfg(test)]` y los nombres en `.arg("x", ..)`
  no cuentan). Mensajes guardados como clave + argumentos y traducidos al
  dibujar, para que el cambio de idioma en caliente los alcance.

## Interfaz (corrección mayor del 2026-10-06; reemplaza lo anterior)

Referencia: **Soundshaper 6**. Problemas que motivan la corrección: texto y
controles que se salían de sus límites, interfaz poco parecida a Soundshaper,
funcionamiento que escondía la mecánica.

### Layout responsivo
- **Se elimina el lienzo fijo 1280x800 escalado con `zoom_factor`.** Paneles
  de egui (TopBottomPanel / SidePanel / CentralPanel) que llenan la ventana,
  redimensionables con divisores; tamaños recordados en preferencias.
- Ventana mínima 1024x700; bien hasta 2560x1440. El zoom de UI queda solo
  como preferencia de accesibilidad.
- **Regla dura: ningún texto ni control sale de su contenedor.** Etiquetas
  largas: truncar con "…" + tooltip con el texto completo. Descripciones:
  ajuste de línea. Lo que no cabe: scroll, nunca recorte. Sliders y campos:
  ancho relativo al panel.
- Verificar en español y en inglés (el español es más largo) y con zoom
  100/125/150 %.
- **Tests de layout con egui_kittest** (0.31.1, MIT/Apache, compatible con
  egui 0.31, sin GPU): página principal y de parámetros a 1024x700, 1280x800
  y 1920x1080, en ES y EN; fallan si un widget queda fuera de su panel.

### Página principal (estructura tipo Soundshaper)
a) Barra de menús con teclas de acceso (Alt+letra): Sesión · Editar ·
   Edición/Mezcla · Soundfile/Tiempo · Espectral · Pitch Data ·
   Synth/Generadores · Destrucción · Filtro/Espacio · Info · Datos · Patch ·
   Marcadores · Herramientas · Preferencias · Ayuda. Todo proceso (nativo o
   CDP) se elige desde su menú, con distintivo de motor.
b) Toolbar plana, íconos 16x16 con tooltip: nueva, abrir, guardar, importar,
   deshacer · render, render en cascada · reproducir, detener · consola,
   preferencias.
c) Barra lateral izquierda con secciones plegables, encabezado de color
   distinto cada una: Archivos (fuentes, recientes, drag & drop) · Procesos
   (catálogo con búsqueda) · Patches (plantillas y recetas) · Guardar archivo
   (celda seleccionada -> archivo permanente). Debajo, controles de
   patch/celda: Cargar, Guardar, Ejecutar, Limpiar, Re-editar, Reemplazar,
   Copiar, Borrar; casillas: incluir fuentes, fila única, patch completo,
   copiar fila, bulk, etc.
d) Visor de onda arriba, ancho completo del área central: regla de tiempo,
   lecturas de cursor, duración y selección.
e) Barra de transporte bajo el visor: reproducir, pausa, detener,
   inicio/fin, loop · nombre de archivo, duración, canales, picos por canal ·
   loops/marcadores · radio "Procesar: archivo completo / selección" con
   inicio y fin editables.
f) Columna derecha angosta de botones rápidos: Info, Nivel, Editar, Cortar,
   Reset.
g) **Grilla de patch tipo planilla** en el resto del alto: columnas 0, 1,
   2... (0 = fuente) y filas A, B, C...; hasta 16 filas y 99 columnas,
   scroll en ambos ejes. Celdas compactas: nombre del proceso, etiqueta de
   tipo de archivo, ícono de estado. Clic: selecciona y carga en el visor.
   Doble clic: página de parámetros. Hover: estado y parámetros en la línea
   de ayuda.
h) Línea de ayuda contextual y, debajo, barra de estado con paneles
   hundidos: "Celda: A_3 · Entrada: ~A_2.wav · Entrada 2: ~A_0.wav · Salida:
   ~A_3.wav · <comando de la celda>".

### Página de parámetros (reemplaza el panel fijo NODO)
- Se abre al elegir un proceso o con doble clic en una celda; ocupa el área
  central con el visor de la entrada visible arriba.
- Selector de MODO cuando el programa CDP tiene modos (los parámetros
  cambian según el modo).
- Cada parámetro: nombre traducido + nombre CDP original; campo numérico con
  spinner + slider; mínimo, máximo y defecto; unidad; casilla **T-V**
  (variable en el tiempo) que abre el Graph-Edit; rango de randomización
  propio. Valores exactos tipeables.
- Seed, variantes/previews, snapshots/presets y parámetros por canal en
  group boxes o pestañas.
- **Línea de comando visible**, en vivo, seleccionable y copiable: CDP = los
  comandos exactos (incluidas conversiones); nativo = descriptor equivalente
  (p. ej. `nat.stretch factor=50 ventana=0.25 seed=922299`).
- Botones: RENDER (ejecuta y vuelve a la principal; configurable),
  Previsualizar, Cancelar/Volver.
- **Graph-Edit** (página propia): breakpoints sobre la onda de la entrada,
  con reproducción; tabla de puntos editable y sliders X/Y; presets de forma
  (rampas, exp/log, aleatorio con seed); importar/exportar texto; envío
  a/desde marcadores.
- **Página de mezcla**: página aparte, misma lógica.

### Look Win32 clásico (emulado en egui, sin cambiar de toolkit)
- Módulo de widgets propio `surshape-app/src/win32/` que emula el tema
  clásico de Windows 2000/XP: botones con bisel 3D en relieve (hundidos al
  presionar); campos de texto y numéricos hundidos con fondo blanco y
  spinners; combos con botón de flecha; checkboxes y radios clásicos; group
  boxes con borde grabado y título; pestañas clásicas; scrollbars con
  flechas; toolbar plana con hot-tracking; barra de estado con paneles
  hundidos; divisores arrastrables; diálogos modales Aceptar / Cancelar /
  Aplicar.
- Sin esquinas redondeadas, sin sombras difusas, sin animaciones.
- Compacta como Soundshaper: paddings y alturas de fila reducidos; mucha
  información visible sin verse apretada.
- Tipografía: Tahoma o Segoe UI cargada en tiempo de ejecución desde
  `C:\Windows\Fonts` (no se distribuye), a 8-9 pt equivalentes; fallback IBM
  Plex Sans. Courier Prime solo en lecturas del visor, línea de comando y
  consola. **Se elimina Anton de la interfaz.**
- Ventana principal con barra de título nativa del sistema.
- Referencia visual: captura de Soundshaper 6.3 (`sounshaperlooksample.jpg`
  en la raíz del proyecto).

### Color Win32: tema "Crepúsculo de invierno" (0.9.1, por defecto)
Paleta "Crepúsculo de invierno" (cielo invernal de Los Ríos). Sin negro ni
rojo, para que SURSHAPE no se confunda con NOISEGEK; el piso es #142A4E.
Oscuros para fondos, paneles, grilla y visor; claros solo para texto, onda y
acentos. Se cambia el color, no la forma: biseles, campos hundidos, group
boxes, sin esquinas redondeadas, sombras ni animaciones. egui parte de
`Visuals::dark()` y el tema lo sobrescribe.

- **Paleta permitida (solo estos valores; lo exige el test
  `paleta_solo_valores_permitidos`)**: base #29559C #376BBF #748FD2 #9D96C0
  #9397C7 · escala oscura (#29559C con negro al 15/25/35/42/50 %) #234885
  #1F4075 #1B3765 #18315A #142A4E (piso) · claros #EAEEF8 #D4D5E9 #C7D2ED.
  Un derivado nuevo se obtiene igual (base con blanco o negro) y se avisa
  antes de agregarlo.
- ButtonFace / ventana #18315A · paneles y barra lateral #1B3765 · bisel
  claro #748FD2 · sombra y sombra oscura #142A4E · borde superior izquierdo
  de lo hundido (campos, radios) #376BBF.
- Campos #142A4E con texto #EAEEF8. Texto #EAEEF8, secundario #D4D5E9,
  lavandas #9D96C0 / #9397C7 solo sobre #142A4E o #18315A. Deshabilitado
  #748FD2 (exento de AA, 4,49:1 sobre campo).
- Selección #29559C con texto #EAEEF8. Barras de título: degradado #1B3765 ->
  #376BBF, texto #EAEEF8 en negrita a la izquierda.
- Visor: fondo #142A4E, onda #9D96C0, lecturas #D4D5E9, cursor #EAEEF8,
  selección #376BBF semitransparente (alfa 56), regla y líneas #234885,
  marcadores #748FD2, bucle #9397C7.
- Espectrograma #142A4E -> #29559C -> #376BBF -> #748FD2 -> #9D96C0 -> #EAEEF8.
- Encabezados de la barra lateral: #29559C y #376BBF con texto #EAEEF8;
  #9D96C0 y #C7D2ED con texto #142A4E.
- Grilla: celdas #1B3765, vacías #18315A, columna 0 #1F4075, encabezados
  #18315A con bisel, líneas #234885 (decorativas), selección #29559C con
  texto #EAEEF8 y borde de foco #9D96C0; entrada 2 / marca #234885.
- Acentos (Render, foco, hot-tracking) #748FD2 y #9D96C0. Botón de toolbar
  encendido #234885. Íconos: acento #748FD2, carpeta #9D96C0.
- Aviso: texto #D4D5E9 con su ícono. Error: texto #EAEEF8 sobre franja
  #29559C con ícono de error (`Level::rich`): se distingue del aviso por algo
  más que el tono.
- **Tema "Claro (clásico)"** en Preferencias -> Accesibilidad: la paleta
  acero/celeste/turquesa de la 0.9.0 (`theme::CLASICO`, `pal_clasico`). La
  preferencia se guarda (`Prefs::tema`); los tests y el modo captura usan
  Crepúsculo. Único cambio: acento #2887AC en lugar de #04B5E9 (la línea de
  envolvente sobre blanco daba 2,3:1).
- Código: `theme::t()` devuelve el tema activo; los colores son campos en
  mayúsculas (`theme::t().FACE`). Ningún color literal fuera de `theme.rs`.
- **Historial de decisiones de contraste** (test
  `contraste_text_pairs_meet_wcag_aa`, ambos temas; texto >= 4,5:1, gráficos
  y bordes funcionales >= 3:1):
  - 0.9.1: cuarto encabezado lateral #C7D2ED y no #748FD2 (#142A4E sobre
    #748FD2 = 4,49:1). Bisel claro #748FD2 y no #376BBF (2,47:1 contra la
    cara: los botones se veían planos). El piso #142A4E no se ve sobre la
    cara, así que lo hundido lleva #376BBF arriba a la izquierda. Selección
    del visor con alfa 56 (con 70, la lectura del tema clásico daba 4,29:1).
    Foco #9D96C0 sobre la selección #29559C da 2,63:1 (decorativo; contra las
    celdas vecinas 4,24 y 4,65).
  - Hasta 0.9.0 (tema claro): texto secundario #2B5468, aviso #0B4A78 y
    error #082F66 fuera de paleta por legibilidad; líneas de grilla #93B9D2
    decorativas (2,08:1).

### Esquemas aprobados (2026-10-06) y agregados
- Esquemas ASCII de principal, parámetros, Graph-Edit, Preferencias y
  Consola: aprobados con estos agregados (herramientas de onda tipo
  Soundshaper):
  - Edición no destructiva (menú Edición/Mezcla; cada operación = celda
    nueva): cortar/extraer, recortar, silenciar selección, fundidos,
    invertir polaridad, ganancia, normalizar.
  - "Editar": abre la celda en un editor externo configurable (p. ej.
    Audacity) y trae el resultado como fuente nueva al cerrar.
  - Loops: Get/Set Loops (bucle por celda), LoopPt, Endtime.
  - Zoom vertical (amplitud) y mostrar/ocultar canales en el visor.
- Decisiones de contraste: título de las barras degradadas alineado a la
  izquierda (el texto blanco cae sobre la parte oscura; #2887AC solo da
  4,08:1); líneas de grilla #93B9D2 (2,08:1 sobre blanco) aceptadas como
  decorativas.

### Decisiones técnicas de la fase 6
- **Tipos de dato**: `surshape-engine::FileKind` (Wav, Ana, Frq, For, Env,
  Brk, Txt). Cada proceso declara tipos de entrada y salida. Los procesos
  nativos trabajan con `AudioBuf` (Wav); los CDP trabajan sobre archivos.
  Los archivos espectrales y de datos son **uno por canal** (CDP es mono).
- **Auto-conversión** en el runner, cacheada por clave como cualquier render
  (p. ej. Wav -> Ana con `pvoc anal` y los parámetros PVOC de Preferencias);
  la celda y su línea de comando lo muestran.
- **Archivos de celda**: `~A_3.wav` / `~A_3.ana` en `<sesión>/celdas/`,
  enlaces duros a la cache (sin duplicar disco), regenerados al cambiar la
  grilla. "Guardar archivo" copia a un archivo permanente.
- **Consola**: registro global de ejecuciones (CDP y nativas) con comando,
  stdout/stderr, código, tiempo y archivos.
- **Deshacer**: pila de estados del patch (instantáneas del grafo); la cache
  por clave hace que deshacer recupere los renders al instante.
- **Grilla tipo planilla**: filas A..P (16), columnas 0..98; columna 0 =
  fuente, generador, mezcla o referencia (`→A_1`) a otra celda.
- **Patches como texto**: formato propio legible (`.sspatch`), usado para
  plantillas/recetas; `sesion.json` sigue siendo el guardado interno.
- **Tests de layout**: los widgets del módulo `win32` registran su
  rectángulo y el de su contenedor en una auditoría; los tests (con
  egui_kittest como arnés) fallan si alguno se sale.
- **Capturas**: modo `--captura` del ejecutable (captura de eframe, PNG
  propio sin dependencias) para `docs/capturas/`.

### Entrega de la fase 6
1. Esquemas ASCII de la página principal, parámetros (modo, rangos, T-V,
   línea de comando), Graph-Edit, Preferencias y Consola. **Esperar
   aprobación antes de implementar.**
2. Después: capturas en `docs/capturas/` (1280x800 y 1920x1080, ES y EN) y
   un patch de ejemplo de 4-5 celdas (.wav -> .ana -> proceso espectral ->
   resíntesis -> proceso nativo) que verifique el flujo completo.
- Todo pasa por i18n ES/EN (términos `# [TÉRMINO]`) y por los tests.
- Versión 0.7.0 y CHANGELOG actualizado.

## Funcionamiento técnico (corrección mayor del 2026-10-06)

- **Celdas = archivos**: cada celda produce un archivo temporal nombrado por
  posición (`~A_0.wav`, `~A_3.wav`, `~B_2.ana`...) en la carpeta temporal de
  la sesión. La cache por hash sigue por dentro, pero al usuario se le
  muestran archivos de celda. Todo es temporal hasta guardar la celda como
  archivo permanente.
- **Tipos de archivo CDP visibles**: .wav (sonido), .ana (espectral), .frq
  (pitch), .for (formantes), .env (envolvente), .brk (breakpoint), .txt
  (texto). Cada proceso declara qué tipo acepta y produce. Menú Espectral
  opera sobre .ana; Pitch Data sobre .frq; Tiempo / Edición-Mezcla / Synth
  sobre .wav.
- **Auto-conversión** como Soundshaper: si un proceso pide otro tipo, se
  convierte en segundo plano (PVOC análisis, getpitch, formantes,
  resíntesis) con los parámetros por defecto de Preferencias; la celda lo
  indica ("auto: PVOC análisis") y aparece en su comando. Las conversiones
  también existen como procesos explícitos. .wav reproducible; .ana
  reproducible vía resíntesis rápida; el resto se visualiza como gráfico o
  texto.
- **Catálogo CDP**: ampliarlo de forma sistemática, por menús, priorizando
  ambient/noise; cada programa nuevo pasa por i18n y tests.
- **Consola**: ventana/página con stdout/stderr de cada ejecución, código de
  salida, tiempo y archivos involucrados. Desde Herramientas y con atajo.
- **Info** (adelantado desde la fase 8): menú Info con funciones de
  información de CDP (propiedades, maxsamp, duración, loudness...) en
  ventana de texto; botón Info sobre cualquier celda: sample rate, canales,
  duración en segundos y samples, pico y RMS por canal, DC offset, tipo y
  tamaño.
- **Operaciones de celda** (semántica Soundshaper): Re-editar; Reemplazar
  (cambia el proceso manteniendo posición y entradas); Copiar (celda, fila o
  tramo); Borrar; Deshacer por celda; Limpiar; Ejecutar (celda, fila, desde
  aquí, patch completo); "Incluir fuentes" al guardar patch.
- **Patches como texto** legible y editable a mano (celdas, procesos, modos,
  parámetros, breakpoints, entradas); "Exportar como script .bat" para
  cadenas puramente CDP.
- **Unidades de tiempo** seleccionables en todo el programa: segundos,
  samples, h:m:s.ms.
- **Preferencias con pestañas**: rutas (CDP, carpeta temporal); PVOC por
  defecto (ventana, overlap); dispositivo de audio, unidades de tiempo,
  idioma; comportamiento tras Render; zoom de UI.

### Color (`surshape-app/src/theme.rs`, nombres semánticos)

- **Prohibidos negro y rojo; nada más oscuro que #142A4E.** Paleta y
  asignación: ver "Color Win32" arriba.
- WCAG AA (4,5:1) en todo texto y >= 3:1 en gráficos y bordes funcionales,
  en los dos temas, verificado por test
  (`cargo test -p surshape-app contraste -- --nocapture` imprime los pares).
- Estados de celda (renderizada / desactualizada / error / en proceso):
  ícono + texto, nunca solo color.

## Calidad

- Tests que renderizan cada proceso (nativo y CDP) sobre ruido y seno, en
  defaults y extremos de rango: sin NaN/Inf, duración esperada, nunca
  > 0 dBFS sin aviso; seed reproducible.
- Tests del grafo: hash estable, invalidación en cascada, ciclos rechazados,
  cache que evita recalcular.
- Limitador opcional y aviso de clipping. Autosave en cada cambio.

## Scripts

- `. .\env.ps1` · `.\build.ps1 [-Test]` (dist\SURSHAPE.exe + licencias) ·
  `.\ejecutar.ps1 [-Debug]` · `.\limpiar.ps1 [-Todo]`.

## Plan por fases (una a la vez; compila, prueba y espera OK del usuario)

1. Workspace, scripts, i18n, tema. Carga, reproducción, forma de onda,
   render en hilo, export WAV. Procesos: normalize, reverse, extreme stretch,
   bitcrush, waveshaper. **Además: modelo de grafo (`surshape-patch`), hash y
   cache de renders; UI = grilla mínima de una fila.**
2. Motor STFT nativo, procesos espectrales, espectrograma.
3. CDP: wrapper, subconjunto curado (5-8), **generación de breakpoints para
   CDP**, build_cdp.ps1, empaquetado, créditos y licencias.
4. Granular, convolución, dos entradas, generadores, meta-procesos, más CDP.
   **Editor de breakpoints, previews, snapshots, presets, parámetros por
   canal.**
5. **Grilla completa multi-fila, copiar/ramificar, salidas múltiples,
   mezcla, re-render en cascada, plantillas con otra fuente.** A/B.
6. **Corrección mayor (2026-10-06)**: layout responsivo con paneles y tests
   de layout, estructura Soundshaper (menús, toolbar, barra lateral, visor,
   transporte, grilla tipo planilla, página de parámetros, Graph-Edit,
   mezcla), funcionamiento técnico (celdas = archivos, tipos CDP,
   auto-conversión, consola, Info, operaciones de celda, patches como texto,
   unidades de tiempo, preferencias con pestañas), look Win32 clásico.
7. (Lo que era la fase 6; se retoma después porque el manual debe reflejar
   la interfaz nueva.) Pulido de UI, atajos, manual (generar_docs.py),
   revisión de traducciones, paquete de distribución.
8. **Marcadores (Audacity), bulk processing, sub-patches.** (Info se
   adelantó a la fase 6.)

## Estado

- **0.9.1 implementada (2026-10-07), pendiente de aprobación**: tema oscuro
  "Crepúsculo de invierno" por defecto y "Claro (clásico)" como opción
  (Preferencias -> Accesibilidad). Ver "Color Win32" y CHANGELOG.md.
- **Fase 8 (versión 0.9.0) implementada (2026-10-07), pendiente de
  aprobación**: marcadores (`surshape-app/src/markers.rs`; etiquetas de
  Audacity), bulk (casilla y menú Patch; `ImportTarget::DuplicateRow`),
  sub-patches (`NodeKind::SubPatch { plantilla, nombre, pasos }`; el runner
  arma una fila temporal con `Patch::from_steps` y registra el resultado del
  último paso con `cache_alias`).
- Fase 7 (0.8.0) aprobada (2026-10-07).

- **Fase 7 (versión 0.8.0) implementada (2026-10-07), pendiente de
  aprobación**: manual ES/EN reescrito con capturas (`{{CAPTURA:x}}` en
  `docs/manual/*.md`, las toma `generar_docs.py` de `docs/capturas`), textos
  viejos actualizados, test de flujo con teclado, paquete
  `paquetes/SURSHAPE-0.8.0-win64.zip`.

- **Fase 6 (corrección mayor, versión 0.7.0) aprobada (2026-10-07).** Los
  nombres de proceso ya no llevan "(CDP)" (pedido del usuario); el motor se
  ve solo en la página de parámetros, la línea de comando y la consola. Ver CHANGELOG.md. Notas:
  - Interfaz en `surshape-app/src/`: `shell.rs` (menús propios con letra de
    acceso, toolbar, estado), `sidebar.rs`, `mainpage.rs`, `grid.rs`
    (planilla), `page.rs` (parámetros y mezcla), `bpedit.rs` (Graph-Edit),
    `console.rs`, `windows.rs` (diálogos), `ops.rs` (acciones y operaciones
    de celda), `capture.rs` (`--captura`), `win32/` (widgets y auditoría).
  - Auditoría de layout: mide desbordes a lo ancho contra el contenedor que
    tenía el `ui` antes de ocupar lugar (a lo alto las filas crecen y las
    páginas se desplazan). Las scrollbars de egui (barra lateral, páginas)
    son sólidas sin flechas; la planilla tiene scrollbars clásicas propias.
  - Motor: `FileKind` y `DataSet` en surshape-engine, `Process::process_files`
    para CDP con .ana, `run_files`, consola global (`engine::console`),
    `RenderOptions::pvoc` (solo entra en la clave de procesos que leen .ana).
    El runner hace la auto-conversión (clave propia, `cache/<clave>.conv`) y
    guarda la resíntesis de cada .ana como su .wav.
  - CDP: 23 procesos; `tools/build_cdp.ps1` compila además strange, spec,
    modify, filter, sndinfo, housekeep y submix.
  - Marcadores y bulk siguen en la fase 8 (menú y casilla deshabilitados).


- Fases 1 a 5: aprobadas por el usuario (2026-10-05/06).
- **Fase 6 anterior NO aprobada** (2026-10-06): reemplazada por la
  corrección mayor (nueva fase 6). Lo hecho en ella (atajos, manual,
  distribución) se conserva y se rehace donde la interfaz nueva lo cambie,
  en la fase 7. Pendiente del usuario: la especificación completa de la
  sección 4 (look Win32; el mensaje llegó cortado) y la captura de
  Soundshaper 6 de referencia.
- Lo hecho en la fase 6 anterior (versión 0.6.0):
  - Atajos: lista única en `surshape-app/src/shortcuts.rs` (ventana F1, menú
    Ayuda, manual). Navegación con flechas, Ctrl+R/Alt/Mayús, Ctrl+C/V/B,
    Supr, Ctrl+I/E, G (grilla ampliada), +/−/0, Inicio, L, B, Esc.
  - Pulido: menú Ayuda (Atajos, Manual, Acerca de), grilla ampliable,
    ícono de ventana (dibujado en código) y del .exe (`build.rs` + `windres`
    de MSYS2, `tools/generar_icono.py`), abrir carpetas de presets/recetas,
    selección ignorando celdas inexistentes.
  - i18n: test de argumentos `{…}` iguales en ambos idiomas;
    `docs/revision_traducciones.html` (generado) para la revisión del usuario.
  - Manual: `docs/manual/{es,en}.md` (texto) + `docs/catalogo.json`
    (`cargo run -p check_i18n --bin exportar_catalogo`) ->
    `docs/generar_docs.py` -> `dist/manual/manual_{es,en}.html`.
  - Distribución (`build.ps1`): dist\ con exe, cdp\, manual\,
    traducciones\, licenses\, codigo_fuente\ (SURSHAPE + noisegek-dsp; CDP8)
    y `paquetes\SURSHAPE-<versión>-win64.zip`.
- Fase 8 pendiente: marcadores (Audacity), bulk, sub-patches.
- Licencias BSL-1.0 y Unicode-3.0 aprobadas (2026-10-05) para lo que trae
  eframe; anotadas en `tools/licencias.py`.
- Créditos sin URL pública para SURSHAPE y noisegek-dsp hasta que existan.
- Repositorio: https://github.com/ahrsml/surshape (rama main, creado a pedido del usuario el 2026-10-07). No se suben `third_party/` (CDP se compila con tools/build_cdp.ps1), `dist/`, `paquetes/` ni la captura de referencia de Soundshaper.
- Pendiente conocido: la cache no se limpia sola (renders que ya no usa
  ningún nodo quedan en `cache/`); agregar "limpiar cache" más adelante.
