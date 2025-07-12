use anyhow::Result;
use std::path::Path;
use std::sync::Arc;

use crate::infra::{fs_handler::FsHandler, url_fetcher::UrlFetcher};
use crate::util::file_trie::{Dir, File, FileContent};
use crate::util::fs_converter::TrieToFsConverter;

#[async_trait::async_trait]
pub trait TrieLoader {
    /// Writes the contents of the file trie to the specified base path.
    async fn mount_contents(&self, trie: &Dir, base_path: &Path) -> Result<()>;

    /// Loads the content of a file from the trie.
    async fn load_content(&self, trie: &File) -> Result<Vec<u8>>;
}

pub struct DefaultTrieLoader {
    converter: TrieToFsConverter,
    fs_handler: Arc<dyn FsHandler + Send + Sync>,
    url_fetcher: Arc<dyn UrlFetcher + Send + Sync>,
}

impl DefaultTrieLoader {
    pub fn new(
        fs_handler: Arc<dyn FsHandler + Send + Sync>,
        url_fetcher: Arc<dyn UrlFetcher + Send + Sync>,
    ) -> Self {
        Self {
            converter: TrieToFsConverter::new(fs_handler.clone(), url_fetcher.clone()),
            fs_handler,
            url_fetcher,
        }
    }
}

#[async_trait::async_trait]
impl TrieLoader for DefaultTrieLoader {
    async fn mount_contents(&self, trie: &Dir, base_path: &Path) -> Result<()> {
        self.converter.write_directory(trie, base_path).await
    }

    async fn load_content(&self, file: &File) -> Result<Vec<u8>> {
        match &file.content {
            FileContent::Inline(data) => Ok(data.clone()),
            FileContent::Path(path) => self.fs_handler.read(path).map_err(|e| {
                anyhow::anyhow!("Failed to read source file {}: {}", path.display(), e)
            }),
            FileContent::Url(url) => self
                .url_fetcher
                .fetch_binary(url)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to fetch URL {}: {}", url, e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infra::fs_handler::OnMemoryFsHandler;
    use crate::infra::url_fetcher::DummyUrlFetcher;
    use crate::util::file_trie::{File, Path as VirtualPath, Permission};
    use std::path::PathBuf;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_trie_loader() {
        let fs_handler = Arc::new(OnMemoryFsHandler::new());
        let loader = DefaultTrieLoader::new(fs_handler.clone(), Arc::new(DummyUrlFetcher::new()));

        // Create a test trie
        let mut trie = Dir::new();
        trie.put_file(
            VirtualPath::from_str("test.txt"),
            File::inline(b"hello world".to_vec(), Permission::read_write()),
        )
        .unwrap();

        trie.put_dir(VirtualPath::from_str("subdir"), Dir::new())
            .unwrap();
        trie.put_file(
            VirtualPath::from_str("subdir/nested.txt"),
            File::inline(b"nested content".to_vec(), Permission::read_write()),
        )
        .unwrap();

        // Write to filesystem
        loader
            .mount_contents(&trie, &PathBuf::from("/output"))
            .await
            .unwrap();

        // Verify files were written
        let content = fs_handler.read(&PathBuf::from("/output/test.txt")).unwrap();
        assert_eq!(content, b"hello world");

        let nested_content = fs_handler
            .read(&PathBuf::from("/output/subdir/nested.txt"))
            .unwrap();
        assert_eq!(nested_content, b"nested content");
    }
}
