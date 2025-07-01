use crate::model::{McChunk, McChunkPos, McDimension, McVersion};
use crate::util::fs_util;
use std::fs;
use std::path::{Path, PathBuf};

pub trait RegionHandler: Clone {
    fn list_chunks(&self, region_dir: &Path) -> Result<Vec<McChunkPos>, String>;
    fn load_chunk(&self, region_dir: &Path, pos: &McChunkPos) -> Result<McChunk, String>;
    fn save_chunk(&self, region_dir: &Path, chunk: &McChunk) -> Result<(), String>;
}

pub struct ChunkGenerationConfig<'a> {
    pub java_path: Option<&'a Path>,
    pub version: McVersion,
    pub jar_file_name: String,
}

pub trait ServerHandler {
    fn list_chunks(&self, dimension: McDimension) -> Result<Vec<McChunkPos>, String>;
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerDirStructure {
    Vanilla,
    Plugin,
}

pub struct ServerHandlerImpl<T: RegionHandler> {
    path: PathBuf,
    structure: ServerDirStructure,
    world_dir_name: String,
    region_handler: T,
}

impl<T: RegionHandler> ServerHandlerImpl<T> {
    pub fn new(path: PathBuf, structure: ServerDirStructure, world_dir_name: String, region_handler: T) -> Self {
        ServerHandlerImpl {
            path,
            structure,
            world_dir_name,
            region_handler,
        }
    }

    fn get_region_dir(&self, dimension: &McDimension) -> PathBuf {
        match dimension {
            McDimension::Overworld => self.path.join(format!("{}/region", self.world_dir_name)),
            McDimension::Nether => self.path.join(format!(
                "{}/DIM-1/region",
                match self.structure {
                    ServerDirStructure::Vanilla => &self.world_dir_name,
                    ServerDirStructure::Plugin => &format!("{}_nether", self.world_dir_name),
                }
            )),
            McDimension::TheEnd => self.path.join(format!(
                "{}/DIM1/region",
                match self.structure {
                    ServerDirStructure::Vanilla => &self.world_dir_name,
                    ServerDirStructure::Plugin => &format!("{}_the_end", self.world_dir_name),
                }
            )),
        }
    }
}

impl<T: RegionHandler> ServerHandler for ServerHandlerImpl<T> {
    fn list_chunks(&self, dimension: McDimension) -> Result<Vec<McChunkPos>, String> {
        let region_dir = self.get_region_dir(&dimension);
        self.region_handler.list_chunks(&region_dir)
    }

    fn generate_chunks(
        &mut self,
        _pos: &[McChunkPos],
        _config: &ChunkGenerationConfig,
    ) -> Result<(), String> {
        // 実装は省略
        Ok(())
    }

    fn load_chunk(&self, pos: &McChunkPos) -> Result<McChunk, String> {
        let region_dir = self.get_region_dir(&pos.dimension);
        self.region_handler.load_chunk(&region_dir, pos)
    }

    fn save_chunk(&mut self, chunk: &McChunk) -> Result<(), String> {
        let region_dir = self.get_region_dir(&chunk.pos.dimension);
        self.region_handler.save_chunk(&region_dir, chunk)
    }

    fn copy_to(&self, path: &Path) -> Result<Box<dyn ServerHandler>, String> {
        fs_util::overwrite_dir(&self.path, path)
            .map_err(|e| format!("Failed to copy directory: {}", e))?;
        Ok(Box::new(ServerHandlerImpl::new(
            path.to_path_buf(),
            self.structure.clone(),
            self.world_dir_name.clone(),
            self.region_handler.clone(),
        )))
    }

    fn clear_dimension(&mut self, dimension: McDimension) -> Result<(), String> {
        let region_dir = self.get_region_dir(&dimension);
        if region_dir.exists() {
            fs::remove_dir_all(&region_dir)
                .map_err(|e| format!("Failed to clear dimension {:?}: {}", dimension, e))?;
        }
        Ok(())
    }

    fn clear_dimension_all(&mut self) -> Result<(), String> {
        self.clear_dimension(McDimension::Overworld)?;
        self.clear_dimension(McDimension::Nether)?;
        self.clear_dimension(McDimension::TheEnd)?;
        Ok(())
    }
}
