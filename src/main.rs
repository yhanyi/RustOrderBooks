mod engine;
use engine::{api::run_api_server, core::start_engine};
use tokio;

#[tokio::main]
async fn main() {
    let engine_tx = start_engine();
    let api_tx = engine_tx.clone();
    tokio::spawn(async move {
        run_api_server(api_tx).await;
    });
    tokio::signal::ctrl_c().await.unwrap();
    println!("Shutting down.");
}
