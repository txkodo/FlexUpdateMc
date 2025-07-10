use std::{
    io::{BufRead, BufReader},
    net::IpAddr,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
};

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use serde::Deserialize;
use ssmc_core::domain::McVanillaVersionId;

#[async_trait]
pub trait BotSpawner {
    async fn spawn_bot(
        &self,
        host: &IpAddr,
        port: u16,
        version: &McVanillaVersionId,
        name: &str,
    ) -> Result<Box<dyn BotHandle>>;
}

pub trait BotHandle: Sync + Send {
    fn name(&self) -> String;
    fn stop(self: Box<Self>) -> Result<()>;
}

pub struct AzaleaBotSpawner {
    bot_file_path: PathBuf,
}

impl AzaleaBotSpawner {
    pub fn new(bot_file_path: PathBuf) -> Self {
        AzaleaBotSpawner {
            bot_file_path,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "event")]
enum BotEvent {
    #[serde(rename = "login")]
    Login {},
}

#[async_trait]
impl BotSpawner for AzaleaBotSpawner {
    async fn spawn_bot(
        &self,
        host: &IpAddr,
        port: u16,
        version: &McVanillaVersionId,
        name: &str,
    ) -> Result<Box<dyn BotHandle>> {
        if !self.bot_file_path.exists() {
            download_bot_executable(&self.bot_file_path, &version.id()).await?;
        }
        let mut command = std::process::Command::new(&self.bot_file_path);

        command
            .args([name, &host.to_string(), &port.to_string()])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = command.spawn()?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("Failed to capture bot process stdout"))?;

        // ログイン完了まで待機
        while let Some(Ok(line)) = BufReader::new(stdout).lines().next() {
            let event: BotEvent = serde_json::from_str(&line)?;
            match event {
                BotEvent::Login {} => {
                    println!("Bot {} logged in successfully", name);
                    break;
                }
            }
        }

        Ok(Box::new(AzaleaBotHandle {
            process: child,
            name: name.to_string(),
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
        "https://github.com/txkodo/FlexUpdateMcBot/releases/download/v{}/{}",
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
