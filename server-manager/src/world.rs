use crate::types::WorldInfo;
use anyhow::{Context, Result};
use std::path::Path;
use tokio::fs;

pub async fn list_worlds(server_dir: &Path) -> Result<Vec<WorldInfo>> {
    let mut worlds = Vec::new();

    let mut entries = fs::read_dir(server_dir)
        .await
        .context("Failed to read server directory")?;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        
        if !path.is_dir() {
            continue;
        }

        // Check if it's a world directory (contains level.dat)
        let level_dat = path.join("level.dat");
        if !level_dat.exists() {
            continue;
        }

        let metadata = fs::metadata(&path).await?;
        let size = calculate_dir_size(&path).await?;
        
        worlds.push(WorldInfo {
            name: path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string(),
            size_mb: size / 1024 / 1024,
            last_modified: metadata.modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0),
        });
    }

    Ok(worlds)
}

pub async fn backup_world(server_dir: &Path, world_name: &str) -> Result<String> {
    let world_path = server_dir.join(world_name);
    
    if !world_path.exists() {
        anyhow::bail!("World '{}' not found", world_name);
    }

    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let backup_name = format!("{}_{}.zip", world_name, timestamp);
    let backup_path = server_dir.join("backups").join(&backup_name);

    fs::create_dir_all(backup_path.parent().unwrap()).await?;

    // Create zip archive
    let file = std::fs::File::create(&backup_path)
        .context("Failed to create backup file")?;
    
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::FileOptions::<()>::default()
        .compression_method(zip::CompressionMethod::Deflated);

    add_dir_to_zip(&mut zip, &world_path, world_name, options).await?;
    
    zip.finish().context("Failed to finalize zip")?;

    Ok(backup_name)
}

pub async fn delete_world(server_dir: &Path, world_name: &str) -> Result<()> {
    let world_path = server_dir.join(world_name);
    
    if !world_path.exists() {
        anyhow::bail!("World '{}' not found", world_name);
    }

    fs::remove_dir_all(&world_path)
        .await
        .context("Failed to delete world")?;

    Ok(())
}

pub fn upload_world(server_dir: &Path, world_name: &str, zip_data: Vec<u8>) -> Result<()> {
    let world_path = server_dir.join(world_name);
    if world_path.exists() {
        anyhow::bail!("World '{}' already exists", world_name);
    }

    std::fs::create_dir_all(&world_path).context("Failed to create world directory")?;

    let cursor = std::io::Cursor::new(zip_data);
    let mut archive = zip::ZipArchive::new(cursor).context("Failed to open zip archive")?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i).context("Failed to access file in zip")?;
        let outpath = match file.enclosed_name() {
            Some(path) => world_path.join(path),
            None => continue,
        };

        if (*file.name()).ends_with('/') {
            std::fs::create_dir_all(&outpath).context("Failed to create directory in zip")?;
        } else {
            if let Some(p) = outpath.parent() {
                if !p.exists() {
                    std::fs::create_dir_all(&p).context("Failed to create parent directory in zip")?;
                }
            }
            let mut outfile = std::fs::File::create(&outpath).context("Failed to create output file")?;
            std::io::copy(&mut file, &mut outfile).context("Failed to extract file")?;
        }
    }

    Ok(())
}

async fn calculate_dir_size(path: &Path) -> Result<u64> {
    let mut total = 0u64;
    let mut stack = vec![path.to_path_buf()];

    while let Some(current) = stack.pop() {
        let mut entries = fs::read_dir(&current).await?;
        
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let metadata = fs::metadata(&path).await?;
            
            if metadata.is_dir() {
                stack.push(path);
            } else {
                total += metadata.len();
            }
        }
    }

    Ok(total)
}

async fn add_dir_to_zip(
    zip: &mut zip::ZipWriter<std::fs::File>,
    dir: &Path,
    prefix: &str,
    options: zip::write::FileOptions<'_, ()>,
) -> Result<()> {
    let mut entries = fs::read_dir(dir).await?;
    
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        let name = path.file_name()
            .and_then(|n| n.to_str())
            .context("Invalid filename")?;
        
        let zip_path = format!("{}/{}", prefix, name);

        if path.is_dir() {
            zip.add_directory(&zip_path, options.clone())
                .context("Failed to add directory to zip")?;
            Box::pin(add_dir_to_zip(zip, &path, &zip_path, options.clone())).await?;
        } else {
            zip.start_file(&zip_path, options.clone())
                .context("Failed to start file in zip")?;
            
            let content = std::fs::read(&path)
                .context("Failed to read file")?;
            
            std::io::Write::write_all(zip, &content)
                .context("Failed to write file to zip")?;
        }
    }

    Ok(())
}
