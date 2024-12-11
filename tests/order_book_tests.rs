use engine::engine::models::{Order, OrderType, TradingPair};
use engine::engine::order_book::{OrderBook, SimpleOrderBook};

#[tokio::test]
async fn test_add_and_match_orders() {
    let order_book = SimpleOrderBook::new(TradingPair::new("BTC".to_string(), "USD".to_string()));

    // Add buy order
    let buy_order = Order {
        id: 1,
        trading_pair: TradingPair::new("BTC".to_string(), "USD".to_string()),
        order_type: OrderType::Buy,
        price: 50000.0,
        quantity: 1.0,
        timestamp: chrono::Utc::now(),
    };
    order_book.add_order(buy_order).await;

    // Add matching sell order
    let sell_order = Order {
        id: 2,
        trading_pair: TradingPair::new("BTC".to_string(), "USD".to_string()),
        order_type: OrderType::Sell,
        price: 50000.0,
        quantity: 1.0,
        timestamp: chrono::Utc::now(),
    };
    order_book.add_order(sell_order).await;

    // Match orders
    let trades = order_book.match_orders().await;
    assert_eq!(trades.len(), 1);
    assert_eq!(trades[0].quantity, 1.0);
    assert_eq!(trades[0].price, 50000.0);
}
