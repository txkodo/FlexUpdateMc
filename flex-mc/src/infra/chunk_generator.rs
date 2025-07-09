use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Cursor, Write},
    path::PathBuf,
    process::Stdio,
    thread,
    time::Duration,
};

use anyhow::Result;
use java_properties;
use ssmc_core::{
    domain::{McServerLoader, McVanillaVersionId, ServerRunOptions},
    infra::{
        file_bundle_loader::FileBundleLoader,
        vanilla::{McVanillaVersion, McVanillaVersionType, VanillaVersionLoader},
        virtual_fs::{VirtualEntryType, VirtualFs, VirtualPath},
    },
};

use crate::infra::{
    bot_spawner::BotSpawner, free_port_finder::FreePortFinder, region_loader::ChunkPos,
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
}

impl DefaultChunkGenerator {
    pub fn new(
        version_loader: VanillaVersionLoader,
        file_bundle_loader: Box<dyn FileBundleLoader + Send + Sync>,
        bot_spawner: Box<dyn BotSpawner + Send + Sync>,
        free_port_finder: Box<dyn FreePortFinder + Send + Sync>,
        work_dir: PathBuf,
    ) -> Self {
        DefaultChunkGenerator {
            version_loader,
            file_bundle_loader,
            bot_spawner,
            free_port_finder,
            work_dir,
        }
    }
}

static MAX_BOT_COUNT: usize = 100;

#[async_trait::async_trait]
impl ChunkGenerator for DefaultChunkGenerator {
    async fn generate_chunks(
        &self,
        mut world_data: VirtualFs,
        version: &McVanillaVersionId,
        chunk_list: &[ChunkPos],
    ) -> Result<()> {
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

            props.insert("online-mode".into(), "false".into());
            props.insert("max-players".into(), MAX_BOT_COUNT.to_string());
            props.insert("server-port".into(), port.to_string());
            props.insert("gamemode".into(), "creative".to_string());

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
        let thread = thread::spawn(move || -> Result<()> {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                let line = line?;
                println!("{}", line);
            }
            Ok(())
        });

        println!("Server started on {}:{}", host, port);

        thread::sleep(Duration::from_secs(15));

        let bot = self.bot_spawner.spawn_bot(&host, port, version).await?;
        println!("spawn_bot");

        thread::sleep(Duration::from_secs(10));

        stdin.write("tp bot00 10000 100 10000\n".as_bytes())?;
        println!("tp");

        thread::sleep(Duration::from_secs(10));

        bot.stop()?;

        child.kill()?;
        child.wait()?;

        thread.join();

        tmpdir;

        Ok(())
    }
}
