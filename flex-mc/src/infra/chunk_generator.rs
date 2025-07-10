use anyhow::Result;
use itertools::Itertools;
use java_properties;
use ssmc_core::{
    domain::{McServerLoader, McVanillaVersionId, ServerRunOptions},
    infra::{
        file_bundle_loader::FileBundleLoader,
        vanilla::{McVanillaVersion, McVanillaVersionType, VanillaVersionLoader},
        virtual_fs::{VirtualEntryType, VirtualFs, VirtualPath},
    },
};
use std::{
    cmp::Ordering,
    collections::{BTreeSet, HashMap, HashSet},
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

#[async_trait::async_trait]
pub trait ChunkGenerator {
    async fn generate_chunks(
        &self,
        world_data: VirtualFs,
        version: &McVanillaVersionId,
        chunk_list: &[ChunkPos],
    ) -> Result<()>;
}

pub struct DefaultChunkGenerator {
    version_loader: VanillaVersionLoader,
    file_bundle_loader: Box<dyn FileBundleLoader + Send + Sync>,
    bot_spawner: Box<dyn BotSpawner + Send + Sync>,
    free_port_finder: Box<dyn FreePortFinder + Send + Sync>,
    work_dir: PathBuf,
    max_bot_count: NonZeroUsize,
}

impl DefaultChunkGenerator {
    pub fn new(
        version_loader: VanillaVersionLoader,
        file_bundle_loader: Box<dyn FileBundleLoader + Send + Sync>,
        bot_spawner: Box<dyn BotSpawner + Send + Sync>,
        free_port_finder: Box<dyn FreePortFinder + Send + Sync>,
        work_dir: PathBuf,
        max_bot_count: NonZeroUsize,
    ) -> Self {
        DefaultChunkGenerator {
            version_loader,
            file_bundle_loader,
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
        mut world_data: VirtualFs,
        version: &McVanillaVersionId,
        chunk_list: &[ChunkPos],
    ) -> Result<()> {
        let quad_chunks = BTreeSet::from_iter(chunk_list.iter().map(QuadChunkPos::from_chunk));

        let (filebundle, mut command) = {
            let (filebundle, command_factory) = self
                .version_loader
                .ready_server(
                    world_data.export_to_file_bundle()?,
                    &McVanillaVersion {
                        version: McVanillaVersionId::new(version.id().to_string()),
                        version_type: McVanillaVersionType::Release,
                    },
                )
                .await
                .map_err(|x| anyhow::anyhow!(x))?;
            let command = command_factory(ServerRunOptions::default());
            (filebundle, command)
        };

        world_data.clear();
        world_data.load_file_bundle(&filebundle)?;

        let host = [127, 0, 0, 1].into();
        let port = self.free_port_finder.find_free_port(host)?;
        {
            let properties_path = VirtualPath::from_str("server.properties");
            let mut props = {
                if world_data.get_entry_type(&properties_path) == Some(VirtualEntryType::File) {
                    let properties = world_data.read_file_content(&properties_path).await?;
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
            world_data.write_file_content(&properties_path, &buffer, 0o644)?;
        }
        {
            world_data.write_file_content(
                &VirtualPath::from_str("eula.txt"),
                "eula=true".as_bytes(),
                0o644,
            )?;
        }

        let tmpdir = self.work_dir.join("server");
        world_data.mount_to_physical_fs(&tmpdir).await?;

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

        let join = thread.join();

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
