use std::{
    io::{BufRead, BufReader},
    net::IpAddr,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    sync::mpsc,
    thread,
    time::Duration,
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
    ) -> Result<(Box<dyn BotHandle>, mpsc::Receiver<(i32, i32)>)>;
}

pub trait BotHandle: Send {
    fn name(&self) -> String;
    fn stop(self: Box<Self>) -> Result<()>;
}

pub struct AzaleaBotSpawner {
    bot_file_path: PathBuf,
    max_retries: u32,
    retry_delay: Duration,
}

impl AzaleaBotSpawner {
    pub fn new(bot_file_path: PathBuf) -> Self {
        AzaleaBotSpawner { 
            bot_file_path,
            max_retries: 3,
            retry_delay: Duration::from_secs(5),
        }
    }

    pub fn with_retry_config(bot_file_path: PathBuf, max_retries: u32, retry_delay: Duration) -> Self {
        AzaleaBotSpawner { 
            bot_file_path,
            max_retries,
            retry_delay,
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "type")]
pub enum BotEvent {
    #[serde(rename = "spawn")]
    Spawn {},
    #[serde(rename = "disconnect")]
    Disconnect { reason: String },
    #[serde(rename = "chunk")]
    Chunk { x: i32, z: i32 },
}

#[async_trait]
impl BotSpawner for AzaleaBotSpawner {
    async fn spawn_bot(
        &self,
        host: &IpAddr,
        port: u16,
        version: &McVanillaVersionId,
        name: &str,
    ) -> Result<(Box<dyn BotHandle>, mpsc::Receiver<(i32, i32)>)> {
        let mut retry_count = 0;
        
        let (child, mut lines, tx, rx) = loop {
            if !self.bot_file_path.exists() {
                download_bot_executable(&self.bot_file_path, &version.id()).await?;
            }
            let mut command = std::process::Command::new(&self.bot_file_path);

            command
                .args([
                    "--username",
                    name,
                    "--host",
                    &host.to_string(),
                    "--port",
                    &port.to_string(),
                ])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped());

            let mut child = command.spawn()?;

            let (tx, rx) = mpsc::channel::<(i32, i32)>();

            let stdout = child
                .stdout
                .take()
                .ok_or_else(|| anyhow!("Failed to capture bot process stdout"))?;
            let mut lines = BufReader::new(stdout).lines();

            let stderr = child
                .stderr
                .take()
                .ok_or_else(|| anyhow!("Failed to capture bot process stdout"))?;
            let mut err_lines = BufReader::new(stderr).lines();
            let name2 = name.to_string();
            // エラーログ処理スレッドを起動
            thread::spawn(move || {
                while let Some(Ok(line)) = err_lines.next() {
                    println!("Received err: {} {}", name2, line);
                }
            });

            // ログイン完了まで待機
            let mut logged_in = false;
            let mut should_retry = false;
            
            while let Some(Ok(line)) = lines.next() {
                let event: BotEvent = serde_json::from_str(&line)?;
                match event {
                    BotEvent::Spawn {} => {
                        println!("Bot {} logged in successfully", name);
                        logged_in = true;
                        break;
                    }
                    BotEvent::Disconnect { reason } => {
                        println!("Bot {} disconnected during login: {}", name, reason);
                        should_retry = true;
                        break;
                    }
                    BotEvent::Chunk { x, z } => {
                        tx.send((x, z)).unwrap();
                    }
                }
            }

            if logged_in {
                break (child, lines, tx, rx);
            }

            if should_retry && retry_count < self.max_retries {
                retry_count += 1;
                println!("Retrying bot {} connection (attempt {}/{})", name, retry_count, self.max_retries);
                tokio::time::sleep(self.retry_delay).await;
                continue;
            }

            return Err(anyhow!("Bot {} failed to log in after {} attempts", name, retry_count + 1));
        };

        let name_clone = name.to_string();
        let tx_clone = tx.clone();

        // ログイン完了後も継続してイベントを処理するスレッドを起動
        thread::spawn(move || {
            while let Some(Ok(line)) = lines.next() {
                if let Ok(event) = serde_json::from_str::<BotEvent>(&line) {
                    match event {
                        BotEvent::Chunk { x, z } => {
                            if tx_clone.send((x, z)).is_err() {
                                // レシーバーが閉じられた場合はスレッドを終了
                                break;
                            }
                        }
                        BotEvent::Disconnect { reason } => {
                            println!("Bot {} disconnected after login: {}", name_clone, reason);
                            break;
                        }
                        BotEvent::Spawn {} => {
                            // Already logged in, ignore
                        }
                    }
                }
            }
        });

        let handle = Box::new(AzaleaBotHandle {
            process: child,
            name: name.to_string(),
        });

        Ok((handle, rx))
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
