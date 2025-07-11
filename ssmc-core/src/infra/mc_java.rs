use crate::domain::McVanillaVersionId;
use crate::infra::trie_loader::TrieLoader;
use crate::infra::url_fetcher::UrlFetcher;
use crate::util::file_trie::{Dir, File, Path};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use url::Url;

#[async_trait::async_trait]
pub trait McJavaLoader {
    /** Javaランタイムのリストを取得 */
    async fn list_runtimes(&self) -> Result<Vec<McJava>, String>;
    /** Javaをインストールして実行パスを返す */
    async fn ready_runtime(&self, version_id: &McVanillaVersionId) -> Result<PathBuf, String>;
}

pub struct DefaultMcJavaLoader {
    url_fetcher: Box<dyn UrlFetcher + Send + Sync>,
    trie_loader: Box<dyn TrieLoader + Send + Sync>,
    cache_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McJava {
    pub version_id: String,
    pub major_version: u8,
}

#[derive(Debug, Deserialize)]
struct JavaRuntimesResponse {
    #[serde(flatten)]
    platforms: HashMap<String, HashMap<String, Vec<JavaRuntime>>>,
}

#[derive(Debug, Deserialize)]
struct JavaRuntime {
    manifest: Manifest,
    version: Version,
}

#[derive(Debug, Deserialize)]
struct Manifest {
    url: String,
}

#[derive(Debug, Deserialize)]
struct Version {
    name: String,
}

impl McJava {
    pub fn new(version_id: String, major_version: u8) -> Self {
        Self {
            version_id,
            major_version,
        }
    }
    pub fn version_id(&self) -> &str {
        &self.version_id
    }

    pub fn major_version(&self) -> u8 {
        self.major_version
    }
}

impl DefaultMcJavaLoader {
    pub fn new(
        url_fetcher: Box<dyn UrlFetcher + Send + Sync>,
        trie_loader: Box<dyn TrieLoader + Send + Sync>,
        cache_path: PathBuf,
    ) -> Self {
        Self {
            url_fetcher,
            trie_loader,
            cache_path,
        }
    }

    async fn list_runtimes(&self) -> Result<Vec<McJava>, String> {
        let url = Url::parse("https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json")
            .map_err(|e| format!("Invalid URL: {}", e))?;

        let data = self.url_fetcher.fetch_binary(&url).await?;
        let response: JavaRuntimesResponse =
            serde_json::from_slice(&data).map_err(|e| format!("Failed to parse JSON: {}", e))?;

        let mut runtimes = Vec::new();
        let current_platform = self.get_current_platform();

        for (platform_name, runtime_types) in response.platforms {
            if platform_name != current_platform {
                continue;
            }

            for (runtime_type, runtime_list) in runtime_types {
                if let Some(runtime) = runtime_list.iter().next() {
                    let major_version = self.extract_major_version(&runtime.version.name);
                    runtimes.push(McJava {
                        version_id: runtime_type,
                        major_version,
                    });
                }
            }
        }

        Ok(runtimes)
    }

    async fn ready_runtime(&self, version_id: &McVanillaVersionId) -> Result<PathBuf, String> {
        let runtime_path = self.cache_path.join(&version_id.id());
        let java_executable = runtime_path.join(self.get_java_executable_path());

        if !java_executable.exists() {
            let url: Url = Url::parse("https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json")
                        .map_err(|e| format!("Invalid URL: {}", e))?;

            let data = self.url_fetcher.fetch_binary(&url).await?;
            let response: JavaRuntimesResponse = serde_json::from_slice(&data)
                .map_err(|e| format!("Failed to parse JSON: {}", e))?;

            let current_platform = self.get_current_platform();
            let manifest_url = self
                .find_manifest_url(&response, version_id, &current_platform)
                .ok_or_else(|| format!("Runtime not found: {}", version_id.id()))?;

            let manifest_data = self.url_fetcher.fetch_binary(&manifest_url).await?;
            let manifest: ManifestResponse = serde_json::from_slice(&manifest_data)
                .map_err(|e| format!("Failed to parse manifest JSON: {}", e))?;

            let mut trie = Dir::new();

            for (path, file_info) in manifest.files {
                match file_info {
                    ManifestFile::File {
                        downloads,
                        executable: _,
                    } => {
                        if let Some(raw_download) = downloads.raw {
                            let url = Url::parse(&raw_download.url)
                                .map_err(|e| format!("Invalid file URL: {}", e))?;
                            let file = File::Url(url);
                            let virtual_path = Path::from_str(&path);
                            trie.put_file(virtual_path, file)
                                .map_err(|_| format!("Failed to add file {} to trie", path))?;
                        }
                    }
                    ManifestFile::Directory {} => {
                        let virtual_path = Path::from_str(&path);
                        trie.put_dir(virtual_path, Dir::new())
                            .map_err(|_| format!("Failed to add directory {} to trie", path))?;
                    }
                }
            }

            // Download and install the runtime
            self.trie_loader
                .write_contents(&trie, &runtime_path)
                .await
                .map_err(|e| e.to_string())?;
        }

        let java_executable = fs::canonicalize(&java_executable)
            .map_err(|e| format!("Failed to canonicalize path: {}", e))?;
        Ok(java_executable)
    }

    fn extract_major_version(&self, version_name: &str) -> u8 {
        version_name
            .split('.')
            .next()
            .and_then(|v| v.parse().ok())
            .unwrap_or(8)
    }

    fn get_current_platform(&self) -> String {
        #[cfg(all(target_os = "linux", target_arch = "x86"))]
        {
            "linux-i386".to_string()
        }
        #[cfg(all(target_os = "linux", not(target_arch = "x86")))]
        {
            "linux".to_string()
        }
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            "mac-os-arm64".to_string()
        }
        #[cfg(all(target_os = "macos", not(target_arch = "aarch64")))]
        {
            "mac-os".to_string()
        }
        #[cfg(all(target_os = "windows", target_arch = "x86"))]
        {
            "windows-x86".to_string()
        }
        #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
        {
            "windows-x64".to_string()
        }
        #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
        {
            "windows-arm64".to_string()
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        {
            "linux".to_string() // fallback
        }
    }

    fn get_java_executable_path(&self) -> &'static str {
        #[cfg(target_os = "windows")]
        {
            "bin/java.exe"
        }
        #[cfg(not(target_os = "windows"))]
        {
            "bin/java"
        }
    }

    fn find_manifest_url(
        &self,
        response: &JavaRuntimesResponse,
        version_id: &McVanillaVersionId,
        platform: &str,
    ) -> Option<Url> {
        for (platform_name, runtime_types) in &response.platforms {
            if platform_name != platform {
                continue;
            }

            for (runtime_type, runtime_list) in runtime_types {
                for runtime in runtime_list {
                    if runtime_type == version_id.id() {
                        return Url::parse(&runtime.manifest.url).ok();
                    }
                }
            }
        }
        None
    }
}

#[async_trait::async_trait]
impl McJavaLoader for DefaultMcJavaLoader {
    async fn list_runtimes(&self) -> Result<Vec<McJava>, String> {
        self.list_runtimes().await
    }

    async fn ready_runtime(&self, version_id: &McVanillaVersionId) -> Result<PathBuf, String> {
        self.ready_runtime(version_id).await
    }
}

#[derive(Debug, Deserialize)]
struct ManifestResponse {
    files: HashMap<String, ManifestFile>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ManifestFile {
    File {
        downloads: Downloads,
        executable: Option<bool>,
    },
    Directory {},
}

#[derive(Debug, Deserialize)]
struct Downloads {
    raw: Option<DownloadInfo>,
}

#[derive(Debug, Deserialize)]
struct DownloadInfo {
    url: String,
}

#[cfg(test)]
mod tests {
    use crate::infra::{
        trie_loader::DefaultTrieLoader, fs_handler::DefaultFsHandler,
        url_fetcher::DummyUrlFetcher,
    };

    use super::*;
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_list_runtimes_success() {
        let mut url_fetcher = DummyUrlFetcher::new();
        let url = Url::parse("https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json").unwrap();

        let mock_response = r#"{"linux": {"java-runtime-alpha": [{"manifest": {"url": "https://example.com/manifest.json" },"version": {"name": "17.0.1" }}]}}"#;

        url_fetcher.add_data(url, mock_response.as_bytes());

        let loader = DefaultMcJavaLoader::new(
            Box::new(url_fetcher),
            Box::new(DefaultTrieLoader::new(
                Arc::new(DefaultFsHandler::new()),
                Arc::new(DummyUrlFetcher::new()),
            )),
            PathBuf::from("/tmp/test_cache"),
        );
        let runtimes = loader.list_runtimes().await.unwrap();

        assert_eq!(runtimes.len(), 1);
        assert_eq!(runtimes[0].version_id, "java-runtime-alpha");
        assert_eq!(runtimes[0].major_version, 17);
    }

    #[tokio::test]
    async fn test_ready_runtime_success() {
        let mut url_fetcher = DummyUrlFetcher::new();
        let manifest_url = Url::parse("https://example.com/manifest.json").unwrap();

        url_fetcher.add_data(
            Url::parse("https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json").unwrap(),
             r#"{"linux": {"java-runtime-alpha": [{"manifest": {"url": "https://example.com/manifest.json"}, "version": {"name": "17.0.1"}}]}}"#);

        let manifest_response = r#"{"files": {"bin/java": {"downloads": {"raw": {"url": "https://example.com/java" } },"executable": true },"lib/": {}}}"#;

        url_fetcher.add_data(manifest_url, manifest_response.as_bytes());

        // Create shared URL fetcher for file bundle loader
        let mut file_url_fetcher = DummyUrlFetcher::new();
        file_url_fetcher.add_data(
            Url::parse("https://example.com/java").unwrap(),
            b"fake java binary content",
        );

        let loader = DefaultMcJavaLoader::new(
            Box::new(url_fetcher),
            Box::new(DefaultTrieLoader::new(
                Arc::new(DefaultFsHandler::new()),
                Arc::new(file_url_fetcher),
            )),
            PathBuf::from("/tmp/test_cache"),
        );
        let java_path = loader
            .ready_runtime(&McVanillaVersionId::new("java-runtime-alpha".to_string()))
            .await
            .unwrap();

        #[cfg(target_os = "windows")]
        assert_eq!(
            java_path,
            PathBuf::from("/tmp/test_cache/java-runtime-alpha/bin/java.exe")
        );
        #[cfg(not(target_os = "windows"))]
        assert_eq!(
            java_path,
            PathBuf::from("/tmp/test_cache/java-runtime-alpha/bin/java")
        );
    }

    #[tokio::test]
    async fn test_ready_runtime_not_found() {
        let mut url_fetcher = DummyUrlFetcher::new();
        let url = Url::parse("https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json").unwrap();

        let mock_response = r#"{"linux": {"java-runtime-alpha": []}}"#;

        url_fetcher.add_data(url, mock_response.as_bytes());

        let loader = DefaultMcJavaLoader::new(
            Box::new(url_fetcher),
            Box::new(DefaultTrieLoader::new(
                Arc::new(DefaultFsHandler::new()),
                Arc::new(DummyUrlFetcher::new()),
            )),
            PathBuf::from("/tmp/test_cache"),
        );
        let result = loader
            .ready_runtime(&McVanillaVersionId::new("nonexistent:1.0.0".to_string()))
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Runtime not found"));
    }

    #[test]
    fn test_extract_major_version() {
        let loader = DefaultMcJavaLoader::new(
            Box::new(DummyUrlFetcher::new()),
            Box::new(DefaultTrieLoader::new(
                Arc::new(DefaultFsHandler::new()),
                Arc::new(DummyUrlFetcher::new()),
            )),
            PathBuf::from("/tmp/test_cache"),
        );

        assert_eq!(loader.extract_major_version("17.0.1"), 17);
        assert_eq!(loader.extract_major_version("11.0.16"), 11);
        assert_eq!(loader.extract_major_version("8u352"), 8);
        assert_eq!(loader.extract_major_version("invalid"), 8);
    }
}
