use engine::engine::concurrent::ConcurrentOrderBook;
use engine::engine::lockfree::LockFreeOrderBook;
use engine::engine::order_book::{OrderBook, SimpleOrderBook};
use std::fs::OpenOptions;
use std::io::Write;
mod stress_tests;
use engine::engine::models::TradingPair;
use stress_tests::run_stress_test;

type OrderBookFactory = fn(TradingPair) -> Box<dyn OrderBook>;

#[tokio::test]
async fn benchmark_all_orderbooks() {
    let _ = tracing_subscriber::fmt()
        .with_thread_ids(true)
        .with_target(false)
        .with_line_number(true)
        .with_file(true)
        .with_max_level(tracing::Level::INFO)
        .try_init();

    let mut results = String::new();
    results.push_str("=== OrderBook Implementation Benchmarks ===\n\n");

    let implementations: &[(&str, OrderBookFactory)] = &[
        (
            "SimpleOrderBook",
            simple_orderbook_factory as OrderBookFactory,
        ),
        (
            "ConcurrentOrderBook",
            concurrent_orderbook_factory as OrderBookFactory,
        ),
        (
            "LockFreeOrderBook",
            lockfree_orderbook_factory as OrderBookFactory,
        ),
    ];

    for (name, factory) in implementations {
        let result = run_stress_test(name, *factory).await;
        results.push_str(&format!("{} Results:\n{}\n", name, result));
    }

    let file_path = format!(
        "{}/benchmark_results.txt",
        std::env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "target".to_string())
    );

    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(file_path)
        .expect("Failed to open benchmark_results.txt");

    file.write_all(results.as_bytes())
        .expect("Failed to write benchmark results");
}

fn simple_orderbook_factory(trading_pair: TradingPair) -> Box<dyn OrderBook> {
    Box::new(SimpleOrderBook::new(trading_pair))
}

fn concurrent_orderbook_factory(trading_pair: TradingPair) -> Box<dyn OrderBook> {
    let (book, _rx) = ConcurrentOrderBook::new(trading_pair);
    Box::new(book)
}

fn lockfree_orderbook_factory(trading_pair: TradingPair) -> Box<dyn OrderBook> {
    let (book, _rx) = LockFreeOrderBook::new(trading_pair);
    Box::new(book)
}
