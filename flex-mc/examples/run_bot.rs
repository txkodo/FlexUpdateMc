use std::{num::NonZeroUsize, path::PathBuf, sync::Arc};

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
        fs_handler::DefaultFsHandler, mc_java::DefaultMcJavaLoader, trie_loader::DefaultTrieLoader,
        url_fetcher::DefaultUrlFetcher, vanilla::VanillaVersionLoader,
    },
    util::file_trie::Dir,
};

#[tokio::main]
async fn main() -> Result<()> {
    let dim = PathBuf::from("examples/work/server");

    let fs_handler = Arc::new(DefaultFsHandler::new());
    let url_fetcher = Arc::new(DefaultUrlFetcher);
    let trie_loader = Arc::new(DefaultTrieLoader::new(
        fs_handler.clone(),
        url_fetcher.clone(),
    ));
    let java_loader = Arc::new(DefaultMcJavaLoader::new(
        url_fetcher.clone(),
        trie_loader,
        dim.join("java"),
    ));

    let chunk_generator = DefaultChunkGenerator::new(
        VanillaVersionLoader::new(url_fetcher.clone(), java_loader),
        Box::new(AzaleaBotSpawner::new(dim.join("azalea-bot"))),
        Box::new(DefaultFreePortFinder),
        Arc::new(DefaultTrieLoader::new(fs_handler.clone(), url_fetcher)),
        dim.clone(),
        unsafe { NonZeroUsize::new_unchecked(10) },
    );

    let world_data = Dir::new();

    let chunks: Vec<ChunkPos> = (-5..5)
        .flat_map(|x| (-5..5).map(move |z| ChunkPos::new(x, z)))
        .collect();

    chunk_generator
        .generate_chunks(
            world_data,
            &McVanillaVersionId::new("1.21.7".to_string()),
            &chunks,
        )
        .await?;

    println!("チャンク生成完了！");

    Ok(())
}
