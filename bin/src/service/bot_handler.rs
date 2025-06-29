use crate::model::{McChunkPos, McVersion};
use async_trait::async_trait;
use azalea::{prelude::*, JoinOpts};
use azalea_viaversion::ViaVersionPlugin;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::UnboundedReceiver;

#[derive(Default, Clone, Component)]
pub struct BotState {
    pub bot_id: String,
}

#[async_trait]
pub trait BotHandler {
    async fn teleport_chunk(&mut self, chunk: McChunkPos) -> anyhow::Result<()>;
    async fn connect(&mut self, address: &str, port: u16, version: McVersion)
    -> anyhow::Result<()>;
    async fn connect_with_version(
        &mut self,
        address: &str,
        port: u16,
        target_version: u32,
    ) -> anyhow::Result<()>;
    async fn disconnect(&mut self) -> anyhow::Result<()>;
}

// pub struct AzaleaBotHandler {
//     client: Arc<Mutex<Option<(Client, UnboundedReceiver<Event>)>>>,
// }

// impl AzaleaBotHandler {
//     pub fn new() -> Self {
//         Self {
//             client: Arc::new(Mutex::new(None)),
//         }
//     }
// }

// #[async_trait]
// impl BotHandler for AzaleaBotHandler {
//     async fn connect(
//         &mut self,
//         address: &str,
//         port: u16,
//         version: McVersion,
//     ) -> anyhow::Result<()> {
//         let account = Account::offline("flex_updater_bot");
//         let server_address = format!("{}:{}", address, port);

//         let (client, mut rx) = {
//             // ViaVersionを起動して指定バージョンで接続
//             let viaversion = ViaVersionPlugin::start(version.version).await;

//             azalea::ClientBuilder::new().add_plugins(viaversion).start_with_opts(account, address, JoinOpts {
                
//             });

//             azalea::Client::join(account, server_address);
//         };

//         while let Some(event) = rx.recv().await {
//             match event {
//                 Event::Login => {
//                     break;
//                 }
//                 Event::Disconnect(_) => {
//                     return Err(anyhow::anyhow!("Disconnected from server"));
//                 }
//                 _ => {}
//             }
//         }

//         {
//             let mut c = self
//                 .client
//                 .lock()
//                 .map_err(|e| anyhow::anyhow!("Failed to lock client: {}", e))?;

//             if c.is_some() {
//                 anyhow::bail!("Client is already connected.");
//             }
//             *c = Some((client, rx));
//         }

//         return Ok(());
//     }

//     async fn disconnect(&mut self) -> anyhow::Result<()> {
//         let mut lock = self.client.lock().expect("Failed to lock client mutex");

//         let (client, rx) = lock.as_mut().expect("Client is not connected");
//         client.disconnect();

//         loop {
//             match rx.try_recv() {
//                 Ok(Event::Disconnect(_)) => {
//                     println!("[Bot] Disconnected from server");
//                     break;
//                 }
//                 Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
//                     break;
//                 }
//                 _ => continue,
//             }
//         }
//         Ok(())
//     }

//     async fn teleport_chunk(&mut self, chunk: McChunkPos) -> anyhow::Result<()> {
//         // チャンクの中央座標を計算 (チャンク座標 * 16 + 8)
//         let x = chunk.x * 16 + 8;
//         let z = chunk.y * 16 + 8;
//         let y = 100; // 安全な高度を仮定

//         // テレポートコマンドを送信
//         let tp_command = format!("/tp {} {} {}", x, y, z);
//         println!("[Bot] Executing teleport command: {}", tp_command);

//         let mut lock = self.client.lock().expect("Failed to lock client mutex");
//         let (client, _) = lock.as_mut().expect("Client is not connected");
//         client.send_command_packet(&tp_command);
//         Ok(())
//     }
// }
