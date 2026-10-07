//! SURSHAPE · patch vivo · por @ahrsml
//!
//! La sesión contiene un [`Patch`]: un grafo dirigido acíclico de nodos
//! ([`Node`]). Cada nodo es una fuente (archivo, nunca se modifica), un
//! proceso, una mezcla o un sub-patch, con sus parámetros, seed y entradas
//! (`nodo:salida`, [`PortRef`]).
//!
//! - **Clave de cache** ([`Patch::keys`]): hash estable de proceso + versión +
//!   parámetros completos (breakpoints incluidos) + seed (si el proceso la
//!   usa) + región + opciones de render + claves de las entradas. Es un
//!   árbol de Merkle: cambiar algo arriba cambia la clave de todo lo que
//!   depende de ello.
//! - **Estado** ([`Patch::status`]): se deriva comparando la clave actual con
//!   la del último render; nunca se marca a mano.
//! - **Plan** ([`Patch::render_plan`]): nodos a renderizar, en orden
//!   topológico, para obtener los nodos pedidos (o todo lo desactualizado).
//! - **Grilla** ([`Row`]): vista separada del grafo. Cada fila es una cadena
//!   con su fuente; las celdas son nodos de izquierda a derecha. Una entrada
//!   secundaria puede apuntar a una celda de cualquier fila.
//!
//! Nada de esto toca disco: la persistencia y la cache en disco están en
//! `surshape-session`.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use surshape_engine::{FileKind, ParamSet, Process, Registry, RenderOptions, StableHasher};

pub mod text;

pub type NodeId = u32;

/// Una salida de un nodo.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PortRef {
    pub nodo: NodeId,
    #[serde(default)]
    pub salida: u16,
}

impl PortRef {
    pub fn main(nodo: NodeId) -> Self {
        Self { nodo, salida: 0 }
    }
}

/// Tramo de la entrada principal que se procesa (en muestras).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Region {
    pub inicio: u64,
    pub fin: u64,
}

/// Marcador sobre una fuente (fase 7: importables desde Audacity).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Marker {
    /// Segundos.
    pub t: f64,
    /// Fin en segundos si es una región.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fin: Option<f64>,
    #[serde(default)]
    pub etiqueta: String,
}

/// Archivo de origen. `hash` identifica su contenido (bytes del archivo).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SourceInfo {
    pub ruta: PathBuf,
    pub hash: String,
    pub sr: u32,
    pub canales: u16,
    pub frames: u64,
    /// Tamaño y fecha de modificación del archivo cuando se calculó `hash`:
    /// si cambian, hay que recalcularlo (y todo lo que depende queda
    /// desactualizado).
    #[serde(default)]
    pub bytes: u64,
    #[serde(default)]
    pub modificado: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub marcadores: Vec<Marker>,
}

/// Entrada de una mezcla (fase 5).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct MixInput {
    pub ganancia_db: f64,
    /// -1 (izquierda) .. 1 (derecha).
    pub paneo: f64,
    /// Segundos de retraso al inicio.
    pub inicio: f64,
}

impl Default for MixInput {
    fn default() -> Self {
        Self { ganancia_db: 0.0, paneo: 0.0, inicio: 0.0 }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum NodeKind {
    Fuente(SourceInfo),
    Proceso { proceso: String },
    /// Una configuración por entrada, en el mismo orden que `entradas`.
    Mezcla { canales: Vec<MixInput> },
    /// Una cadena de procesos usada como un solo nodo (sub-patch). Los pasos
    /// van guardados en el nodo (copiados de la receta `plantilla`): así el
    /// resultado no cambia aunque la receta cambie después.
    SubPatch {
        plantilla: PathBuf,
        #[serde(default)]
        nombre: String,
        #[serde(default)]
        pasos: Vec<NodeTemplate>,
    },
}

/// Juego de parámetros guardado con nombre (fase 4).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub nombre: String,
    pub params: ParamSet,
    pub seed: u64,
}

/// Una salida ya renderizada.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OutputInfo {
    /// Sonido de la salida, relativo a la carpeta de la sesión. Si la salida
    /// son datos (.ana), es su resíntesis (para verla y escucharla).
    pub archivo: PathBuf,
    pub sr: u32,
    pub canales: u16,
    pub frames: u64,
    /// Tipo de archivo de la salida ("ana"...); None = sonido (.wav).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tipo: Option<String>,
    /// Archivos de datos, uno por canal (relativos a la sesión).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub datos: Vec<PathBuf>,
}

impl OutputInfo {
    /// Tipo de archivo de la salida.
    pub fn kind(&self) -> FileKind {
        self.tipo.as_deref().and_then(FileKind::from_ext).unwrap_or(FileKind::Wav)
    }
}

/// Último render de un nodo.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RenderRecord {
    /// Clave con la que se hizo: si la actual difiere, está desactualizado.
    pub clave: String,
    pub salidas: Vec<OutputInfo>,
    /// Avisos de la validación.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub avisos: Vec<Notice>,
    #[serde(default)]
    pub segundos: f64,
    /// Versión del motor externo usado (CDP), para el linaje.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version_motor: Option<String>,
}

/// Mensaje traducible guardado: clave i18n + argumentos `{nombre}`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Notice {
    pub mensaje: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<(String, String)>,
}

impl Notice {
    pub fn new(mensaje: &str, args: &[(&str, String)]) -> Self {
        Self { mensaje: mensaje.to_string(), args: args.iter().map(|(k, v)| (k.to_string(), v.clone())).collect() }
    }
}

/// Último error de un nodo, con la clave de cache con la que ocurrió (si la
/// clave cambia, el error deja de valer).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NodeError {
    pub clave: String,
    pub aviso: Notice,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub tipo: NodeKind,
    #[serde(default)]
    pub params: ParamSet,
    #[serde(default)]
    pub seed: u64,
    #[serde(default)]
    pub entradas: Vec<PortRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<Region>,
    /// Meta-proceso "iterar": el proceso se aplica N veces sobre su propio
    /// resultado (1 = una vez, lo normal).
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub iteraciones: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub snapshots: Vec<Snapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub render: Option<RenderRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<NodeError>,
    /// Bucle de la celda (Get/Set Loops), en muestras de su salida. No
    /// cambia el sonido: no entra en la clave.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bucle: Option<Region>,
    /// Rango propio de cada parámetro para "aleatorizar" (id -> (mín, máx)).
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub aleatorio: std::collections::BTreeMap<String, (f64, f64)>,
}

impl Node {
    pub fn process_id(&self) -> Option<&str> {
        match &self.tipo {
            NodeKind::Proceso { proceso } => Some(proceso),
            _ => None,
        }
    }
    pub fn source(&self) -> Option<&SourceInfo> {
        match &self.tipo {
            NodeKind::Fuente(s) => Some(s),
            _ => None,
        }
    }
}

fn one() -> u32 {
    1
}
fn is_one(n: &u32) -> bool {
    *n == 1
}

/// Filas de la grilla tipo planilla (A..P).
pub const MAX_ROWS: usize = 16;
/// Columnas (0..98).
pub const MAX_COLS: usize = 99;

/// Letra de una fila: A, B, ... P (y más allá, AA, AB... por si un patch
/// viejo tiene más filas).
pub fn row_name(r: usize) -> String {
    const L: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    if r < 26 {
        (L[r] as char).to_string()
    } else {
        format!("{}{}", L[(r / 26 - 1).min(25)] as char, L[r % 26] as char)
    }
}

/// Índice de fila de una letra ("A" = 0).
pub fn row_index(name: &str) -> Option<usize> {
    let b = name.as_bytes();
    let v = |c: u8| c.is_ascii_uppercase().then(|| (c - b'A') as usize);
    match b.len() {
        1 => v(b[0]),
        2 => Some((v(b[0])? + 1) * 26 + v(b[1])?),
        _ => None,
    }
}

/// Una fila de la grilla: una cadena con su fuente. Una fila puede nacer de
/// otra celda (rama) o de una salida extra de un proceso con varias salidas:
/// entonces `origen` dice de dónde, y su primera celda toma eso de entrada.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Row {
    pub celdas: Vec<NodeId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origen: Option<PortRef>,
}

/// Una entrada de una celda copiada: dentro del tramo copiado (índice) o
/// fuera de él (se conserva la referencia).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TemplateInput {
    Interna { indice: usize, salida: u16 },
    Externa(PortRef),
}

/// Una celda copiada (o guardada en una receta), sin id ni render.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NodeTemplate {
    pub tipo: NodeKind,
    #[serde(default)]
    pub params: ParamSet,
    #[serde(default)]
    pub seed: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<Region>,
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub iteraciones: u32,
    /// Entradas a partir de la segunda (la primera es la celda anterior).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub otras_entradas: Vec<TemplateInput>,
}

impl NodeTemplate {
    /// Proceso del paso (None si no es un proceso).
    pub fn tipo_proceso(&self) -> Option<&str> {
        match &self.tipo {
            NodeKind::Proceso { proceso } => Some(proceso),
            _ => None,
        }
    }
}

/// Receta: una cadena de procesos lista para aplicar a otra fuente.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Recipe {
    pub version: u32,
    pub nombre: String,
    pub pasos: Vec<NodeTemplate>,
}

/// Por qué no se puede calcular la clave de un nodo.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyError {
    /// Proceso que no existe en este catálogo (p. ej. CDP no instalado).
    UnknownProcess(String),
    /// Falta una entrada o apunta a un nodo inexistente.
    MissingInput,
    /// La cantidad de entradas no es la que el proceso pide.
    WrongInputs,
    /// Función todavía no implementada (sub-patch).
    NotImplemented,
    /// Una entrada no tiene clave (error aguas arriba).
    Upstream,
    Cycle,
}

impl KeyError {
    /// Clave i18n.
    pub fn i18n_key(&self) -> &'static str {
        match self {
            KeyError::UnknownProcess(_) => "err.patch.proceso_desconocido",
            KeyError::MissingInput => "err.patch.falta_entrada",
            KeyError::WrongInputs => "err.patch.entradas",
            KeyError::NotImplemented => "err.patch.no_implementado",
            KeyError::Upstream => "err.patch.entrada_con_error",
            KeyError::Cycle => "err.patch.ciclo",
        }
    }
}

/// Error al editar el patch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PatchError {
    NoSuchNode,
    /// La conexión crearía un ciclo.
    Cycle,
    /// Otros nodos dependen del que se quiere borrar.
    HasDependents,
}

impl PatchError {
    pub fn i18n_key(&self) -> &'static str {
        match self {
            PatchError::NoSuchNode => "err.patch.no_existe",
            PatchError::Cycle => "err.patch.ciclo",
            PatchError::HasDependents => "err.patch.dependientes",
        }
    }
}

/// Estado de una celda (derivado). "En proceso" lo agrega la interfaz.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// Fuente: siempre disponible.
    Fuente,
    /// Proceso nunca renderizado.
    SinRender,
    Renderizado,
    /// Cambió algo (en el nodo o aguas arriba) desde el último render.
    Desactualizado,
    /// El último intento con la clave actual falló, o no se puede calcular.
    Error,
}

/// Claves de todos los nodos.
pub type Keys = HashMap<NodeId, Result<String, KeyError>>;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Patch {
    pub nodos: Vec<Node>,
    pub filas: Vec<Row>,
    pub siguiente_id: NodeId,
}

impl Patch {
    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodos.iter().find(|n| n.id == id)
    }

    pub fn node_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.nodos.iter_mut().find(|n| n.id == id)
    }

    fn new_node(&mut self, tipo: NodeKind, entradas: Vec<PortRef>) -> NodeId {
        self.siguiente_id = self.siguiente_id.max(1);
        let id = self.siguiente_id;
        self.siguiente_id += 1;
        self.nodos.push(Node {
            id,
            tipo,
            params: ParamSet::default(),
            seed: 0,
            entradas,
            region: None,
            iteraciones: 1,
            snapshots: Vec::new(),
            render: None,
            error: None,
            bucle: None,
            aleatorio: Default::default(),
        });
        id
    }

    /// Agrega una fuente en una fila nueva. Devuelve (nodo, fila).
    pub fn add_source(&mut self, info: SourceInfo) -> (NodeId, usize) {
        let id = self.new_node(NodeKind::Fuente(info), Vec::new());
        self.filas.push(Row { celdas: vec![id], origen: None });
        (id, self.filas.len() - 1)
    }

    /// Agrega un generador (proceso sin entradas: ruido...) en una fila
    /// nueva. Devuelve (nodo, fila).
    pub fn add_generator(&mut self, proc_: &dyn Process, seed: u64) -> (NodeId, usize) {
        let id = self.new_node(NodeKind::Proceso { proceso: proc_.id().to_string() }, Vec::new());
        if let Some(n) = self.node_mut(id) {
            n.params = ParamSet::defaults(proc_.params());
            n.seed = seed;
        }
        self.filas.push(Row { celdas: vec![id], origen: None });
        (id, self.filas.len() - 1)
    }

    /// Lo que alimenta una celda nueva al final de la fila: su última celda
    /// o, si está vacía, su origen.
    pub fn row_tail(&self, row: usize) -> Option<PortRef> {
        let r = self.filas.get(row)?;
        r.celdas.last().map(|&c| PortRef::main(c)).or(r.origen)
    }

    /// Agrega un proceso al final de la fila `row`, tomando como entrada
    /// principal la última celda de esa fila (o su origen).
    pub fn append(&mut self, row: usize, proc_: &dyn Process, seed: u64) -> Option<NodeId> {
        let last = self.row_tail(row)?;
        let id = self.new_node(NodeKind::Proceso { proceso: proc_.id().to_string() }, vec![last]);
        let n = self.node_mut(id)?;
        n.params = ParamSet::defaults(proc_.params());
        n.seed = seed;
        self.filas[row].celdas.push(id);
        Some(id)
    }

    /// Fila nueva que nace de `from` (rama o salida extra). Si ya existe una
    /// fila vacía con ese origen, se reutiliza.
    pub fn add_branch(&mut self, from: PortRef) -> Option<usize> {
        self.node(from.nodo)?;
        if let Some(i) = self.filas.iter().position(|r| r.origen == Some(from) && r.celdas.is_empty()) {
            return Some(i);
        }
        let after = self.cell_of(from.nodo).map_or(self.filas.len(), |(r, _)| r + 1);
        let pos = (after..self.filas.len()).find(|&i| self.filas[i].origen.is_none_or(|o| o.nodo != from.nodo)).unwrap_or(self.filas.len());
        self.filas.insert(pos, Row { celdas: Vec::new(), origen: Some(from) });
        Some(pos)
    }

    /// Una fila por cada salida extra (1..n) de un nodo, si todavía no hay.
    /// Devuelve cuántas creó.
    pub fn ensure_output_rows(&mut self, id: NodeId) -> usize {
        let n = self.node(id).and_then(|n| n.render.as_ref()).map_or(0, |r| r.salidas.len());
        let mut created = 0;
        for k in 1..n {
            let port = PortRef { nodo: id, salida: k as u16 };
            if !self.filas.iter().any(|r| r.origen == Some(port)) {
                self.add_branch(port);
                created += 1;
            }
        }
        created
    }

    /// Agrega un sub-patch (una cadena de procesos como un solo nodo) al
    /// final de la fila `row`. Solo se guardan los pasos de proceso con
    /// entradas internas.
    pub fn append_subpatch(&mut self, row: usize, nombre: &str, plantilla: PathBuf, pasos: &[NodeTemplate]) -> Option<NodeId> {
        let last = self.row_tail(row)?;
        let pasos: Vec<NodeTemplate> = pasos
            .iter()
            .filter(|t| matches!(t.tipo, NodeKind::Proceso { .. }))
            .cloned()
            .map(|mut t| {
                t.otras_entradas.retain(|e| matches!(e, TemplateInput::Interna { .. }));
                t
            })
            .collect();
        let id = self.new_node(NodeKind::SubPatch { plantilla, nombre: nombre.to_string(), pasos }, vec![last]);
        self.filas[row].celdas.push(id);
        Some(id)
    }

    /// Patch de una sola fila con los pasos de un sub-patch sobre `fuente`
    /// (el runner lo usa para calcularlo). Devuelve el patch y los nodos de
    /// los pasos, en orden.
    pub fn from_steps(fuente: SourceInfo, pasos: &[NodeTemplate]) -> (Patch, Vec<NodeId>) {
        let mut p = Patch::default();
        let (_, row) = p.add_source(fuente);
        let ids = p.paste(row, pasos);
        (p, ids)
    }

    /// Agrega un nodo de mezcla en una fila nueva (sin entradas aún).
    pub fn add_mix(&mut self) -> (NodeId, usize) {
        let id = self.new_node(NodeKind::Mezcla { canales: Vec::new() }, Vec::new());
        self.filas.push(Row { celdas: vec![id], origen: None });
        (id, self.filas.len() - 1)
    }

    /// Quita la entrada `slot` (mezclas: también su configuración).
    pub fn remove_input(&mut self, node: NodeId, slot: usize) {
        if let Some(n) = self.node_mut(node) {
            if slot < n.entradas.len() {
                n.entradas.remove(slot);
            }
            if let NodeKind::Mezcla { canales } = &mut n.tipo {
                if slot < canales.len() {
                    canales.remove(slot);
                }
            }
        }
    }

    /// Cambia el archivo de una fuente: todo lo que depende queda
    /// desactualizado (re-ejecutar el patch con otra fuente).
    pub fn set_source(&mut self, id: NodeId, info: SourceInfo) -> bool {
        match self.node_mut(id).map(|n| &mut n.tipo) {
            Some(NodeKind::Fuente(s)) => {
                *s = info;
                true
            }
            _ => false,
        }
    }

    /// Copia un tramo de celdas (en orden) como plantillas.
    pub fn copy_cells(&self, ids: &[NodeId]) -> Vec<NodeTemplate> {
        ids.iter()
            .filter_map(|id| self.node(*id))
            .map(|n| NodeTemplate {
                tipo: n.tipo.clone(),
                params: n.params.clone(),
                seed: n.seed,
                region: n.region,
                iteraciones: n.iteraciones,
                otras_entradas: n
                    .entradas
                    .iter()
                    .skip(1)
                    .map(|p| match ids.iter().position(|x| *x == p.nodo) {
                        Some(i) => TemplateInput::Interna { indice: i, salida: p.salida },
                        None => TemplateInput::Externa(*p),
                    })
                    .collect(),
            })
            .collect()
    }

    /// Pega plantillas al final de la fila `row`, encadenadas. Las fuentes
    /// copiadas no se pegan (una fila tiene una sola fuente). Devuelve los
    /// nodos nuevos.
    pub fn paste(&mut self, row: usize, items: &[NodeTemplate]) -> Vec<NodeId> {
        let mut created: Vec<NodeId> = Vec::new();
        let mut map: Vec<Option<NodeId>> = Vec::new();
        for t in items {
            if matches!(t.tipo, NodeKind::Fuente(_)) {
                map.push(None);
                continue;
            }
            let Some(prev) = self.row_tail(row) else { break };
            let first = match t.tipo {
                NodeKind::Mezcla { .. } => Vec::new(),
                _ => vec![prev],
            };
            let id = self.new_node(t.tipo.clone(), first);
            let extra: Vec<PortRef> = t
                .otras_entradas
                .iter()
                .filter_map(|i| match i {
                    TemplateInput::Interna { indice, salida } => map.get(*indice).copied().flatten().map(|n| PortRef { nodo: n, salida: *salida }),
                    TemplateInput::Externa(p) => self.node(p.nodo).map(|_| *p),
                })
                .collect();
            if let Some(n) = self.node_mut(id) {
                n.params = t.params.clone();
                n.seed = t.seed;
                n.region = t.region;
                n.iteraciones = t.iteraciones.max(1);
                n.entradas.extend(extra);
            }
            self.filas[row].celdas.push(id);
            map.push(Some(id));
            created.push(id);
        }
        created
    }

    /// Receta con los procesos de una fila (sin su fuente). Las entradas
    /// secundarias que apuntan fuera de la fila no viajan: al aplicarla hay
    /// que volver a elegirlas.
    pub fn recipe_from_row(&self, row: usize, nombre: &str) -> Option<Recipe> {
        let r = self.filas.get(row)?;
        let ids: Vec<NodeId> = r.celdas.iter().copied().filter(|id| self.node(*id).is_some_and(|n| n.source().is_none())).collect();
        let mut pasos = self.copy_cells(&ids);
        for p in &mut pasos {
            p.otras_entradas.retain(|i| matches!(i, TemplateInput::Interna { .. }));
        }
        Some(Recipe { version: 1, nombre: nombre.to_string(), pasos })
    }

    /// Fila y columna visibles (tipo planilla) de un nodo: la columna 0 es
    /// la fuente de la fila o, si la fila nace de otra celda, la referencia
    /// a esa celda; entonces las celdas empiezan en la columna 1.
    pub fn grid_pos(&self, id: NodeId) -> Option<(usize, usize)> {
        let (r, c) = self.cell_of(id)?;
        Some((r, c + usize::from(self.filas[r].origen.is_some())))
    }

    /// Nodo en la fila y columna visibles (None en la columna 0 de una fila
    /// con origen, o fuera de la fila).
    pub fn at(&self, row: usize, col: usize) -> Option<NodeId> {
        let r = self.filas.get(row)?;
        let off = usize::from(r.origen.is_some());
        col.checked_sub(off).and_then(|c| r.celdas.get(c).copied())
    }

    /// Nombre de celda tipo planilla: "A_3".
    pub fn cell_label(&self, id: NodeId) -> Option<String> {
        self.grid_pos(id).map(|(r, c)| format!("{}_{c}", row_name(r)))
    }

    /// Fila y columna de un nodo en la grilla.
    pub fn cell_of(&self, id: NodeId) -> Option<(usize, usize)> {
        self.filas.iter().enumerate().find_map(|(r, row)| row.celdas.iter().position(|&c| c == id).map(|c| (r, c)))
    }

    /// Nodos que usan directamente alguna salida de `id`.
    pub fn direct_dependents(&self, id: NodeId) -> impl Iterator<Item = &Node> {
        self.nodos.iter().filter(move |n| n.entradas.iter().any(|p| p.nodo == id))
    }

    /// Todos los nodos aguas abajo de `id` (sin incluirlo).
    pub fn dependents(&self, id: NodeId) -> BTreeSet<NodeId> {
        let mut out = BTreeSet::new();
        let mut stack = vec![id];
        while let Some(x) = stack.pop() {
            for n in self.direct_dependents(x) {
                if out.insert(n.id) {
                    stack.push(n.id);
                }
            }
        }
        out
    }

    /// Todos los nodos aguas arriba de `id` (sin incluirlo).
    pub fn upstream(&self, id: NodeId) -> BTreeSet<NodeId> {
        let mut out = BTreeSet::new();
        let mut stack = vec![id];
        while let Some(x) = stack.pop() {
            for p in self.node(x).map(|n| n.entradas.as_slice()).unwrap_or(&[]) {
                if out.insert(p.nodo) {
                    stack.push(p.nodo);
                }
            }
        }
        out
    }

    /// Conecta la entrada `slot` de `node` a `port` (agrega entradas si hace
    /// falta). Rechaza ciclos.
    pub fn connect(&mut self, node: NodeId, slot: usize, port: PortRef) -> Result<(), PatchError> {
        if self.node(port.nodo).is_none() || self.node(node).is_none() {
            return Err(PatchError::NoSuchNode);
        }
        if port.nodo == node || self.upstream(port.nodo).contains(&node) {
            return Err(PatchError::Cycle);
        }
        let n = self.node_mut(node).ok_or(PatchError::NoSuchNode)?;
        if n.entradas.len() <= slot {
            n.entradas.resize(slot + 1, port);
        }
        n.entradas[slot] = port;
        // Una mezcla lleva una configuración por entrada.
        if let NodeKind::Mezcla { canales } = &mut n.tipo {
            let len = n.entradas.len();
            canales.resize(len, MixInput::default());
        }
        Ok(())
    }

    /// Borra un nodo si nadie depende de él (y lo saca de la grilla; una
    /// fila sin celdas desaparece).
    pub fn remove(&mut self, id: NodeId) -> Result<(), PatchError> {
        if self.node(id).is_none() {
            return Err(PatchError::NoSuchNode);
        }
        if self.direct_dependents(id).next().is_some() {
            return Err(PatchError::HasDependents);
        }
        self.nodos.retain(|n| n.id != id);
        for r in &mut self.filas {
            r.celdas.retain(|&c| c != id);
        }
        // Filas vacías sin origen, o cuyo origen era este nodo, desaparecen.
        self.filas.retain(|r| !r.celdas.is_empty() || r.origen.is_some_and(|o| o.nodo != id));
        Ok(())
    }

    /// Orden topológico de todos los nodos (entradas antes que quienes las
    /// usan). Err si hay un ciclo (archivo dañado).
    pub fn topo_order(&self) -> Result<Vec<NodeId>, KeyError> {
        let mut indeg: HashMap<NodeId, usize> = self.nodos.iter().map(|n| (n.id, 0)).collect();
        for n in &self.nodos {
            let distinct: BTreeSet<NodeId> = n.entradas.iter().map(|p| p.nodo).filter(|i| indeg.contains_key(i)).collect();
            *indeg.get_mut(&n.id).unwrap() = distinct.len();
        }
        let mut ready: Vec<NodeId> = self.nodos.iter().filter(|n| indeg[&n.id] == 0).map(|n| n.id).collect();
        ready.reverse();
        let mut out = Vec::with_capacity(self.nodos.len());
        while let Some(id) = ready.pop() {
            out.push(id);
            let deps: BTreeSet<NodeId> = self.direct_dependents(id).map(|n| n.id).collect();
            for d in deps {
                let e = indeg.get_mut(&d).unwrap();
                *e -= 1;
                if *e == 0 {
                    ready.push(d);
                }
            }
        }
        if out.len() == self.nodos.len() {
            Ok(out)
        } else {
            Err(KeyError::Cycle)
        }
    }

    /// Clave de cache de cada nodo con el catálogo y las opciones de render
    /// actuales. Las fuentes valen por el hash de su contenido.
    pub fn keys(&self, reg: &Registry, opts: &RenderOptions) -> Keys {
        let mut keys: Keys = HashMap::new();
        let order = match self.topo_order() {
            Ok(o) => o,
            Err(e) => return self.nodos.iter().map(|n| (n.id, Err(e.clone()))).collect(),
        };
        for id in order {
            let n = self.node(id).expect("nodo del orden topológico");
            let k = self.key_of(n, reg, opts, &keys);
            keys.insert(id, k);
        }
        keys
    }

    fn key_of(&self, n: &Node, reg: &Registry, opts: &RenderOptions, keys: &Keys) -> Result<String, KeyError> {
        let mut h = StableHasher::new();
        let inputs = |h: &mut StableHasher| -> Result<(), KeyError> {
            h.u64(n.entradas.len() as u64);
            for p in &n.entradas {
                match keys.get(&p.nodo) {
                    None => return Err(KeyError::MissingInput),
                    Some(Err(_)) => return Err(KeyError::Upstream),
                    Some(Ok(k)) => {
                        h.str(k);
                        h.u64(p.salida as u64);
                    }
                }
            }
            Ok(())
        };
        match &n.tipo {
            NodeKind::Fuente(s) => {
                h.str("fuente"); // i18n-ok
                h.str(&s.hash);
            }
            NodeKind::Proceso { proceso } => {
                let p = reg.get(proceso).ok_or_else(|| KeyError::UnknownProcess(proceso.clone()))?;
                if n.entradas.len() < p.inputs().min() {
                    return Err(KeyError::MissingInput);
                }
                if !p.inputs().accepts(n.entradas.len()) {
                    return Err(KeyError::WrongInputs);
                }
                h.str("proceso"); // i18n-ok
                h.str(proceso);
                h.u64(p.version() as u64);
                h.str(&p.engine_version().unwrap_or_default());
                let mut params = n.params.completed(p.params());
                if !p.per_channel() {
                    params.por_canal = None;
                }
                params.hash_into(&mut h);
                h.u64(if p.uses_seed() { n.seed } else { 0 });
                match n.region {
                    None => h.u8(0),
                    Some(r) => {
                        h.u8(1);
                        h.u64(r.inicio);
                        h.u64(r.fin);
                    }
                }
                h.u64(n.iteraciones.max(1) as u64);
                opts.hash_into(&mut h);
                // Si lee .ana, el análisis de la auto-conversión cuenta.
                if p.input_kinds().iter().any(|k| *k == FileKind::Ana) {
                    opts.pvoc.hash_into(&mut h);
                }
                inputs(&mut h)?;
            }
            NodeKind::Mezcla { canales } => {
                if n.entradas.is_empty() {
                    return Err(KeyError::MissingInput);
                }
                h.str("mezcla"); // i18n-ok
                h.u64(canales.len() as u64);
                for c in canales {
                    h.f64(c.ganancia_db);
                    h.f64(c.paneo);
                    h.f64(c.inicio);
                }
                opts.hash_into(&mut h);
                inputs(&mut h)?;
            }
            NodeKind::SubPatch { pasos, .. } => {
                if n.entradas.len() != 1 {
                    return Err(KeyError::MissingInput);
                }
                if pasos.is_empty() {
                    return Err(KeyError::NotImplemented);
                }
                h.str("subpatch"); // i18n-ok
                h.u64(pasos.len() as u64);
                for t in pasos {
                    let NodeKind::Proceso { proceso } = &t.tipo else { return Err(KeyError::NotImplemented) };
                    let p = reg.get(proceso).ok_or_else(|| KeyError::UnknownProcess(proceso.clone()))?;
                    if p.inputs().is_generator() {
                        return Err(KeyError::NotImplemented);
                    }
                    h.str(proceso);
                    h.u64(p.version() as u64);
                    h.str(&p.engine_version().unwrap_or_default());
                    let mut params = t.params.completed(p.params());
                    if !p.per_channel() {
                        params.por_canal = None;
                    }
                    params.hash_into(&mut h);
                    h.u64(if p.uses_seed() { t.seed } else { 0 });
                    match t.region {
                        None => h.u8(0),
                        Some(r) => {
                            h.u8(1);
                            h.u64(r.inicio);
                            h.u64(r.fin);
                        }
                    }
                    h.u64(t.iteraciones.max(1) as u64);
                    h.u64(t.otras_entradas.len() as u64);
                    for e in &t.otras_entradas {
                        match e {
                            TemplateInput::Interna { indice, salida } => {
                                h.u64(*indice as u64);
                                h.u64(*salida as u64);
                            }
                            TemplateInput::Externa(_) => return Err(KeyError::NotImplemented),
                        }
                    }
                    if p.input_kinds().iter().any(|k| *k == FileKind::Ana) {
                        opts.pvoc.hash_into(&mut h);
                    }
                }
                opts.hash_into(&mut h);
                inputs(&mut h)?;
            }
        }
        Ok(h.finish_hex())
    }

    /// Estado de un nodo según las claves actuales.
    pub fn status(&self, id: NodeId, keys: &Keys) -> Status {
        let Some(n) = self.node(id) else { return Status::Error };
        if matches!(n.tipo, NodeKind::Fuente(_)) {
            return Status::Fuente;
        }
        match keys.get(&id) {
            None | Some(Err(_)) => Status::Error,
            Some(Ok(k)) => match (&n.render, &n.error) {
                (Some(r), _) if &r.clave == k => Status::Renderizado,
                (_, Some(e)) if &e.clave == k => Status::Error,
                (Some(_), _) => Status::Desactualizado,
                (None, _) => Status::SinRender,
            },
        }
    }

    /// Nodos a renderizar, en orden, para tener al día `targets` (o todo el
    /// patch si `targets` está vacío). Incluye lo necesario aguas arriba.
    /// Omite fuentes, nodos al día y nodos cuya clave no se puede calcular.
    pub fn render_plan(&self, targets: &[NodeId], keys: &Keys) -> Vec<NodeId> {
        let wanted: BTreeSet<NodeId> = if targets.is_empty() {
            self.nodos.iter().map(|n| n.id).collect()
        } else {
            let mut s: BTreeSet<NodeId> = targets.iter().copied().collect();
            for t in targets {
                s.extend(self.upstream(*t));
            }
            s
        };
        let order = self.topo_order().unwrap_or_default();
        order
            .into_iter()
            .filter(|id| wanted.contains(id))
            .filter(|id| matches!(self.status(*id, keys), Status::SinRender | Status::Desactualizado))
            .collect()
    }

    /// Copia del patch con otras fuentes (`mapa`: nodo fuente -> nueva
    /// fuente). Los renders se descartan; las claves que no cambian se
    /// recuperan igual de la cache. Es la base de "re-ejecutar un patch con
    /// otra fuente" y de las plantillas (las "recetas").
    pub fn with_sources(&self, mapa: &HashMap<NodeId, SourceInfo>) -> Patch {
        let mut p = self.clone();
        for n in &mut p.nodos {
            if let (NodeKind::Fuente(s), Some(nueva)) = (&mut n.tipo, mapa.get(&n.id)) {
                *s = nueva.clone();
            }
            n.render = None;
            n.error = None;
        }
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn src(h: &str) -> SourceInfo {
        SourceInfo {
            ruta: PathBuf::from(format!("{h}.wav")),
            hash: h.into(),
            sr: 48000,
            canales: 2,
            frames: 48000,
            bytes: 0,
            modificado: 0,
            marcadores: vec![],
        }
    }

    fn reg() -> Registry {
        surshape_native::registry()
    }

    fn rendered(p: &mut Patch, id: NodeId, keys: &Keys) {
        let k = keys[&id].clone().unwrap();
        p.node_mut(id).unwrap().render = Some(RenderRecord { clave: k, salidas: vec![], avisos: vec![], segundos: 0.0, version_motor: None });
    }

    #[test]
    fn keys_cascade_and_status_is_derived() {
        let reg = reg();
        let opts = RenderOptions::default();
        let mut p = Patch::default();
        let (s, row) = p.add_source(src("a"));
        let a = p.append(row, reg.get("nat.reverse").unwrap().as_ref(), 1).unwrap();
        let b = p.append(row, reg.get("nat.bitcrush").unwrap().as_ref(), 1).unwrap();
        let keys = p.keys(&reg, &opts);
        assert_eq!(p.status(s, &keys), Status::Fuente);
        assert_eq!(p.status(b, &keys), Status::SinRender);
        assert_eq!(p.render_plan(&[b], &keys), vec![a, b]);
        rendered(&mut p, a, &keys);
        rendered(&mut p, b, &keys);
        assert!(p.render_plan(&[], &keys).is_empty());

        // Cambiar un parámetro de `a` desactualiza `a` y `b`.
        p.node_mut(a).unwrap().params.comun.set("x", 1.0); // id desconocido: no cambia nada
        assert_eq!(p.keys(&reg, &opts), keys);
        let mut p2 = p.clone();
        let keys_same = p2.keys(&reg, &opts);
        assert_eq!(p2.status(b, &keys_same), Status::Renderizado);
        p2.node_mut(b).unwrap().params.comun.set("bits", 3.0);
        let k2 = p2.keys(&reg, &opts);
        assert_eq!(p2.status(a, &k2), Status::Renderizado);
        assert_eq!(p2.status(b, &k2), Status::Desactualizado);
        assert_eq!(p2.render_plan(&[], &k2), vec![b]);

        // Cambiar la fuente desactualiza toda la cadena.
        let p3 = p.with_sources(&HashMap::from([(s, src("otra"))]));
        let k3 = p3.keys(&reg, &opts);
        assert_ne!(k3[&a], keys[&a]);
        assert_eq!(p3.render_plan(&[], &k3), vec![a, b]);

        // El limitador también entra en la clave.
        let lim = RenderOptions { limiter: Some(Default::default()), ..Default::default() };
        assert_ne!(p.keys(&reg, &lim)[&a], keys[&a]);
    }

    #[test]
    fn seed_counts_only_if_the_process_uses_it() {
        let reg = reg();
        let opts = RenderOptions::default();
        let mut p = Patch::default();
        let (_, row) = p.add_source(src("a"));
        let rev = p.append(row, reg.get("nat.reverse").unwrap().as_ref(), 1).unwrap();
        let ws = p.append(row, reg.get("nat.waveshaper").unwrap().as_ref(), 1).unwrap();
        let k1 = p.keys(&reg, &opts);
        p.node_mut(rev).unwrap().seed = 99;
        assert_eq!(p.keys(&reg, &opts)[&rev], k1[&rev]);
        p.node_mut(ws).unwrap().seed = 99;
        assert_ne!(p.keys(&reg, &opts)[&ws], k1[&ws]);
    }

    #[test]
    fn cycles_are_rejected_and_removal_respects_dependents() {
        let reg = reg();
        let mut p = Patch::default();
        let (s, row) = p.add_source(src("a"));
        let a = p.append(row, reg.get("nat.reverse").unwrap().as_ref(), 1).unwrap();
        let b = p.append(row, reg.get("nat.reverse").unwrap().as_ref(), 1).unwrap();
        assert_eq!(p.connect(a, 0, PortRef::main(b)), Err(PatchError::Cycle));
        assert_eq!(p.connect(a, 0, PortRef::main(a)), Err(PatchError::Cycle));
        assert_eq!(p.remove(a), Err(PatchError::HasDependents));
        p.remove(b).unwrap();
        p.remove(a).unwrap();
        assert_eq!(p.filas[0].celdas, vec![s]);
        // Una entrada de más hace inválida la clave (reverse pide 1).
        let c = p.append(0, reg.get("nat.reverse").unwrap().as_ref(), 1).unwrap();
        p.connect(c, 1, PortRef::main(s)).unwrap();
        assert_eq!(p.keys(&reg, &RenderOptions::default())[&c], Err(KeyError::WrongInputs));
    }

    #[test]
    fn unknown_process_and_error_propagation() {
        let reg = reg();
        let opts = RenderOptions::default();
        let mut p = Patch::default();
        let (_, row) = p.add_source(src("a"));
        let x = p.append(row, reg.get("nat.reverse").unwrap().as_ref(), 1).unwrap();
        let y = p.append(row, reg.get("nat.reverse").unwrap().as_ref(), 1).unwrap();
        if let NodeKind::Proceso { proceso } = &mut p.node_mut(x).unwrap().tipo {
            *proceso = "cdp.no_instalado".into();
        }
        let k = p.keys(&reg, &opts);
        assert_eq!(k[&x], Err(KeyError::UnknownProcess("cdp.no_instalado".into())));
        assert_eq!(k[&y], Err(KeyError::Upstream));
        assert_eq!(p.status(y, &k), Status::Error);
        assert!(p.render_plan(&[], &k).is_empty());
    }

    #[test]
    fn branches_copy_paste_and_output_rows() {
        let reg = reg();
        let opts = RenderOptions::default();
        let mut p = Patch::default();
        let (s, row) = p.add_source(src("a"));
        let a = p.append(row, reg.get("nat.reverse").unwrap().as_ref(), 1).unwrap();
        let b = p.append(row, reg.get("nat.bitcrush").unwrap().as_ref(), 5).unwrap();
        p.node_mut(b).unwrap().params.comun.set("bits", 3.0);

        // Rama desde `a`: la primera celda de la rama toma `a` de entrada.
        let br = p.add_branch(PortRef::main(a)).unwrap();
        assert_eq!(br, 1);
        let tpl = p.copy_cells(&[b]);
        let pasted = p.paste(br, &tpl);
        assert_eq!(pasted.len(), 1);
        let c = p.node(pasted[0]).unwrap();
        assert_eq!(c.entradas, vec![PortRef::main(a)]);
        assert_eq!(c.params.comun.get("bits"), 3.0);
        // Misma cadena y mismos valores = misma clave que `b`: sale de la cache.
        let k = p.keys(&reg, &opts);
        assert_eq!(k[&pasted[0]], k[&b]);

        // Salidas extra: una fila por salida, con su origen.
        p.node_mut(a).unwrap().render = Some(RenderRecord {
            clave: "x".into(),
            salidas: vec![
                OutputInfo { archivo: "a0".into(), sr: 48000, canales: 1, frames: 1, tipo: None, datos: vec![] },
                OutputInfo { archivo: "a1".into(), sr: 48000, canales: 1, frames: 1, tipo: None, datos: vec![] },
                OutputInfo { archivo: "a2".into(), sr: 48000, canales: 1, frames: 1, tipo: None, datos: vec![] },
            ],
            avisos: vec![],
            segundos: 0.0,
            version_motor: None,
        });
        assert_eq!(p.ensure_output_rows(a), 2);
        assert_eq!(p.ensure_output_rows(a), 0);
        assert!(p.filas.iter().any(|r| r.origen == Some(PortRef { nodo: a, salida: 2 })));

        // Cambiar la fuente desactualiza todo lo que sigue.
        p.set_source(s, src("b"));
        let k2 = p.keys(&reg, &opts);
        assert_ne!(k2[&b], k[&b]);
    }

    #[test]
    fn mix_needs_inputs_and_keeps_one_setting_per_input() {
        let reg = reg();
        let mut p = Patch::default();
        let (s1, _) = p.add_source(src("a"));
        let (s2, _) = p.add_source(src("b"));
        let (m, _) = p.add_mix();
        assert_eq!(p.keys(&reg, &RenderOptions::default())[&m], Err(KeyError::MissingInput));
        p.connect(m, 0, PortRef::main(s1)).unwrap();
        p.connect(m, 1, PortRef::main(s2)).unwrap();
        let NodeKind::Mezcla { canales } = &p.node(m).unwrap().tipo else { panic!() };
        assert_eq!(canales.len(), 2);
        assert!(p.keys(&reg, &RenderOptions::default())[&m].is_ok());
        p.remove_input(m, 0);
        let NodeKind::Mezcla { canales } = &p.node(m).unwrap().tipo else { panic!() };
        assert_eq!((canales.len(), p.node(m).unwrap().entradas.len()), (1, 1));
    }

    #[test]
    fn recipe_drops_external_secondary_inputs() {
        let reg = reg();
        let mut p = Patch::default();
        let (ir, _) = p.add_source(src("ir"));
        let (_, row) = p.add_source(src("a"));
        let c = p.append(row, reg.get("nat.convolve").unwrap().as_ref(), 1).unwrap();
        p.connect(c, 1, PortRef::main(ir)).unwrap();
        let r = p.recipe_from_row(row, "x").unwrap();
        assert_eq!(r.pasos.len(), 1);
        assert!(r.pasos[0].otras_entradas.is_empty());
        let (_, row2) = p.add_source(src("otra"));
        let new = p.paste(row2, &r.pasos);
        assert_eq!(p.keys(&reg, &RenderOptions::default())[&new[0]], Err(KeyError::MissingInput));
    }

    #[test]
    fn json_roundtrip_with_breakpoints() {
        use surshape_engine::{Breakpoints, Interp, ParamValue, TimeMode};
        let reg = reg();
        let mut p = Patch::default();
        let (_, row) = p.add_source(src("a"));
        let b = p.append(row, reg.get("nat.bitcrush").unwrap().as_ref(), 1).unwrap();
        p.node_mut(b).unwrap().params.comun.set_value(
            "bits",
            ParamValue::Envelope(Breakpoints::new(TimeMode::Normalizado, Interp::Lineal, vec![(0.0, 2.0), (1.0, 12.0)])),
        );
        let txt = serde_json::to_string_pretty(&p).unwrap();
        assert!(txt.contains("\"reduccion\": 1.0"), "un valor fijo queda como número:\n{txt}");
        let back: Patch = serde_json::from_str(&txt).unwrap();
        assert_eq!(back, p);
        assert_eq!(back.keys(&reg, &RenderOptions::default()), p.keys(&reg, &RenderOptions::default()));
    }
}
