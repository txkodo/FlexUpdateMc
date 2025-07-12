use ssmc_core::domain::{McServerLoader, McVersionQuerier, ServerRunOptions};
use ssmc_core::infra::mc_java::DefaultMcJavaLoader;
use ssmc_core::infra::trie_loader::{DefaultTrieLoader, TrieLoader};
use ssmc_core::infra::url_fetcher::DefaultUrlFetcher;
use ssmc_core::infra::vanilla::{McVanillaVersionQuery, VanillaVersionLoader};
use ssmc_core::util::file_trie::Dir;
use std::error::Error;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("=== Vanilla Minecraft Server Loader Example ===\n");

    // Create dependencies
    let url_fetcher = Arc::new(DefaultUrlFetcher);
    let java_loader = Arc::new(DefaultMcJavaLoader::new(
        Arc::new(DefaultUrlFetcher),
        Arc::new(DefaultTrieLoader::new(
            Arc::new(ssmc_core::infra::fs_handler::DefaultFsHandler::new()),
            Arc::new(DefaultUrlFetcher),
        )),
        PathBuf::from("temp_workspace/java-cache"),
    ));

    let loader = VanillaVersionLoader::new(url_fetcher, java_loader);

    // Step 1: Query available versions
    println!("Fetching available Minecraft versions...");
    let versions = loader.query_versions(&McVanillaVersionQuery::Release).await;

    if versions.is_empty() {
        println!("❌ No versions found. Check your internet connection.");
        return Ok(());
    }

    println!("✅ Found {} release versions", versions.len());
    println!("Latest versions:");
    for (i, version) in versions.iter().take(5).enumerate() {
        println!("  {}. {}", i + 1, version.version.id());
    }

    // Step 2: Select a version to download (use the latest)
    let target_version = &versions[0];
    println!(
        "\n📦 Downloading Minecraft server: {}",
        target_version.version.id()
    );

    let start_time = Instant::now();

    // Step 3: Prepare the server (download jar and setup Java)
    let (dir, command_factory) = loader.ready_server(Dir::new(), target_version).await?;

    let setup_time = start_time.elapsed();
    println!(
        "✅ Server preparation completed in {:.2} seconds",
        setup_time.as_secs_f64()
    );

    // Step 4: Write files to disk
    println!("\n📁 Writing server files...");
    let output_dir = PathBuf::from("temp_workspace/minecraft-server");

    // Remove existing directory if it exists
    if output_dir.exists() {
        println!("🗑️  Removing existing directory: {:?}", output_dir);
        std::fs::remove_dir_all(&output_dir)?;
    }

    let trie_loader = DefaultTrieLoader::new(
        Arc::new(ssmc_core::infra::fs_handler::DefaultFsHandler::new()),
        Arc::new(DefaultUrlFetcher),
    );

    let write_start = Instant::now();
    trie_loader.mount_contents(&dir, &output_dir).await?;
    let write_time = write_start.elapsed();

    println!(
        "✅ Files written in {:.2} seconds",
        write_time.as_secs_f64()
    );
    println!("📂 Server files saved to: {:?}", output_dir);

    // Step 6: Generate sample command
    println!("\n🚀 Sample server startup commands:");

    let options_basic = ServerRunOptions {
        max_memory: None,
        initial_memory: None,
    };
    let cmd_basic = command_factory(options_basic);
    println!(
        "  Basic: {} {:?}",
        cmd_basic.get_program().to_string_lossy(),
        cmd_basic.get_args().collect::<Vec<_>>()
    );

    let options_with_memory = ServerRunOptions {
        max_memory: Some(2048),
        initial_memory: Some(1024),
    };
    let cmd_with_memory = command_factory(options_with_memory);
    println!(
        "  With Memory: {} {:?}",
        cmd_with_memory.get_program().to_string_lossy(),
        cmd_with_memory.get_args().collect::<Vec<_>>()
    );

    // Step 7: Verify downloaded files
    let server_jar = output_dir.join("server.jar");
    if server_jar.exists() {
        let metadata = std::fs::metadata(&server_jar)?;
        println!("\n✅ Server JAR verification:");
        println!("  📦 File: {:?}", server_jar);
        println!(
            "  📏 Size: {:.2} MB",
            metadata.len() as f64 / 1024.0 / 1024.0
        );
        println!("  🎯 Ready to run!");

        // Check if we can get java version
        let java_executable = cmd_basic.get_program();
        match std::process::Command::new(java_executable)
            .arg("-version")
            .output()
        {
            Ok(output) => {
                let version_info = String::from_utf8_lossy(&output.stderr);
                if let Some(first_line) = version_info.lines().next() {
                    println!("  ☕ Java: {}", first_line);
                }
            }
            Err(_) => {
                println!("  ⚠️  Could not verify Java installation");
            }
        }
    } else {
        println!("❌ Server JAR not found at expected location");
    }

    let total_time = start_time.elapsed();
    println!("\n🎉 Total time: {:.2} seconds", total_time.as_secs_f64());
    println!("📝 You can now run the server with the generated command from the output directory.");

    Ok(())
}
