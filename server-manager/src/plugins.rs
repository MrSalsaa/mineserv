use crate::types::{PluginInfo, ServerType};
use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;
use tokio::fs;

const MODRINTH_API_BASE: &str = "https://api.modrinth.com/v2";

#[derive(Debug, Deserialize)]
struct ModrinthSearchResponse {
    hits: Vec<ModrinthProject>,
}

#[derive(Debug, Deserialize)]
struct ModrinthProject {
    title: String,
    description: String,
    author: String,
    project_id: String,
}

#[derive(Debug, Deserialize)]
struct ModrinthVersion {
    #[serde(rename = "version_number")]
    _version_number: String,
    game_versions: Vec<String>,
    loaders: Vec<String>,
    files: Vec<ModrinthFile>,
}

#[derive(Debug, Deserialize)]
struct ModrinthFile {
    url: String,
    filename: String,
}

fn get_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent("mineserv-manager (https://github.com/vpastila/mineserv)")
        .build()
        .unwrap_or_default()
}

pub async fn search_plugins(query: &str, server_type: &str) -> Result<Vec<PluginInfo>> {
    let facets = match server_type.to_lowercase().as_str() {
        "paper" => r#"[["categories:paper"]]"#,
        "spigot" => r#"[["categories:spigot"]]"#,
        _ => r#"[["project_type:plugin"]]"#,
    };

    let url = format!(
        "{}/search?query={}&facets={}&limit=20",
        MODRINTH_API_BASE,
        urlencoding::encode(query),
        urlencoding::encode(facets)
    );

    let client = get_client();
    let response = client.get(&url)
        .send()
        .await
        .context("Failed to search plugins")?;

    let search_result: ModrinthSearchResponse = response
        .json()
        .await
        .context("Failed to parse search results")?;

    let plugins = search_result
        .hits
        .into_iter()
        .map(|project| PluginInfo {
            name: project.title,
            version: String::new(), // Will be filled when installing
            description: Some(project.description),
            author: Some(project.author),
            installed: false,
        })
        .collect();

    Ok(plugins)
}

pub async fn install_plugin(
    server_dir: &Path,
    plugin_name: &str,
    minecraft_version: &str,
    server_type: ServerType,
) -> Result<()> {
    tracing::info!("Installing plugin: {}", plugin_name);

    // Search for the plugin
    let url = format!(
        "{}/search?query={}&limit=1",
        MODRINTH_API_BASE,
        urlencoding::encode(plugin_name)
    );

    let client = get_client();
    let response = client.get(&url)
        .send()
        .await
        .context("Failed to search for plugin")?;

    let search_result: ModrinthSearchResponse = response
        .json()
        .await
        .context("Failed to parse search results")?;

    let project = search_result
        .hits
        .first()
        .context("Plugin not found")?;

    // Get versions for this project
    let versions_url = format!(
        "{}/project/{}/version",
        MODRINTH_API_BASE,
        project.project_id
    );

    let client = get_client();
    let response = client.get(&versions_url)
        .send()
        .await
        .context("Failed to fetch plugin versions")?;

    let all_versions: Vec<ModrinthVersion> = response
        .json()
        .await
        .context("Failed to parse versions")?;

    // Find a version that matches the game version and loader
    let loader = match server_type {
        crate::types::ServerType::Paper => "paper",
        crate::types::ServerType::Spigot => "spigot",
    };

    let version = all_versions.iter().find(|v| {
        let mc_match = v.game_versions.iter().any(|gv| gv == minecraft_version);
        let loader_match = v.loaders.iter().any(|l| l.to_lowercase() == loader);
        mc_match && loader_match
    }).or_else(|| {
        // Fallback: Latest version that supports the loader
        all_versions.iter().find(|v| {
            v.loaders.iter().any(|l| l.to_lowercase() == loader)
        })
    }).context("No compatible version found for this plugin and server type")?;

    let file = version
        .files
        .first()
        .context("No files available for this version")?;

    // Download the plugin
    let plugins_dir = server_dir.join("plugins");
    fs::create_dir_all(&plugins_dir).await?;

    let plugin_path = plugins_dir.join(&file.filename);

    let client = get_client();
    let response = client.get(&file.url)
        .send()
        .await
        .context("Failed to download plugin")?;

    let bytes = response
        .bytes()
        .await
        .context("Failed to read plugin bytes")?;

    fs::write(&plugin_path, &bytes)
        .await
        .context("Failed to write plugin file")?;

    tracing::info!("Successfully installed plugin: {}", plugin_name);
    Ok(())
}


pub async fn list_installed_plugins(server_dir: &Path) -> Result<Vec<PluginInfo>> {
    let plugins_dir = server_dir.join("plugins");
    
    if !plugins_dir.exists() {
        return Ok(Vec::new());
    }

    let mut plugins = Vec::new();
    let mut entries = fs::read_dir(&plugins_dir).await?;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        
        if path.extension().and_then(|s| s.to_str()) != Some("jar") {
            continue;
        }

        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        plugins.push(PluginInfo {
            name,
            version: String::from("unknown"),
            description: None,
            author: None,
            installed: true,
        });
    }

    Ok(plugins)
}

pub async fn remove_plugin(server_dir: &Path, plugin_name: &str) -> Result<()> {
    let plugins_dir = server_dir.join("plugins");
    let plugin_path = plugins_dir.join(format!("{}.jar", plugin_name));

    if !plugin_path.exists() {
        anyhow::bail!("Plugin '{}' not found", plugin_name);
    }

    fs::remove_file(&plugin_path)
        .await
        .context("Failed to remove plugin")?;

    Ok(())
}
