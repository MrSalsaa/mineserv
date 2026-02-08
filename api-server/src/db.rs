use anyhow::{Context, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool};
use sqlx::Row;
use server_manager::{ServerConfig, ServerType};
use uuid::Uuid;
use std::str::FromStr;

pub async fn init_db(database_url: &str) -> Result<SqlitePool> {
    let connection_options = SqliteConnectOptions::from_str(database_url)
        .context("Invalid database URL")?
        .create_if_missing(true);

    let pool = SqlitePool::connect_with(connection_options)
        .await
        .context("Failed to connect to database")?;

    // Run migrations
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS servers (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            server_type TEXT NOT NULL,
            minecraft_version TEXT NOT NULL,
            port INTEGER NOT NULL,
            max_players INTEGER NOT NULL,
            memory_mb INTEGER NOT NULL,
            auto_start INTEGER NOT NULL,
            properties TEXT NOT NULL,
            created_at INTEGER NOT NULL
        )
        "#,
    )
    .execute(&pool)
    .await
    .context("Failed to create servers table")?;

    Ok(pool)
}

pub async fn create_server(pool: &SqlitePool, config: &ServerConfig) -> Result<()> {
    let properties_json = serde_json::to_string(&config.properties)?;
    let server_type_str = match config.server_type {
        ServerType::Paper => "paper",
        ServerType::Spigot => "spigot",
    };

    sqlx::query(
        r#"
        INSERT INTO servers (id, name, server_type, minecraft_version, port, max_players, memory_mb, auto_start, properties, created_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(config.id.to_string())
    .bind(&config.name)
    .bind(server_type_str)
    .bind(&config.minecraft_version)
    .bind(config.port as i64)
    .bind(config.max_players as i64)
    .bind(config.memory_mb as i64)
    .bind(config.auto_start as i64)
    .bind(properties_json)
    .bind(chrono::Utc::now().timestamp())
    .execute(pool)
    .await
    .context("Failed to insert server")?;

    Ok(())
}

pub async fn get_server(pool: &SqlitePool, id: Uuid) -> Result<Option<ServerConfig>> {
    let row = sqlx::query(
        r#"
        SELECT id, name, server_type, minecraft_version, port, max_players, memory_mb, auto_start, properties
        FROM servers
        WHERE id = ?
        "#,
    )
    .bind(id.to_string())
    .fetch_optional(pool)
    .await?;

    if let Some(row) = row {
        let server_type = match row.get::<String, _>("server_type").as_str() {
            "paper" => ServerType::Paper,
            "spigot" => ServerType::Spigot,
            _ => ServerType::Paper,
        };

        let properties: std::collections::HashMap<String, String> =
            serde_json::from_str(row.get("properties"))?;

        Ok(Some(ServerConfig {
            id: Uuid::parse_str(row.get("id"))?,
            name: row.get("name"),
            server_type,
            minecraft_version: row.get("minecraft_version"),
            port: row.get::<i64, _>("port") as u16,
            max_players: row.get::<i64, _>("max_players") as u32,
            memory_mb: row.get::<i64, _>("memory_mb") as u32,
            auto_start: row.get::<i64, _>("auto_start") != 0,
            properties,
        }))
    } else {
        Ok(None)
    }
}

pub async fn list_servers(pool: &SqlitePool) -> Result<Vec<ServerConfig>> {
    let rows = sqlx::query(
        r#"
        SELECT id, name, server_type, minecraft_version, port, max_players, memory_mb, auto_start, properties
        FROM servers
        ORDER BY created_at DESC
        "#,
    )
    .fetch_all(pool)
    .await?;

    let mut servers = Vec::new();

    for row in rows {
        let server_type = match row.get::<String, _>("server_type").as_str() {
            "paper" => ServerType::Paper,
            "spigot" => ServerType::Spigot,
            _ => ServerType::Paper,
        };

        let properties: std::collections::HashMap<String, String> =
            serde_json::from_str(row.get("properties"))?;

        servers.push(ServerConfig {
            id: Uuid::parse_str(row.get("id"))?,
            name: row.get("name"),
            server_type,
            minecraft_version: row.get("minecraft_version"),
            port: row.get::<i64, _>("port") as u16,
            max_players: row.get::<i64, _>("max_players") as u32,
            memory_mb: row.get::<i64, _>("memory_mb") as u32,
            auto_start: row.get::<i64, _>("auto_start") != 0,
            properties,
        });
    }

    Ok(servers)
}

pub async fn update_server(pool: &SqlitePool, config: &ServerConfig) -> Result<()> {
    let properties_json = serde_json::to_string(&config.properties)?;
    let server_type_str = match config.server_type {
        ServerType::Paper => "paper",
        ServerType::Spigot => "spigot",
    };

    sqlx::query(
        r#"
        UPDATE servers
        SET name = ?, server_type = ?, minecraft_version = ?, port = ?, max_players = ?, memory_mb = ?, auto_start = ?, properties = ?
        WHERE id = ?
        "#,
    )
    .bind(&config.name)
    .bind(server_type_str)
    .bind(&config.minecraft_version)
    .bind(config.port as i64)
    .bind(config.max_players as i64)
    .bind(config.memory_mb as i64)
    .bind(config.auto_start as i64)
    .bind(properties_json)
    .bind(config.id.to_string())
    .execute(pool)
    .await
    .context("Failed to update server")?;

    Ok(())
}

pub async fn delete_server(pool: &SqlitePool, id: Uuid) -> Result<()> {
    sqlx::query("DELETE FROM servers WHERE id = ?")
        .bind(id.to_string())
        .execute(pool)
        .await
        .context("Failed to delete server")?;

    Ok(())
}
