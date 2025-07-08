use crate::domain::{FileBundle, FileEntry, FileSource};
use crate::infra::fs_handler::FsHandler;
use std::path::Path;

pub struct FileBundleCreator<T: FsHandler> {
    fs_handler: T,
}

impl<T: FsHandler> FileBundleCreator<T> {
    pub fn new(fs_handler: T) -> Self {
        Self { fs_handler }
    }

    pub fn create_from_path(&self, path: &Path) -> Result<FileBundle, String> {
        let mut entries = Vec::new();
        self.collect_files_recursive(path, path, &mut entries)?;
        Ok(FileBundle::new(entries))
    }

    fn collect_files_recursive(
        &self,
        base_path: &Path,
        current_path: &Path,
        entries: &mut Vec<FileEntry>,
    ) -> Result<(), String> {
        let dir_entries = self.fs_handler.list_entries(current_path)?;

        for entry_path in dir_entries {
            let metadata = std::fs::metadata(&entry_path).map_err(|e| {
                format!("Failed to get metadata for {}: {}", entry_path.display(), e)
            })?;

            if metadata.is_file() {
                let rel_path = entry_path
                    .strip_prefix(base_path)
                    .map_err(|e| format!("Failed to create relative path: {}", e))?
                    .to_path_buf();

                let executable = self.is_executable(&entry_path)?;
                let source = FileSource::LocalPath(entry_path.clone());

                entries.push(FileEntry::new(rel_path, executable, source));
            } else if metadata.is_dir() {
                self.collect_files_recursive(base_path, &entry_path, entries)?;
            }
        }

        Ok(())
    }

    fn is_executable(&self, path: &Path) -> Result<bool, String> {
        use std::fs;

        let metadata = fs::metadata(path)
            .map_err(|e| format!("Failed to get metadata for {}: {}", path.display(), e))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let permissions = metadata.permissions();
            Ok(permissions.mode() & 0o111 != 0)
        }

        #[cfg(windows)]
        {
            let extension = path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.to_lowercase());
            Ok(matches!(
                extension.as_deref(),
                Some("exe") | Some("bat") | Some("cmd")
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infra::fs_handler::OnMemoryFsHandler;
    use std::path::PathBuf;

    #[test]
    fn test_create_from_path_with_memory_fs() {
        let fs_handler = OnMemoryFsHandler::new();

        fs_handler
            .write(&PathBuf::from("/test/file1.txt"), b"content1", false)
            .unwrap();
        fs_handler
            .write(&PathBuf::from("/test/file2.txt"), b"content2", true)
            .unwrap();
        fs_handler
            .write(&PathBuf::from("/test/subdir/file3.txt"), b"content3", false)
            .unwrap();

        let creator = FileBundleCreator::new(fs_handler);
        let bundle = creator.create_from_path(&PathBuf::from("/test")).unwrap();

        assert_eq!(bundle.len(), 3);

        let entries = bundle.entries();
        assert_eq!(entries[0].rel_path(), &PathBuf::from("file1.txt"));
        assert_eq!(entries[1].rel_path(), &PathBuf::from("file2.txt"));
        assert_eq!(entries[2].rel_path(), &PathBuf::from("subdir/file3.txt"));
    }
}
