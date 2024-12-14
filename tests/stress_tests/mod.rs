use futures::future::join_all;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Barrier;
use tracing::info;

use engine::engine::{
    core::{start_engine, Message},
    models::TradingPair,
    order_book::OrderBook,
};

pub const NUM_MARKET_MAKERS: usize = 5;
pub const NUM_TRADERS: usize = 20;
pub const NUM_PRICE_CHECKERS: usize = 10;
pub const ORDERS_PER_TRADER: usize = 1000;
pub const TEST_DURATION_SECS: u64 = 60;
pub const TRADING_PAIRS: &[&str] = &["BTC/USD", "ETH/USD", "SOL/USD"];

pub mod metrics;
pub mod simulator;

pub use metrics::TestMetrics;
pub use simulator::{simulate_market_maker, simulate_price_checker, simulate_trader};

pub async fn run_stress_test<F>(name: &str, order_book_factory: F) -> String
where
    F: Fn(TradingPair) -> Box<dyn OrderBook> + Send + Sync + 'static,
{
    info!("Starting {} stress test", name);

    let metrics = TestMetrics::new();
    let engine_tx = start_engine(order_book_factory);

    let num_tasks = NUM_MARKET_MAKERS + NUM_TRADERS + NUM_PRICE_CHECKERS;
    let barrier = Arc::new(Barrier::new(num_tasks * TRADING_PAIRS.len()));

    let start = Instant::now();
    let mut handles = Vec::new();

    for &pair_str in TRADING_PAIRS {
        let trading_pair = TradingPair::from_string(pair_str).unwrap();

        // Spawn market makers
        for i in 0..NUM_MARKET_MAKERS {
            info!("Spawning market maker {} for {}", i, pair_str);
            let handle = tokio::spawn(simulate_market_maker(
                engine_tx.clone(),
                barrier.clone(),
                metrics.clone(),
                trading_pair.clone(),
            ));
            handles.push(handle);
        }

        // Spawn traders
        for i in 0..NUM_TRADERS {
            info!("Spawning trader {} for {}", i, pair_str);
            let handle = tokio::spawn(simulate_trader(
                engine_tx.clone(),
                barrier.clone(),
                metrics.clone(),
                trading_pair.clone(),
            ));
            handles.push(handle);
        }

        // Spawn price checkers
        for i in 0..NUM_PRICE_CHECKERS {
            info!("Spawning price checker {} for {}", i, pair_str);
            let handle = tokio::spawn(simulate_price_checker(
                engine_tx.clone(),
                barrier.clone(),
                metrics.clone(),
                trading_pair.clone(),
            ));
            handles.push(handle);
        }
    }

    info!("All tasks spawned, waiting for completion");
    join_all(handles).await;
    let duration = start.elapsed();

    let report = metrics.report(duration);
    let _ = engine_tx.send(Message::Shutdown).await;

    report
}
