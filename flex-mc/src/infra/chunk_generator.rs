use anyhow::Result;
use java_properties;
use rand::{SeedableRng, rngs::StdRng, seq::IteratorRandom};
use ssmc_core::{
    domain::{McServerLoader, McVanillaVersionId, ServerRunOptions},
    infra::{
        trie_loader::TrieLoader,
        vanilla::{McVanillaVersion, McVanillaVersionType, VanillaVersionLoader},
    },
    util::file_trie::{Dir, Entry, File, Path as VirtualPath},
};
use std::{
    collections::{HashMap, HashSet},
    io::Cursor,
    num::NonZeroUsize,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
    vec,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{ChildStdin, Command},
    sync::{Mutex, mpsc},
};

use crate::infra::{
    bot_spawner::BotSpawner, free_port_finder::FreePortFinder, region_loader::ChunkPos,
};
use futures::future;

// ヘルパー関数：ファイルの存在確認
fn file_exists(dir: &Dir, path: &VirtualPath) -> bool {
    match dir.get(path.clone()) {
        Some(Entry::File(_)) => true,
        _ => false,
    }
}

// ヘルパー関数：ファイルを書き込み
fn write_file_content(dir: &mut Dir, path: &VirtualPath, data: &[u8]) -> Result<()> {
    let file = File::inline(data.to_vec(), 0o644);
    dir.put_file(path.clone(), file)
        .map_err(|_| anyhow::anyhow!("Failed to write file"))?;
    Ok(())
}

#[async_trait::async_trait]
pub trait ChunkGenerator {
    async fn generate_chunks(
        &self,
        world_data: Dir,
        version: &McVanillaVersionId,
        chunk_list: &[ChunkPos],
    ) -> Result<()>;
}

pub struct DefaultChunkGenerator {
    version_loader: VanillaVersionLoader,
    bot_spawner: Arc<dyn BotSpawner + Send + Sync>,
    free_port_finder: Box<dyn FreePortFinder + Send + Sync>,
    trie_loader: Arc<dyn TrieLoader + Send + Sync>,
    work_dir: PathBuf,
    max_bot_count: NonZeroUsize,
}

impl DefaultChunkGenerator {
    pub fn new(
        version_loader: VanillaVersionLoader,
        bot_spawner: Arc<dyn BotSpawner + Send + Sync>,
        free_port_finder: Box<dyn FreePortFinder + Send + Sync>,
        trie_loader: Arc<dyn TrieLoader + Send + Sync>,
        work_dir: PathBuf,
        max_bot_count: NonZeroUsize,
    ) -> Self {
        DefaultChunkGenerator {
            version_loader,
            bot_spawner,
            free_port_finder,
            trie_loader,
            work_dir,
            max_bot_count,
        }
    }
}

#[async_trait::async_trait]
impl ChunkGenerator for DefaultChunkGenerator {
    async fn generate_chunks(
        &self,
        mut world_data: Dir,
        version: &McVanillaVersionId,
        chunk_list: &[ChunkPos],
    ) -> Result<()> {
        // ボットを中心に 21 x 21 チャンクが生成される
        let view_distance = 5;
        let bot_count = 3;

        let (new_world_data, command) = {
            let (new_world_data, command_factory) = self
                .version_loader
                .ready_server(
                    world_data,
                    &McVanillaVersion {
                        version: McVanillaVersionId::new(version.id().to_string()),
                        version_type: McVanillaVersionType::Release,
                    },
                )
                .await
                .map_err(|x| anyhow::anyhow!(x))?;
            let command = command_factory(ServerRunOptions::default());
            (new_world_data, command)
        };

        world_data = new_world_data;

        let host = [127, 0, 0, 1].into();
        let port = self.free_port_finder.find_free_port(host)?;
        {
            let properties_path = VirtualPath::from_str("server.properties");
            let mut props = {
                if file_exists(&world_data, &properties_path) {
                    let properties = self
                        .trie_loader
                        .load_content(
                            world_data
                                .get_file(&properties_path)
                                .unwrap_or(&File::inline(vec![], 0o644)),
                        )
                        .await?;
                    java_properties::read(Cursor::new(properties))?
                } else {
                    HashMap::new()
                }
            };

            props.insert("online-mode".into(), "false".to_string());
            props.insert("max-players".into(), 1000.to_string());
            props.insert("server-port".into(), port.to_string());
            props.insert("view-distance".into(), (view_distance).to_string());
            props.insert("gamemode".into(), "creative".to_string());
            props.insert("allow-flight".into(), "true".to_string());

            let mut buffer = vec![];
            java_properties::write(&mut buffer, &props)?;
            write_file_content(&mut world_data, &properties_path, &buffer)?;
        }
        {
            write_file_content(
                &mut world_data,
                &VirtualPath::from_str("eula.txt"),
                "eula=true".as_bytes(),
            )?;
        }

        let tmpdir = self.work_dir.join("server");
        self.trie_loader
            .mount_contents(&world_data, &tmpdir)
            .await?;

        println!("Starting server at {:?}", &tmpdir);
        println!("Starting server at {:?}", &command);
        let mut child = Command::from(command)
            .current_dir(&tmpdir)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()?;
        let stdin = child.stdin.take().unwrap();

        let stdout = child.stdout.take().unwrap();
        let mut lines = BufReader::new(stdout).lines();
        while let Some(line) = lines.next_line().await? {
            if line.ends_with("For help, type \"help\"") {
                break;
            }
        }

        let ungenarated_chunks = Arc::new(std::sync::Mutex::new(
            chunk_list.iter().copied().collect::<HashSet<_>>(),
        ));
        let stdin_shared = Arc::new(Mutex::new(stdin));

        let bot_tasks = (0..bot_count).map(|idx| {
            let bot_id = format!("bot{:02}", idx);
            let bot_spawner = self.bot_spawner.clone();
            let version = version.clone();
            let stdin_clone = stdin_shared.clone();
            let host = host.clone();
            let port = port;
            let ungenarated_chunks = ungenarated_chunks.clone();

            tokio::spawn(async move {
                let (bot, rx) = bot_spawner
                    .spawn_bot(&host, port, &version, &bot_id)
                    .await?;
                spawn_random_gen_bot(bot_id, ungenarated_chunks, rx, stdin_clone).await?;
                bot.stop()?;
                anyhow::Ok(())
            })
        });

        // すべてのタスクの完了を待機

        let results = future::join_all(bot_tasks).await;
        for result in results {
            result??
        }

        // サーバーを停止
        {
            let mut stdin_guard = stdin_shared.lock().await;
            stdin_guard.write_all("stop\n".as_bytes()).await?;
            stdin_guard.flush().await?;
        }
        child.wait().await?;

        Ok(())
    }
}

async fn spawn_random_gen_bot(
    bot_id: String,
    ungenarated_chunks: Arc<std::sync::Mutex<HashSet<ChunkPos>>>,
    mut rx: mpsc::Receiver<(i32, i32)>,
    stdin_mutex: Arc<Mutex<ChildStdin>>,
) -> anyhow::Result<()> {
    let mut rng = StdRng::seed_from_u64(rand::random());

    loop {
        let random_chunk = {
            match ungenarated_chunks.lock().unwrap().iter().choose(&mut rng) {
                Some(chunk) => chunk.clone(),
                None => break,
            }
        };
        // ボットをテレポート
        {
            let mut stdin = stdin_mutex.lock().await;
            println!(
                "tp {} {} 100 {}\n",
                bot_id,
                random_chunk.x * 16 + 8,
                random_chunk.z * 16 + 8
            );
            stdin
                .write_all(
                    format!(
                        "tp {} {} 100 {}\n",
                        bot_id,
                        random_chunk.x * 16 + 8,
                        random_chunk.z * 16 + 8
                    )
                    .as_bytes(),
                )
                .await?;
            stdin.flush().await?;
        }

        let start = Instant::now();
        let duration = Duration::from_secs(5);

        while start.elapsed() < duration {
            let remaining = duration.saturating_sub(start.elapsed());
            match tokio::time::timeout(remaining.min(Duration::from_millis(500)), rx.recv()).await {
                Ok(Some((x, z))) => {
                    let mut ungenarated_chunks = ungenarated_chunks.lock().unwrap();
                    ungenarated_chunks.remove(&ChunkPos::new(x as isize, z as isize));
                    println!(
                        "{} received chunk at ({}, {}) {}",
                        bot_id,
                        x,
                        z,
                        ungenarated_chunks.len()
                    );
                }
                Ok(None) => break, // channel closed
                Err(_) => {
                    // タイムアウト → ループを続ける（時間切れチェック）
                }
            }
        }
    }
    println!("{} finished", bot_id,);
    Ok::<(), anyhow::Error>(())
}
