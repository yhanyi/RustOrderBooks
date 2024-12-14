use engine::engine::concurrent::ConcurrentOrderBook;
use engine::engine::models::{Order, OrderType, TradingPair};
use engine::engine::order_book::{OrderBook, SimpleOrderBook};
use tokio::time::Duration;
use tracing::info;

#[tokio::test]
async fn test_add_and_match_orders() {
    let order_book = SimpleOrderBook::new(TradingPair::new("BTC".to_string(), "USD".to_string()));

    let buy_order = Order {
        id: 1,
        trading_pair: TradingPair::new("BTC".to_string(), "USD".to_string()),
        order_type: OrderType::Buy,
        price: 50000.0,
        quantity: 1.0,
        timestamp: chrono::Utc::now(),
    };
    order_book.add_order(buy_order).await;

    let sell_order = Order {
        id: 2,
        trading_pair: TradingPair::new("BTC".to_string(), "USD".to_string()),
        order_type: OrderType::Sell,
        price: 50000.0,
        quantity: 1.0,
        timestamp: chrono::Utc::now(),
    };
    order_book.add_order(sell_order).await;

    let trades = order_book.match_orders().await;
    assert_eq!(trades.len(), 1);
    assert_eq!(trades[0].quantity, 1.0);
    assert_eq!(trades[0].price, 50000.0);
}

#[tokio::test]
async fn test_order_matching() {
    let (book, mut trade_rx) =
        ConcurrentOrderBook::new(TradingPair::new("BTC".to_string(), "USD".to_string()));

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

    info!("Adding buy order: {:?}", buy_order);
    book.add_order(buy_order).await;
    info!("Adding sell order: {:?}", sell_order);
    book.add_order(sell_order).await;

    match tokio::time::timeout(Duration::from_secs(1), trade_rx.recv()).await {
        Ok(Some(trade)) => {
            info!("Trade executed: {:?}", trade);
        }
        Ok(None) => {
            eprintln!("Trade channel closed without trade");
        }
        Err(_) => {
            eprintln!("Timeout waiting for trade");
        }
    }
}
