use crate::{
    model::{McChunk, McChunkPos, McVersion},
    service::world_handler::WorldHandler,
};
use std::path::Path;
pub struct ChunkGenerationConfig<'a> {
    pub java_path: Option<&'a Path>,
    pub version: McVersion,
}

pub trait ServerHandler: WorldHandler {
    fn list_chunks(&self) -> Result<Vec<McChunkPos>, String>;
    /// 該当バージョンで指定チャンクを生成/更新する
    fn generate_chunks(
        &self,
        pos: &[McChunkPos],
        config: &ChunkGenerationConfig,
    ) -> Result<(), String>;
    fn load_chunk(&self, pos: &McChunkPos) -> Result<McChunk, String>;
    fn save_chunk(&self, chunk: &McChunk) -> Result<(), String>;
}