use axum::Router;
use engine::engine::api::AppState;
use engine::engine::core::Message;
use tokio::sync::mpsc;

pub fn create_test_channel() -> mpsc::Sender<Message> {
    let (tx, mut rx) = mpsc::channel(100);
    tokio::spawn(async move { while let Some(_msg) = rx.recv().await {} });
    tx
}

pub fn create_test_app(state: AppState) -> Router {
    engine::engine::api::create_test_app(state)
}
