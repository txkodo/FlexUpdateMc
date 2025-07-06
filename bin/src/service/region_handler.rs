use crate::model::{McChunk, McChunkPos, McDimension};
use crate::nbt::{NbtReader, NbtWriter, NbtValue, NbtTag, parse_nbt, write_nbt};
use super::server_handler::RegionHandler;
use std::fs::{self, File};
use std::io::{self, Read, Write, Seek, SeekFrom, Cursor};
use std::path::Path;
use std::collections::HashMap;

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

        // リージョンファイルからチャンクデータを読み込み
        let chunk_data = self.read_chunk_from_region(&region_file, pos.x, pos.y)
            .map_err(|e| format!("Failed to read chunk data: {}", e))?;

        Ok(McChunk {
            pos: pos.clone(),
            data: chunk_data,
            last_update: None, // リージョンファイルから読み取る場合は後で実装
            inhabited_time: None,
        })
    }

    fn save_chunk(&self, region_dir: &Path, chunk: &McChunk) -> Result<(), String> {
        let (rx, rz) = Self::chunk_to_region_coords(chunk.pos.x, chunk.pos.y);
        let region_file = Self::get_region_file_path(region_dir, rx, rz);

        // リージョンディレクトリを作成
        fs::create_dir_all(region_dir)
            .map_err(|e| format!("Failed to create region directory: {}", e))?;

        // チャンクデータをリージョンファイルに書き込み
        if let Some(ref data) = chunk.data {
            self.write_chunk_to_region(&region_file, chunk.pos.x, chunk.pos.y, data)
                .map_err(|e| format!("Failed to write chunk data: {}", e))?;
        } else {
            // データがない場合は空のチャンクを作成
            let empty_chunk_data = self.create_empty_chunk_data(chunk.pos.x, chunk.pos.y);
            self.write_chunk_to_region(&region_file, chunk.pos.x, chunk.pos.y, &empty_chunk_data)
                .map_err(|e| format!("Failed to write empty chunk data: {}", e))?;
        }

        Ok(())
    }

    // 実際のリージョンファイル操作メソッド
    
    fn read_chunk_from_region(&self, region_file: &Path, chunk_x: i32, chunk_z: i32) -> io::Result<Option<NbtTag>> {
        let mut file = File::open(region_file)?;
        
        // チャンクのリージョン内オフセットを計算
        let local_x = (chunk_x & 31) as usize;
        let local_z = (chunk_z & 31) as usize;
        let chunk_index = local_x + local_z * 32;
        
        // ヘッダーからチャンクの位置とサイズを読み取り
        file.seek(SeekFrom::Start((chunk_index * 4) as u64))?;
        let mut location_data = [0u8; 4];
        file.read_exact(&mut location_data)?;
        
        let offset = (u32::from_be_bytes([0, location_data[0], location_data[1], location_data[2]]) * 4096) as u64;
        let size = location_data[3] as u32;
        
        if offset == 0 || size == 0 {
            // チャンクが存在しない
            return Ok(None);
        }
        
        // チャンクデータの位置に移動
        file.seek(SeekFrom::Start(offset))?;
        
        // チャンクデータのヘッダーを読み取り
        let mut chunk_header = [0u8; 5];
        file.read_exact(&mut chunk_header)?;
        
        let length = u32::from_be_bytes([0, chunk_header[0], chunk_header[1], chunk_header[2]]);
        let compression_type = chunk_header[4];
        
        // 圧縮されたデータを読み取り
        let mut compressed_data = vec![0u8; (length - 1) as usize];
        file.read_exact(&mut compressed_data)?;
        
        // 圧縮解除とNBT解析
        let decompressed_data = match compression_type {
            1 => {
                // GZip圧縮（現在は未実装）
                return Err(io::Error::new(io::ErrorKind::Unsupported, 
                    "GZip compression not implemented"));
            }
            2 => {
                // Zlib圧縮（現在は未実装）
                return Err(io::Error::new(io::ErrorKind::Unsupported, 
                    "Zlib compression not implemented"));
            }
            3 => {
                // 圧縮なし
                compressed_data
            }
            _ => {
                return Err(io::Error::new(io::ErrorKind::InvalidData, 
                    format!("Unknown compression type: {}", compression_type)));
            }
        };
        
        // NBTデータを解析
        let cursor = Cursor::new(decompressed_data);
        parse_nbt(cursor)
    }
    
    fn write_chunk_to_region(&self, region_file: &Path, chunk_x: i32, chunk_z: i32, chunk_data: &NbtTag) -> io::Result<()> {
        // 現在は簡単な実装（圧縮なし）
        let mut nbt_data = Vec::new();
        write_nbt(&mut nbt_data, chunk_data)?;
        
        // リージョンファイルの存在確認と作成
        if !region_file.exists() {
            // 新しいリージョンファイルを作成
            let mut file = File::create(region_file)?;
            // ヘッダー（8KB）を初期化
            let header = vec![0u8; 8192];
            file.write_all(&header)?;
        }
        
        let mut file = File::options().read(true).write(true).open(region_file)?;
        
        // チャンクのリージョン内オフセットを計算
        let local_x = (chunk_x & 31) as usize;
        let local_z = (chunk_z & 31) as usize;
        let chunk_index = local_x + local_z * 32;
        
        // ファイルの末尾にチャンクデータを追加
        file.seek(SeekFrom::End(0))?;
        let data_offset = file.stream_position()?;
        
        // チャンクデータヘッダーを書き込み
        let length = (nbt_data.len() + 1) as u32;
        let length_bytes = length.to_be_bytes();
        file.write_all(&length_bytes[1..])?; // 最初の1バイトは除く（3バイト）
        file.write_all(&[3u8])?; // 圧縮タイプ（圧縮なし）
        
        // NBTデータを書き込み
        file.write_all(&nbt_data)?;
        
        // ヘッダーを更新
        file.seek(SeekFrom::Start((chunk_index * 4) as u64))?;
        let offset_sectors = (data_offset / 4096) as u32;
        let size_sectors = ((length + 4 + 4095) / 4096) as u8; // 4バイトヘッダー + データサイズを4KBでアライン
        
        let location_data = [
            (offset_sectors >> 16) as u8,
            (offset_sectors >> 8) as u8,
            offset_sectors as u8,
            size_sectors,
        ];
        file.write_all(&location_data)?;
        
        // タイムスタンプも更新
        file.seek(SeekFrom::Start(4096 + (chunk_index * 4) as u64))?;
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as u32;
        file.write_all(&timestamp.to_be_bytes())?;
        
        Ok(())
    }
    
    fn create_empty_chunk_data(&self, chunk_x: i32, chunk_z: i32) -> NbtTag {
        let mut chunk_compound = HashMap::new();
        
        // 基本的なチャンクデータ構造
        chunk_compound.insert("xPos".to_string(), NbtValue::Int(chunk_x));
        chunk_compound.insert("zPos".to_string(), NbtValue::Int(chunk_z));
        chunk_compound.insert("LastUpdate".to_string(), NbtValue::Long(0));
        chunk_compound.insert("TerrainPopulated".to_string(), NbtValue::Byte(1));
        chunk_compound.insert("LightPopulated".to_string(), NbtValue::Byte(1));
        chunk_compound.insert("InhabitedTime".to_string(), NbtValue::Long(0));
        
        // 空のセクション配列
        chunk_compound.insert("Sections".to_string(), NbtValue::List(Vec::new()));
        
        // 空のエンティティとタイルエンティティ
        chunk_compound.insert("Entities".to_string(), NbtValue::List(Vec::new()));
        chunk_compound.insert("TileEntities".to_string(), NbtValue::List(Vec::new()));
        
        // バイオーム配列（全て平原バイオーム = 1）
        let biomes = vec![1i32; 1024]; // 32x32 = 1024
        chunk_compound.insert("Biomes".to_string(), NbtValue::IntArray(biomes));
        
        NbtTag {
            name: "Level".to_string(),
            value: NbtValue::Compound(chunk_compound),
        }
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