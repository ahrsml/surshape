# Cambios de SURSHAPE

## 0.9.0 — marcadores, bulk y sub-patches (fase 8)

- **Marcadores** por fuente: importar y exportar etiquetas de Audacity,
  agregar en el cursor (`M`) o como región, ir al anterior / siguiente
  (`,` `.`; una región queda elegida para procesar ese fragmento), dibujados
  en el visor. En el Graph-Edit, puntos desde marcadores y marcadores desde
  puntos. Se guardan en la sesión y en los patches de texto.
- **Bulk**: la cadena de una fila aplicada a varios archivos (menú Patch o
  casilla Bulk al importar); guardar el final de cada fila en una carpeta.
- **Sub-patches**: una receta como una sola celda, con sus pasos guardados en
  ella; cada paso se calcula y se cachea por separado. Página propia y
  "abrir como fila". También en los patches de texto (líneas `paso`).

## 0.8.0 — pulido, manual y distribución (fase 7)

- Los nombres de los procesos ya no llevan "(CDP)": el motor se ve solo en la
  página de parámetros, la línea de comando y la consola. "Estirar espectro"
  de CDP pasa a llamarse "Estirar parciales" (había otro con el mismo nombre).
- Manual (español e inglés) reescrito para la interfaz nueva, con capturas.
- Textos de ayuda que describían la interfaz anterior, actualizados.
- Revisión de traducciones regenerada (`docs/revision_traducciones.html`).
- Test del flujo con teclado y operaciones de celda (Alt+letra, agregar,
  reemplazar, deshacer, borrar).
- Las preferencias no se escriben en el modo captura ni en los tests.
- Paquete de distribución con los programas de CDP que usa el catálogo
  ampliado (strange, spec, modify, filter, sndinfo, housekeep, submix).

## 0.7.0 — corrección mayor de la interfaz (fase 6)

### Interfaz (tipo Soundshaper, look Win32 clásico)
- Ventana armada con paneles que llenan el espacio (se eliminó el lienzo fijo
  de 1280x800); barra lateral y visor redimensionables, tamaños recordados.
  Ventana mínima 1024x700.
- 16 menús con letra de acceso (Alt+letra): Sesión, Editar, Edición/Mezcla,
  Soundfile/Tiempo, Espectral, Pitch Data, Synth/Generadores, Destrucción,
  Filtro/Espacio, Info, Datos, Patch, Marcadores, Herramientas, Preferencias,
  Ayuda. Toolbar plana con íconos de 16x16.
- Barra lateral con secciones de color (Archivos, Procesos con búsqueda,
  Patches, Guardar archivo) y controles de patch/celda (Cargar, Guardar,
  Ejecutar, Limpiar, Re-editar, Reemplazar, Copiar, Borrar).
- Visor a todo el ancho (fondo marino, onda celeste, lecturas en Courier),
  zoom vertical, mostrar/ocultar canales, transporte (reproducir, pausa,
  detener, inicio, fin, bucle), A/B, inicio y fin editables, botones rápidos
  (Info, Nivel, Editar, Cortar, Reset) y "Procesar: todo / selección".
- Grilla tipo planilla: filas A–P, columnas 0–98, encabezados con bisel,
  scrollbars con flechas. Cada celda muestra estado, tipo de archivo y "auto"
  si convierte su entrada sola.
- Página de parámetros: modo del programa CDP, nombre traducido + nombre CDP,
  valor con spinner y slider, mínimo/máximo/defecto, unidad, casilla T-V
  (abre el Graph-Edit), rango propio para aleatorizar, pestañas (Seed,
  Variantes, Instantáneas/Presets, Por canal), línea de comando en vivo y
  copiable, RENDER / Previsualizar / Volver.
- Graph-Edit como página propia (breakpoints sobre la onda, sliders X/Y,
  tabla, formas, importar/exportar, Aceptar/Cancelar/Aplicar). Página de
  mezcla. Consola con cada ejecución.
- Preferencias con pestañas (Rutas, PVOC, Audio, Unidades e idioma,
  Comportamiento, Accesibilidad). Unidades de tiempo: segundos, muestras o
  h:m:s.ms. Dispositivo de salida elegible. Zoom de interfaz 100/125/150 %.
- Look Win32: biseles, campos hundidos blancos, combos, casillas y radios
  clásicos, group boxes, pestañas, barras de título con degradado, diálogos
  Aceptar/Cancelar/Aplicar. Letra Tahoma (o Segoe UI) de Windows; Courier
  Prime solo en lecturas, comandos y consola. Sin Anton.
- Los procesos de dos sonidos aparecen como "(2 sonidos)" y la segunda
  entrada se elige de una lista o en la grilla; por defecto es el sonido de
  la misma fila. Los distintivos NAT/CDP ya no están en la vista principal.

### Funcionamiento
- Las celdas producen archivos: `~A_3.wav`, `~B_2.ana`… en `<sesión>/celdas/`
  (enlaces, sin ocupar más disco).
- Tipos de archivo de CDP a la vista (.wav, .ana…). "PVOC análisis" y "PVOC
  resíntesis" son procesos; los espectrales leen y escriben .ana. Si una
  celda recibe sonido y pide .ana, se analiza sola (auto-conversión,
  cacheada, con los parámetros de Preferencias).
- Consola: comando, salida, código, tiempo y archivos de cada ejecución.
- Deshacer (Ctrl+Z) y deshacer por celda; Reemplazar; Limpiar; ejecutar
  celda, fila, desde aquí o el patch completo.
- Patches como texto legible y editable (`.sspatch`), con o sin las fuentes;
  exportar como script `.bat` de CDP.
- Info de celda (frecuencia, canales, duración, pico, RMS y continua por
  canal, tipo, tamaño) e Info de CDP (sndinfo).
- Edición no destructiva: extraer, recortar, silenciar, fundidos, invertir
  polaridad, ganancia. Editor externo (p. ej. Audacity). Bucles por celda
  (Get/Set Loops, LoopPt, Endtime).
- Catálogo CDP: 11 procesos nuevos (promediar canales, suprimir parciales,
  desplazar frecuencias, estirar espectro, plegar espectro, filtro
  espectral, ganancia espectral, promediar y multiplicar ciclos, transponer
  velocidad, pasa bajos/altos).

### Correcciones
- Zigzag (CDP) fallaba con archivos más largos que 20 s: ahora el largo es
  un factor del sonido.
- "Repetir ciclos" y "Fractal de ciclos" (CDP) fallaban con silencios largos
  o corrimiento de continua ("cycle_search exceeds buffer size").

### Calidad
- Tests de layout (egui_kittest) a 1024x700, 1280x800 y 1920x1080 en
  español e inglés: ningún control sale de su contenedor.
- Capturas en `docs/capturas/` (`SURSHAPE.exe --captura <carpeta>`).
- Patch de ejemplo de punta a punta: .wav -> .ana -> espectral ->
  resíntesis -> nativo.

## 0.6.0 y anteriores
Fases 1 a 5: motor, procesos nativos y CDP, grilla multi-fila, mezcla,
cascada, recetas, A/B (ver `CLAUDE.md`).
