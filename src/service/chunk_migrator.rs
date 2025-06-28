use crate::model::McChunk;

pub trait ChunkMigrator {
    /// 現在のワールドのチャンク / 旧バージョンの未変更のワールドのチャンク / 新バージョンの未変更のワールドのチャンク
    /// からマイグレーションを行い、新しいチャンクを返す
    fn migrate(
        &self,
        old_edited: &McChunk,
        old_plain: &McChunk,
        new_plain: &McChunk,
    ) -> Result<McChunk, String>;
}
