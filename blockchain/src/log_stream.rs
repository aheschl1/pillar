use axum::{extract::{ws::{Message}, WebSocketUpgrade}, response::IntoResponse};
use tokio::sync::broadcast;
use tracing_subscriber::fmt::MakeWriter;
use std::io;

static LOG_CHANNEL: once_cell::sync::Lazy<broadcast::Sender<String>> = once_cell::sync::Lazy::new(|| {
    let (tx, _) = broadcast::channel(1000);
    tx
});

pub fn get_log_sender() -> broadcast::Sender<String> {
    LOG_CHANNEL.clone()
}

pub(crate) struct BroadcastWriter;

impl io::Write for BroadcastWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if let Ok(msg) = std::str::from_utf8(buf) {
            let _ = get_log_sender().send(msg.to_string());
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl MakeWriter<'_> for BroadcastWriter {
    type Writer = BroadcastWriter;

    fn make_writer(&self) -> Self::Writer {
        BroadcastWriter
    }
}



pub(crate) async fn ws_logs(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(|mut socket| async move {
        let mut rx = get_log_sender().subscribe();

        while let Ok(line) = rx.recv().await {
            if socket.send(Message::Text(line.into())).await.is_err() {
                break;
            }
        }
    })
}
