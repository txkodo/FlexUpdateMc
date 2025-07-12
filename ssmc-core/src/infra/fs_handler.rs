use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

#[async_trait::async_trait]
pub trait FsHandler: Send + Sync {
    fn list_entries(&self, path: &Path) -> Result<Vec<PathBuf>, String>;
    fn mkdir(&self, path: &Path) -> Result<(), String>;
    fn create_symlink(&self, path: &Path, target: &Path) -> Result<(), String>;
    fn read(&self, path: &Path) -> Result<Vec<u8>, String>;
    fn write(&self, path: &Path, data: &[u8], executable: bool) -> Result<(), String>;
    fn delete(&self, path: &Path) -> Result<(), String>;
    fn is_file(&self, path: &Path) -> bool;
    fn is_dir(&self, path: &Path) -> bool;
}

#[derive(Debug, Clone)]
pub struct DefaultFsHandler {}

impl DefaultFsHandler {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait::async_trait]
impl FsHandler for DefaultFsHandler {
    fn list_entries(&self, path: &Path) -> Result<Vec<PathBuf>, String> {
        let entries = fs::read_dir(path)
            .map_err(|e| format!("Failed to read directory {}: {}", path.display(), e))?;

        let mut result = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
            result.push(entry.path());
        }

        Ok(result)
    }

    fn create_symlink(&self, path: &Path, target: &Path) -> Result<(), String> {
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(target, path).map_err(|e| {
                format!(
                    "Failed to create symlink {} -> {}: {}",
                    path.display(),
                    target.display(),
                    e
                )
            })
        }
        #[cfg(windows)]
        {
            use std::fs;
            use std::os::windows::fs::{symlink_dir, symlink_file};
            if target.is_dir() {
                symlink_dir(target, path).map_err(|e| {
                    format!(
                        "Failed to create directory symlink {} -> {}: {}",
                        path.display(),
                        target.display(),
                        e
                    )
                })
            } else {
                symlink_file(target, path).map_err(|e| {
                    format!(
                        "Failed to create file symlink {} -> {}: {}",
                        path.display(),
                        target.display(),
                        e
                    )
                })
            }
        }
        #[cfg(not(any(unix, windows)))]
        {
            Err("Symlinks are not supported on this platform".to_string())
        }
    }

    fn mkdir(&self, path: &Path) -> Result<(), String> {
        fs::create_dir_all(path)
            .map_err(|e| format!("Failed to create directory {}: {}", path.display(), e))
    }

    fn read(&self, path: &Path) -> Result<Vec<u8>, String> {
        fs::read(path).map_err(|e| format!("Failed to read file {}: {}", path.display(), e))
    }

    fn write(&self, path: &Path, data: &[u8], executable: bool) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                format!(
                    "Failed to create parent directory {}: {}",
                    parent.display(),
                    e
                )
            })?;
        }

        fs::write(path, data)
            .map_err(|e| format!("Failed to write file {}: {}", path.display(), e))?;

        if executable {
            self.set_executable(path)?;
        }

        Ok(())
    }

    fn delete(&self, path: &Path) -> Result<(), String> {
        if !path.exists() {
            return Ok(());
        }

        if path.is_dir() {
            fs::remove_dir_all(path)
                .map_err(|e| format!("Failed to remove directory {}: {}", path.display(), e))
        } else {
            fs::remove_file(path)
                .map_err(|e| format!("Failed to remove file {}: {}", path.display(), e))
        }
    }

    fn is_file(&self, path: &Path) -> bool {
        path.is_file()
    }

    fn is_dir(&self, path: &Path) -> bool {
        path.is_dir()
    }
}

impl DefaultFsHandler {
    #[cfg(unix)]
    fn set_executable(&self, path: &Path) -> Result<(), String> {
        use std::os::unix::fs::PermissionsExt;

        let metadata = fs::metadata(path)
            .map_err(|e| format!("Failed to get metadata for {}: {}", path.display(), e))?;

        let mut permissions = metadata.permissions();
        permissions.set_mode(permissions.mode() | 0o111);

        fs::set_permissions(path, permissions).map_err(|e| {
            format!(
                "Failed to set executable permission for {}: {}",
                path.display(),
                e
            )
        })?;

        Ok(())
    }

    #[cfg(windows)]
    fn set_executable(&self, _path: &Path) -> Result<(), String> {
        // On Windows, executable permission is determined by file extension
        Ok(())
    }
}

pub struct OnMemoryFsHandler {
    files: Arc<RwLock<HashMap<PathBuf, Vec<u8>>>>,
    links: Arc<RwLock<HashMap<PathBuf, PathBuf>>>,
    directories: Arc<RwLock<HashMap<PathBuf, bool>>>,
    executable_files: Arc<RwLock<HashMap<PathBuf, bool>>>,
}

impl OnMemoryFsHandler {
    pub fn new() -> Self {
        Self {
            files: Arc::new(RwLock::new(HashMap::new())),
            links: Arc::new(RwLock::new(HashMap::new())),
            directories: Arc::new(RwLock::new(HashMap::new())),
            executable_files: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait::async_trait]
impl FsHandler for OnMemoryFsHandler {
    fn list_entries(&self, path: &Path) -> Result<Vec<PathBuf>, String> {
        let mut entries = Vec::new();

        // Find all files and directories that are direct children of the given path
        let files = self
            .files
            .read()
            .map_err(|e| format!("Lock error: {}", e))?;
        for file_path in files.keys() {
            if let Some(parent) = file_path.parent() {
                if parent == path {
                    entries.push(file_path.clone());
                }
            }
        }
        drop(files);

        let directories = self
            .directories
            .read()
            .map_err(|e| format!("Lock error: {}", e))?;
        for dir_path in directories.keys() {
            if let Some(parent) = dir_path.parent() {
                if parent == path {
                    entries.push(dir_path.clone());
                }
            }
        }
        drop(directories);

        Ok(entries)
    }

    fn mkdir(&self, path: &Path) -> Result<(), String> {
        let mut directories = self
            .directories
            .write()
            .map_err(|e| format!("Lock error: {}", e))?;
        directories.insert(path.to_path_buf(), true);

        // Create parent directories if they don't exist
        let mut current = path;
        while let Some(parent) = current.parent() {
            if parent == Path::new("") || parent == Path::new("/") {
                break;
            }
            directories.insert(parent.to_path_buf(), true);
            current = parent;
        }

        Ok(())
    }

    fn create_symlink(&self, path: &Path, target: &Path) -> Result<(), String> {
        // Create parent directories
        if let Some(parent) = path.parent() {
            self.mkdir(parent)?;
        }

        self.links
            .write()
            .map_err(|e| format!("Lock error: {}", e))?
            .insert(path.to_path_buf(), target.to_path_buf());

        Ok(())
    }

    fn read(&self, path: &Path) -> Result<Vec<u8>, String> {
        let files = self
            .files
            .read()
            .map_err(|e| format!("Lock error: {}", e))?;
        files
            .get(path)
            .cloned()
            .ok_or_else(|| format!("File not found: {}", path.display()))
    }

    fn write(&self, path: &Path, data: &[u8], executable: bool) -> Result<(), String> {
        // Create parent directories
        if let Some(parent) = path.parent() {
            self.mkdir(parent)?;
        }

        self.files
            .write()
            .map_err(|e| format!("Lock error: {}", e))?
            .insert(path.to_path_buf(), data.to_vec());
        self.executable_files
            .write()
            .map_err(|e| format!("Lock error: {}", e))?
            .insert(path.to_path_buf(), executable);

        Ok(())
    }

    fn delete(&self, path: &Path) -> Result<(), String> {
        // Remove file if it exists
        if self
            .files
            .write()
            .map_err(|e| format!("Lock error: {}", e))?
            .remove(path)
            .is_some()
        {
            self.executable_files
                .write()
                .map_err(|e| format!("Lock error: {}", e))?
                .remove(path);
            return Ok(());
        }

        // Remove directory if it exists
        if self
            .directories
            .write()
            .map_err(|e| format!("Lock error: {}", e))?
            .remove(path)
            .is_some()
        {
            // Also remove all files and subdirectories within this directory
            let path_str = path.to_string_lossy().to_string();
            self.files
                .write()
                .map_err(|e| format!("Lock error: {}", e))?
                .retain(|k, _| !k.to_string_lossy().starts_with(&path_str));
            self.directories
                .write()
                .map_err(|e| format!("Lock error: {}", e))?
                .retain(|k, _| !k.to_string_lossy().starts_with(&path_str));
            self.executable_files
                .write()
                .map_err(|e| format!("Lock error: {}", e))?
                .retain(|k, _| !k.to_string_lossy().starts_with(&path_str));
            return Ok(());
        }

        Ok(())
    }

    fn is_file(&self, path: &Path) -> bool {
        self.files
            .read()
            .map(|files| files.contains_key(path))
            .unwrap_or(false)
    }

    fn is_dir(&self, path: &Path) -> bool {
        self.directories
            .read()
            .map(|directories| directories.contains_key(path))
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_default_fs_handler_write_read() {
        let temp_dir = TempDir::new().unwrap();
        let fs_handler = DefaultFsHandler::new();

        let file_path = temp_dir.path().join("test.txt");
        let data = b"hello world";

        fs_handler.write(&file_path, data, false).unwrap();
        let read_data = fs_handler.read(&file_path).unwrap();

        assert_eq!(data, &read_data[..]);
    }

    #[tokio::test]
    async fn test_default_fs_handler_mkdir_list() {
        let temp_dir = TempDir::new().unwrap();
        let fs_handler = DefaultFsHandler::new();

        let dir_path = temp_dir.path().join("subdir");
        fs_handler.mkdir(&dir_path).unwrap();

        let entries = fs_handler.list_entries(temp_dir.path()).unwrap();
        assert!(entries.iter().any(|p| p.file_name().unwrap() == "subdir"));
    }

    #[tokio::test]
    async fn test_default_fs_handler_delete() {
        let temp_dir = TempDir::new().unwrap();
        let fs_handler = DefaultFsHandler::new();

        let file_path = temp_dir.path().join("test.txt");
        fs_handler.write(&file_path, b"test", false).unwrap();

        assert!(file_path.exists());

        fs_handler.delete(&file_path).unwrap();

        assert!(!file_path.exists());
    }

    #[tokio::test]
    async fn test_on_memory_fs_handler_write_read() {
        let fs_handler = OnMemoryFsHandler::new();

        let file_path = PathBuf::from("/test.txt");
        let data = b"hello world";

        fs_handler.write(&file_path, data, false).unwrap();
        let read_data = fs_handler.read(&file_path).unwrap();

        assert_eq!(data, &read_data[..]);
    }

    #[tokio::test]
    async fn test_on_memory_fs_handler_mkdir_list() {
        let fs_handler = OnMemoryFsHandler::new();

        let dir_path = PathBuf::from("/subdir");
        fs_handler.mkdir(&dir_path).unwrap();

        let entries = fs_handler.list_entries(&PathBuf::from("/")).unwrap();
        assert!(entries.contains(&dir_path));
    }

    #[tokio::test]
    async fn test_on_memory_fs_handler_delete() {
        let fs_handler = OnMemoryFsHandler::new();

        let file_path = PathBuf::from("/test.txt");
        fs_handler.write(&file_path, b"test", false).unwrap();

        assert!(fs_handler.read(&file_path).is_ok());

        fs_handler.delete(&file_path).unwrap();

        assert!(fs_handler.read(&file_path).is_err());
    }
}
