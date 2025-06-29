use crate::model::{McChunk, McChunkPos, McDimension, McVersion};
use std::path::Path;

pub struct ChunkGenerationConfig<'a> {
    pub java_path: Option<&'a Path>,
    pub version: McVersion,
    pub jar_file_name: String,
}

pub trait ServerHandler {
    fn list_chunks(&self) -> Result<Vec<McChunkPos>, String>;
    /// 該当バージョンで指定チャンクを生成/更新する
    fn generate_chunks(
        &mut self,
        pos: &[McChunkPos],
        config: &ChunkGenerationConfig,
    ) -> Result<(), String>;
    fn load_chunk(&self, pos: &McChunkPos) -> Result<McChunk, String>;
    fn save_chunk(&mut self, chunk: &McChunk) -> Result<(), String>;
    fn copy_to(&self, path: &Path) -> Result<Box<dyn ServerHandler>, String>;
    fn clear_dimension(&mut self, dimension: McDimension) -> Result<(), String>;
    /// overworld nether the_end のワールドデータを削除する
    fn clear_dimension_all(&mut self) -> Result<(), String>;
}
