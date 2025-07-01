#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McDimension {
    Overworld,
    Nether,
    TheEnd,
    // Custom(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McChunkPos {
    pub x: i32,
    pub y: i32,
    pub dimension: McDimension,
}

#[derive(Debug)]
pub struct McChunk {
    pub pos: McChunkPos,
    // Add other fields as necessary, such as blocks, entities, etc.
}

impl Default for McChunk {
    fn default() -> Self {
        McChunk {
            pos: McChunkPos {
                x: 0,
                y: 0,
                dimension: McDimension::Overworld,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McVersion {
    pub version: String,
}
