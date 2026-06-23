use crate::api::auth::UserSession;
use crate::api::event_bus::init_broadcaster;
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Extension,
    },
    response::IntoResponse,
};

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    Extension(session): Extension<UserSession>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, session))
}

async fn handle_socket(mut socket: WebSocket, session: UserSession) {
    println!("[WebSocket] Operator {} connected", session.username);
    let mut rx = init_broadcaster();

    loop {
        tokio::select! {
            // Receive broadcasted events from the global event bus
            res = rx.recv() => {
                match res {
                    Ok(msg) => {
                        if socket.send(Message::Text(msg)).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            // Detect client disconnect or closing
            client_msg = socket.recv() => {
                match client_msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
        }
    }
    println!("[WebSocket] Operator {} disconnected", session.username);
}
