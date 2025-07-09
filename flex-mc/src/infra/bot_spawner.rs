use std::{net::IpAddr, os::unix::fs::PermissionsExt, path::PathBuf};

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use ssmc_core::domain::McVanillaVersionId;

#[async_trait]
pub trait BotSpawner {
    async fn spawn_bot(
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
    bot_id: std::sync::Arc<std::sync::Mutex<usize>>,
}

impl AzaleaBotSpawner {
    pub fn new(bot_file_path: PathBuf) -> Self {
        AzaleaBotSpawner {
            bot_file_path,
            bot_id: std::sync::Arc::new(std::sync::Mutex::new(0)),
        }
    }
}

#[async_trait]
impl BotSpawner for AzaleaBotSpawner {
    async fn spawn_bot(
        &self,
        host: &IpAddr,
        port: u16,
        version: &McVanillaVersionId,
    ) -> Result<Box<dyn BotHandle>> {
        if !self.bot_file_path.exists() {
            download_bot_executable(&self.bot_file_path, &version.id()).await?;
        }
        let mut command = std::process::Command::new(&self.bot_file_path);

        let bot_id = {
            let mut id = self.bot_id.lock().unwrap();
            let current_id = *id;
            *id += 1;
            current_id
        };
        let username = format!("bot{:02}", bot_id);

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
        // First try to terminate gracefully
        if let Err(_) = self.process.kill() {
            // If kill fails, the process might already be dead
            return Ok(());
        }

        // Wait for the process to actually terminate
        match self.process.wait() {
            Ok(_) => Ok(()),
            Err(e) => {
                eprintln!(
                    "Warning: Failed to wait for bot process to terminate: {}",
                    e
                );
                Ok(()) // Don't fail if we can't wait
            }
        }
    }
}

impl Drop for AzaleaBotHandle {
    fn drop(&mut self) {
        // Ensure the process is killed when the handle is dropped
        if let Err(e) = self.process.kill() {
            eprintln!("Warning: Failed to kill bot process during drop: {}", e);
        }
    }
}

async fn download_bot_executable(bot_file_path: &PathBuf, version: &str) -> Result<()> {
    let (os, arch) = get_os_and_arch()?;
    let executable_name = format!(
        "flex-update-mc-bot-{}-{}-{}{}",
        version,
        os,
        arch,
        if os == "windows" { ".exe" } else { "" }
    );

    let client = reqwest::Client::new();
    let url = format!(
        "https://github.com/txkodo/FlexUpdateMcBot/releases/download/mc-{}/{}",
        version, executable_name
    );
    println!("Downloading bot executable from: {}", url);

    let response = client.get(&url).send().await?;

    if !response.status().is_success() {
        return Err(anyhow!(
            "Failed to download bot executable: HTTP {}",
            response.status()
        ));
    }

    let bytes = response.bytes().await?;
    std::fs::write(bot_file_path, bytes)?;

    let mut perms = std::fs::metadata(bot_file_path)?.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(bot_file_path, perms)?;

    Ok(())
}

fn get_os_and_arch() -> Result<(String, String)> {
    let os = match std::env::consts::OS {
        "linux" => "linux",
        "macos" => "macos",
        "windows" => "windows",
        other => return Err(anyhow!("Unsupported OS: {}", other)),
    };

    let arch = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        other => return Err(anyhow!("Unsupported architecture: {}", other)),
    };

    Ok((os.to_string(), arch.to_string()))
}
