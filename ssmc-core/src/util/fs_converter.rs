use anyhow::Result;
use std::path::Path as StdPath;
use std::sync::Arc;

use crate::infra::{fs_handler::FsHandler, url_fetcher::UrlFetcher};
use crate::util::file_trie::{Dir, Entry, File, Path};

/// 物理ファイルシステムからfile_trieを作成するハンドラ
pub struct FsToTrieConverter {
    fs_handler: Arc<dyn FsHandler + Send + Sync>,
}

impl FsToTrieConverter {
    pub fn new(fs_handler: Arc<dyn FsHandler + Send + Sync>) -> Self {
        Self { fs_handler }
    }

    /// 指定されたディレクトリパスからDirを作成
    pub fn load_directory(&self, physical_path: &StdPath) -> Result<Dir> {
        let mut dir = Dir::new();
        self.load_directory_recursive(physical_path, physical_path, &mut dir)?;
        Ok(dir)
    }

    /// 単一ファイルを読み込んでFileを作成
    pub fn load_file(&self, physical_path: &StdPath) -> Result<File> {
        Ok(File::Path(physical_path.to_path_buf()))
    }

    fn load_directory_recursive(
        &self,
        base_path: &StdPath,
        current_path: &StdPath,
        dir: &mut Dir,
    ) -> Result<()> {
        let entries = self.fs_handler.list_entries(current_path).map_err(|e| {
            anyhow::anyhow!("Failed to list directory {}: {}", current_path.display(), e)
        })?;

        for entry_path in entries {
            // Use only the file/directory name, not the full path
            let name = entry_path
                .file_name()
                .ok_or_else(|| anyhow::anyhow!("No file name found"))?
                .to_string_lossy()
                .to_string();
            let virtual_path = Path::from(vec![name]);

            // Check if this is a file or directory using efficient existence check
            if self.fs_handler.is_file(&entry_path) {
                // It's a file
                let file = File::Path(entry_path.to_path_buf());
                dir.put_file(virtual_path, file)
                    .map_err(|_| anyhow::anyhow!("Failed to add file to trie"))?;
            } else if self.fs_handler.is_dir(&entry_path) {
                // It's a directory
                let mut subdir = Dir::new();
                self.load_directory_recursive(base_path, &entry_path, &mut subdir)?;
                dir.put_dir(virtual_path, subdir)
                    .map_err(|_| anyhow::anyhow!("Failed to add directory to trie"))?;
            }
        }

        Ok(())
    }
}

/// file_trieから物理ファイルシステムに書き込むハンドラ
pub struct TrieToFsConverter {
    fs_handler: Arc<dyn FsHandler + Send + Sync>,
    url_fetcher: Arc<dyn UrlFetcher + Send + Sync>,
}

impl TrieToFsConverter {
    pub fn new(
        fs_handler: Arc<dyn FsHandler + Send + Sync>,
        url_fetcher: Arc<dyn UrlFetcher + Send + Sync>,
    ) -> Self {
        Self {
            fs_handler,
            url_fetcher,
        }
    }

    /// DirをベースパスからPhysical FSに書き込み
    pub async fn write_directory(&self, dir: &Dir, base_path: &StdPath) -> Result<()> {
        let mut dirs = Vec::new();
        let mut files = Vec::new();

        dir.iter_all().for_each(|(path, entry)| match entry {
            Entry::File(file) => {
                files.push((path, file));
            }
            Entry::Dir(subdir) => {
                dirs.push((path, subdir));
            }
        });

        // まずディレクトリを作成
        for (path, _dir) in dirs {
            let physical_path = self.get_physical_path(base_path, &path);
            self.fs_handler.mkdir(&physical_path).map_err(|e| {
                anyhow::anyhow!(
                    "Failed to create directory {}: {}",
                    physical_path.display(),
                    e
                )
            })?;
        }

        // 次にファイルを書き込み
        for (path, file) in files {
            println!("Writing file: {:?}", path);
            let physical_path = self.get_physical_path(base_path, &path);
            self.write_file(file, &physical_path, false).await?;
        }

        Ok(())
    }

    /// 単一ファイルを物理パスに書き込み
    pub async fn write_file(
        &self,
        file: &File,
        physical_path: &StdPath,
        executable: bool,
    ) -> Result<()> {
        let data = self.get_file_data(file).await?;
        self.fs_handler
            .write(physical_path, &data, executable)
            .map_err(|e| {
                anyhow::anyhow!("Failed to write file {}: {}", physical_path.display(), e)
            })?;
        Ok(())
    }

    async fn get_file_data(&self, file: &File) -> Result<Vec<u8>> {
        match file {
            File::Inline(data) => Ok(data.clone()),
            File::Path(path) => self.fs_handler.read(path).map_err(|e| {
                anyhow::anyhow!("Failed to read source file {}: {}", path.display(), e)
            }),
            File::Url(url) => self
                .url_fetcher
                .fetch_binary(url)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to fetch URL {}: {}", url, e)),
        }
    }

    fn get_physical_path(&self, base_path: &StdPath, virtual_path: &Path) -> std::path::PathBuf {
        let path_str = virtual_path.components().join("/");
        base_path.join(path_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infra::fs_handler::OnMemoryFsHandler;
    use crate::infra::url_fetcher::DummyUrlFetcher;
    use std::path::PathBuf;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_roundtrip_conversion() {
        let fs_handler = Arc::new(OnMemoryFsHandler::new());

        // Setup: Create some files in the memory fs
        fs_handler
            .write(&PathBuf::from("/test.txt"), b"test content", false)
            .unwrap();
        fs_handler.mkdir(&PathBuf::from("/subdir")).unwrap();
        fs_handler
            .write(
                &PathBuf::from("/subdir/nested.txt"),
                b"nested content",
                false,
            )
            .unwrap();

        // Convert FS to Trie
        let fs_to_trie = FsToTrieConverter::new(fs_handler.clone());
        let dir = fs_to_trie.load_directory(&PathBuf::from("/")).unwrap();

        // Verify trie content
        assert!(dir.get_file(Path::from_str("test.txt")).is_some());
        assert!(dir.get_dir(Path::from_str("subdir")).is_some());
        assert!(dir.get_file(Path::from_str("subdir/nested.txt")).is_some());

        // Convert Trie back to FS
        let trie_to_fs =
            TrieToFsConverter::new(fs_handler.clone(), Arc::new(DummyUrlFetcher::new()));
        trie_to_fs
            .write_directory(&dir, &PathBuf::from("/output"))
            .await
            .unwrap();

        // Verify output
        let output_content = fs_handler.read(&PathBuf::from("/output/test.txt")).unwrap();
        assert_eq!(output_content, b"test content");

        let nested_content = fs_handler
            .read(&PathBuf::from("/output/subdir/nested.txt"))
            .unwrap();
        assert_eq!(nested_content, b"nested content");
    }
}
