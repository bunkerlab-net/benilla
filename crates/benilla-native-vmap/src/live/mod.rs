// SPDX-License-Identifier: GPL-3.0-or-later
//! Stateful pinned VMap owners over immutable diagnostic source products.
//! CMaNGOS 8ec338a1 VMapManager2.cpp:97-183,310-346; MapTree.cpp:272-493.
#[allow(dead_code)]
// The shared app reader additionally validates bounds and uses nearest-hit wrappers.
mod bih;
mod math;
mod scene;
mod store;
mod trig;
use bih::Bih;
use math::*;
pub use scene::{AreaInfo, LiquidOutput, LocationInfo};
use std::{collections::BTreeMap, sync::Arc, time::Instant};
pub use store::{FileState, ProductFiles};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Format,
    Framing,
    Budget,
    Deadline,
    Missing,
    Unsupported,
    SourceReadFailure,
    ProductUnavailable,
    ImmutableSource,
    UndefinedSlot,
    UndefinedRead,
    NumericBinding,
    RotationDomain,
    ArithmeticEnvironment,
    StaleGeneration,
    InvalidMap,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}
pub type Result<T, E = Error> = std::result::Result<T, E>;
const MAX_TREE_WORDS: usize = 16_777_216;
const MAX_OBJECT_WORDS: usize = 1_048_576;

/// Native IEEE basic operations are separate, unfused, RNE with gradual underflow.
/// No host transcendental function is used. Nonzero Euler angles remain refused.
fn arithmetic_environment() -> Result<()> {
    #[cfg(target_arch = "x86_64")]
    {
        let mut csr = 0u32;
        // Read only. SSE rounding, FTZ and DAZ must match the source profile.
        unsafe {
            std::arch::asm!("stmxcsr [{}]", in(reg) &mut csr, options(nostack, preserves_flags));
        }
        if csr & 0xe040 != 0 {
            return Err(Error::ArithmeticEnvironment);
        }
        Ok(())
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        Err(Error::ArithmeticEnvironment)
    }
}
#[derive(Clone, Copy, Debug)]
struct Ray {
    origin: Vector,
    direction: Vector,
}
#[derive(Clone, Copy, Debug, PartialEq)]
struct Bounds {
    low: Vector,
    high: Vector,
}
impl Bounds {
    fn contains(self, p: Vector) -> bool {
        (0..3).all(|i| self.low[i] <= p[i] && p[i] <= self.high[i])
    }
}
struct QueryBudget {
    remaining: usize,
    deadline: Instant,
}
impl QueryBudget {
    fn new(deadline: Instant) -> Self {
        Self {
            remaining: usize::MAX,
            deadline,
        }
    }
    fn take(&mut self) -> Result<()> {
        if Instant::now() >= self.deadline {
            return Err(Error::Deadline);
        }
        if self.remaining == 0 {
            return Err(Error::Budget);
        }
        self.remaining -= 1;
        Ok(())
    }
}
struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.at.checked_add(n).ok_or(Error::Framing)?;
        let out = self.bytes.get(self.at..end).ok_or(Error::Framing)?;
        self.at = end;
        Ok(out)
    }
    fn magic(&mut self, v: &[u8]) -> Result<()> {
        if self.take(v.len())? == v {
            Ok(())
        } else {
            Err(Error::Format)
        }
    }
    fn word(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn float(&mut self) -> Result<f32> {
        let f = f32::from_bits(self.word()?);
        if f.is_finite() {
            Ok(f)
        } else {
            Err(Error::Unsupported)
        }
    }
    fn vector(&mut self) -> Result<Vector> {
        Ok([self.float()?, self.float()?, self.float()?])
    }
    fn bounds(&mut self) -> Result<Bounds> {
        let b = Bounds {
            low: self.vector()?,
            high: self.vector()?,
        };
        if (0..3).any(|i| b.low[i] > b.high[i]) {
            Err(Error::Unsupported)
        } else {
            Ok(b)
        }
    }
}
fn reserve<T>(n: usize) -> Result<Vec<T>> {
    let mut out = Vec::new();
    out.try_reserve_exact(n).map_err(|_| Error::Budget)?;
    Ok(out)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadResult {
    Ignored,
    Ok,
    Error,
}
#[derive(Clone, Debug)]
pub struct ModelGeneration {
    pub name: Arc<[u8]>,
    pub generation: u64,
}
impl PartialEq for ModelGeneration {
    fn eq(&self, other: &Self) -> bool {
        self.generation == other.generation && Arc::ptr_eq(&self.name, &other.name)
    }
}
impl Eq for ModelGeneration {}
struct SharedModel {
    owner: ModelGeneration,
    references: u32,
    flags: u32,
    model: scene::WorldModel,
}
struct Instance {
    spawn: Spawn,
    model: Option<ModelGeneration>,
    inverse: Matrix,
    inverse_scale: f32,
    generation: u64,
}
struct Spawn {
    name: Arc<[u8]>,
    flags: u32,
    adt: u16,
    id: u32,
    position: Vector,
    angles: Vector,
    scale: f32,
    bounds: Option<Bounds>,
}
impl Spawn {
    fn decode(c: &mut Cursor<'_>) -> Result<Self> {
        let flags = c.word()?;
        let adt = u16::from_le_bytes(c.take(2)?.try_into().unwrap());
        let id = c.word()?;
        let position = c.vector()?;
        let angles = c.vector()?;
        let scale = c.float()?;
        // G3D AABox.h:42-43 leaves missing bounds uninitialized.
        let bounds = if flags & 4 != 0 {
            Some(c.bounds()?)
        } else {
            None
        };
        let n = c.word()? as usize;
        if n > 500 {
            return Err(Error::Format);
        }
        let name = Arc::from(c.take(n)?);
        Ok(Self {
            name,
            flags,
            adt,
            id,
            position,
            angles,
            scale,
            bounds,
        })
    }
}
struct Tree {
    tiled: bool,
    bih: Bih,
    slots: Vec<Option<Instance>>,
    references: BTreeMap<usize, u32>,
    tiles: BTreeMap<(u32, u32), bool>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeInventory {
    pub map: u32,
    pub tiled: bool,
    pub tiles: Vec<((u32, u32), bool)>,
    pub slots: Vec<(usize, u32, u32, Option<ModelGeneration>)>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelInventory {
    pub owner: ModelGeneration,
    pub references: u32,
    pub flags: u32,
}
/// Exact source filename keys. Product bytes are immutable, independently of residency.
pub struct Manager {
    pub files: ProductFiles,
    trees: BTreeMap<u32, Option<Tree>>,
    models: BTreeMap<Arc<[u8]>, SharedModel>,
    next_generation: u64,
    thread_unsafe: bool,
    los_enabled: bool,
    height_enabled: bool,
}
impl Default for Manager {
    fn default() -> Self {
        Self {
            files: ProductFiles::default(),
            trees: BTreeMap::new(),
            models: BTreeMap::new(),
            next_generation: 0,
            thread_unsafe: false,
            los_enabled: true,
            height_enabled: true,
        }
    }
}
impl Manager {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn configure(&mut self, los: bool, height: bool) {
        self.los_enabled = los;
        self.height_enabled = height;
    }
    pub fn initialize_thread_unsafe(&mut self, maps: impl IntoIterator<Item = u32>) {
        for m in maps {
            self.trees.entry(m).or_insert(None);
        }
        self.thread_unsafe = true;
    }
    pub fn has_tree(&self, map: u32) -> bool {
        self.trees.get(&map).is_some_and(Option::is_some)
    }
    pub fn map_slots(&self) -> usize {
        self.trees.len()
    }
    pub fn loaded_trees(&self) -> usize {
        self.trees.values().filter(|t| t.is_some()).count()
    }
    pub fn tree_inventory(&self) -> Vec<TreeInventory> {
        self.trees
            .iter()
            .filter_map(|(&map, t)| {
                t.as_ref().map(|t| TreeInventory {
                    map,
                    tiled: t.tiled,
                    tiles: t.tiles.iter().map(|(k, v)| (*k, *v)).collect(),
                    slots: t
                        .slots
                        .iter()
                        .enumerate()
                        .filter_map(|(i, s)| {
                            s.as_ref().map(|s| {
                                (
                                    i,
                                    t.references.get(&i).copied().unwrap_or(0),
                                    s.spawn.id,
                                    s.model.clone(),
                                )
                            })
                        })
                        .collect(),
                })
            })
            .collect()
    }
    pub fn model_inventory(&self) -> Vec<ModelInventory> {
        self.models
            .values()
            .map(|m| ModelInventory {
                owner: m.owner.clone(),
                references: m.references,
                flags: m.flags,
            })
            .collect()
    }
    pub fn validate_model(&self, handle: &ModelGeneration) -> Result<()> {
        self.model(handle).map(|_| ())
    }
    fn model(&self, h: &ModelGeneration) -> Result<&SharedModel> {
        self.models
            .get(&h.name)
            .filter(|m| m.owner == *h)
            .ok_or(Error::StaleGeneration)
    }
    fn acquire(&mut self, name: &Arc<[u8]>, flags: u32) -> Result<Option<ModelGeneration>> {
        if !self.models.contains_key(name) {
            let mut path = name.to_vec();
            path.extend_from_slice(b".vmo");
            let Some(bytes) = self.files.read(&path)? else {
                return Ok(None);
            };
            let model = match scene::WorldModel::decode(&bytes) {
                Ok(model) => model,
                Err(Error::SourceReadFailure | Error::Framing) => return Ok(None),
                Err(Error::Format) => return Err(Error::Unsupported),
                Err(other) => return Err(other),
            };
            self.next_generation = self.next_generation.checked_add(1).ok_or(Error::Budget)?;
            self.models.insert(
                Arc::clone(name),
                SharedModel {
                    owner: ModelGeneration {
                        name: Arc::clone(name),
                        generation: self.next_generation,
                    },
                    references: 0,
                    flags,
                    model,
                },
            );
        }
        let m = self.models.get_mut(name).unwrap();
        m.references = m.references.wrapping_add(1);
        m.flags = flags;
        Ok(Some(m.owner.clone()))
    }
    fn release(&mut self, name: &[u8]) {
        if let Some(m) = self.models.get_mut(name) {
            m.references = m.references.wrapping_sub(1);
            if m.references == 0 {
                self.models.remove(name);
            }
        }
    }
    fn install(&mut self, t: &mut Tree, s: Spawn, c: &mut Cursor<'_>) -> Result<()> {
        let model = self.acquire(&s.name, s.flags)?;
        let slot = c.word().map_err(|_| Error::UndefinedRead)? as usize;
        // Source checks > rather than >=. Equality reaches a C++ out-of-bounds write.
        if slot > t.slots.len() {
            return Ok(());
        }
        if slot == t.slots.len() {
            return Err(Error::UndefinedSlot);
        }
        if let Some(r) = t.references.get_mut(&slot) {
            *r = r.wrapping_add(1);
            return Ok(());
        }
        arithmetic_environment()?;
        let inverse = inverse(rotation(s.angles)?)?;
        let inverse_scale = 1.0 / s.scale;
        self.next_generation = self.next_generation.checked_add(1).ok_or(Error::Budget)?;
        t.slots[slot] = Some(Instance {
            spawn: s,
            model,
            inverse,
            inverse_scale,
            generation: self.next_generation,
        });
        t.references.insert(slot, 1);
        Ok(())
    }
    pub fn tile_filename(map: u32, x: u32, y: u32) -> String {
        format!("{map:03}_{y:02}_{x:02}.vmtile")
    }
    pub fn load_map(&mut self, map: u32, x: u32, y: u32) -> Result<LoadResult> {
        if !self.los_enabled && !self.height_enabled {
            return Ok(LoadResult::Ignored);
        }
        if self.thread_unsafe && !self.trees.contains_key(&map) {
            return Err(Error::InvalidMap);
        }
        self.trees.entry(map).or_insert(None);
        if !self.has_tree(map) {
            let name = format!("{map:03}.vmtree");
            let Some(bytes) = self.files.read(name.as_bytes())? else {
                return Ok(LoadResult::Error);
            };
            let mut c = Cursor::new(&bytes);
            let init = (|| {
                c.magic(b"VMAP_7.0")?;
                let tiled = c.take(1)?[0] != 0;
                c.magic(b"NODE")?;
                let bih = Bih::decode(&mut c).map_err(|error| match error {
                    Error::Format => Error::Unsupported,
                    other => other,
                })?;
                c.magic(b"GOBJ")?;
                Ok((tiled, bih))
            })();
            let (tiled, bih) = match init {
                Ok(v) => v,
                Err(Error::Format | Error::Framing) => return Ok(LoadResult::Error),
                Err(e) => return Err(e),
            };
            let mut slots = reserve(bih.primitive_count())?;
            slots.resize_with(bih.primitive_count(), || None);
            let mut tree = Tree {
                tiled,
                bih,
                slots,
                references: BTreeMap::new(),
                tiles: BTreeMap::new(),
            };
            if !tiled {
                while c.at < c.bytes.len() {
                    let spawn = match Spawn::decode(&mut c) {
                        Ok(s) => s,
                        Err(Error::Format | Error::Framing) => break,
                        Err(e) => return Err(e),
                    };
                    self.install(&mut tree, spawn, &mut c)?;
                }
            }
            self.trees.insert(map, Some(tree));
        }
        let mut tree = self.trees.get_mut(&map).unwrap().take().unwrap();
        let result = self.load_tile(&mut tree, map, x, y);
        self.trees.insert(map, Some(tree));
        result
    }
    fn load_tile(&mut self, t: &mut Tree, map: u32, x: u32, y: u32) -> Result<LoadResult> {
        if !t.tiled {
            t.tiles.insert((x, y), false);
            return Ok(LoadResult::Ok);
        }
        let Some(bytes) = self.files.read(Self::tile_filename(map, x, y).as_bytes())? else {
            t.tiles.insert((x, y), false);
            return Ok(LoadResult::Ok);
        };
        let mut c = Cursor::new(&bytes);
        let result = (|| {
            c.magic(b"VMAP_7.0")?;
            let n = c.word()?;
            for _ in 0..n {
                self.install(t, Spawn::decode(&mut c)?, &mut c)?;
            }
            Ok(LoadResult::Ok)
        })();
        t.tiles.insert((x, y), true);
        match result {
            Err(Error::Format | Error::Framing) => Ok(LoadResult::Error),
            v => v,
        }
    }
    pub fn is_tile_loaded(&self, map: u32, x: u32, y: u32) -> bool {
        self.trees
            .get(&map)
            .and_then(Option::as_ref)
            .is_some_and(|t| t.tiles.contains_key(&(x, y)))
    }
    pub fn unload_tile(&mut self, map: u32, x: u32, y: u32) -> Result<()> {
        let Some(mut t) = self.trees.get_mut(&map).and_then(Option::take) else {
            return Ok(());
        };
        let result = (|| {
            if t.tiles.get(&(x, y)) == Some(&true) {
                if let Some(bytes) = self.files.read(Self::tile_filename(map, x, y).as_bytes())? {
                    let mut c = Cursor::new(&bytes);
                    if c.magic(b"VMAP_7.0").is_ok() {
                        let n = c.word()?;
                        for _ in 0..n {
                            let s = Spawn::decode(&mut c)?;
                            self.release(&s.name);
                            let slot = c.word().map_err(|_| Error::UndefinedRead)? as usize;
                            if let Some(r) = t.references.get_mut(&slot) {
                                *r = r.wrapping_sub(1);
                                if *r == 0 {
                                    t.references.remove(&slot);
                                    t.slots[slot].as_mut().unwrap().model = None;
                                }
                            }
                        }
                    }
                }
            }
            t.tiles.remove(&(x, y));
            Ok(())
        })();
        if result.is_err() || !t.tiles.is_empty() {
            self.trees.insert(map, Some(t));
        }
        // The source destructor does not release non-tiled global models here.
        result
    }
    pub fn unload_map(&mut self, map: u32) {
        if let Some(t) = self.trees.get_mut(&map).and_then(Option::take) {
            for (slot, refs) in t.references {
                if let Some(i) = &t.slots[slot] {
                    for _ in 0..refs {
                        self.release(&i.spawn.name);
                    }
                }
            }
        }
    }
}
/// VMapManager2.cpp:65-74. Conversion is its own inverse only after source rounding.
pub fn to_internal(p: [f32; 3]) -> [f32; 3] {
    const MID: f32 = 0.5 * 64.0 * 533.333_3;
    [MID - p[0], MID - p[1], p[2]]
}
#[cfg(test)]
mod tests;
