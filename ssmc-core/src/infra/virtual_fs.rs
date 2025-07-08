use std::path::Path;
use std::{collections::BTreeMap, ops::Bound, path::PathBuf};

use anyhow::Result;
use url::Url;

use crate::domain::{FileBundle, FileEntry, FileSource};
use crate::infra::{fs_handler::FsHandler, url_fetcher::UrlFetcher};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct VirtualPath {
    segments: Vec<String>,
}

impl VirtualPath {
    pub fn from_segments(segments: Vec<String>) -> Self {
        Self::validate_segments(&segments);
        VirtualPath { segments }
    }

    pub fn from_str(path: &str) -> Self {
        if path.is_empty() {
            panic!("VirtualPath cannot be empty");
        }

        if path.starts_with('/') || path.ends_with('/') {
            panic!("VirtualPath must not start or end with '/': {}", path);
        }

        let segments: Vec<String> = path.split('/').map(String::from).collect();
        Self::validate_segments(&segments);
        VirtualPath { segments }
    }

    fn validate_segments(segments: &[String]) {
        if segments.is_empty() {
            panic!("VirtualPath segments cannot be empty");
        }

        for segment in segments {
            if segment.is_empty() {
                panic!("VirtualPath segment cannot be empty string");
            }

            Self::validate_segment_chars(segment);
        }
    }

    fn validate_segment_chars(segment: &str) {
        // 禁止文字のチェック
        const INVALID_CHARS: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|', '\0'];

        for &invalid_char in INVALID_CHARS {
            if segment.contains(invalid_char) {
                panic!(
                    "VirtualPath segment cannot contain '{}': {}",
                    invalid_char, segment
                );
            }
        }

        // 制御文字のチェック (ASCII 0-31, 127)
        for ch in segment.chars() {
            if ch.is_control() {
                panic!(
                    "VirtualPath segment cannot contain control character (code {}): {}",
                    ch as u32, segment
                );
            }
        }

        // 予約名のチェック (Windows互換)
        let upper_segment = segment.to_uppercase();
        const RESERVED_NAMES: &[&str] = &[
            "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
            "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
        ];

        for &reserved in RESERVED_NAMES {
            if upper_segment == reserved || upper_segment.starts_with(&format!("{}.", reserved)) {
                panic!("VirtualPath segment cannot be reserved name: {}", segment);
            }
        }

        // ドットのみのセグメントを禁止
        if segment == "." || segment == ".." {
            panic!("VirtualPath segment cannot be '.' or '..': {}", segment);
        }

        // 末尾のドットやスペースを禁止 (Windows互換)
        if segment.ends_with('.') || segment.ends_with(' ') {
            panic!(
                "VirtualPath segment cannot end with '.' or space: {}",
                segment
            );
        }
    }

    pub fn to_string(&self) -> String {
        self.segments.join("/")
    }
}

pub enum VirtualFileSource {
    Inline(Vec<u8>),
    Path(PathBuf),
    Url(String),
}

impl VirtualFileSource {
    pub fn from_file_source(source: &FileSource) -> Self {
        match source {
            FileSource::InMemory(data) => VirtualFileSource::Inline(data.clone()),
            FileSource::LocalPath(path) => VirtualFileSource::Path(path.clone()),
            FileSource::RemoteUrl(url) => VirtualFileSource::Url(url.to_string()),
            FileSource::Directory() => VirtualFileSource::Inline(Vec::new()),
        }
    }

    pub async fn read_data(
        &self,
        url_fetcher: &dyn UrlFetcher,
        fs_handler: &dyn FsHandler,
    ) -> Result<Vec<u8>, String> {
        match self {
            VirtualFileSource::Inline(data) => Ok(data.clone()),
            VirtualFileSource::Path(path) => fs_handler.read(path),
            VirtualFileSource::Url(url_str) => {
                let url = Url::parse(url_str).map_err(|e| format!("Invalid URL: {}", e))?;
                url_fetcher.fetch_binary(&url).await
            }
        }
    }
}

pub struct VirtualFile {
    source: VirtualFileSource,
    permissions: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VirtualEntryType {
    File,
    Directory,
}

pub struct VirtualFs {
    url_fetcher: Box<dyn UrlFetcher + Send + Sync>,
    fs_accessor: Box<dyn FsHandler + Send + Sync>,
    entries: BTreeMap<VirtualPath, VirtualFile>,
}

impl VirtualFs {
    pub fn new(
        url_fetcher: Box<dyn UrlFetcher + Send + Sync>,
        fs_accessor: Box<dyn FsHandler + Send + Sync>,
    ) -> Self {
        VirtualFs {
            url_fetcher,
            fs_accessor,
            entries: BTreeMap::new(),
        }
    }

    pub fn load_file_bundle(&mut self, bundle: &FileBundle) -> Result<()> {
        for entry in bundle.entries() {
            self.load_file_entry(entry)
                .map_err(|x| anyhow::anyhow!(x))?;
        }
        Ok(())
    }

    fn load_file_entry(&mut self, entry: &FileEntry) -> Result<(), String> {
        let virtual_path = VirtualPath::from_str(&entry.rel_path().to_string_lossy());

        match entry.source() {
            FileSource::Directory() => {
                // ディレクトリエントリは特別な処理は不要
                return Ok(());
            }
            _ => {
                let virtual_file = VirtualFile {
                    source: VirtualFileSource::from_file_source(entry.source()),
                    permissions: if entry.executable() { 0o755 } else { 0o644 },
                };
                self.entries.insert(virtual_path, virtual_file);
            }
        }
        Ok(())
    }

    pub fn get_entry_type(&self, path: &VirtualPath) -> Option<VirtualEntryType> {
        // 直接ファイルとして存在するかチェック
        if self.entries.contains_key(path) {
            return Some(VirtualEntryType::File);
        }

        // パスがディレクトリかどうかをチェック
        // pathを含まない範囲で最初のキーを取得して、子パスがあるかチェック
        if let Some((entry_path, _)) = self
            .entries
            .range((Bound::Excluded(path), Bound::Unbounded))
            .next()
        {
            if entry_path.segments.len() > path.segments.len()
                && entry_path.segments[..path.segments.len()] == path.segments
            {
                return Some(VirtualEntryType::Directory);
            }
        }

        None
    }

    pub fn get_file(&self, path: &VirtualPath) -> Option<&VirtualFile> {
        self.entries.get(path)
    }

    pub fn set_file(
        &mut self,
        path: &VirtualPath,
        source: VirtualFileSource,
        permissions: u32,
    ) -> Result<()> {
        let virtual_file = VirtualFile {
            source,
            permissions,
        };
        self.entries.insert(path.clone(), virtual_file);
        Ok(())
    }

    pub fn delete_file(&mut self, path: &VirtualPath) -> Result<()> {
        self.entries.remove(path);
        Ok(())
    }
    pub async fn read_file_content(&self, path: &VirtualPath) -> Result<Vec<u8>> {
        let virtual_file = self
            .entries
            .get(path)
            .ok_or_else(|| anyhow::anyhow!("File not found: {}", path.to_string()))?;

        virtual_file
            .source
            .read_data(&*self.url_fetcher, &*self.fs_accessor)
            .await
            .map_err(|x| anyhow::anyhow!(x))
    }

    pub fn write_file_content(
        &mut self,
        path: &VirtualPath,
        value: &[u8],
        permissions: u32,
    ) -> Result<()> {
        let virtual_file = VirtualFile {
            source: VirtualFileSource::Inline(value.to_vec()),
            permissions,
        };
        self.entries.insert(path.clone(), virtual_file);
        Ok(())
    }

    pub fn export_to_file_bundle(&self) -> Result<FileBundle> {
        let mut entries = Vec::new();

        for (virtual_path, virtual_file) in &self.entries {
            let path_buf = PathBuf::from(virtual_path.to_string());
            let executable = virtual_file.permissions & 0o111 != 0;

            let file_source = match &virtual_file.source {
                VirtualFileSource::Inline(data) => FileSource::InMemory(data.clone()),
                VirtualFileSource::Path(path) => FileSource::LocalPath(path.clone()),
                VirtualFileSource::Url(url_str) => {
                    let url = Url::parse(url_str)?;
                    FileSource::RemoteUrl(url)
                }
            };

            let entry = FileEntry::new(path_buf, executable, file_source);
            entries.push(entry);
        }

        Ok(FileBundle::new(entries))
    }

    pub fn list_files(&self) -> Vec<VirtualPath> {
        self.entries.keys().cloned().collect()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub async fn mount_to_physical_fs(&self, base_path: &Path) -> Result<()> {
        for (virtual_path, virtual_file) in &self.entries {
            let physical_path = base_path.join(virtual_path.to_string());
            let data = virtual_file
                .source
                .read_data(&*self.url_fetcher, &*self.fs_accessor)
                .await
                .map_err(|x| anyhow::anyhow!(x))?;
            let executable = virtual_file.permissions & 0o111 != 0;
            println!(
                "Mounting {} to {}",
                virtual_path.to_string(),
                physical_path.display()
            );
            self.fs_accessor
                .write(&physical_path, &data, executable)
                .map_err(|x| anyhow::anyhow!(x))?;
        }
        Ok(())
    }
}
