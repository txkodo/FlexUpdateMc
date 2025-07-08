// use std::path::{Path, PathBuf};

// use ssmc_core::{domain::McVanillaVersionId, infra::file_bundle_creator::FileBundleCreator};

// use crate::service::{
//     chunk_migrator::ChunkMigrator,
//     server_handler::{ChunkGenerationConfig, ServerHandler},
// };

// pub struct FlexUpdater {
//     work_path: PathBuf,
//     chunk_migrator: Box<dyn ChunkMigrator>,
// }

// impl FlexUpdater {
//     pub fn new(work_path: PathBuf, chunk_migrator: Box<dyn ChunkMigrator>) -> Self {
//         FlexUpdater {
//             work_path,
//             chunk_migrator,
//         }
//     }

//     pub fn update_flex(
//         &self,
//         source_world_path: &Path,
//         target_world_path: &Path,
//         old: &McVanillaVersionId,
//         new: &McVanillaVersionId,
//     ) -> Result<(), String> {
//         let regions = FileBundleCreator::create_from_path(
//             &self,
//             source_world_path.to_path_buf().push("world/region"),
//         );

//         // 元のワールドデータを用いてサーバーを作成
//         let mut target_server = source_world.copy_to(&self.work_path.join("old_edited"))?;

//         let mut old_plain_server = source_world.copy_to(&self.work_path.join("old_edited"))?;
//         old_plain_server.clear_dimension_all()?;

//         let mut new_plain_server = old_plain_server.copy_to(&self.work_path.join("old_edited"))?;
//         let mut result_server = old_plain_server.copy_to(target_world_path)?;

//         let chunks = target_server.list_chunks()?;

//         // すべてのチャンクを新バージョンのフォーマットに更新
//         target_server.generate_chunks(&chunks, new)?;

//         // 旧バージョンでチャンクを新規生成し、新バージョンのフォーマットに更新
//         old_plain_server.generate_chunks(&chunks, old)?;
//         old_plain_server.generate_chunks(&chunks, new)?;

//         // 新バージョンのチャンクを新規生成
//         new_plain_server.generate_chunks(&chunks, new)?;

//         // すべてのチャンクを移行
//         for pos in &chunks {
//             let old_edited_chunk = target_server.load_chunk(pos)?;
//             let old_plain_chunk = old_plain_server.load_chunk(pos)?;
//             let new_plain_chunk = new_plain_server.load_chunk(pos)?;

//             // マイグレーションを行い、新しいチャンクを生成
//             let new_chunk = self.chunk_migrator.migrate(
//                 &old_edited_chunk,
//                 &old_plain_chunk,
//                 &new_plain_chunk,
//             )?;
//             // チャンクを更新
//             result_server.save_chunk(&new_chunk)?;
//         }

//         return Ok(());
//     }
// }
