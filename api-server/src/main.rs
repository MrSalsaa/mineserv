mod auth;
mod db;
mod routes;
mod state;

use anyhow::{Context, Result};
use axum::{
    extract::DefaultBodyLimit,
    routing::{delete, get, post, put},
    Router,
};
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::state::AppState;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "api_server=debug,server_manager=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Load environment variables
    dotenvy::dotenv().ok();

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "sqlite://mineserv.db".to_string());
    let api_host = std::env::var("API_HOST")
        .unwrap_or_else(|_| "127.0.0.1".to_string());
    let api_port = std::env::var("API_PORT")
        .unwrap_or_else(|_| "8080".to_string())
        .parse::<u16>()
        .context("Invalid API_PORT")?;
    let servers_dir = std::env::var("SERVERS_DIR")
        .unwrap_or_else(|_| "./servers".to_string());
    let admin_password = std::env::var("ADMIN_PASSWORD")
        .unwrap_or_else(|_| "changeme".to_string());
    let jwt_secret = std::env::var("JWT_SECRET")
        .unwrap_or_else(|_| "your-secret-key-change-this-in-production".to_string());

    // Initialize database
    let db = db::init_db(&database_url).await?;

    // Create servers directory
    let servers_path = std::env::current_dir()?.join(servers_dir);
    tokio::fs::create_dir_all(&servers_path).await?;
    let servers_path = servers_path.canonicalize().context("Failed to canonicalize servers path")?;

    // Create application state
    let state = Arc::new(AppState::new(
        db,
        servers_path,
        admin_password,
        jwt_secret,
    ));

    // Recover existing processes
    let state_clone = state.clone();
    tokio::spawn(async move {
        if let Err(e) = state_clone.recover_processes().await {
            tracing::error!("Failed to recover processes: {}", e);
        }
    });

    // Build router
    let app = Router::new()
        // Auth routes (no auth required)
        .route("/api/auth/login", post(auth::login))
        // Server routes
        .route("/api/servers", get(routes::servers::list_servers))
        .route("/api/servers", post(routes::servers::create_server))
        .route("/api/servers/:id", get(routes::servers::get_server))
        .route("/api/servers/:id", delete(routes::servers::delete_server))
        .route("/api/servers/:id/start", post(routes::servers::start_server))
        .route("/api/servers/:id/stop", post(routes::servers::stop_server))
        .route("/api/servers/:id/force-stop", post(routes::servers::force_stop_server))
        .route("/api/servers/:id/restart", post(routes::servers::restart_server))
        .route("/api/versions/:type", get(routes::servers::get_versions))
        // Console routes
        .route("/api/servers/:id/console", get(routes::console::console_handler))
        // Config routes
        .route("/api/servers/:id/config", get(routes::config::get_config))
        .route("/api/servers/:id/config", put(routes::config::update_config))
        .route("/api/servers/:id/worlds", get(routes::config::list_worlds))
        .route("/api/servers/:id/worlds/backup", post(routes::config::backup_world))
        .route(
            "/api/servers/:id/worlds/upload",
            post(routes::config::upload_world)
                .layer(DefaultBodyLimit::max(1024 * 1024 * 1024)), // 1GB limit
        )
        .route("/api/servers/:id/worlds/:name", delete(routes::config::delete_world))
        .route("/api/servers/:id/worlds/:name/default", post(routes::config::set_default_world))
        // Plugin routes
        .route("/api/plugins/search", get(routes::plugins::search_plugins))
        .route("/api/servers/:id/plugins", get(routes::plugins::list_installed_plugins))
        .route("/api/servers/:id/plugins", post(routes::plugins::install_plugin))
        .route("/api/servers/:id/plugins/:name", delete(routes::plugins::remove_plugin))
        // Stats routes
        .route("/api/servers/:id/stats", get(routes::stats::get_server_stats))
        .route("/api/stats", get(routes::stats::get_system_stats))
        // File routes
        .route("/api/servers/:id/files", get(routes::files::list_files))
        .route("/api/servers/:id/files/*path", get(routes::files::read_file))
        .route("/api/servers/:id/files/*path", put(routes::files::write_file))
        .fallback_service(tower_http::services::ServeDir::new("frontend").fallback(tower_http::services::ServeFile::new("frontend/index.html")))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .with_state(state);

    // Start server
    let addr = format!("{}:{}", api_host, api_port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    
    tracing::info!("API server listening on {}", addr);
    
    axum::serve(listener, app).await?;

    Ok(())
}
