use std::{num::NonZeroUsize, path::PathBuf};

use anyhow::Result;
use flex_mc::infra::{
    bot_spawner::AzaleaBotSpawner,
    chunk_generator::{ChunkGenerator, DefaultChunkGenerator},
    free_port_finder::DefaultFreePortFinder,
    region_loader::ChunkPos,
};
use ssmc_core::{
    domain::McVanillaVersionId,
    infra::{
        file_bundle_loader::DefaultFileBundleLoader, fs_handler::DefaultFsHandler,
        mc_java::DefaultMcJavaLoader, url_fetcher::DefaultUrlFetcher,
        vanilla::VanillaVersionLoader, virtual_fs::VirtualFs,
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
        unsafe { NonZeroUsize::new_unchecked(10) },
    );

    let vfs = VirtualFs::new(url_fetcher, fs_handler);

    let chunks: Vec<ChunkPos> = (-100..100)
        .flat_map(|x| (-100..100).map(move |z| ChunkPos::new(x, z)))
        .collect();

    chunk_generator
        .generate_chunks(vfs, &McVanillaVersionId::new("1.21.5".to_string()), &chunks)
        .await?;

    Ok(())
}
