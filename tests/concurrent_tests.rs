use engine::engine::concurrent::ConcurrentOrderBook;
use engine::engine::models::{Order, OrderType, TradingPair};
use engine::engine::order_book::OrderBook;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::sync::Arc;
use std::time::Instant;
use tokio::task;

#[tokio::test]
async fn stress_test_concurrent_order_book() {
    let (order_book, mut trade_rx) =
        ConcurrentOrderBook::new(TradingPair::new("BTC".to_string(), "USD".to_string()));
    let order_book = Arc::new(order_book);

    // Spawn trade receiver
    let trade_counter = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let trade_counter_clone = trade_counter.clone();

    task::spawn(async move {
        while let Some(_trade) = trade_rx.recv().await {
            trade_counter_clone.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    });

    const NUM_ORDERS: usize = 100_000;
    const NUM_CONCURRENT_TASKS: usize = 10;
    const PRICE_RANGE: (f64, f64) = (45000.0, 55000.0);
    const QUANTITY_RANGE: (f64, f64) = (0.1, 2.0);

    let start = Instant::now();
    let mut handles = Vec::new();

    for task_id in 0..NUM_CONCURRENT_TASKS {
        let order_book = order_book.clone();
        let handle = task::spawn(async move {
            let mut rng = StdRng::seed_from_u64(task_id as u64);

            for i in 0..NUM_ORDERS / NUM_CONCURRENT_TASKS {
                let order = Order {
                    id: i as u64,
                    trading_pair: TradingPair::new("BTC".to_string(), "USD".to_string()),
                    order_type: if rng.gen_bool(0.5) {
                        OrderType::Buy
                    } else {
                        OrderType::Sell
                    },
                    price: rng.gen_range(PRICE_RANGE.0..PRICE_RANGE.1),
                    quantity: rng.gen_range(QUANTITY_RANGE.0..QUANTITY_RANGE.1),
                    timestamp: chrono::Utc::now(),
                };
                order_book.add_order(order).await;
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.unwrap();
    }

    let duration = start.elapsed();
    let total_trades = trade_counter.load(std::sync::atomic::Ordering::Relaxed);

    println!("Stress test results:");
    println!("Total orders processed: {}", NUM_ORDERS);
    println!("Total trades executed: {}", total_trades);
    println!("Time taken: {:?}", duration);
    println!(
        "Orders per second: {:.2}",
        NUM_ORDERS as f64 / duration.as_secs_f64()
    );

    let (bids, asks) = order_book.get_order_book().await;
    println!("Final order book state:");
    println!("Number of bid levels: {}", bids.len());
    println!("Number of ask levels: {}", asks.len());
}

#[tokio::test]
async fn test_concurrent_matching() {
    let (order_book, mut trade_rx) =
        ConcurrentOrderBook::new(TradingPair::new("BTC".to_string(), "USD".to_string()));
    let order_book = Arc::new(order_book);

    let trade_counter = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let trade_counter_clone = trade_counter.clone();

    task::spawn(async move {
        while let Some(_trade) = trade_rx.recv().await {
            trade_counter_clone.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    });

    let buy_order = Order {
        id: 1,
        trading_pair: TradingPair::new("BTC".to_string(), "USD".to_string()),
        order_type: OrderType::Buy,
        price: 50000.0,
        quantity: 1.0,
        timestamp: chrono::Utc::now(),
    };

    let sell_order = Order {
        id: 2,
        trading_pair: TradingPair::new("BTC".to_string(), "USD".to_string()),
        order_type: OrderType::Sell,
        price: 50000.0,
        quantity: 1.0,
        timestamp: chrono::Utc::now(),
    };

    let order_book_clone = order_book.clone();
    let buy_handle = task::spawn(async move {
        order_book_clone.add_order(buy_order).await;
    });

    let sell_handle = task::spawn(async move {
        order_book.add_order(sell_order).await;
    });

    buy_handle.await.unwrap();
    sell_handle.await.unwrap();

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let total_trades = trade_counter.load(std::sync::atomic::Ordering::Relaxed);
    assert_eq!(total_trades, 1, "Expected one trade to be executed");
}
