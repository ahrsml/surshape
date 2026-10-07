# Manual de SURSHAPE

SURSHAPE es una aplicación de diseño sonoro **offline** para ambient y noise, en el espíritu de Soundshaper y del Composers Desktop Project (CDP). No es un plugin ni un efecto en tiempo real: tomas un sample, le aplicas procesos y obtienes archivos nuevos. El original nunca se modifica.

La idea central es el **patch vivo**: cada sesión es una planilla de celdas (fuentes, procesos y mezclas) que puedes editar en cualquier momento. Si cambias algo, SURSHAPE sabe qué quedó desactualizado y vuelve a calcular solo eso.

## Primeros pasos

- Abre `SURSHAPE.exe`. La primera vez se crea una sesión vacía en `Documentos\SURSHAPE\Sesiones\`.
- Arrastra uno o varios archivos WAV, AIFF o FLAC a la ventana (o **Sesión → Importar audio…**, `Ctrl+I`). Cada archivo ocupa la columna 0 de una fila nueva de la grilla.
- Elige un proceso en un menú (por ejemplo **Espectral**) o con doble clic en la sección **Procesos** de la barra lateral. Se agrega a la fila de la celda elegida y se abre su **página de parámetros**.
- Ajusta los valores y pulsa **RENDER**. El resultado aparece en el visor; escúchalo con la barra espaciadora.
- Guarda la celda como archivo con **Guardar archivo** (barra lateral) o `Ctrl+E`, en WAV de 24 bit o de 32 bit float.

Todo se guarda solo: la sesión (`sesion.json`) se escribe cada vez que cambia algo y cada render queda en la carpeta `cache` de la sesión.

## La ventana

{{CAPTURA:principal}}

- **Menús** (arriba): Sesión, Editar, Edición/Mezcla, Soundfile/Tiempo, Espectral, Pitch Data, Synth/Generadores, Destrucción, Filtro/Espacio, Info, Datos, Patch, Marcadores, Herramientas, Preferencias y Ayuda. La letra subrayada abre el menú con `Alt`+letra; las flechas pasan de un menú a otro.
- **Toolbar**: nueva sesión, abrir, guardar patch, importar, deshacer · render de la celda, render de todo · reproducir, detener · consola, preferencias. A la derecha, el avance del render y **Cancelar**.
- **Barra lateral** (izquierda): secciones plegables de color — **Archivos** (fuentes y recientes), **Procesos** (catálogo con búsqueda), **Patches** (recetas y patches de texto) y **Guardar archivo** — y los **controles de patch/celda**.
- **Visor** (arriba al centro): forma de onda o espectrograma de la celda elegida, con lecturas en las unidades de tiempo elegidas.
- **Transporte** (bajo el visor): archivo de la celda, A (entrada) / B (resultado), duración, canales, picos por canal; reproducir, pausa, detener, inicio, fin, bucle; zoom vertical y canales visibles; inicio y fin de la selección.
- **Botones rápidos** (derecha): **Info**, **Nivel** (normalizar), **Editar** (editor externo), **Cortar** (extraer la selección), **Reset** (zoom completo) y **Procesar: Todo / Selección**.
- **Grilla** (abajo): la planilla del patch.
- **Línea de ayuda**: explica cualquier control al pasar el mouse; si no, muestra el último mensaje (avisos con triángulo, errores con octógono, siempre con texto).
- **Barra de estado**: celda elegida, sus archivos de entrada y salida, y su comando.

Los divisores entre la barra lateral, el visor y la grilla se arrastran; SURSHAPE recuerda los tamaños. La ventana mínima es de 1024x700.

## La grilla

Las filas son **A** a **P** y las columnas **0** a **98**. La columna 0 de cada fila es su origen: un archivo, un generador, una mezcla o una **referencia** a otra celda (`→A_2`), que es como se ve una rama. Las celdas siguientes son los procesos, de izquierda a derecha: cada uno toma como entrada la celda anterior.

Cada celda muestra el nombre del proceso, el **tipo de archivo** que produce (`wav`, `ana`…), "auto" si convierte su entrada sola y su **estado** con un ícono (el texto aparece en la línea de ayuda):

- **Fuente**: un archivo original. Nunca se modifica.
- **Sin render**: todavía no se calculó.
- **Renderizada**: su resultado corresponde a los valores actuales.
- **Desactualizada**: cambió algo en la celda o antes en la cadena.
- **En cola** / **En proceso**: forma parte del render en curso.
- **Error**: el último intento falló; la página de parámetros explica por qué y la consola muestra la salida completa.

Clic: elige la celda y la carga en el visor. Doble clic o `Intro`: página de parámetros. Clic derecho: operaciones de celda. `Mayús`+clic: tramo de celdas de la fila. Flechas: moverse.

Cada resultado se identifica por todo lo que lo produce (proceso, valores, curvas, seed, región, entradas). Si vuelves a valores que ya usaste, el resultado sale de la cache **al instante**.

### Archivos de celda

Cada celda es un archivo: `~A_0.wav`, `~A_1.ana`, `~A_3.wav`… Están en la carpeta `celdas` de la sesión (**Datos → Abrir la carpeta de celdas**), enlazados a la cache sin ocupar más disco. Son temporales: para conservar uno, usa **Guardar archivo**.

### Operaciones de celda

- **Re-editar**: abre la página de parámetros.
- **Reemplazar**: cambia el proceso de la celda manteniendo su lugar y sus entradas (elige el proceso nuevo en un menú; `Esc` cancela).
- **Copiar** / **Pegar** (`Ctrl+C` / `Ctrl+V`): la celda, el tramo elegido o, con la casilla **Copiar fila**, la fila entera. Se pega al final de la fila elegida.
- **Ramificar** (`Ctrl+B`): fila nueva que parte de la celda, para probar otro camino.
- **Borrar** (`Supr`): si otras celdas la usan, pide confirmación y las borra también.
- **Limpiar**: borra desde la celda hasta el final de la fila.
- **Deshacer** (`Ctrl+Z`) vuelve el patch atrás; **Deshacer en la celda** vuelve solo esa celda a sus valores anteriores. Los renders vuelven al instante desde la cache.

### Ejecutar

- **Ejecutar la celda** (`Ctrl+R`): la celda y lo que necesite antes.
- **Ejecutar desde aquí** (`Ctrl+Alt+R`): la celda y todas las que dependen de ella.
- **Ejecutar el patch completo** (`Ctrl+Mayús+R`).
- **Ejecutar la fila**. Las casillas **Fila única** y **Patch completo** cambian lo que hace el botón **Ejecutar** de la barra lateral.

El render corre en segundo plano: la interfaz sigue disponible y **Cancelar** lo detiene sin guardar nada a medias.

## Tipos de archivo y CDP

Como en Soundshaper, los tipos de archivo de CDP están a la vista: `.wav` (sonido), `.ana` (análisis espectral), `.frq` (pitch), `.for` (formantes), `.env` (envolvente), `.brk` (breakpoints), `.txt` (texto).

- **PVOC análisis** pasa de sonido a `.ana`; **PVOC resíntesis** vuelve a sonido. Los procesos del menú **Espectral** leen y escriben `.ana`.
- **Conversión automática**: si un proceso espectral recibe sonido, SURSHAPE lo analiza solo (con los valores de **Preferencias → PVOC**) y la celda dice "auto". El análisis queda en la cache.
- Una celda `.ana` también guarda su resíntesis: se ve y se escucha en el visor, y los procesos de sonido que vengan después la usan.
- CDP trabaja en mono: cada canal se procesa por separado y los datos se guardan un archivo por canal.

SURSHAPE usa los programas de CDP como programas externos. La distribución incluye en la carpeta `cdp` una compilación propia hecha sin modificaciones desde el código oficial de CDP8; otra instalación se elige en **Preferencias → Rutas**. Si CDP falta, el resto de SURSHAPE funciona igual.

## Página de parámetros

{{CAPTURA:parametros}}

- Arriba, la **entrada** de la celda (con su origen si se convirtió sola), escuchable.
- **Modo**: los programas de CDP con modos muestran un selector; los parámetros cambian según el modo.
- **Entrada 2**: los procesos de dos sonidos (marcados "(2 sonidos)") usan por defecto el sonido de la misma fila; se cambia en la lista o con **Elegir en la grilla**.
- **Parámetros**: nombre, nombre original de CDP, valor (campo con flechas: se puede escribir el valor exacto), deslizador, mínimo, máximo, valor por defecto y unidad.
- **T-V** (variable en el tiempo): convierte el valor en una curva y abre el **Graph-Edit**.
- **Random**: rango propio de cada parámetro para **Aleatorizar**.
- Pestañas **Seed** (seed, iteraciones, región), **Variantes** (aleatorizar y previews), **Instantáneas/Presets** y **Por canal**.
- **Línea de comando**: los comandos de CDP exactos (con la conversión automática) o el descriptor del proceso nativo; se puede copiar.
- **RENDER** calcula y vuelve a la página principal (se puede cambiar en Preferencias); **Previsualizar** calcula y escucha sin salir; **Volver** vuelve sin calcular.

### Graph-Edit

La curva se dibuja sobre la onda de la entrada. Clic en un lugar vacío agrega un punto; arrastrar lo mueve; clic derecho lo quita. El punto elegido también se mueve con los sliders **X** e **Y**. Debajo: la tabla de puntos, el tiempo **absoluto** (segundos) o **normalizado** (0 = inicio, 1 = final), la forma entre puntos, formas predefinidas (rampas, exponencial, logarítmica, azar con seed) e **Importar / Exportar** en el formato de CDP («tiempo valor» por línea). **Aceptar** vuelve a los parámetros, **Cancelar** deja el valor como estaba.

### Seed, variantes, instantáneas y presets

- La **seed** hace reproducible el azar: la misma seed da el mismo resultado.
- **Aleatorizar** mueve los valores dentro de su rango Random; **Previews** renderiza varias versiones cortas para escuchar y elegir.
- **Instantáneas**: juegos de valores guardados en la celda. **Presets**: archivos por proceso, válidos en cualquier sesión; uno puede ser **Por defecto**.
- **Recetas** (menú **Patch**): guardan la cadena de procesos de una fila para aplicarla a otra fuente.

## El visor

- **Onda / Espectrograma** (escala logarítmica de frecuencia).
- **A entrada / B resultado** (tecla `B`): alterna sin perder el punto de reproducción.
- **Selección**: arrastra sobre la onda; **Inicio** y **Fin** la muestran y la editan. Con **Procesar: Selección**, los procesos nuevos toman solo ese tramo.
- **Zoom**: rueda del mouse (`Mayús`+rueda desplaza, `Ctrl`+rueda es zoom vertical), `+`, `−` y `0`.
- **Canales**: las casillas C1, C2… muestran u ocultan cada canal.
- **Bucles**: **Set** guarda la selección como bucle de la celda y **Get** la recupera; LoopPt y Endtime (menú Edición/Mezcla) fijan el inicio y el fin en el cursor. Con **Bucle** activo, la reproducción repite la selección o el bucle de la celda.

## Edición y mezcla

En **Edición/Mezcla** (cada operación es una celda nueva, el original no cambia): **Extraer**, **Recortar**, **Silenciar selección**, **Fundidos**, **Invertir polaridad**, **Ganancia** y **Normalizar**. Los tiempos se toman de la selección del visor.

- **Mezcla nueva**: una fila con un nodo de mezcla; cada entrada tiene ganancia, paneo e inicio.
- **Editar en editor externo**: abre una copia de la celda en el editor elegido en Preferencias (por ejemplo Audacity); al cerrarlo, si cambió, vuelve como fuente nueva.

## Patches de texto

**Patch → Guardar patch…** (`Ctrl+S`) escribe el patch como texto legible y editable (`.sspatch`): una línea por celda con su proceso, sus valores, curvas y entradas. Con **Incluir fuentes**, los archivos se copian junto al patch. **Cargar patch…** (`Ctrl+O`) lo trae a la sesión; lo que ya esté en la cache no se vuelve a calcular.

**Exportar como script .bat** escribe los comandos de las celdas de CDP (canal por canal) para ejecutarlos fuera de SURSHAPE.

## Marcadores, bulk y sub-patches

### Marcadores

Los marcadores son de la **fuente** de cada fila (una rama usa los de la fila de donde nace) y se ven en el visor como líneas con su nombre; las regiones, además, con una franja en la regla.

- **Marcadores → Importar etiquetas de Audacity…**: lee un archivo de etiquetas exportado por Audacity (`Archivo → Exportar → Exportar etiquetas`). **Exportar etiquetas…** hace lo inverso.
- **Marcador en el cursor** (`M`) y **Región con la selección**.
- **Anterior / siguiente** (`,` y `.`, o las flechas de "Marc." en el transporte): mueven el cursor; si el marcador es una región, la deja elegida. Con **Procesar: Selección**, el proceso que agregues toma solo ese fragmento.
- En el **Graph-Edit**, **← Desde marcadores** pone un punto en cada marcador y **→ A marcadores** crea un marcador en cada punto.

### Bulk

Para procesar muchos archivos con la misma cadena: elige una celda de la fila modelo y usa **Patch → Bulk: aplicar la fila a varios archivos…**, o marca la casilla **Bulk** y arrastra los archivos. Cada archivo abre una fila con la misma cadena. **Ejecutar el patch completo** los calcula todos y **Patch → Guardar el final de cada fila…** guarda la última celda de cada fila en una carpeta.

### Sub-patches

Una **receta** (Patch → Guardar la fila como receta) puede entrar como **una sola celda**: **Patch → Insertar receta como sub-patch**. Sus pasos quedan guardados en la celda (si la receta cambia después, el resultado no cambia) y se calculan uno por uno, cada uno en la cache. Su página muestra los pasos; **Abrir el sub-patch como fila** los pone en una fila nueva para editarlos.

## Info y consola

- **Info de la celda**: frecuencia, canales, duración en segundos y muestras, pico, RMS y continua por canal, tipo y tamaño.
- **Info → Información de CDP**: propiedades, muestra máxima, duración y canal más fuerte (`sndinfo`).
- **Consola** (`Ctrl+K`): cada ejecución con hora, celda, programa, código, tiempo y archivos; al elegir una, su comando y su salida completa.

{{CAPTURA:consola}}

## Preferencias

{{CAPTURA:preferencias}}

- **Rutas**: carpeta de CDP, carpeta temporal, carpeta de sesiones, editor externo; carpetas de presets, recetas y patches.
- **PVOC**: puntos y superposición del análisis automático.
- **Audio**: dispositivo de salida.
- **Unidades e idioma**: segundos, muestras o h:m:s.ms; español o inglés.
- **Comportamiento**: volver tras RENDER, limitador y formato de archivo.
- **Accesibilidad**: zoom de la interfaz (100, 125 o 150 %).

## Atajos de teclado

{{ATAJOS}}

## Catálogo de procesos

{{CATALOGO}}

## Archivos y carpetas

- **Sesión**: una carpeta con `sesion.json` (el patch), `cache` (los renders), `celdas` (los archivos de celda) y `editados` (copias para el editor externo).
- **Preferencias**, **presets**, **recetas** y **patches**: en `%APPDATA%\SURSHAPE\`.
- **Traducciones**: si junto a `SURSHAPE.exe` hay una carpeta `locales` con `es.lang` o `en.lang`, esos textos reemplazan a los incluidos.

## Solución de problemas

- **No suena**: revisa el dispositivo en **Preferencias → Audio**.
- **Una celda de CDP da error**: la página de parámetros dice por qué y **Ver consola** muestra la salida de CDP.
- **«Un archivo de origen cambió»**: el archivo se modificó o se movió; si se movió, usa **Cambiar archivo…** en la página de la fuente.
- **«El resultado sería demasiado largo»**: cada render se calcula en memoria; baja el factor o usa una región más corta.

## Licencia

SURSHAPE es software libre bajo la GPL-3.0-or-later. CDP se distribuye bajo LGPL-2.1, como programas aparte y sin modificar. Los componentes de terceros y sus licencias están en **Ayuda → Acerca de**, en `THIRD_PARTY.md` y en la carpeta `licenses`. El código fuente completo acompaña a la distribución en la carpeta `codigo_fuente`.
