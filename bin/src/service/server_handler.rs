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

    fn execute_chunk_generation(&mut self, pos: &[McChunkPos], config: &ChunkGenerationConfig) -> Result<(), String> {
        use std::process::{Command, Stdio};
        use std::io::Write;
        use std::time::Duration;

        // Java実行可能ファイルのパスを決定
        let java_path = config.java_path
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "java".to_string());

        // サーバーJARファイルのパス
        let jar_path = self.path.join(&config.jar_file_name);
        if !jar_path.exists() {
            return Err(format!("Server JAR file not found: {:?}", jar_path));
        }

        // サーバープロパティファイルを作成/更新
        self.setup_server_properties()?;

        // EULAファイルを作成
        self.create_eula_file()?;

        // Minecraftサーバーを起動
        let mut child = Command::new(&java_path)
            .args(&[
                "-Xmx2G",
                "-Xms1G",
                "-jar",
                &config.jar_file_name,
                "nogui"
            ])
            .current_dir(&self.path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to start Minecraft server: {}", e))?;

        // サーバーの起動を待機
        std::thread::sleep(Duration::from_secs(10));

        // チャンク生成のためのボット呼び出し
        self.generate_chunks_with_bot(pos, &mut child)?;

        // サーバーを正常終了
        if let Some(stdin) = child.stdin.as_mut() {
            stdin.write_all(b"stop\n")
                .map_err(|e| format!("Failed to stop server: {}", e))?;
        }

        // サーバーの終了を待機
        let output = child.wait_with_output()
            .map_err(|e| format!("Failed to wait for server completion: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Server execution failed: {}", stderr));
        }

        Ok(())
    }

    fn verify_chunks_generated(&self, pos: &[McChunkPos]) -> Result<(), String> {
        for chunk_pos in pos {
            let region_dir = self.get_region_dir(&chunk_pos.dimension);
            match self.region_handler.load_chunk(&region_dir, chunk_pos) {
                Ok(_) => continue,
                Err(_) => return Err(format!("Chunk generation failed for {:?}", chunk_pos)),
            }
        }
        Ok(())
    }

    fn setup_server_properties(&self) -> Result<(), String> {
        let properties_path = self.path.join("server.properties");
        let properties_content = format!(
            "level-name={}\n\
            online-mode=false\n\
            spawn-protection=0\n\
            max-players=1\n\
            difficulty=peaceful\n\
            gamemode=creative\n\
            force-gamemode=true\n\
            level-type=default\n",
            self.world_dir_name
        );

        fs::write(&properties_path, properties_content)
            .map_err(|e| format!("Failed to write server.properties: {}", e))?;

        Ok(())
    }

    fn create_eula_file(&self) -> Result<(), String> {
        let eula_path = self.path.join("eula.txt");
        let eula_content = "eula=true\n";

        fs::write(&eula_path, eula_content)
            .map_err(|e| format!("Failed to write eula.txt: {}", e))?;

        Ok(())
    }

    fn generate_chunks_with_bot(&self, pos: &[McChunkPos], _server_process: &mut std::process::Child) -> Result<(), String> {
        use std::process::Command;
        use std::time::Duration;

        // ボットバイナリのパスを構築
        let bot_binary = self.path
            .parent()
            .ok_or("Invalid server path")?
            .parent()
            .ok_or("Invalid project structure")?
            .join("target/debug/bot");

        if !bot_binary.exists() {
            return Err(format!("Bot binary not found: {:?}. Please build the bot first.", bot_binary));
        }

        // 各チャンクに対してボットを実行
        for chunk_pos in pos {
            let world_x = chunk_pos.x * 16 + 8; // チャンクの中央座標
            let world_z = chunk_pos.y * 16 + 8;

            let mut bot_command = Command::new(&bot_binary)
                .args(&[
                    "--server", "localhost:25565",
                    "--username", "ChunkBot",
                    "--target-x", &world_x.to_string(),
                    "--target-z", &world_z.to_string(),
                    "--dimension", &format!("{:?}", chunk_pos.dimension),
                ])
                .spawn()
                .map_err(|e| format!("Failed to start bot: {}", e))?;

            // ボットの完了を待機（タイムアウト付き）
            let mut timeout_counter = 0;
            loop {
                match bot_command.try_wait() {
                    Ok(Some(status)) => {
                        if !status.success() {
                            return Err(format!("Bot failed for chunk {:?}", chunk_pos));
                        }
                        break;
                    }
                    Ok(None) => {
                        // まだ実行中
                        std::thread::sleep(Duration::from_millis(100));
                        timeout_counter += 1;
                        if timeout_counter > 300 { // 30秒でタイムアウト
                            let _ = bot_command.kill();
                            return Err(format!("Bot timeout for chunk {:?}", chunk_pos));
                        }
                    }
                    Err(e) => return Err(format!("Error waiting for bot: {}", e)),
                }
            }

            // チャンク間の待機時間
            std::thread::sleep(Duration::from_millis(1000));
        }

        Ok(())
    }
}

impl<T: RegionHandler> ServerHandler for ServerHandlerImpl<T> {
    fn list_chunks(&self, dimension: McDimension) -> Result<Vec<McChunkPos>, String> {
        let region_dir = self.get_region_dir(&dimension);
        self.region_handler.list_chunks(&region_dir)
    }

    fn generate_chunks(
        &mut self,
        pos: &[McChunkPos],
        config: &ChunkGenerationConfig,
    ) -> Result<(), String> {
        if pos.is_empty() {
            return Ok(());
        }

        // 1. サーバーを起動してチャンク生成を実行
        self.execute_chunk_generation(pos, config)?;

        // 2. 生成されたチャンクが存在することを確認
        self.verify_chunks_generated(pos)?;

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

pub fn create_server_handler(
    path: PathBuf,
    structure: ServerDirStructure,
    world_dir_name: String,
) -> ServerHandlerImpl<crate::service::region_handler::AnvilRegionHandler> {
    use crate::service::region_handler::AnvilRegionHandler;
    ServerHandlerImpl::new(path, structure, world_dir_name, AnvilRegionHandler::new())
}
