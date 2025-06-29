use crate::model::{McChunkPos, McVersion};
use async_trait::async_trait;
use azalea::{JoinOpts, prelude::*};
use azalea_viaversion::ViaVersionPlugin;
use std::fs;
use std::io::Write;
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use tokio::time::{Duration, sleep};

pub struct BotRunnerFactory {
    bot_binary_path: PathBuf,
}

pub struct BotRunner {
    child: Arc<Mutex<Option<Child>>>,
}

pub struct BotRunnerConfig {
    pub version: McVersion,
    pub name: String,
    pub address: (Ipv4Addr, u16),
}

impl BotRunnerFactory {
    pub fn new<P: AsRef<Path>>(cache_path: P) -> Self {
        let bot_binary_path = cache_path.as_ref().join("flex_bot");
        
        BotRunnerFactory {
            bot_binary_path,
        }
    }
    
    pub async fn start(&self, config: BotRunnerConfig) -> anyhow::Result<BotRunner> {
        // ディレクトリが存在しない場合は作成
        if let Some(parent) = self.bot_binary_path.parent() {
            fs::create_dir_all(parent)?;
        }
        
        // botバイナリを書き出し (まだ存在しない場合のみ)
        if !self.bot_binary_path.exists() {
            let bot_binary = include_bytes!(env!("BOT_BINARY_PATH"));
            fs::write(&self.bot_binary_path, bot_binary)?;

            // 実行権限を付与 (Unix系のみ)
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = fs::metadata(&self.bot_binary_path)?.permissions();
                perms.set_mode(0o755);
                fs::set_permissions(&self.bot_binary_path, perms)?;
            }
        }
        
        // botを実行
        let child = Command::new(&self.bot_binary_path)
            .arg("--username")
            .arg(&config.name)
            .arg("--host")
            .arg(config.address.0.to_string())
            .arg("--port")
            .arg(config.address.1.to_string())
            .arg("--version")
            .arg(&config.version.to_string())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        Ok(BotRunner {
            child: Arc::new(Mutex::new(Some(child))),
        })
    }
}

impl BotRunner {
    pub async fn stop(&self) -> anyhow::Result<()> {
        if let Ok(mut child_guard) = self.child.lock() {
            if let Some(mut child) = child_guard.take() {
                // プロセスを終了
                let _ = child.kill();
                let _ = child.wait();
            }
        }
        
        Ok(())
    }
}
