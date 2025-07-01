use crate::model::{McChunk, McChunkPos, McDimension};
use super::server_handler::RegionHandler;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct AnvilRegionHandler;

impl AnvilRegionHandler {
    pub fn new() -> Self {
        AnvilRegionHandler
    }

    fn parse_region_filename(filename: &str) -> Option<(i32, i32)> {
        // r.x.z.mca 形式のファイル名をパース
        if !filename.starts_with("r.") || !filename.ends_with(".mca") {
            return None;
        }
        
        let parts: Vec<&str> = filename.split('.').collect();
        if parts.len() != 4 {
            return None;
        }
        
        let rx = parts[1].parse::<i32>().ok()?;
        let rz = parts[2].parse::<i32>().ok()?;
        
        Some((rx, rz))
    }

    fn region_to_chunk_coords(rx: i32, rz: i32) -> Vec<(i32, i32)> {
        // 1つのリージョンファイルは32x32チャンクを格納
        let mut chunks = Vec::new();
        for cx in 0..32 {
            for cz in 0..32 {
                chunks.push((rx * 32 + cx, rz * 32 + cz));
            }
        }
        chunks
    }
    
    fn chunk_to_region_coords(chunk_x: i32, chunk_z: i32) -> (i32, i32) {
        // チャンク座標からリージョン座標を計算
        (chunk_x >> 5, chunk_z >> 5)
    }
    
    fn get_region_file_path(region_dir: &Path, rx: i32, rz: i32) -> std::path::PathBuf {
        region_dir.join(format!("r.{}.{}.mca", rx, rz))
    }
}

impl RegionHandler for AnvilRegionHandler {
    fn list_chunks(&self, region_dir: &Path) -> Result<Vec<McChunkPos>, String> {
        if !region_dir.exists() {
            return Ok(Vec::new());
        }

        let mut chunks = Vec::new();
        let entries = fs::read_dir(region_dir)
            .map_err(|e| format!("Failed to read region directory: {}", e))?;

        for entry in entries {
            let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
            let filename = entry.file_name();
            let filename_str = filename.to_string_lossy();

            if let Some((rx, rz)) = Self::parse_region_filename(&filename_str) {
                // リージョンファイルから全チャンク座標を生成
                // 実際の実装では、リージョンファイルを読んで存在するチャンクのみを返すべき
                let chunk_coords = Self::region_to_chunk_coords(rx, rz);
                
                // TODO: 実際のNBTパーサーでリージョンファイルを読み、存在するチャンクのみを取得
                for (cx, cz) in chunk_coords {
                    chunks.push(McChunkPos {
                        x: cx,
                        y: cz,
                        dimension: McDimension::Overworld, // ディメンションは呼び出し元で適切に設定される
                    });
                }
            }
        }

        Ok(chunks)
    }

    fn load_chunk(&self, region_dir: &Path, pos: &McChunkPos) -> Result<McChunk, String> {
        let (rx, rz) = Self::chunk_to_region_coords(pos.x, pos.y);
        let region_file = Self::get_region_file_path(region_dir, rx, rz);

        if !region_file.exists() {
            return Err(format!("Region file does not exist: {:?}", region_file));
        }

        // TODO: 実際のNBTパーサーでリージョンファイルからチャンクデータを読み込み
        // 現在はダミー実装
        Ok(McChunk {
            pos: pos.clone(),
        })
    }

    fn save_chunk(&self, region_dir: &Path, chunk: &McChunk) -> Result<(), String> {
        let (rx, rz) = Self::chunk_to_region_coords(chunk.pos.x, chunk.pos.y);
        let region_file = Self::get_region_file_path(region_dir, rx, rz);

        // リージョンディレクトリを作成
        fs::create_dir_all(region_dir)
            .map_err(|e| format!("Failed to create region directory: {}", e))?;

        // TODO: 実際のNBTパーサーでチャンクデータをリージョンファイルに書き込み
        // 現在はダミー実装（空ファイルを作成）
        if !region_file.exists() {
            fs::File::create(&region_file)
                .map_err(|e| format!("Failed to create region file: {}", e))?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_region_filename() {
        assert_eq!(AnvilRegionHandler::parse_region_filename("r.0.0.mca"), Some((0, 0)));
        assert_eq!(AnvilRegionHandler::parse_region_filename("r.-1.2.mca"), Some((-1, 2)));
        assert_eq!(AnvilRegionHandler::parse_region_filename("invalid.mca"), None);
        assert_eq!(AnvilRegionHandler::parse_region_filename("r.0.0.dat"), None);
    }

    #[test]
    fn test_chunk_to_region_coords() {
        assert_eq!(AnvilRegionHandler::chunk_to_region_coords(0, 0), (0, 0));
        assert_eq!(AnvilRegionHandler::chunk_to_region_coords(31, 31), (0, 0));
        assert_eq!(AnvilRegionHandler::chunk_to_region_coords(32, 32), (1, 1));
        assert_eq!(AnvilRegionHandler::chunk_to_region_coords(-1, -1), (-1, -1));
        assert_eq!(AnvilRegionHandler::chunk_to_region_coords(-32, -32), (-1, -1));
        assert_eq!(AnvilRegionHandler::chunk_to_region_coords(-33, -33), (-2, -2));
    }

    #[test]
    fn test_region_to_chunk_coords() {
        let chunks = AnvilRegionHandler::region_to_chunk_coords(0, 0);
        assert_eq!(chunks.len(), 32 * 32);
        assert!(chunks.contains(&(0, 0)));
        assert!(chunks.contains(&(31, 31)));
        assert!(!chunks.contains(&(32, 32)));
    }
}