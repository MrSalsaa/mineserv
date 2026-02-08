use crate::types::ServerType;
use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;
use tokio::fs;
use tokio::io::AsyncWriteExt;

const PAPER_API_BASE: &str = "https://api.papermc.io/v2";
const SPIGOT_BUILDTOOLS_URL: &str = "https://hub.spigotmc.org/jenkins/job/BuildTools/lastSuccessfulBuild/artifact/target/BuildTools.jar";

#[derive(Debug, Deserialize)]
struct PaperVersions {
    versions: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct PaperBuilds {
    builds: Vec<u32>,
}

#[derive(Debug, Deserialize)]
struct PaperBuildInfo {
    downloads: PaperDownloads,
}

#[derive(Debug, Deserialize)]
struct PaperDownloads {
    application: PaperApplication,
}

#[derive(Debug, Deserialize)]
struct PaperApplication {
    name: String,
}

pub async fn get_available_versions(server_type: ServerType) -> Result<Vec<String>> {
    match server_type {
        ServerType::Paper => {
            let url = format!("{}/projects/paper", PAPER_API_BASE);
            let response = reqwest::get(&url)
                .await
                .context("Failed to fetch Paper versions")?;
            
            let versions: PaperVersions = response
                .json()
                .await
                .context("Failed to parse Paper versions")?;
            
            Ok(versions.versions)
        }
        ServerType::Spigot => {
            // For Spigot, we'll support common versions
            // BuildTools can build any version, but we'll list popular ones
            Ok(vec![
                "1.21.1".to_string(),
                "1.21".to_string(),
                "1.20.6".to_string(),
                "1.20.4".to_string(),
                "1.20.1".to_string(),
                "1.19.4".to_string(),
            ])
        }
    }
}

pub async fn download_server_jar(
    server_type: ServerType,
    version: &str,
    destination: &Path,
) -> Result<()> {
    match server_type {
        ServerType::Paper => download_paper(version, destination).await,
        ServerType::Spigot => download_spigot(version, destination).await,
    }
}

async fn download_paper(version: &str, destination: &Path) -> Result<()> {
    tracing::info!("Downloading Paper {} to {:?}", version, destination);

    // Get latest build for version
    let builds_url = format!("{}/projects/paper/versions/{}", PAPER_API_BASE, version);
    let response = reqwest::get(&builds_url)
        .await
        .context("Failed to fetch Paper builds")?;
    
    let builds: PaperBuilds = response
        .json()
        .await
        .context("Failed to parse Paper builds")?;
    
    let latest_build = builds.builds.last()
        .context("No builds available for this version")?;

    // Get build info
    let build_url = format!(
        "{}/projects/paper/versions/{}/builds/{}",
        PAPER_API_BASE, version, latest_build
    );
    let response = reqwest::get(&build_url)
        .await
        .context("Failed to fetch build info")?;
    
    let build_info: PaperBuildInfo = response
        .json()
        .await
        .context("Failed to parse build info")?;

    // Download JAR
    let jar_url = format!(
        "{}/projects/paper/versions/{}/builds/{}/downloads/{}",
        PAPER_API_BASE, version, latest_build, build_info.downloads.application.name
    );

    download_file(&jar_url, destination).await?;
    
    tracing::info!("Successfully downloaded Paper {}", version);
    Ok(())
}

async fn download_spigot(version: &str, destination: &Path) -> Result<()> {
    tracing::info!("Downloading Spigot {} to {:?}", version, destination);
    
    // For Spigot, we need to download BuildTools and run it
    // This is a simplified version - in production, you'd want to cache BuildTools
    let build_dir = destination.parent()
        .context("Invalid destination path")?
        .join("build");
    
    fs::create_dir_all(&build_dir).await?;
    
    let buildtools_path = build_dir.join("BuildTools.jar");
    
    // Download BuildTools if not exists
    if !buildtools_path.exists() {
        tracing::info!("Downloading Spigot BuildTools...");
        download_file(SPIGOT_BUILDTOOLS_URL, &buildtools_path).await?;
    }

    // Run BuildTools
    tracing::info!("Building Spigot {} (this may take a while)...", version);
    let output = tokio::process::Command::new("java")
        .arg("-jar")
        .arg(&buildtools_path)
        .arg("--rev")
        .arg(version)
        .current_dir(&build_dir)
        .output()
        .await
        .context("Failed to run BuildTools")?;

    if !output.status.success() {
        anyhow::bail!(
            "BuildTools failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // Find the built JAR and move it to destination
    let spigot_jar = build_dir.join(format!("spigot-{}.jar", version));
    if !spigot_jar.exists() {
        anyhow::bail!("Built JAR not found at {:?}", spigot_jar);
    }

    fs::copy(&spigot_jar, destination).await?;
    
    tracing::info!("Successfully built Spigot {}", version);
    Ok(())
}

async fn download_file(url: &str, destination: &Path) -> Result<()> {
    let response = reqwest::get(url)
        .await
        .context("Failed to download file")?;

    if !response.status().is_success() {
        anyhow::bail!("Download failed with status: {}", response.status());
    }

    let bytes = response.bytes()
        .await
        .context("Failed to read response bytes")?;

    let mut file = fs::File::create(destination)
        .await
        .context("Failed to create destination file")?;

    file.write_all(&bytes)
        .await
        .context("Failed to write file")?;

    Ok(())
}
