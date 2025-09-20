mod models;
mod orderbooks;
mod server;

use rustorderbooks::models::TradingPair;
use rustorderbooks::orderbooks::concurrent::ConcurrentOrderBook;
use rustorderbooks::orderbooks::lockfree::LockFreeOrderBook;
use rustorderbooks::orderbooks::simple::SimpleOrderBook;
use std::env;
use std::sync::Arc;

#[derive(Debug, Clone)]
enum OrderBookType {
    Simple,
    Concurrent,
    LockFree,
}

impl OrderBookType {
    fn from_str(s: &str) -> Result<Self, String> {
        match s.to_lowercase().as_str() {
            "simple" => Ok(OrderBookType::Simple),
            "concurrent" => Ok(OrderBookType::Concurrent),
            "lockfree" => Ok(OrderBookType::LockFree),
            _ => Err(format!(
                "Unknown implementation: {}. Available: simple, concurrent, lockfree",
                s
            )),
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            OrderBookType::Simple => "simple",
            OrderBookType::Concurrent => "concurrent",
            OrderBookType::LockFree => "lockfree",
        }
    }
}

fn create_orderbook(
    orderbook_type: OrderBookType,
    trading_pair: TradingPair,
) -> Arc<dyn rustorderbooks::models::OrderBook> {
    match orderbook_type {
        OrderBookType::Simple => Arc::new(SimpleOrderBook::new(trading_pair)),
        OrderBookType::Concurrent => Arc::new(ConcurrentOrderBook::new(trading_pair)),
        OrderBookType::LockFree => Arc::new(LockFreeOrderBook::new(trading_pair)),
    }
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() != 3 {
        println!("Usage: {} <port> <implementation>", args[0]);
        println!();
        println!("Implementations:");
        println!("  simple     - SimpleOrderBook (default)");
        println!("  concurrent - ConcurrentOrderBook");
        println!("  lockfree   - LockFreeOrderBook");
        return;
    }

    let port: u16 = args[1].parse().expect("Invalid port number");
    let orderbook_type = OrderBookType::from_str(&args[2]).unwrap_or_else(|_| {
        println!("Invalid orderbook type, defaulting to 'simple'.");
        OrderBookType::Simple
    });
    start_server(port, orderbook_type).await;
}

async fn start_server(port: u16, orderbook_type: OrderBookType) {
    println!(
        "Starting Orderbook Server on port {} with {} implementation",
        port,
        orderbook_type.as_str()
    );

    rustorderbooks::server::start_server(port, move |trading_pair| {
        create_orderbook(orderbook_type.clone(), trading_pair)
    })
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_orderbook_type_parsing() {
        assert!(matches!(
            OrderBookType::from_str("simple"),
            Ok(OrderBookType::Simple)
        ));
        assert!(matches!(
            OrderBookType::from_str("concurrent"),
            Ok(OrderBookType::Concurrent)
        ));
        assert!(matches!(
            OrderBookType::from_str("lockfree"),
            Ok(OrderBookType::LockFree)
        ));
        assert!(OrderBookType::from_str("invalid").is_err());
    }

    #[tokio::test]
    async fn test_all_orderbook_implementations() {
        let trading_pair = TradingPair::new("TEST", "USD");

        // Test simple implementation.
        let simple = create_orderbook(OrderBookType::Simple, trading_pair.clone());
        assert_eq!(simple.get_active_orders_count().await, 0);

        // Test concurrent implementation.
        let concurrent = create_orderbook(OrderBookType::Concurrent, trading_pair.clone());
        assert_eq!(concurrent.get_active_orders_count().await, 0);

        // Test lockfree implementation.
        let lockfree = create_orderbook(OrderBookType::LockFree, trading_pair);
        assert_eq!(lockfree.get_active_orders_count().await, 0);
    }
}
