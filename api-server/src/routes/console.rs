use axum::{
    extract::{
        ws::WebSocket,
        Path, State, WebSocketUpgrade,
    },
    response::Response,
};
use futures::{SinkExt, StreamExt};
use axum::extract::ws as ax_ws;
use std::sync::Arc;
use uuid::Uuid;

use crate::{db, routes::servers::ServerError, state::AppState};

pub async fn console_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Response, ServerError> {
    // Verify server exists
    db::get_server(&state.db, id)
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?
        .ok_or(ServerError::NotFound)?;

    Ok(ws.on_upgrade(move |socket| handle_console_socket(socket, state, id)))
}

async fn handle_console_socket(socket: WebSocket, state: Arc<AppState>, server_id: Uuid) {
    let (mut sender, mut receiver) = socket.split();

    // Get the broadcast receiver
    let mut rx = {
        let processes = state.processes.read().await;
        if let Some(process) = processes.get(&server_id) {
            process.subscribe()
        } else {
            let _ = sender
                .send(ax_ws::Message::Text("Server is not running".to_string()))
                .await;
            return;
        }
    };

    // Task to pipe console output to WebSocket
    let mut send_task = tokio::spawn(async move {
        while let Ok(line) = rx.recv().await {
            if let Err(_) = sender.send(ax_ws::Message::Text(line)).await {
                break;
            }
        }
    });

    // Task to pipe WebSocket messages to server stdin
    let state_clone = state.clone();
    let mut receive_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            if let ax_ws::Message::Text(text) = msg {
                let processes = state_clone.processes.read().await;
                if let Some(process) = processes.get(&server_id) {
                    let _ = process.send_command(text).await;
                }
            }
        }
    });

    // Wait for either task to finish
    tokio::select! {
        _ = (&mut send_task) => receive_task.abort(),
        _ = (&mut receive_task) => send_task.abort(),
    };
}
