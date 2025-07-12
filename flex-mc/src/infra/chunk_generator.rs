use anyhow::Result;
use java_properties;
use ssmc_core::{
    domain::{McServerLoader, McVanillaVersionId, ServerRunOptions},
    infra::{
        fs_handler::FsHandler,
        trie_loader::TrieLoader,
        url_fetcher::UrlFetcher,
        vanilla::{McVanillaVersion, McVanillaVersionType, VanillaVersionLoader},
    },
    util::file_trie::{Dir, Entry, File, Path as VirtualPath},
};
use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
    io::{BufRead, BufReader, Cursor, Write},
    num::NonZeroUsize,
    path::PathBuf,
    process::Stdio,
    sync::{Arc, Mutex},
    vec,
};

use crate::infra::{
    bot_spawner::BotSpawner,
    free_port_finder::FreePortFinder,
    region_loader::{ChunkPos, RegionPos},
};

// ヘルパー関数：ファイルの内容を読み取る
async fn read_file_content(
    dir: &Dir,
    path: &VirtualPath,
    fs_handler: &dyn FsHandler,
    url_fetcher: &dyn UrlFetcher,
) -> Result<Vec<u8>> {
    let file = dir
        .get_file(path.clone())
        .ok_or_else(|| anyhow::anyhow!("File not found: {:?}", path))?;

    match file {
        File::Inline(data) => Ok(data.clone()),
        File::Path(path_buf) => fs_handler
            .read(path_buf)
            .map_err(|e| anyhow::anyhow!("Failed to read file: {}", e)),
        File::Url(url) => url_fetcher
            .fetch_binary(url)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to fetch URL: {}", e)),
    }
}

// ヘルパー関数：ファイルの存在確認
fn file_exists(dir: &Dir, path: &VirtualPath) -> bool {
    match dir.get(path.clone()) {
        Some(Entry::File(_)) => true,
        _ => false,
    }
}

// ヘルパー関数：ファイルを書き込み
fn write_file_content(dir: &mut Dir, path: &VirtualPath, data: &[u8]) -> Result<()> {
    let file = File::Inline(data.to_vec());
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
    bot_spawner: Box<dyn BotSpawner + Send + Sync>,
    free_port_finder: Box<dyn FreePortFinder + Send + Sync>,
    trie_loader: Arc<dyn TrieLoader + Send + Sync>,
    work_dir: PathBuf,
    max_bot_count: NonZeroUsize,
}

impl DefaultChunkGenerator {
    pub fn new(
        version_loader: VanillaVersionLoader,
        bot_spawner: Box<dyn BotSpawner + Send + Sync>,
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
        // ボットを中心に 41 x 41 チャンクが生成される
        let view_distance = 20;

        let (new_world_data, mut command) = {
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
                                .unwrap_or(&File::Inline(vec![])),
                        )
                        .await?;
                    java_properties::read(Cursor::new(properties))?
                } else {
                    HashMap::new()
                }
            };

            props.insert("online-mode".into(), "false".to_string());
            props.insert("max-players".into(), self.max_bot_count.to_string());
            props.insert("server-port".into(), port.to_string());
            props.insert("view-distance".into(), view_distance.to_string());
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
        let mut child = command
            .current_dir(&tmpdir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;
        let stdin = child.stdin.take().unwrap();

        let stdout = child.stdout.take().unwrap();
        let mut lines = BufReader::new(stdout).lines();
        while let Some(line) = lines.next() {
            {
                if let Ok(line) = line {
                    if line.ends_with("For help, type \"help\"") {
                        break;
                    }
                }
            }
        }

        let chunk_groups = group_chunks(chunk_list.to_vec(), view_distance);
        let stdin_shared = Arc::new(Mutex::new(stdin));

        // 並列でボットタスクを実行
        let mut tasks = vec![];

        // 並列処理のために、spawn_botを各タスクで非同期実行
        for (idx, (center, chunks)) in chunk_groups.into_iter().enumerate() {
            let chunks: HashSet<_> = chunks.into_iter().collect();
            let botname = format!("bot{:02}", idx);
            let stdin_clone = stdin_shared.clone();
            let host = host.clone();
            let port = port;
            let version = version.clone();

            let (bot, rx) = self
                .bot_spawner
                .spawn_bot(&host, port, &version, &botname)
                .await?;

            let task = tokio::spawn(async move {
                // ボットをテレポート
                {
                    let mut stdin_guard = stdin_clone.lock().unwrap();
                    stdin_guard.write(
                        format!(
                            "tp {} {} 100 {}\n",
                            botname,
                            center.x * 16 + 8,
                            center.z * 16 + 8
                        )
                        .as_bytes(),
                    )?;
                    stdin_guard.flush()?;
                }

                // チャンクイベントを待機
                let mut remaining_chunks = chunks;
                let chunk_count = remaining_chunks.len();
                for (x, z) in rx {
                    remaining_chunks.remove(&ChunkPos::new(x as isize, z as isize));
                    println!(
                        "Bot {} received chunk at ({}, {}) {}/{})",
                        botname,
                        x,
                        z,
                        chunk_count - remaining_chunks.len(),
                        chunk_count
                    );
                    if remaining_chunks.is_empty() {
                        break;
                    }
                }

                bot.stop()?;
                Ok::<(), anyhow::Error>(())
            });

            tasks.push(task);
        }

        // すべてのタスクの完了を待機
        for task in tasks {
            task.await??;
        }

        // サーバーを停止
        {
            let mut stdin_guard = stdin_shared.lock().unwrap();
            stdin_guard.write("stop\n".as_bytes())?;
            stdin_guard.flush()?;
        }
        child.wait()?;

        Ok(())
    }
}

fn group_chunks(chunks: Vec<ChunkPos>, render_distance: usize) -> Vec<(ChunkPos, Vec<ChunkPos>)> {
    let mut groups = HashMap::<ChunkPos, Vec<ChunkPos>>::new();

    let box_size = (render_distance * 2 + 1) as isize;

    for chunk in chunks {
        let x = (chunk.x + render_distance as isize).div_euclid(box_size);
        let z = (chunk.z + render_distance as isize).div_euclid(box_size);
        let center_chunk = ChunkPos::new(x, z);
        groups
            .entry(center_chunk)
            .or_insert_with(Vec::new)
            .push(chunk);
    }
    return groups
        .into_iter()
        .map(|(center, chunks)| (center, chunks))
        .collect();
}
