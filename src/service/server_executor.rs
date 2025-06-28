use crate::model::McVersion;
use std::path::Path;

pub struct ChunkExecutionConfig<'a> {
    pub java_path: Option<&'a Path>,
    pub version: McVersion,
    pub jar_file_name: String,
    pub port: u16,
}

pub trait ServerExecutor {
    fn start(&mut self, config: ChunkExecutionConfig) -> Result<(), String>;
    fn stop(&mut self) -> Result<(), String>;
    fn send_command(&mut self) -> Result<(), String>;
}
