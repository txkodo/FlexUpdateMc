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
    pub data: Option<crate::nbt::NbtTag>, // NBTデータとして実際のチャンクデータを保持
    pub last_update: Option<i64>, // チャンクの最終更新時刻
    pub inhabited_time: Option<i64>, // プレイヤーがチャンクにいた時間
}

impl Default for McChunk {
    fn default() -> Self {
        McChunk {
            pos: McChunkPos {
                x: 0,
                y: 0,
                dimension: McDimension::Overworld,
            },
            data: None,
            last_update: None,
            inhabited_time: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McVersion {
    pub version: String,
}
