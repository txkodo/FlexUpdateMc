use crate::{
    domain::{McServerLoader, McVanillaVersionId, McVersion},
    infra::{mc_java::McJavaLoader, url_fetcher::UrlFetcher},
    util::file_trie::{Dir, File, Path},
};
use serde::Deserialize;
use url::Url;

pub enum McVanillaVersionType {
    Release,
    Snapshot,
}

pub struct McVanillaVersion {
    pub version: McVanillaVersionId,
    pub version_type: McVanillaVersionType,
}

impl McVersion for McVanillaVersion {
    fn vanilla_id(&self) -> McVanillaVersionId {
        self.version.clone()
    }
}

pub enum McVanillaVersionQuery {
    All,
    Release,
    Snapshot,
}

pub struct VanillaVersionLoader {
    url_fetcher: Box<dyn UrlFetcher + Send + Sync>,
    java_loader: Box<dyn McJavaLoader + Send + Sync>,
}

impl VanillaVersionLoader {
    pub fn new(
        url_fetcher: Box<dyn UrlFetcher + Send + Sync>,
        java_loader: Box<dyn McJavaLoader + Send + Sync>,
    ) -> Self {
        Self {
            url_fetcher,
            java_loader,
        }
    }
}

#[async_trait::async_trait]
impl McServerLoader for VanillaVersionLoader {
    type Version = McVanillaVersion;
    async fn ready_server(
        &self,
        mut world_data: Dir,
        version: &Self::Version,
    ) -> Result<
        (
            Dir,
            Box<dyn Fn(crate::domain::ServerRunOptions) -> std::process::Command>,
        ),
        String,
    > {
        // Step 1: Get version manifest
        let manifest_url =
            Url::parse("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json")
                .map_err(|e| format!("Invalid manifest URL: {}", e))?;

        let manifest_data = self.url_fetcher.fetch_binary(&manifest_url).await?;
        let manifest: VersionManifest = serde_json::from_slice(&manifest_data)
            .map_err(|e| format!("Failed to parse version manifest: {}", e))?;

        // Step 2: Find the specific version
        let version_info = manifest
            .versions
            .into_iter()
            .find(|v| v.id == version.version.id())
            .ok_or_else(|| format!("Version '{}' not found", version.version.id()))?;

        // Step 3: Get version details
        let version_url =
            Url::parse(&version_info.url).map_err(|e| format!("Invalid version URL: {}", e))?;

        let version_data = self.url_fetcher.fetch_binary(&version_url).await?;
        let version_details: VersionDetails = serde_json::from_slice(&version_data)
            .map_err(|e| format!("Failed to parse version details: {}", e))?;

        // Step 4: Create file bundle with server jar
        let server_download = version_details.downloads.server.ok_or_else(|| {
            format!(
                "Server download not available for version '{}'",
                version.version.id()
            )
        })?;

        let server_url =
            Url::parse(&server_download.url).map_err(|e| format!("Invalid server URL: {}", e))?;

        let server_file = File::Url(server_url);
        world_data.put_file(Path::from_str("server.jar"), server_file)
            .map_err(|_| "Failed to add server.jar to world data".to_string())?;

        // Step 5: Get Java runtime path
        // Determine required Java version from version details
        let java_version_id = McVanillaVersionId::new(
            version_details
                .java_version
                .map_or("jre-legacy".to_string(), |x| x.component),
        );

        let java_path = self.java_loader.ready_runtime(&java_version_id).await?;

        // Step 6: Create command factory with full Java path
        let command_factory = Box::new(move |options: crate::domain::ServerRunOptions| {
            let mut cmd = std::process::Command::new(&java_path);

            // Add JVM arguments
            if let Some(xmx) = options.max_memory {
                cmd.arg(format!("-Xmx{}M", xmx));
            }
            if let Some(xms) = options.initial_memory {
                cmd.arg(format!("-Xms{}M", xms));
            }

            // Add jar and nogui arguments
            cmd.arg("-jar").arg("server.jar").arg("nogui");

            cmd
        });

        Ok((world_data, command_factory))
    }
}

#[async_trait::async_trait]
impl crate::domain::McVersionQuerier for VanillaVersionLoader {
    type Version = McVanillaVersion;
    type VersionQuery = McVanillaVersionQuery;

    async fn query_versions(&self, query: &Self::VersionQuery) -> Vec<Self::Version> {
        let url =
            match Url::parse("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json") {
                Ok(url) => url,
                Err(_) => return vec![],
            };

        let data = match self.url_fetcher.fetch_binary(&url).await {
            Ok(data) => data,
            Err(_) => return vec![],
        };

        let manifest: VersionManifest = match serde_json::from_slice(&data) {
            Ok(manifest) => manifest,
            Err(_) => return vec![],
        };

        manifest
            .versions
            .into_iter()
            .filter_map(|version_info| {
                let version_type = match version_info.version_type.as_str() {
                    "release" => McVanillaVersionType::Release,
                    "snapshot" => McVanillaVersionType::Snapshot,
                    _ => return None,
                };

                let should_include = match query {
                    McVanillaVersionQuery::All => true,
                    McVanillaVersionQuery::Release => {
                        matches!(version_type, McVanillaVersionType::Release)
                    }
                    McVanillaVersionQuery::Snapshot => {
                        matches!(version_type, McVanillaVersionType::Snapshot)
                    }
                };

                if should_include {
                    Some(McVanillaVersion {
                        version: McVanillaVersionId::new(version_info.id),
                        version_type,
                    })
                } else {
                    None
                }
            })
            .collect()
    }
}

#[derive(Debug, Deserialize)]
struct VersionManifest {
    versions: Vec<VersionInfo>,
}

#[derive(Debug, Deserialize)]
struct VersionInfo {
    id: String,
    #[serde(rename = "type")]
    version_type: String,
    url: String,
}

#[derive(Debug, Deserialize)]
struct VersionDetails {
    downloads: Downloads,
    #[serde(rename = "javaVersion")]
    java_version: Option<JavaVersion>,
}

#[derive(Debug, Deserialize)]
struct Downloads {
    server: Option<DownloadInfo>,
}

#[derive(Debug, Deserialize)]
struct DownloadInfo {
    url: String,
}

#[derive(Debug, Deserialize)]
struct JavaVersion {
    component: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        domain::McVersionQuerier,
        infra::{mc_java::McJavaLoader, url_fetcher::DummyUrlFetcher},
    };
    use std::path::PathBuf;

    struct DummyJavaLoader;

    #[async_trait::async_trait]
    impl McJavaLoader for DummyJavaLoader {
        async fn list_runtimes(&self) -> Result<Vec<crate::infra::mc_java::McJava>, String> {
            Ok(vec![])
        }

        async fn ready_runtime(&self, _version_id: &McVanillaVersionId) -> Result<PathBuf, String> {
            Ok(PathBuf::from("/usr/bin/java"))
        }
    }

    fn create_test_loader(url_fetcher: DummyUrlFetcher) -> VanillaVersionLoader {
        VanillaVersionLoader::new(Box::new(url_fetcher), Box::new(DummyJavaLoader))
    }

    #[tokio::test]
    async fn test_query_versions_all() {
        let mut url_fetcher = DummyUrlFetcher::new();
        let manifest_url =
            Url::parse("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json").unwrap();

        let mock_manifest = r#"{
            "versions": [
                {
                    "id": "1.20.1",
                    "type": "release",
                    "url": "https://example.com/1.20.1.json",
                    "time": "2023-06-07T10:46:16+00:00",
                    "releaseTime": "2023-06-07T10:46:16+00:00"
                },
                {
                    "id": "23w18a",
                    "type": "snapshot",
                    "url": "https://example.com/23w18a.json",
                    "time": "2023-05-03T11:12:34+00:00",
                    "releaseTime": "2023-05-03T11:12:34+00:00"
                },
                {
                    "id": "1.19.4",
                    "type": "release",
                    "url": "https://example.com/1.19.4.json",
                    "time": "2023-03-14T12:56:18+00:00",
                    "releaseTime": "2023-03-14T12:56:18+00:00"
                }
            ]
        }"#;

        url_fetcher.add_data(manifest_url, mock_manifest.as_bytes());

        let loader = create_test_loader(url_fetcher);
        let versions = loader.query_versions(&McVanillaVersionQuery::All).await;

        assert_eq!(versions.len(), 3);
        assert_eq!(versions[0].version.id(), "1.20.1");
        assert!(matches!(
            versions[0].version_type,
            McVanillaVersionType::Release
        ));
        assert_eq!(versions[1].version.id(), "23w18a");
        assert!(matches!(
            versions[1].version_type,
            McVanillaVersionType::Snapshot
        ));
        assert_eq!(versions[2].version.id(), "1.19.4");
        assert!(matches!(
            versions[2].version_type,
            McVanillaVersionType::Release
        ));
    }

    #[tokio::test]
    async fn test_query_versions_release_only() {
        let mut url_fetcher = DummyUrlFetcher::new();
        let manifest_url =
            Url::parse("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json").unwrap();

        let mock_manifest = r#"{
            "versions": [
                {
                    "id": "1.20.1",
                    "type": "release",
                    "url": "https://example.com/1.20.1.json",
                    "time": "2023-06-07T10:46:16+00:00",
                    "releaseTime": "2023-06-07T10:46:16+00:00"
                },
                {
                    "id": "23w18a",
                    "type": "snapshot",
                    "url": "https://example.com/23w18a.json",
                    "time": "2023-05-03T11:12:34+00:00",
                    "releaseTime": "2023-05-03T11:12:34+00:00"
                }
            ]
        }"#;

        url_fetcher.add_data(manifest_url, mock_manifest.as_bytes());

        let loader = create_test_loader(url_fetcher);
        let versions = loader.query_versions(&McVanillaVersionQuery::Release).await;

        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].version.id(), "1.20.1");
        assert!(matches!(
            versions[0].version_type,
            McVanillaVersionType::Release
        ));
    }

    #[tokio::test]
    async fn test_query_versions_snapshot_only() {
        let mut url_fetcher = DummyUrlFetcher::new();
        let manifest_url =
            Url::parse("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json").unwrap();

        let mock_manifest = r#"{
            "versions": [
                {
                    "id": "1.20.1",
                    "type": "release",
                    "url": "https://example.com/1.20.1.json",
                    "time": "2023-06-07T10:46:16+00:00",
                    "releaseTime": "2023-06-07T10:46:16+00:00"
                },
                {
                    "id": "23w18a",
                    "type": "snapshot",
                    "url": "https://example.com/23w18a.json",
                    "time": "2023-05-03T11:12:34+00:00",
                    "releaseTime": "2023-05-03T11:12:34+00:00"
                }
            ]
        }"#;

        url_fetcher.add_data(manifest_url, mock_manifest.as_bytes());

        let loader = create_test_loader(url_fetcher);
        let versions = loader
            .query_versions(&McVanillaVersionQuery::Snapshot)
            .await;

        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].version.id(), "23w18a");
        assert!(matches!(
            versions[0].version_type,
            McVanillaVersionType::Snapshot
        ));
    }

    #[tokio::test]
    async fn test_query_versions_invalid_response() {
        let mut url_fetcher = DummyUrlFetcher::new();
        let manifest_url =
            Url::parse("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json").unwrap();

        url_fetcher.add_data(manifest_url, b"invalid json");

        let loader = create_test_loader(url_fetcher);
        let versions = loader.query_versions(&McVanillaVersionQuery::All).await;

        assert_eq!(versions.len(), 0);
    }

    #[tokio::test]
    async fn test_ready_version_success() {
        let mut url_fetcher = DummyUrlFetcher::new();

        // Mock version manifest
        let manifest_url =
            Url::parse("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json").unwrap();
        let mock_manifest = r#"{
            "versions": [
                {
                    "id": "1.20.1",
                    "type": "release",
                    "url": "https://example.com/1.20.1.json",
                    "time": "2023-06-07T10:46:16+00:00",
                    "releaseTime": "2023-06-07T10:46:16+00:00"
                }
            ]
        }"#;
        url_fetcher.add_data(manifest_url, mock_manifest.as_bytes());

        // Mock version details
        let version_url = Url::parse("https://example.com/1.20.1.json").unwrap();
        let mock_version_details = r#"{
            "id": "1.20.1",
            "type": "release",
            "downloads": {
                "server": {
                    "url": "https://example.com/server.jar",
                    "sha1": "abc123",
                    "size": 12345
                }
            },
            "javaVersion": {
                "component": "java-runtime-gamma",
                "majorVersion": 17
            }
        }"#;
        url_fetcher.add_data(version_url, mock_version_details.as_bytes());

        let loader = create_test_loader(url_fetcher);
        let result = loader
            .ready_server(
                Dir::new(), // Empty world data for this test
                &McVanillaVersion {
                    version: McVanillaVersionId::new("1.20.1".to_string()),
                    version_type: McVanillaVersionType::Release,
                },
            )
            .await;

        assert!(result.is_ok());
        let (world_data, command_factory) = result.unwrap();

        // Check that server.jar was added to world data
        assert!(world_data.get_file(Path::from_str("server.jar")).is_some());

        // Check command factory
        let options = crate::domain::ServerRunOptions {
            max_memory: Some(2048),
            initial_memory: Some(1024),
        };
        let command = command_factory(options);
        let args: Vec<&str> = command
            .get_args()
            .map(|arg| arg.to_str().unwrap())
            .collect();

        assert!(args.contains(&"-Xmx2048M"));
        assert!(args.contains(&"-Xms1024M"));
        assert!(args.contains(&"-jar"));
        assert!(args.contains(&"server.jar"));
        assert!(args.contains(&"nogui"));
    }

    #[tokio::test]
    async fn test_ready_version_not_found() {
        let mut url_fetcher = DummyUrlFetcher::new();

        let manifest_url =
            Url::parse("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json").unwrap();
        let mock_manifest = r#"{
            "versions": [
                {
                    "id": "1.20.1",
                    "type": "release",
                    "url": "https://example.com/1.20.1.json",
                    "time": "2023-06-07T10:46:16+00:00",
                    "releaseTime": "2023-06-07T10:46:16+00:00"
                }
            ]
        }"#;
        url_fetcher.add_data(manifest_url, mock_manifest.as_bytes());

        let loader = create_test_loader(url_fetcher);
        let result = loader
            .ready_server(
                Dir::new(), // Empty world data for this test
                &McVanillaVersion {
                    version: McVanillaVersionId::new("1.19.4".to_string()),
                    version_type: McVanillaVersionType::Release,
                },
            )
            .await;

        assert!(result.is_err());
        let error_msg = result.err().unwrap();
        assert!(error_msg.contains("Version '1.19.4' not found"));
    }

    #[tokio::test]
    async fn test_ready_version_no_server_download() {
        let mut url_fetcher = DummyUrlFetcher::new();

        // Mock version manifest
        let manifest_url =
            Url::parse("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json").unwrap();
        let mock_manifest = r#"{
            "versions": [
                {
                    "id": "1.20.1",
                    "type": "release",
                    "url": "https://example.com/1.20.1.json",
                    "time": "2023-06-07T10:46:16+00:00",
                    "releaseTime": "2023-06-07T10:46:16+00:00"
                }
            ]
        }"#;
        url_fetcher.add_data(manifest_url, mock_manifest.as_bytes());

        // Mock version details without server download
        let version_url = Url::parse("https://example.com/1.20.1.json").unwrap();
        let mock_version_details = r#"{
            "id": "1.20.1",
            "type": "release",
            "downloads": {
                "client": {
                    "url": "https://example.com/client.jar",
                    "sha1": "def456",
                    "size": 67890
                }
            }
        }"#;
        url_fetcher.add_data(version_url, mock_version_details.as_bytes());

        let loader = create_test_loader(url_fetcher);
        let result = loader
            .ready_server(
                Dir::new(), // Empty world data for this test
                &McVanillaVersion {
                    version: McVanillaVersionId::new("1.20.1".to_string()),
                    version_type: McVanillaVersionType::Release,
                },
            )
            .await;

        assert!(result.is_err());
        let error_msg = result.err().unwrap();
        assert!(error_msg.contains("Server download not available"));
    }

    #[tokio::test]
    async fn test_command_factory_minimal_options() {
        let mut url_fetcher = DummyUrlFetcher::new();

        // Mock version manifest
        let manifest_url =
            Url::parse("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json").unwrap();
        let mock_manifest = r#"{
            "versions": [
                {
                    "id": "1.20.1",
                    "type": "release",
                    "url": "https://example.com/1.20.1.json",
                    "time": "2023-06-07T10:46:16+00:00",
                    "releaseTime": "2023-06-07T10:46:16+00:00"
                }
            ]
        }"#;
        url_fetcher.add_data(manifest_url, mock_manifest.as_bytes());

        // Mock version details
        let version_url = Url::parse("https://example.com/1.20.1.json").unwrap();
        let mock_version_details = r#"{
            "id": "1.20.1",
            "type": "release",
            "downloads": {
                "server": {
                    "url": "https://example.com/server.jar",
                    "sha1": "abc123",
                    "size": 12345
                }
            },
            "javaVersion": {
                "component": "java-runtime-gamma",
                "majorVersion": 17
            }
        }"#;
        url_fetcher.add_data(version_url, mock_version_details.as_bytes());

        let loader = create_test_loader(url_fetcher);
        let result = loader
            .ready_server(
                Dir::new(), // Empty world data for this test
                &McVanillaVersion {
                    version: McVanillaVersionId::new("1.20.1".to_string()),
                    version_type: McVanillaVersionType::Release,
                },
            )
            .await;

        assert!(result.is_ok());
        let (_, command_factory) = result.unwrap();

        // Test with no memory options
        let options = crate::domain::ServerRunOptions {
            max_memory: None,
            initial_memory: None,
        };
        let command = command_factory(options);
        let args: Vec<&str> = command
            .get_args()
            .map(|arg| arg.to_str().unwrap())
            .collect();

        // Should not contain memory arguments
        assert!(!args.iter().any(|arg| arg.starts_with("-Xmx")));
        assert!(!args.iter().any(|arg| arg.starts_with("-Xms")));

        // Should still contain jar and nogui
        assert!(args.contains(&"-jar"));
        assert!(args.contains(&"server.jar"));
        assert!(args.contains(&"nogui"));
    }

    #[tokio::test]
    async fn test_world_data_includes_server_jar() {
        let mut url_fetcher = DummyUrlFetcher::new();

        // Mock version manifest
        let manifest_url =
            Url::parse("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json").unwrap();
        let mock_manifest = r#"{
            "versions": [
                {
                    "id": "1.20.1",
                    "type": "release",
                    "url": "https://example.com/1.20.1.json",
                    "time": "2023-06-07T10:46:16+00:00",
                    "releaseTime": "2023-06-07T10:46:16+00:00"
                }
            ]
        }"#;
        url_fetcher.add_data(manifest_url, mock_manifest.as_bytes());

        // Mock version details
        let version_url = Url::parse("https://example.com/1.20.1.json").unwrap();
        let mock_version_details = r#"{
            "id": "1.20.1",
            "type": "release",
            "downloads": {
                "server": {
                    "url": "https://example.com/server.jar",
                    "sha1": "abc123",
                    "size": 12345
                }
            },
            "javaVersion": {
                "component": "java-runtime-gamma",
                "majorVersion": 17
            }
        }"#;
        url_fetcher.add_data(version_url, mock_version_details.as_bytes());

        let loader = create_test_loader(url_fetcher);

        // Create initial world data with some files
        let mut initial_world_data = Dir::new();
        initial_world_data.put_file(
            Path::from_str("level.dat"),
            File::Inline(b"world data".to_vec())
        ).unwrap();
        initial_world_data.put_file(
            Path::from_str("region/r.0.0.mca"), 
            File::Inline(b"region data".to_vec())
        ).unwrap();

        let result = loader
            .ready_server(
                initial_world_data,
                &McVanillaVersion {
                    version: McVanillaVersionId::new("1.20.1".to_string()),
                    version_type: McVanillaVersionType::Release,
                },
            )
            .await;

        assert!(result.is_ok());
        let (final_world_data, _command_factory) = result.unwrap();

        // Check for original world files
        let level_dat = final_world_data.get_file(Path::from_str("level.dat"))
            .expect("level.dat should be present");
        assert!(matches!(level_dat, File::Inline(_)));

        let region_file = final_world_data.get_file(Path::from_str("region/r.0.0.mca"))
            .expect("region file should be present");
        assert!(matches!(region_file, File::Inline(_)));

        // Check for added server.jar
        let server_jar = final_world_data.get_file(Path::from_str("server.jar"))
            .expect("server.jar should be added to world data");
        assert!(matches!(server_jar, File::Url(_)));
    }
}
