use anyhow::Result;
use itertools::Itertools;
use java_properties;
use ssmc_core::{
    domain::{McServerLoader, McVanillaVersionId, ServerRunOptions},
    infra::{
        fs_handler::FsHandler, 
        url_fetcher::UrlFetcher,
        vanilla::{McVanillaVersion, McVanillaVersionType, VanillaVersionLoader},
    },
    util::file_trie::{Dir, Entry, File, Path as VirtualPath},
};
use std::{
    cmp::Ordering,
    collections::{BTreeSet, HashMap},
    sync::Arc,
    io::{BufRead, BufReader, Cursor, Write},
    num::NonZeroUsize,
    path::PathBuf,
    process::Stdio,
    thread,
    time::Duration,
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
    url_fetcher: &dyn UrlFetcher
) -> Result<Vec<u8>> {
    let file = dir.get_file(path.clone())
        .ok_or_else(|| anyhow::anyhow!("File not found: {:?}", path))?;

    match file {
        File::Inline(data) => Ok(data.clone()),
        File::Path(path_buf) => {
            fs_handler.read(path_buf)
                .map_err(|e| anyhow::anyhow!("Failed to read file: {}", e))
        }
        File::Url(url) => {
            url_fetcher.fetch_binary(url).await
                .map_err(|e| anyhow::anyhow!("Failed to fetch URL: {}", e))
        }
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

// ヘルパー関数：物理ファイルシステムにマウント
async fn mount_to_physical_fs(
    dir: &Dir, 
    base_path: &std::path::Path, 
    fs_handler: &dyn FsHandler, 
    url_fetcher: &dyn UrlFetcher
) -> Result<()> {
    mount_dir_to_physical_fs(base_path, &VirtualPath::new(), dir, fs_handler, url_fetcher).await
}

fn mount_dir_to_physical_fs<'a>(
    base_path: &'a std::path::Path, 
    rel_path: &'a VirtualPath, 
    dir: &'a Dir,
    fs_handler: &'a dyn FsHandler, 
    url_fetcher: &'a dyn UrlFetcher
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
    Box::pin(async move {
        if !rel_path.is_empty() {
            let path_str = rel_path.components().join("/");
            let physical_path = base_path.join(&path_str);
            println!("Mounting directory {} to {}", path_str, physical_path.display());
            fs_handler.mkdir(&physical_path)
                .map_err(|e| anyhow::anyhow!("Failed to create directory: {}", e))?;
        }
        
        for (name, child_entry) in dir.iter() {
            let mut child_path = rel_path.clone();
            child_path.push(name);
            
            match child_entry {
                Entry::File(file) => {
                    let path_str = child_path.components().join("/");
                    let physical_path = base_path.join(&path_str);
                    
                    let data = match file {
                        File::Inline(data) => data.clone(),
                        File::Path(path_buf) => {
                            fs_handler.read(path_buf)
                                .map_err(|e| anyhow::anyhow!("Failed to read file: {}", e))?
                        }
                        File::Url(url) => {
                            url_fetcher.fetch_binary(url).await
                                .map_err(|e| anyhow::anyhow!("Failed to fetch URL: {}", e))?
                        }
                    };
                    
                    println!("Mounting file {} to {}", path_str, physical_path.display());
                    fs_handler.write(&physical_path, &data, false)
                        .map_err(|e| anyhow::anyhow!("Failed to write file: {}", e))?;
                }
                Entry::Dir(child_dir) => {
                    mount_dir_to_physical_fs(base_path, &child_path, child_dir, fs_handler, url_fetcher).await?;
                }
            }
        }
        Ok(())
    })
}

#[async_trait::async_trait]
pub trait ChunkGenerator {
    async fn generate_chunks(
        &self,
        world_data: Dir,
        fs_handler: Arc<dyn FsHandler + Send + Sync>,
        url_fetcher: Arc<dyn UrlFetcher + Send + Sync>,
        version: &McVanillaVersionId,
        chunk_list: &[ChunkPos],
    ) -> Result<()>;
}

pub struct DefaultChunkGenerator {
    version_loader: VanillaVersionLoader,
    bot_spawner: Box<dyn BotSpawner + Send + Sync>,
    free_port_finder: Box<dyn FreePortFinder + Send + Sync>,
    work_dir: PathBuf,
    max_bot_count: NonZeroUsize,
}

impl DefaultChunkGenerator {
    pub fn new(
        version_loader: VanillaVersionLoader,
        bot_spawner: Box<dyn BotSpawner + Send + Sync>,
        free_port_finder: Box<dyn FreePortFinder + Send + Sync>,
        work_dir: PathBuf,
        max_bot_count: NonZeroUsize,
    ) -> Self {
        DefaultChunkGenerator {
            version_loader,
            bot_spawner,
            free_port_finder,
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
        fs_handler: Arc<dyn FsHandler + Send + Sync>,
        url_fetcher: Arc<dyn UrlFetcher + Send + Sync>,
        version: &McVanillaVersionId,
        chunk_list: &[ChunkPos],
    ) -> Result<()> {
        let quad_chunks = BTreeSet::from_iter(chunk_list.iter().map(QuadChunkPos::from_chunk));

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
                    let properties = read_file_content(&world_data, &properties_path, &*fs_handler, &*url_fetcher).await?;
                    java_properties::read(Cursor::new(properties))?
                } else {
                    HashMap::new()
                }
            };

            props.insert("online-mode".into(), "false".to_string());
            props.insert("max-players".into(), self.max_bot_count.to_string());
            props.insert("server-port".into(), port.to_string());
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
        mount_to_physical_fs(&world_data, &tmpdir, &*fs_handler, &*url_fetcher).await?;

        println!("Starting server at {:?}", &tmpdir);
        println!("Starting server at {:?}", &command);
        let mut child = command
            .current_dir(&tmpdir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;
        let mut stdin = child.stdin.take().unwrap();

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
        let thread = thread::spawn(move || -> Result<()> { Ok(()) });

        let mut bots = vec![];

        for idx in 0..self.max_bot_count.get() {
            let bot = self
                .bot_spawner
                .spawn_bot(&host, port, version, &format!("bot{:02}", idx))
                .await?;
            bots.push(bot);
        }

        let quad_chunks_iter = quad_chunks.iter().chunks(self.max_bot_count.into());

        let start_time = std::time::Instant::now();
        let mut processesd_qchunk_count = 0;
        let total_qchunk_count = quad_chunks.len();
        let mut processesd_chunk_batch_count = 0;
        for chunk_batch in &quad_chunks_iter {
            let mean_batch_process_millisec = if processesd_chunk_batch_count > 10 {
                start_time.elapsed().as_millis() / processesd_chunk_batch_count
            } else {
                50
            };

            let chunks = chunk_batch.collect::<Vec<_>>();
            // チャンクを生成するために、各ボットを適切な位置にテレポート
            for (idx, chunk) in chunks.iter().enumerate() {
                let (x, z) = chunk.center_block_pos();
                stdin.write(format!("tp bot{:02} {} 100 {}\n", idx, x, z).as_bytes())?;
            }
            stdin.flush()?;

            thread::sleep(Duration::from_millis(
                mean_batch_process_millisec as u64 / 2,
            ));

            'wait_gen: loop {
                for chunk in chunks.iter() {
                    let (x, z) = chunk.center_block_pos();
                    let min_pos = format!("{} 100 {}", x - 1, z - 1);
                    let max_pos = format!("{} 100 {}", x, z);
                    let command =
                        format!("clone {} {} {} replace force\n", min_pos, max_pos, min_pos);
                    stdin.write(command.as_bytes())?;
                }
                stdin.flush()?;

                let mut success_count = 0;
                let mut failure_count = 0;
                while let Some(line) = lines.next() {
                    if let Ok(line) = line {
                        if line.ends_with("That position is not loaded") {
                            failure_count += 1;
                        }
                        if line.contains("Successfully cloned 4 block") {
                            success_count += 1;
                        }
                        if (success_count + failure_count) == chunks.len() {
                            if failure_count == 0 {
                                break 'wait_gen;
                            } else {
                                break;
                            }
                        }
                    }
                }
            }
            processesd_qchunk_count += chunks.len();
            println!(
                "Chunk batch generation {}/{} [mean: {}ms]",
                processesd_qchunk_count, total_qchunk_count, mean_batch_process_millisec
            );
            processesd_chunk_batch_count += 1;
        }
        let elapsed = start_time.elapsed();

        println!(
            "Chunk generation completed in {:.2?}. {:.2}it/s",
            elapsed,
            (chunk_list.len() as f32 / elapsed.as_secs_f32())
        );

        stdin.write("save-all\n".as_bytes())?;

        thread::sleep(Duration::from_secs(30));

        for bot in bots {
            bot.stop()?;
        }

        child.kill()?;
        child.wait()?;

        let _join = thread.join();

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct QuadChunkPos {
    x: isize,
    z: isize,
}

impl PartialOrd for QuadChunkPos {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp_private(other))
    }
}
impl Ord for QuadChunkPos {
    fn cmp(&self, other: &Self) -> Ordering {
        self.cmp_private(other)
    }
}

impl QuadChunkPos {
    pub fn from_chunk(chunk_pos: &ChunkPos) -> Self {
        QuadChunkPos {
            x: chunk_pos.x.div_euclid(2),
            z: chunk_pos.z.div_euclid(2),
        }
    }

    pub fn region_pos(&self) -> RegionPos {
        RegionPos::new(self.x.div_euclid(16), self.z.div_euclid(16))
    }

    fn cmp_private(&self, other: &Self) -> Ordering {
        match self.region_pos().cmp(&other.region_pos()) {
            Ordering::Greater => Ordering::Greater,
            Ordering::Less => Ordering::Less,
            Ordering::Equal => (self.x, self.z).cmp(&(other.x, other.z)),
        }
    }

    fn center_block_pos(&self) -> (isize, isize) {
        let bx = self.x * 32 + 16;
        let bz = self.z * 32 + 16;
        (bx, bz)
    }
}
