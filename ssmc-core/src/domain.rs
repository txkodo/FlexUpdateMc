pub mod fs_content;

use std::process::Command;

pub use fs_content::{FileBundle, FileEntry, FileSource};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McVanillaVersionId(String);
impl McVanillaVersionId {
    pub fn new(id: String) -> Self {
        Self(id)
    }
    pub fn id(&self) -> &str {
        &self.0
    }
}

pub trait McVersion {
    fn vanilla_id(&self) -> McVanillaVersionId;
}

#[derive(Default)]
pub struct ServerRunOptions {
    pub max_memory: Option<u32>,     // MB
    pub initial_memory: Option<u32>, // MB
}

#[async_trait::async_trait]
pub trait McServerLoader {
    type Version: McVersion;
    async fn ready_server(
        &self,
        world_data: FileBundle,
        version: &Self::Version,
    ) -> Result<(FileBundle, Box<dyn Fn(ServerRunOptions) -> Command>), String>;
}

#[async_trait::async_trait]
pub trait McVersionQuerier {
    type Version: McVersion;
    type VersionQuery;
    async fn query_versions(&self, query: &Self::VersionQuery) -> Vec<Self::Version>;
}
