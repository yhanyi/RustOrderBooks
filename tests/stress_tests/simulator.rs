use super::TestMetrics;
use crate::stress_tests::{ORDERS_PER_TRADER, TEST_DURATION_SECS};
use engine::engine::core::Message;
use engine::engine::models::{Order, OrderType, TradingPair};
use rand::Rng;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Barrier;
use tokio::time::sleep;

pub async fn simulate_market_maker(
    engine_tx: tokio::sync::mpsc::Sender<Message>,
    barrier: Arc<Barrier>,
    metrics: TestMetrics,
    trading_pair: TradingPair,
) {
    barrier.wait().await;
    let mut order_id = 0;
    let start = Instant::now();

    while start.elapsed() < Duration::from_secs(TEST_DURATION_SECS) {
        let mid_price = 50000.0;
        let spread = rand::thread_rng().gen_range(0.1..0.5);
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

pub async fn simulate_trader(
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
        let price_offset = rand::thread_rng().gen_range(-0.5..0.5);
        let quantity = rand::thread_rng().gen_range(0.1..2.0);
        let is_buy = rand::thread_rng().gen_bool(0.5);
        let sleep_duration = rand::thread_rng().gen_range(10..50);

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

pub async fn simulate_price_checker(
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
