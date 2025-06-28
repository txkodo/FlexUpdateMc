use crate::model::{McChunkPos, McVersion};

pub trait BotHandler {
    fn teleport_chunk(&mut self, chunk: McChunkPos) -> Result<(), String>;
}

pub trait BotHandlerFactory {
    fn join(port: u16, version: McVersion) -> Result<(), Box<dyn BotHandler>>;
}

pub struct AzaleaBotHandler {}

impl BotHandler for AzaleaBotHandler {
    fn teleport_chunk(&mut self, _chunk: McChunkPos) -> Result<(), String> {
        todo!("Azalea Botのチャンク移動ロジックを実装")
    }
}

pub struct AzaleaBotHandlerFactory {}

impl BotHandlerFactory for AzaleaBotHandlerFactory {
    fn join(_port: u16, _version: McVersion) -> Result<(), Box<dyn BotHandler>> {
        todo!("Azalea Botの接続ロジックを実装")
    }
}
