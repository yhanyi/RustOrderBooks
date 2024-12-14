use futures::future::join_all;
use rand::Rng;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Barrier;
use tokio::time::sleep;
use tracing::info;

use engine::engine::{
    core::{start_engine, Message},
    models::{Order, OrderType, TradingPair},
};

// Configuration for the stress test
const NUM_MARKET_MAKERS: usize = 5;
const NUM_TRADERS: usize = 20;
const NUM_PRICE_CHECKERS: usize = 10;
const ORDERS_PER_TRADER: usize = 1000;
const TEST_DURATION_SECS: u64 = 60;
const TRADING_PAIRS: &[&str] = &["BTC/USD", "ETH/USD", "SOL/USD"];

#[derive(Clone)]
struct TestMetrics {
    orders_processed: Arc<std::sync::atomic::AtomicUsize>,
    trades_executed: Arc<std::sync::atomic::AtomicUsize>,
    price_checks: Arc<std::sync::atomic::AtomicUsize>,
    total_latency: Arc<std::sync::atomic::AtomicU64>,
    max_latency: Arc<std::sync::atomic::AtomicU64>,
}

impl TestMetrics {
    fn new() -> Self {
        Self {
            orders_processed: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            trades_executed: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            price_checks: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            total_latency: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            max_latency: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    fn record_latency(&self, latency: Duration) {
        self.total_latency.fetch_add(
            latency.as_micros() as u64,
            std::sync::atomic::Ordering::Relaxed,
        );

        let mut current_max = self.max_latency.load(std::sync::atomic::Ordering::Relaxed);
        loop {
            if latency.as_micros() as u64 <= current_max {
                break;
            }
            match self.max_latency.compare_exchange(
                current_max,
                latency.as_micros() as u64,
                std::sync::atomic::Ordering::Relaxed,
                std::sync::atomic::Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => current_max = actual,
            }
        }
    }

    fn report(&self, duration: Duration) {
        let orders = self
            .orders_processed
            .load(std::sync::atomic::Ordering::Relaxed);
        let trades = self
            .trades_executed
            .load(std::sync::atomic::Ordering::Relaxed);
        let prices = self.price_checks.load(std::sync::atomic::Ordering::Relaxed);
        let total_latency = self
            .total_latency
            .load(std::sync::atomic::Ordering::Relaxed);
        let max_latency = self.max_latency.load(std::sync::atomic::Ordering::Relaxed);

        let avg_latency = if orders > 0 {
            total_latency as f64 / orders as f64
        } else {
            0.0
        };

        info!("=== Stress Test Results ===");
        info!("Duration: {:?}", duration);
        info!("Total orders processed: {}", orders);
        info!("Total trades executed: {}", trades);
        info!("Total price checks: {}", prices);
        info!(
            "Orders/second: {:.2}",
            orders as f64 / duration.as_secs_f64()
        );
        info!(
            "Trades/second: {:.2}",
            trades as f64 / duration.as_secs_f64()
        );
        info!(
            "Price checks/second: {:.2}",
            prices as f64 / duration.as_secs_f64()
        );
        info!("Average latency: {:.2} μs", avg_latency);
        info!("Maximum latency: {} μs", max_latency);
        info!(
            "Trade/Order ratio: {:.2}%",
            (trades as f64 / orders as f64) * 100.0
        );
    }
}

async fn simulate_market_maker(
    engine_tx: tokio::sync::mpsc::Sender<Message>,
    barrier: Arc<Barrier>,
    metrics: TestMetrics,
    trading_pair: TradingPair,
) {
    barrier.wait().await;
    let mut order_id = 0;
    let start = Instant::now();

    while start.elapsed() < Duration::from_secs(TEST_DURATION_SECS) {
        // Generate all random values before any await points
        let mid_price = 50000.0 + rand::thread_rng().gen_range(-1000.0..1000.0);
        let spread = rand::thread_rng().gen_range(0.1..2.0);
        let buy_quantity = rand::thread_rng().gen_range(0.1..1.0);
        let sell_quantity = rand::thread_rng().gen_range(0.1..1.0);
        let sleep_duration = rand::thread_rng().gen_range(10..50);

        let buy_order = Order {
            id: order_id,
            trading_pair: trading_pair.clone(),
            order_type: OrderType::Buy,
            price: mid_price - spread,
            quantity: buy_quantity,
            timestamp: chrono::Utc::now(),
        };

        let sell_order = Order {
            id: order_id + 1,
            trading_pair: trading_pair.clone(),
            order_type: OrderType::Sell,
            price: mid_price + spread,
            quantity: sell_quantity,
            timestamp: chrono::Utc::now(),
        };

        let start_time = Instant::now();

        // Send the orders
        let _ = engine_tx.send(Message::NewOrder(buy_order)).await;
        let _ = engine_tx.send(Message::NewOrder(sell_order)).await;

        metrics.record_latency(start_time.elapsed());
        metrics
            .orders_processed
            .fetch_add(2, std::sync::atomic::Ordering::Relaxed);

        order_id += 2;
        sleep(Duration::from_millis(sleep_duration)).await;
    }
}

async fn simulate_trader(
    engine_tx: tokio::sync::mpsc::Sender<Message>,
    barrier: Arc<Barrier>,
    metrics: TestMetrics,
    trading_pair: TradingPair,
) {
    barrier.wait().await;
    let mut order_id = 0;

    for _ in 0..ORDERS_PER_TRADER {
        // Generate all random values before any await points
        let base_price = 50000.0;
        let price_offset = rand::thread_rng().gen_range(-5000.0..5000.0);
        let quantity = rand::thread_rng().gen_range(0.1..2.0);
        let is_buy = rand::thread_rng().gen_bool(0.5);
        let sleep_duration = rand::thread_rng().gen_range(50..200);

        let order_type = if is_buy {
            OrderType::Buy
        } else {
            OrderType::Sell
        };

        let order = Order {
            id: order_id,
            trading_pair: trading_pair.clone(),
            order_type,
            price: base_price + price_offset,
            quantity,
            timestamp: chrono::Utc::now(),
        };

        let start_time = Instant::now();
        let _ = engine_tx.send(Message::NewOrder(order)).await;
        metrics.record_latency(start_time.elapsed());
        metrics
            .orders_processed
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        order_id += 1;
        sleep(Duration::from_millis(sleep_duration)).await;
    }
}

async fn simulate_price_checker(
    engine_tx: tokio::sync::mpsc::Sender<Message>,
    barrier: Arc<Barrier>,
    metrics: TestMetrics,
    trading_pair: TradingPair,
) {
    barrier.wait().await;
    let start = Instant::now();

    while start.elapsed() < Duration::from_secs(TEST_DURATION_SECS) {
        // Generate sleep duration before await points
        let sleep_duration = rand::thread_rng().gen_range(100..500);

        let (price_tx, mut price_rx) = tokio::sync::mpsc::channel(1);

        let start_time = Instant::now();
        let _ = engine_tx
            .send(Message::GetPrice(trading_pair.clone(), price_tx))
            .await;

        let _ = price_rx.recv().await;
        metrics.record_latency(start_time.elapsed());
        metrics
            .price_checks
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        sleep(Duration::from_millis(sleep_duration)).await;
    }
}

#[tokio::test]
async fn advanced_concurrent_stress_test() {
    // Initialize tracing for better logging
    let _ = tracing_subscriber::fmt()
        .with_thread_ids(true)
        .with_target(false)
        .with_line_number(true)
        .with_file(true)
        .with_max_level(tracing::Level::INFO)
        .try_init();

    info!("Starting advanced concurrent stress test");

    let metrics = TestMetrics::new();
    let engine_tx = start_engine();

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

    metrics.report(duration);

    // Send shutdown signal to engine
    let _ = engine_tx.send(Message::Shutdown).await;
}
