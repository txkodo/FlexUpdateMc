use ssmc_core::domain::McVanillaVersionId;
use ssmc_core::infra::file_bundle_loader::DefaultFileBundleLoader;
use ssmc_core::infra::mc_java::{DefaultMcJavaLoader, McJavaLoader};
use ssmc_core::infra::url_fetcher::DefaultUrlFetcher;
use std::error::Error;
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Create HTTP URL fetcher for real API calls
    let url_fetcher = Box::new(DefaultUrlFetcher);
    let loader = DefaultMcJavaLoader::new(
        url_fetcher,
        Box::new(DefaultFileBundleLoader::new(
            Box::new(ssmc_core::infra::fs_handler::DefaultFsHandler::new()),
            Box::new(DefaultUrlFetcher),
        )),
        std::path::PathBuf::from("temp_workspace/java-cache")
    );

    println!("Fetching Java runtime list from Mojang API...");

    // List available runtimes for current platform
    match loader.list_runtimes().await {
        Ok(runtimes) => {
            println!(
                "Found {} Java runtimes for current platform.",
                runtimes.len()
            );
            for runtime in &runtimes {
                println!(
                    "  - {}: Java {}",
                    runtime.version_id(),
                    runtime.major_version()
                );
            }
        }
        Err(e) => {
            println!("Failed to fetch runtimes: {}", e);
        }
    }

    println!("\nInstalling Java runtime...");
    let start_time = Instant::now();
    
    let java_path = loader
        .ready_runtime(&McVanillaVersionId::new("java-runtime-alpha".to_string()))
        .await?;

    let elapsed = start_time.elapsed();
    println!(
        "Java runtime installation completed in {:.2} seconds",
        elapsed.as_secs_f64()
    );
    println!("Java executable path: {:?}", java_path);

    Ok(())
}
