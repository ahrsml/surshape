# SURSHAPE manual

SURSHAPE is an **offline** sound design application for ambient and noise, in the spirit of Soundshaper and the Composers Desktop Project (CDP). It is not a plugin or a real-time effect: you take a sample, apply processes to it and get new files. The original is never modified.

The central idea is the **live patch**: each session is a spreadsheet of cells (sources, processes and mixes) that you can edit at any time. If you change something, SURSHAPE knows what is out of date and recomputes only that.

## First steps

- Open `SURSHAPE.exe`. The first time, an empty session is created in `Documents\SURSHAPE\Sesiones\`.
- Drag one or more WAV, AIFF or FLAC files onto the window (or **Session → Import audio…**, `Ctrl+I`). Each file takes column 0 of a new grid row.
- Choose a process from a menu (for example **Spectral**) or by double-clicking it in the **Processes** section of the sidebar. It is added to the chosen cell's row and its **parameter page** opens.
- Adjust the values and press **RENDER**. The result appears in the viewer; listen with the space bar.
- Save the cell as a file with **Save file** (sidebar) or `Ctrl+E`, as 24-bit or 32-bit float WAV.

Everything is saved automatically: the session (`sesion.json`) is written whenever something changes and every render stays in the session's `cache` folder.

## The window

{{CAPTURA:principal}}

- **Menus** (top): Session, Edit, Edit/Mix, Soundfile/Time, Spectral, Pitch Data, Synth/Generators, Destruction, Filter/Space, Info, Data, Patch, Markers, Tools, Preferences and Help. The underlined letter opens the menu with `Alt`+letter; arrows move between menus.
- **Toolbar**: new session, open, save patch, import, undo · render cell, render all · play, stop · console, preferences. On the right, render progress and **Cancel**.
- **Sidebar** (left): coloured foldable sections — **Files** (sources and recent), **Processes** (catalogue with search), **Patches** (recipes and text patches) and **Save file** — and the **patch/cell controls**.
- **Viewer** (top centre): waveform or spectrogram of the chosen cell, with readouts in the chosen time units.
- **Transport** (below the viewer): the cell's file, A (input) / B (result), length, channels, peaks per channel; play, pause, stop, start, end, loop; vertical zoom and visible channels; selection start and end.
- **Quick buttons** (right): **Info**, **Level** (normalise), **Edit** (external editor), **Cut** (extract the selection), **Reset** (full zoom) and **Process: All / Selection**.
- **Grid** (bottom): the patch spreadsheet.
- **Help line**: explains any control under the pointer; otherwise it shows the last message (warnings with a triangle, errors with an octagon, always with text).
- **Status bar**: chosen cell, its input and output files, and its command.

The dividers between sidebar, viewer and grid can be dragged; SURSHAPE remembers the sizes. The minimum window is 1024x700.

## The grid

Rows are **A** to **P** and columns **0** to **98**. Column 0 of each row is its origin: a file, a generator, a mix or a **reference** to another cell (`→A_2`), which is how a branch looks. The following cells are the processes, left to right: each takes the previous cell as input.

Each cell shows the process name, the **file type** it produces (`wav`, `ana`…), "auto" if it converts its input by itself, and its **state** as an icon (the text appears in the help line):

- **Source**: an original file. Never modified.
- **Not rendered**: not computed yet.
- **Rendered**: its result matches the current values.
- **Out of date**: something changed in the cell or earlier in the chain.
- **Queued** / **Processing**: part of the current render.
- **Error**: the last attempt failed; the parameter page explains why and the console shows the full output.

Click: choose the cell and load it in the viewer. Double click or `Enter`: parameter page. Right click: cell operations. `Shift`+click: a stretch of cells in the row. Arrows: move around.

Each result is identified by everything that produces it (process, values, curves, seed, region, inputs). If you go back to values you already used, the result comes from the cache **instantly**.

### Cell files

Each cell is a file: `~A_0.wav`, `~A_1.ana`, `~A_3.wav`… They are in the session's `celdas` folder (**Data → Open the cells folder**), linked to the cache without using more disk. They are temporary: to keep one, use **Save file**.

### Cell operations

- **Re-edit**: opens the parameter page.
- **Replace**: changes the cell's process keeping its place and inputs (choose the new process from a menu; `Esc` cancels).
- **Copy** / **Paste** (`Ctrl+C` / `Ctrl+V`): the cell, the chosen stretch or, with the **Copy row** box, the whole row. It is pasted at the end of the chosen row.
- **Branch** (`Ctrl+B`): a new row starting from the cell, to try another path.
- **Delete** (`Del`): if other cells use it, it asks and deletes them too.
- **Clear**: deletes from the cell to the end of the row.
- **Undo** (`Ctrl+Z`) takes the patch back; **Undo in this cell** takes only that cell back to its previous values. Renders come back instantly from the cache.

### Run

- **Run cell** (`Ctrl+R`): the cell and whatever it needs before it.
- **Run from here** (`Ctrl+Alt+R`): the cell and every cell that depends on it.
- **Run whole patch** (`Ctrl+Shift+R`).
- **Run row**. The **Single row** and **Whole patch** boxes change what the sidebar **Run** button does.

Rendering runs in the background: the interface stays available and **Cancel** stops it without saving anything half-done.

## File types and CDP

As in Soundshaper, CDP file types are visible: `.wav` (sound), `.ana` (spectral analysis), `.frq` (pitch), `.for` (formants), `.env` (envelope), `.brk` (breakpoints), `.txt` (text).

- **PVOC analysis** turns sound into `.ana`; **PVOC resynthesis** goes back to sound. The processes in the **Spectral** menu read and write `.ana`.
- **Automatic conversion**: if a spectral process receives sound, SURSHAPE analyses it by itself (with **Preferences → PVOC**) and the cell says "auto". The analysis stays in the cache.
- An `.ana` cell also keeps its resynthesis: you see and hear it in the viewer, and sound processes after it use it.
- CDP works in mono: each channel is processed separately and data is kept one file per channel.

SURSHAPE runs CDP programs as external programs. The distribution includes in the `cdp` folder its own unmodified build of the official CDP8 code; another installation can be chosen in **Preferences → Paths**. If CDP is missing, the rest of SURSHAPE works the same.

## Parameter page

{{CAPTURA:parametros}}

- At the top, the cell's **input** (with its origin if it was converted automatically), playable.
- **Mode**: CDP programs with modes show a selector; parameters change with the mode.
- **Input 2**: two-sound processes (marked "(2 sounds)") use the same row's sound by default; change it in the list or with **Pick in the grid**.
- **Parameters**: name, original CDP name, value (field with arrows: you can type the exact value), slider, minimum, maximum, default and unit.
- **T-V** (time-varying): turns the value into a curve and opens **Graph-Edit**.
- **Random**: each parameter's own range for **Randomize**.
- Tabs **Seed** (seed, iterations, region), **Variants** (randomize and previews), **Snapshots/Presets** and **Per channel**.
- **Command line**: the exact CDP commands (with the automatic conversion) or the native process descriptor; it can be copied.
- **RENDER** computes and goes back to the main page (can be changed in Preferences); **Preview** computes and plays without leaving; **Back** returns without computing.

### Graph-Edit

The curve is drawn over the input waveform. Click on an empty spot adds a point; dragging moves it; right click removes it. The chosen point also moves with the **X** and **Y** sliders. Below: the point table, **absolute** (seconds) or **normalised** time (0 = start, 1 = end), the shape between points, preset shapes (ramps, exponential, logarithmic, random with seed) and **Import / Export** in CDP format ("time value" per line). **OK** returns to the parameters, **Cancel** leaves the value as it was.

### Seed, variants, snapshots and presets

- The **seed** makes randomness reproducible: the same seed gives the same result.
- **Randomize** moves values within their Random range; **Previews** renders several short versions to listen to and choose from.
- **Snapshots**: value sets saved in the cell. **Presets**: files per process, valid in any session; one can be the **Default**.
- **Recipes** (**Patch** menu): save a row's process chain to apply it to another source.

## The viewer

- **Waveform / Spectrogram** (logarithmic frequency scale).
- **A input / B result** (`B` key): switches without losing the playback position.
- **Selection**: drag over the waveform; **Start** and **End** show and edit it. With **Process: Selection**, new processes take only that stretch.
- **Zoom**: mouse wheel (`Shift`+wheel scrolls, `Ctrl`+wheel is vertical zoom), `+`, `−` and `0`.
- **Channels**: the C1, C2… boxes show or hide each channel.
- **Loops**: **Set** saves the selection as the cell's loop and **Get** brings it back; LoopPt and Endtime (Edit/Mix menu) set start and end at the cursor. With **Loop** on, playback repeats the selection or the cell's loop.

## Edit and mix

In **Edit/Mix** (each operation is a new cell, the original does not change): **Extract**, **Trim**, **Silence selection**, **Fades**, **Invert polarity**, **Gain** and **Normalize**. Times are taken from the viewer selection.

- **New mix**: a row with a mix node; each input has gain, pan and start.
- **Edit in external editor**: opens a copy of the cell in the editor chosen in Preferences (for example Audacity); when closed, if it changed, it comes back as a new source.

## Text patches

**Patch → Save patch…** (`Ctrl+S`) writes the patch as readable, editable text (`.sspatch`): one line per cell with its process, values, curves and inputs. With **Include sources**, the files are copied next to the patch. **Load patch…** (`Ctrl+O`) brings it into the session; whatever is already in the cache is not recomputed.

**Export as .bat script** writes the commands of the CDP cells (channel by channel) to run them outside SURSHAPE.

## Markers, bulk and sub-patches

### Markers

Markers belong to each row's **source** (a branch uses those of the row it comes from) and show in the viewer as lines with their name; regions also have a band on the ruler.

- **Markers → Import Audacity labels…**: reads a labels file exported by Audacity (`File → Export → Export Labels`). **Export labels…** does the opposite.
- **Marker at cursor** (`M`) and **Region from selection**.
- **Previous / next** (`,` and `.`, or the "Mark." arrows in the transport): move the cursor; if the marker is a region, it gets selected. With **Process: Selection**, the process you add takes only that fragment.
- In **Graph-Edit**, **← From markers** puts a point at each marker and **→ To markers** creates a marker at each point.

### Bulk

To process many files with the same chain: choose a cell in the model row and use **Patch → Bulk: apply the row to several files…**, or tick the **Bulk** box and drag the files. Each file opens a row with the same chain. **Run whole patch** computes them all and **Patch → Save the end of every row…** saves each row's last cell to a folder.

### Sub-patches

A **recipe** (Patch → Save row as recipe) can go in as **a single cell**: **Patch → Insert recipe as sub-patch**. Its steps are stored in the cell (if the recipe changes later, the result does not) and are computed one by one, each cached. Its page shows the steps; **Open the sub-patch as a row** puts them in a new row for editing.

## Info and console

- **Cell info**: sample rate, channels, length in seconds and samples, peak, RMS and DC per channel, type and size.
- **Info → CDP information**: properties, maximum sample, length and loudest channel (`sndinfo`).
- **Console** (`Ctrl+K`): every run with time, cell, program, code, duration and files; choose one to see its command and full output.

{{CAPTURA:consola}}

## Preferences

{{CAPTURA:preferencias}}

- **Paths**: CDP folder, temporary folder, sessions folder, external editor; presets, recipes and patches folders.
- **PVOC**: points and overlap of the automatic analysis.
- **Audio**: output device.
- **Units and language**: seconds, samples or h:m:s.ms; Spanish or English.
- **Behaviour**: go back after RENDER, limiter and file format.
- **Accessibility**: interface zoom (100, 125 or 150 %).

## Keyboard shortcuts

{{ATAJOS}}

## Process catalogue

{{CATALOGO}}

## Files and folders

- **Session**: a folder with `sesion.json` (the patch), `cache` (renders), `celdas` (cell files) and `editados` (copies for the external editor).
- **Preferences**, **presets**, **recipes** and **patches**: in `%APPDATA%\SURSHAPE\`.
- **Translations**: if there is a `locales` folder with `es.lang` or `en.lang` next to `SURSHAPE.exe`, those texts replace the built-in ones.

## Troubleshooting

- **No sound**: check the device in **Preferences → Audio**.
- **A CDP cell gives an error**: the parameter page says why and **Show console** shows CDP's output.
- **"A source file changed"**: the file was modified or moved; if it moved, use **Change file…** on the source page.
- **"The result would be too long"**: each render is computed in memory; lower the factor or use a shorter region.

## Licence

SURSHAPE is free software under the GPL-3.0-or-later. CDP is distributed under LGPL-2.1, as separate, unmodified programs. Third-party components and their licences are in **Help → About**, in `THIRD_PARTY.md` and in the `licenses` folder. The complete source code comes with the distribution in the `codigo_fuente` folder.
