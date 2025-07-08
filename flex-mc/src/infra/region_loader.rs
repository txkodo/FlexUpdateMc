use anyhow::Result;
use fastanvil;
use fastnbt::Value;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs::File, path::PathBuf, sync::RwLock};

pub struct Dimension {
    path: PathBuf,
}

impl Dimension {
    pub fn new(path: PathBuf) -> Self {
        Dimension { path }
    }

    pub fn load_region(&self, pos: impl Into<RegionPos>) -> Result<Region> {
        let pos = pos.into();
        let path = self.path.join(pos.to_file_name());

        if !path.exists() {
            return Ok(Region::from_raw(
                pos,
                fastanvil::Region::new(File::open(&path)?)?,
            ));
        }
        Ok(Region::from_raw(
            pos,
            fastanvil::Region::from_stream(File::open(&path)?)?,
        ))
    }
}
pub struct Region {
    pos: RegionPos,
    raw: fastanvil::Region<File>,
}
impl Region {
    fn from_raw(pos: RegionPos, raw: fastanvil::Region<File>) -> Self {
        Region { pos, raw }
    }

    pub fn load_chunk(&mut self, pos: impl Into<ChunkPos>) -> Result<Option<Chunk>> {
        let pos = pos.into();
        if pos.region() != self.pos {
            anyhow::bail!(
                "Different region requested: {:?} != {:?}",
                pos.region(),
                self.pos
            );
        }

        let (ox, oz) = pos.region_offset();

        let bytes = self.raw.read_chunk(ox, oz)?;

        if let Some(bytes) = bytes {
            return Ok(Some(fastnbt::from_bytes(&bytes)?));
        }
        return Ok(None);
    }

    pub fn save_chunk(&mut self, pos: ChunkPos, chunk: &Chunk) -> Result<()> {
        if pos.region() != self.pos {
            anyhow::bail!(
                "Different region requested: {:?} != {:?}",
                pos.region(),
                self.pos
            );
        }
        let (ox, oz) = pos.region_offset();
        self.raw.write_chunk(ox, oz, &fastnbt::to_bytes(&chunk)?)?;
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
pub struct Chunk {
    sections: Vec<Section>,

    #[serde(rename = "Status")]
    status: String,

    #[serde(flatten)]
    other: HashMap<String, Value>,
}

impl Chunk {
    pub fn get_block(&self, x: usize, y: isize, z: usize) -> Result<&Block> {
        if y < 0 || y >= 384 {
            anyhow::bail!("Y coordinate out of bounds: {}", y);
        }
        if x >= 16 || z >= 16 {
            anyhow::bail!("X or Z coordinate out of bounds: x={}, z={}", x, z);
        }
        let sect_idx = y.div_euclid(16) as usize;
        let sect_y = y.rem_euclid(16) as usize;
        Ok(self.sections[sect_idx].block_states.get_block(x, sect_y, z))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RegionPos {
    pub x: isize,
    pub z: isize,
}
impl RegionPos {
    pub fn new(x: isize, z: isize) -> Self {
        RegionPos { x, z }
    }

    pub fn try_parse_file_name(file_name: &str) -> Result<ChunkPos, String> {
        let parts: Vec<&str> = file_name.split('.').collect();
        if parts.len() != 3 || parts[0] != "r" || parts[2] != "mca" {
            return Err("Invalid region file name format".to_string());
        }
        let coords: Vec<&str> = parts[1].split('_').collect();
        if coords.len() != 2 {
            return Err("Invalid region coordinates format".to_string());
        }
        let x = coords[0].parse::<isize>().expect("Invalid x coordinate");
        let z = coords[1].parse::<isize>().expect("Invalid z coordinate");
        Ok(ChunkPos::new(x, z))
    }

    pub fn to_file_name(&self) -> String {
        format!("r.{}.{}.mca", self.x, self.z)
    }

    pub fn chunk_at(&self, chunk_x: isize, chunk_z: isize) -> ChunkPos {
        ChunkPos::new(self.x * 32 + chunk_x, self.z * 32 + chunk_z)
    }
}
impl From<(isize, isize)> for RegionPos {
    fn from(coords: (isize, isize)) -> Self {
        RegionPos::new(coords.0, coords.1)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChunkPos {
    pub x: isize,
    pub z: isize,
}

impl ChunkPos {
    pub fn new(x: isize, z: isize) -> Self {
        ChunkPos { x, z }
    }
    pub fn region(&self) -> RegionPos {
        RegionPos::new(self.x / 32, self.z / 32)
    }
    pub fn region_offset(&self) -> (usize, usize) {
        (
            self.x.rem_euclid(32) as usize,
            self.x.rem_euclid(32) as usize,
        )
    }
}

impl From<(isize, isize)> for ChunkPos {
    fn from(coords: (isize, isize)) -> Self {
        ChunkPos::new(coords.0, coords.1)
    }
}

#[derive(Serialize, Deserialize)]
pub struct Section {
    block_states: Blockstates,
    #[serde(flatten)]
    other: HashMap<String, Value>,
}
impl Section {
    pub fn get_palette_item(&self, x: usize, y: usize, z: usize) -> &Block {
        self.block_states.get_block(x, y, z)
    }
}

#[derive(Serialize, Deserialize)]
pub struct Blockstates {
    palette: Vec<Block>,
    data: Option<fastnbt::LongArray>,
    #[serde(flatten)]
    other: HashMap<String, Value>,

    #[serde(skip)]
    bits_per_block: RwLock<Option<u32>>,
}
impl Blockstates {
    pub fn get_block(&self, x: usize, y: usize, z: usize) -> &Block {
        if x >= 16 || y >= 16 || z >= 16 {
            panic!("X, Y, Z coordinate out of bounds: x={}, z={}", x, z);
        }
        if let Some(data) = &self.data {
            let block_index = ((y * 16 + z) * 16 + x) as usize;

            let bits_per_block = self.calculate_bits_per_block();
            let block_per_long = (u64::BITS / bits_per_block) as usize;
            let data_index = block_index / block_per_long;
            let block_offset = block_index % block_per_long;

            let pallet_index = (data[data_index] >> (block_offset * bits_per_block as usize))
                & ((1 << bits_per_block) - 1);

            &self.palette[pallet_index as usize]
        } else {
            &self.palette[0]
        }
    }

    fn calculate_bits_per_block(&self) -> u32 {
        {
            if let Ok(v) = self.bits_per_block.read() {
                if let Some(bits) = *v {
                    return bits;
                }
            }
        }
        let bits = {
            if self.palette.is_empty() {
                0
            } else {
                (usize::BITS - (self.palette.len().saturating_sub(1)).leading_zeros()).min(4) as u32
            }
        };
        if let Ok(mut v) = self.bits_per_block.write() {
            v.replace(bits);
        }
        return bits;
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Block {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Properties")]
    properties: Option<Value>,
}
