use std::path::PathBuf;

use crate::{
    model::McDimension,
    service::{
        chunk_migrator::ChunkMigrator,
        server_factory::{ServerCreationMethod, ServerFactory},
        server_handler::ChunkGenerationConfig,
        world_handler::WorldHandler,
    },
    util::dir_overwrite,
};

pub struct FlexUpdater {
    work_path: PathBuf,
    chunk_migrator: Box<dyn ChunkMigrator>,
    server_factory: Box<dyn ServerFactory>,
}

impl FlexUpdater {
    pub fn new(
        work_path: PathBuf,
        chunk_migrator: Box<dyn ChunkMigrator>,
        server_factory: Box<dyn ServerFactory>,
    ) -> Self {
        FlexUpdater {
            work_path,
            chunk_migrator,
            server_factory,
        }
    }

    pub fn update_flex(
        &self,
        source_world: &dyn WorldHandler,
        target_world: &dyn WorldHandler,
        old: &ChunkGenerationConfig,
        new: &ChunkGenerationConfig,
    ) -> Result<(), String> {
        // 元のワールドデータを用いてサーバーを作成
        let target_server = self.server_factory.create_server_handler_from_world(
            source_world,
            &self.work_path.join("old_edited"),
            ServerCreationMethod::WithRegion,
        )?;

        let old_plain_server = self.server_factory.create_server_handler_from_world(
            source_world,
            &self.work_path.join("old_plain"),
            ServerCreationMethod::WithoutRegion,
        )?;

        let new_plain_server = self.server_factory.create_server_handler_from_world(
            source_world,
            &self.work_path.join("new_plain"),
            ServerCreationMethod::WithoutRegion,
        )?;

        let chunks = target_server.list_chunks()?;

        // すべてのチャンクを新バージョンのフォーマットに更新
        target_server.generate_chunks(&chunks, new)?;

        // 旧バージョンでチャンクを新規生成し、新バージョンのフォーマットに更新
        old_plain_server.generate_chunks(&chunks, old)?;
        old_plain_server.generate_chunks(&chunks, new)?;

        // 新バージョンのチャンクを新規生成
        new_plain_server.generate_chunks(&chunks, new)?;

        // すべてのチャンクを移行
        for pos in &chunks {
            let old_edited_chunk = target_server.load_chunk(pos)?;
            let old_plain_chunk = old_plain_server.load_chunk(pos)?;
            let new_plain_chunk = new_plain_server.load_chunk(pos)?;

            // マイグレーションを行い、新しいチャンクを生成
            let new_chunk = self.chunk_migrator.migrate(
                &old_edited_chunk,
                &old_plain_chunk,
                &new_plain_chunk,
            )?;
            // チャンクを更新
            target_server.save_chunk(&new_chunk)?;
        }

        let dims = [
            McDimension::Overworld,
            McDimension::Nether,
            McDimension::TheEnd,
        ];
        for dim in &dims {
            // 各ディメンションのチャンクを更新
            dir_overwrite::overwrite_dir(
                &target_world.get_region_dir(dim)?,
                &target_server.get_region_dir(dim)?,
            )
            .map_err(|e| format!("Failed to overwrite region directory for {:?}: {}", dim, e))?;
        }
        return Ok(());
    }
}
