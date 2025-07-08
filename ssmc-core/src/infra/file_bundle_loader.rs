use crate::domain::{FileBundle, FileEntry, FileSource};
use crate::infra::fs_handler::FsHandler;
use crate::infra::url_fetcher::UrlFetcher;
use futures::stream::{self, StreamExt};
use std::path::Path;
use std::sync::Arc;

#[async_trait::async_trait]
pub trait FileBundleLoader {
    /// Writes the contents of the file bundle to the specified base path.
    /// Returns an error message if any operation fails.
    async fn write_contents(
        &self,
        fs_contents: &FileBundle,
        base_path: &Path,
    ) -> Result<(), String>;
}

pub struct DefaultFileBundleLoader {
    fs_handler: Arc<dyn FsHandler + Send + Sync>,
    url_fetcher: Arc<dyn UrlFetcher + Send + Sync>,
    max_concurrency: usize,
}

impl DefaultFileBundleLoader {
    pub fn new(
        fs_handler: Box<dyn FsHandler + Send + Sync>,
        url_fetcher: Box<dyn UrlFetcher + Send + Sync>,
    ) -> Self {
        // Auto-tune based on CPU count (I/O bound operations benefit from higher concurrency)
        let optimal_concurrency = (num_cpus::get() * 2).clamp(4, 32);
        Self::with_max_concurrency(fs_handler, url_fetcher, optimal_concurrency)
    }

    pub fn with_max_concurrency(
        fs_handler: Box<dyn FsHandler + Send + Sync>,
        url_fetcher: Box<dyn UrlFetcher + Send + Sync>,
        max_concurrency: usize,
    ) -> Self {
        Self {
            fs_handler: Arc::from(fs_handler),
            url_fetcher: Arc::from(url_fetcher),
            max_concurrency: max_concurrency,
        }
    }

    async fn write_file_entry(
        entry: FileEntry,
        base_path: &Path,
        fs_handler: &Arc<dyn FsHandler + Send + Sync>,
        url_fetcher: &Arc<dyn UrlFetcher + Send + Sync>,
    ) -> Result<(), String> {
        let target_path = base_path.join(entry.rel_path());

        match entry.source() {
            FileSource::Directory() => {
                // This should not happen as directories are handled separately
                return Err("Directory entry in file processing".to_string());
            }
            FileSource::InMemory(data) => {
                fs_handler.write(&target_path, data, entry.executable())?;
            }
            FileSource::LocalPath(source_path) => {
                let data = fs_handler.read(source_path)?;
                fs_handler.write(&target_path, &data, entry.executable())?;
            }
            FileSource::RemoteUrl(url) => {
                let data = url_fetcher.fetch_binary(url).await?;
                fs_handler.write(&target_path, &data, entry.executable())?;
            }
        }

        Ok(())
    }

    async fn write_entry(&self, entry: &FileEntry, base_path: &Path) -> Result<(), String> {
        let target_path = base_path.join(entry.rel_path());

        match entry.source() {
            FileSource::Directory() => {
                self.fs_handler.mkdir(&target_path)?;
            }
            FileSource::InMemory(data) => {
                self.fs_handler
                    .write(&target_path, data, entry.executable())?;
            }
            FileSource::LocalPath(source_path) => {
                let data = self.fs_handler.read(source_path)?;
                self.fs_handler
                    .write(&target_path, &data, entry.executable())?;
            }
            FileSource::RemoteUrl(url) => {
                let data = self.url_fetcher.fetch_binary(url).await?;
                self.fs_handler
                    .write(&target_path, &data, entry.executable())?;
            }
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl FileBundleLoader for DefaultFileBundleLoader {
    async fn write_contents(
        &self,
        fs_contents: &FileBundle,
        base_path: &Path,
    ) -> Result<(), String> {
        let entries = fs_contents.entries();

        // Step 1: Create all directories first (in order to ensure parent directories exist)
        let mut directories = Vec::new();
        let mut files = Vec::new();

        for entry in entries {
            match entry.source() {
                FileSource::Directory() => directories.push(entry.clone()),
                _ => files.push(entry.clone()),
            }
        }

        // Create directories sequentially
        for entry in directories {
            self.write_entry(&entry, base_path).await?;
        }

        // Step 2: Process files in parallel with controlled concurrency
        let base_path = base_path.to_path_buf();
        let fs_handler = Arc::clone(&self.fs_handler);
        let url_fetcher = Arc::clone(&self.url_fetcher);

        let file_count = files.len();
        if file_count > 0 {
            let results: Result<Vec<()>, String> = stream::iter(files)
                .map(|entry| {
                    let base_path = base_path.clone();
                    let fs_handler = Arc::clone(&fs_handler);
                    let url_fetcher = Arc::clone(&url_fetcher);

                    async move {
                        Self::write_file_entry(entry, &base_path, &fs_handler, &url_fetcher).await
                    }
                })
                .buffer_unordered(self.max_concurrency.min(file_count))
                .collect::<Vec<Result<(), String>>>()
                .await
                .into_iter()
                .collect();
            results?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infra::fs_handler::{DefaultFsHandler, OnMemoryFsHandler};
    use crate::infra::url_fetcher::DummyUrlFetcher;
    use std::path::PathBuf;
    use tempfile::TempDir;
    use url::Url;

    #[tokio::test]
    async fn test_write_contents_binary() {
        let temp_dir = TempDir::new().unwrap();
        let fs_handler = Box::new(DefaultFsHandler::new());
        let url_fetcher = Box::new(DummyUrlFetcher::new());
        let writer = DefaultFileBundleLoader::new(fs_handler, url_fetcher);

        let content = FileEntry::new(
            "test.txt".into(),
            false,
            FileSource::InMemory(b"hello world".to_vec()),
        );
        let fs_contents = FileBundle::new(vec![content]);

        writer
            .write_contents(&fs_contents, temp_dir.path())
            .await
            .unwrap();

        let file_path = temp_dir.path().join("test.txt");
        assert!(file_path.exists());

        let content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "hello world");
    }

    #[tokio::test]
    async fn test_write_contents_directory() {
        let temp_dir = TempDir::new().unwrap();
        let fs_handler = Box::new(DefaultFsHandler::new());
        let url_fetcher = Box::new(DummyUrlFetcher::new());
        let writer = DefaultFileBundleLoader::new(fs_handler, url_fetcher);

        let content = FileEntry::new("subdir".into(), false, FileSource::Directory());
        let fs_contents = FileBundle::new(vec![content]);

        writer
            .write_contents(&fs_contents, temp_dir.path())
            .await
            .unwrap();

        let dir_path = temp_dir.path().join("subdir");
        assert!(dir_path.exists());
        assert!(dir_path.is_dir());
    }

    #[tokio::test]
    async fn test_write_contents_url() {
        let temp_dir = TempDir::new().unwrap();
        let fs_handler = Box::new(DefaultFsHandler::new());
        let mut url_fetcher = DummyUrlFetcher::new();
        let url = Url::parse("https://example.com/file.txt").unwrap();
        url_fetcher.add_data(url.clone(), b"downloaded content");

        let writer = DefaultFileBundleLoader::new(fs_handler, Box::new(url_fetcher));

        let content = FileEntry::new("downloaded.txt".into(), false, FileSource::RemoteUrl(url));
        let fs_contents = FileBundle::new(vec![content]);

        writer
            .write_contents(&fs_contents, temp_dir.path())
            .await
            .unwrap();

        let file_path = temp_dir.path().join("downloaded.txt");
        assert!(file_path.exists());

        let content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "downloaded content");
    }

    #[tokio::test]
    async fn test_write_contents_path() {
        let temp_dir = TempDir::new().unwrap();
        let source_dir = TempDir::new().unwrap();

        // Create source file
        let source_file = source_dir.path().join("source.txt");
        std::fs::write(&source_file, "source content").unwrap();

        let fs_handler = Box::new(DefaultFsHandler::new());
        let url_fetcher = Box::new(DummyUrlFetcher::new());
        let writer = DefaultFileBundleLoader::new(fs_handler, url_fetcher);

        let content = FileEntry::new(
            "copied.txt".into(),
            false,
            FileSource::LocalPath(source_file),
        );
        let fs_contents = FileBundle::new(vec![content]);

        writer
            .write_contents(&fs_contents, temp_dir.path())
            .await
            .unwrap();

        let file_path = temp_dir.path().join("copied.txt");
        assert!(file_path.exists());

        let content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "source content");
    }

    #[tokio::test]
    async fn test_write_contents_with_memory_fs() {
        let fs_handler = Box::new(OnMemoryFsHandler::new());
        let url_fetcher = Box::new(DummyUrlFetcher::new());
        let writer = DefaultFileBundleLoader::new(fs_handler, url_fetcher);

        let content = FileEntry::new(
            "test.txt".into(),
            false,
            FileSource::InMemory(b"hello world".to_vec()),
        );
        let fs_contents = FileBundle::new(vec![content]);

        writer
            .write_contents(&fs_contents, &PathBuf::from("/"))
            .await
            .unwrap();

        // Verify through the fs_handler
        let data = writer.fs_handler.read(&PathBuf::from("/test.txt")).unwrap();
        assert_eq!(data, b"hello world");
    }
}
