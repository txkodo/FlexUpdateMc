use std::{net::IpAddr, os::unix::fs::PermissionsExt, path::PathBuf};

use anyhow::Result;
use ssmc_core::domain::McVanillaVersionId;

pub trait BotSpawner {
    fn spawn_bot(
        &self,
        host: &IpAddr,
        port: u16,
        version: &McVanillaVersionId,
    ) -> Result<Box<dyn BotHandle>>;
}

pub trait BotHandle {
    fn name(&self) -> String;
    fn stop(self: Box<Self>) -> Result<()>;
}

pub struct AzaleaBotSpawner {
    bot_file_path: PathBuf,
    bot_id: usize,
}

impl AzaleaBotSpawner {
    pub fn new(bot_file_path: PathBuf) -> Self {
        AzaleaBotSpawner {
            bot_file_path,
            bot_id: 0,
        }
    }
}

impl BotSpawner for AzaleaBotSpawner {
    fn spawn_bot(
        &self,
        host: &IpAddr,
        port: u16,
        version: &McVanillaVersionId,
    ) -> Result<Box<dyn BotHandle>> {
        if !self.bot_file_path.exists() {
            std::fs::write(
                &self.bot_file_path,
                include_bytes!("../../../azalea-bot/target/debug/azalea-bot"),
            )?;
            let mut perms = std::fs::metadata(&self.bot_file_path)?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&self.bot_file_path, perms)?;
        }
        let mut command = std::process::Command::new(&self.bot_file_path);

        let username = format!("bot{:02}", self.bot_id);

        command.args([
            "--username",
            &username,
            "--host",
            &host.to_string(),
            "--port",
            &port.to_string(),
            "--version",
            version.id(),
        ]);

        let child = command.spawn()?;

        Ok(Box::new(AzaleaBotHandle {
            process: child,
            name: username,
        }))
    }
}

pub struct AzaleaBotHandle {
    process: std::process::Child,
    name: String,
}

impl BotHandle for AzaleaBotHandle {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn stop(mut self: Box<Self>) -> Result<()> {
        self.process.kill()?;
        self.process.wait()?;
        Ok(())
    }
}
