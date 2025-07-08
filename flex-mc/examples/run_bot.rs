use std::path::PathBuf;

use anyhow::Result;
use flex_mc::infra::{
    bot_spawner::AzaleaBotSpawner,
    chunk_generator::{ChunkGenerator, DefaultChunkGenerator},
    free_port_finder::DefaultFreePortFinder,
};
use ssmc_core::{
    domain::McVanillaVersionId,
    infra::{
        file_bundle_loader::DefaultFileBundleLoader,
        fs_handler::DefaultFsHandler,
        mc_java::{DefaultMcJavaLoader, McJavaLoader},
        url_fetcher::{self, DefaultUrlFetcher, DummyUrlFetcher},
        vanilla::VanillaVersionLoader,
        virtual_fs::VirtualFs,
    },
};

#[tokio::main]
async fn main() -> Result<()> {
    let dim = PathBuf::from("examples/work/server");

    let fs_handler = Box::new(DefaultFsHandler::new());
    let url_fetcher = Box::new(DefaultUrlFetcher);
    let file_bundle_loader = Box::new(DefaultFileBundleLoader::new(
        fs_handler.clone(),
        url_fetcher.clone(),
    ));
    let java_loader = Box::new(DefaultMcJavaLoader::new(
        url_fetcher.clone(),
        file_bundle_loader,
        dim.join("java"),
    ));

    let chunk_generator = DefaultChunkGenerator::new(
        VanillaVersionLoader::new(url_fetcher.clone(), java_loader),
        Box::new(DefaultFileBundleLoader::new(
            fs_handler.clone(),
            url_fetcher.clone(),
        )),
        Box::new(AzaleaBotSpawner::new(dim.join("azalea-bot"))),
        Box::new(DefaultFreePortFinder),
        dim.clone(),
    );

    let vfs = VirtualFs::new(url_fetcher, fs_handler);

    chunk_generator
        .generate_chunks(vfs, &McVanillaVersionId::new("1.21.6".to_string()), &[])
        .await?;

    Ok(())
}
