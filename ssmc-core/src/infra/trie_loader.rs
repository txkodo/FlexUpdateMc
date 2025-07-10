use anyhow::Result;
use std::path::Path;

use crate::infra::{fs_handler::FsHandler, url_fetcher::UrlFetcher};
use crate::util::file_trie::Dir;
use crate::util::fs_converter::TrieToFsConverter;

#[async_trait::async_trait]
pub trait TrieLoader {
    /// Writes the contents of the file trie to the specified base path.
    async fn write_contents(&self, trie: &Dir, base_path: &Path) -> Result<()>;
}

pub struct DefaultTrieLoader {
    converter: TrieToFsConverter,
}

impl DefaultTrieLoader {
    pub fn new(
        fs_handler: Box<dyn FsHandler + Send + Sync>,
        url_fetcher: Box<dyn UrlFetcher + Send + Sync>,
    ) -> Self {
        Self {
            converter: TrieToFsConverter::new(fs_handler, url_fetcher),
        }
    }
}

#[async_trait::async_trait]
impl TrieLoader for DefaultTrieLoader {
    async fn write_contents(&self, trie: &Dir, base_path: &Path) -> Result<()> {
        self.converter.write_directory(trie, base_path).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infra::fs_handler::OnMemoryFsHandler;
    use crate::infra::url_fetcher::DummyUrlFetcher;
    use crate::util::file_trie::{File, Path as VirtualPath};
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_trie_loader() {
        let fs_handler = Box::new(OnMemoryFsHandler::new());
        let loader = DefaultTrieLoader::new(
            Box::new(OnMemoryFsHandler::new()),
            Box::new(DummyUrlFetcher::new()),
        );

        // Create a test trie
        let mut trie = Dir::new();
        trie.put_file(
            VirtualPath::from_str("test.txt"),
            File::Inline(b"hello world".to_vec()),
        )
        .unwrap();

        trie.put_dir(VirtualPath::from_str("subdir"), Dir::new())
            .unwrap();
        trie.put_file(
            VirtualPath::from_str("subdir/nested.txt"),
            File::Inline(b"nested content".to_vec()),
        )
        .unwrap();

        // Write to filesystem
        loader
            .write_contents(&trie, &PathBuf::from("/output"))
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
