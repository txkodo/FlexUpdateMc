use std::path::Path as StdPath;
use std::sync::Arc;
use anyhow::Result;

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
        let data = self.fs_handler.read(physical_path)
            .map_err(|e| anyhow::anyhow!("Failed to read file {}: {}", physical_path.display(), e))?;
        Ok(File::Inline(data))
    }

    fn load_directory_recursive(
        &self, 
        base_path: &StdPath, 
        current_path: &StdPath, 
        dir: &mut Dir
    ) -> Result<()> {
        let entries = self.fs_handler.list_entries(current_path)
            .map_err(|e| anyhow::anyhow!("Failed to list directory {}: {}", current_path.display(), e))?;

        for entry_path in entries {
            let relative_path = entry_path.strip_prefix(base_path)
                .map_err(|e| anyhow::anyhow!("Failed to get relative path: {}", e))?;
            
            if relative_path.as_os_str().is_empty() {
                continue; // Skip the base directory itself
            }

            let path_components: Vec<String> = relative_path
                .components()
                .map(|c| c.as_os_str().to_string_lossy().to_string())
                .collect();
            let virtual_path = Path::from(path_components);

            // Check if this is a directory by trying to list its contents
            if self.fs_handler.list_entries(&entry_path).is_ok() {
                // It's a directory
                let mut subdir = Dir::new();
                self.load_directory_recursive(base_path, &entry_path, &mut subdir)?;
                dir.put_dir(virtual_path, subdir)
                    .map_err(|_| anyhow::anyhow!("Failed to add directory to trie"))?;
            } else {
                // It's a file
                let data = self.fs_handler.read(&entry_path)
                    .map_err(|e| anyhow::anyhow!("Failed to read file {}: {}", entry_path.display(), e))?;
                let file = File::Inline(data);
                dir.put_file(virtual_path, file)
                    .map_err(|_| anyhow::anyhow!("Failed to add file to trie"))?;
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
        Self { fs_handler, url_fetcher }
    }

    /// DirをベースパスからPhysical FSに書き込み
    pub async fn write_directory(&self, dir: &Dir, base_path: &StdPath) -> Result<()> {
        self.write_dir_recursive(&Path::new(), dir, base_path).await
    }

    /// 単一ファイルを物理パスに書き込み
    pub async fn write_file(&self, file: &File, physical_path: &StdPath, executable: bool) -> Result<()> {
        let data = self.get_file_data(file).await?;
        self.fs_handler.write(physical_path, &data, executable)
            .map_err(|e| anyhow::anyhow!("Failed to write file {}: {}", physical_path.display(), e))?;
        Ok(())
    }

    fn write_dir_recursive<'a>(&'a self, virtual_path: &'a Path, dir: &'a Dir, base_path: &'a StdPath) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
        // Create directory if it's not the root
        if !virtual_path.is_empty() {
            let physical_path = self.get_physical_path(base_path, virtual_path);
            self.fs_handler.mkdir(&physical_path)
                .map_err(|e| anyhow::anyhow!("Failed to create directory {}: {}", physical_path.display(), e))?;
        }

        // Process all entries in the directory
        for (name, entry) in dir.iter() {
            let mut child_path = virtual_path.clone();
            child_path.push(name);

            match entry {
                Entry::File(file) => {
                    let physical_path = self.get_physical_path(base_path, &child_path);
                    let data = self.get_file_data(file).await?;
                    
                    // Determine if file should be executable (basic heuristic)
                    let executable = self.is_executable_file(&child_path);
                    
                    self.fs_handler.write(&physical_path, &data, executable)
                        .map_err(|e| anyhow::anyhow!("Failed to write file {}: {}", physical_path.display(), e))?;
                }
                Entry::Dir(subdir) => {
                    self.write_dir_recursive(&child_path, subdir, base_path).await?;
                }
            }
        }

        Ok(())
        })
    }

    async fn get_file_data(&self, file: &File) -> Result<Vec<u8>> {
        match file {
            File::Inline(data) => Ok(data.clone()),
            File::Path(path) => {
                self.fs_handler.read(path)
                    .map_err(|e| anyhow::anyhow!("Failed to read source file {}: {}", path.display(), e))
            }
            File::Url(url) => {
                self.url_fetcher.fetch_binary(url).await
                    .map_err(|e| anyhow::anyhow!("Failed to fetch URL {}: {}", url, e))
            }
        }
    }

    fn get_physical_path(&self, base_path: &StdPath, virtual_path: &Path) -> std::path::PathBuf {
        let path_str = virtual_path.components().join("/");
        base_path.join(path_str)
    }

    fn is_executable_file(&self, path: &Path) -> bool {
        if let Some(last_component) = path.components().last() {
            // Simple heuristic: files without extension or with .sh, .jar when run via java, etc.
            if !last_component.contains('.') {
                return true;
            }
            
            let lower = last_component.to_lowercase();
            if lower.ends_with(".sh") || lower.ends_with(".bin") || lower.ends_with(".run") {
                return true;
            }
        }
        false
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
        fs_handler.write(&PathBuf::from("/test.txt"), b"test content", false).unwrap();
        fs_handler.mkdir(&PathBuf::from("/subdir")).unwrap();
        fs_handler.write(&PathBuf::from("/subdir/nested.txt"), b"nested content", false).unwrap();

        // Convert FS to Trie
        let fs_to_trie = FsToTrieConverter::new(fs_handler.clone());
        let dir = fs_to_trie.load_directory(&PathBuf::from("/")).unwrap();

        // Verify trie content
        assert!(dir.get_file(Path::from_str("test.txt")).is_some());
        assert!(dir.get_dir(Path::from_str("subdir")).is_some());
        assert!(dir.get_file(Path::from_str("subdir/nested.txt")).is_some());

        // Convert Trie back to FS
        let trie_to_fs = TrieToFsConverter::new(
            fs_handler.clone(),
            Arc::new(DummyUrlFetcher::new())
        );
        trie_to_fs.write_directory(&dir, &PathBuf::from("/output")).await.unwrap();

        // Verify output
        let output_content = fs_handler.read(&PathBuf::from("/output/test.txt")).unwrap();
        assert_eq!(output_content, b"test content");

        let nested_content = fs_handler.read(&PathBuf::from("/output/subdir/nested.txt")).unwrap();
        assert_eq!(nested_content, b"nested content");
    }
}