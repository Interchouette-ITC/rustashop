//! Cart WebSocket session read loop (Actix upgrade body).

use actix_ws::Message;

/// Drains client frames and forwards hub payloads until the socket closes.
#[allow(clippy::future_not_send)]
pub(crate) async fn run_cart_ws_session(
    mut session: actix_ws::Session,
    mut msg_stream: actix_ws::MessageStream,
    mut events: tokio::sync::broadcast::Receiver<String>,
) {
    loop {
        tokio::select! {
            msg = msg_stream.recv() => {
                match msg {
                    Some(Ok(Message::Ping(bytes))) => {
                        if session.pong(&bytes).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Close(_)) | Err(_)) | None => break,
                    Some(Ok(_)) => {}
                }
            }
            event = events.recv() => {
                match event {
                    Ok(payload) => {
                        if session.text(payload).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                }
            }
        }
    }
    let _ = session.close(None).await;
}
