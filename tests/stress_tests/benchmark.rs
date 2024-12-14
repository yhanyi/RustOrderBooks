use engine::engine::concurrent::ConcurrentOrderBook;
use engine::engine::lockfree::LockFreeOrderBook;
use engine::engine::order_book::SimpleOrderBook;
use std::fs::OpenOptions;
use std::io::Write;

#[tokio::test]
async fn benchmark_all_orderbooks() {
    // Initialize tracing
    let _ = tracing_subscriber::fmt()
        .with_thread_ids(true)
        .with_target(false)
        .with_line_number(true)
        .with_file(true)
        .with_max_level(tracing::Level::INFO)
        .try_init();

    let mut results = String::new();
    results.push_str("=== OrderBook Implementation Benchmarks ===\n\n");

    // Test SimpleOrderBook
    let simple_result = run_stress_test("SimpleOrderBook", |trading_pair| {
        Box::new(SimpleOrderBook::new(trading_pair))
    })
    .await;
    results.push_str(&format!("SimpleOrderBook Results:\n{}\n", simple_result));

    // Test ConcurrentOrderBook
    let concurrent_result = run_stress_test("ConcurrentOrderBook", |trading_pair| {
        let (book, _rx) = ConcurrentOrderBook::new(trading_pair);
        Box::new(book)
    })
    .await;
    results.push_str(&format!(
        "ConcurrentOrderBook Results:\n{}\n",
        concurrent_result
    ));

    // Test LockFreeOrderBook
    let lockfree_result = run_stress_test("LockFreeOrderBook", |trading_pair| {
        let (book, _rx) = LockFreeOrderBook::new(trading_pair);
        Box::new(book)
    })
    .await;
    results.push_str(&format!(
        "LockFreeOrderBook Results:\n{}\n",
        lockfree_result
    ));

    // Write results to file
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open("benchmark_results.txt")
        .expect("Failed to open benchmark_results.txt");

    file.write_all(results.as_bytes())
        .expect("Failed to write benchmark results");
}
