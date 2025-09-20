mod models;
mod orderbooks;
mod server;

use models::OrderBook;
use models::TradingPair;
use orderbooks::simple::SimpleOrderBook;
use std::env;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        println!("Usage: {} <port> [mode]", args[0]);
        println!("Modes:");
        println!("  server  - Start HTTP server (default)");
        println!("  bench   - Run benchmarks");
        println!("  test    - Run interactive test");
        return;
    }

    let port: u16 = args[1].parse().expect("Invalid port number");
    let mode = args.get(2).map(|s| s.as_str()).unwrap_or("server");

    match mode {
        "server" => start_server(port).await,
        "bench" => run_benchmarks().await,
        "test" => run_interactive_test(port).await,
        _ => {
            println!("Unknown mode: {}", mode);
            println!("Available modes: server, bench, test");
        }
    }
}

async fn start_server(port: u16) {
    println!("Starting OrderBook server");

    // TODO: Make this configurable again later.
    server::start_server(port, |trading_pair| {
        Arc::new(SimpleOrderBook::new(trading_pair))
    })
    .await;
}

async fn run_benchmarks() {
    println!("🔥 Running Performance Benchmarks");

    // Simple throughput test
    let trading_pair = TradingPair::new("BTC", "USD");
    let orderbook = Arc::new(SimpleOrderBook::new(trading_pair.clone()));

    let start = std::time::Instant::now();
    let num_orders = 10_000;

    println!("Adding {} orders...", num_orders);

    for i in 0..num_orders {
        let order = models::Order::new(
            i as u64,
            trading_pair.clone(),
            if i % 2 == 0 {
                models::OrderType::Buy
            } else {
                models::OrderType::Sell
            },
            50000.0 + (i as f64 % 100.0),
            1.0,
        );
        orderbook.add_order(order).await;
    }

    let duration = start.elapsed();
    let orders_per_sec = num_orders as f64 / duration.as_secs_f64();

    println!("✅ Processed {} orders in {:?}", num_orders, duration);
    println!("📊 Throughput: {:.2} orders/second", orders_per_sec);
    println!(
        "📈 Active orders: {}",
        orderbook.get_active_orders_count().await
    );

    if let Some(price) = orderbook.get_current_price().await {
        println!("💰 Current price: ${:.2}", price);
    }
}

async fn run_interactive_test(port: u16) {
    println!("🧪 Running Interactive Test");

    // Start server in background
    let server_handle = tokio::spawn(async move {
        server::start_server(port, |trading_pair| {
            Arc::new(SimpleOrderBook::new(trading_pair))
        })
        .await;
    });

    // Wait a bit for server to start
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Run some test requests
    let client = reqwest::Client::new();
    let base_url = format!("http://localhost:{}", port);

    println!("Testing endpoints...");

    // Test health
    match client.get(&format!("{}/health", base_url)).send().await {
        Ok(response) => println!("✅ Health check: {}", response.status()),
        Err(e) => println!("❌ Health check failed: {}", e),
    }

    // Test price query
    match client
        .get(&format!("{}/price/BTC/USD", base_url))
        .send()
        .await
    {
        Ok(response) => {
            println!("✅ Price query: {}", response.status());
            if let Ok(text) = response.text().await {
                println!("   Response: {}", text);
            }
        }
        Err(e) => println!("❌ Price query failed: {}", e),
    }

    // Test order placement
    let order_request = serde_json::json!({
        "trading_pair": "BTC/USD",
        "order_type": "buy",
        "price": 50000.0,
        "quantity": 1.0
    });

    match client
        .post(&format!("{}/order", base_url))
        .json(&order_request)
        .send()
        .await
    {
        Ok(response) => {
            println!("✅ Order placement: {}", response.status());
            if let Ok(text) = response.text().await {
                println!("   Response: {}", text);
            }
        }
        Err(e) => println!("❌ Order placement failed: {}", e),
    }

    // Test price after order
    match client
        .get(&format!("{}/price/BTC/USD", base_url))
        .send()
        .await
    {
        Ok(response) => {
            if let Ok(text) = response.text().await {
                println!("📊 Price after order: {}", text);
            }
        }
        Err(e) => println!("❌ Price query after order failed: {}", e),
    }

    println!("🎯 Interactive test complete! Server is still running...");

    // Keep server running
    server_handle.await.unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_orderbook_creation() {
        let trading_pair = TradingPair::new("TEST", "USD");
        let orderbook = SimpleOrderBook::new(trading_pair);

        assert_eq!(orderbook.get_active_orders_count().await, 0);
        assert_eq!(orderbook.get_current_price().await, None);
    }
}
