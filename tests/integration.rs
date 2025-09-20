use engine::models::{Order, OrderBook, OrderType, TradingPair};
use engine::orderbooks::simple::SimpleOrderBook;
use std::sync::Arc;
// use tokio::time::Duration;
// use tracing::info;

#[tokio::test]
async fn test_empty_orderbook() {
    let trading_pair = TradingPair::new("BTC", "USD");
    let orderbook = SimpleOrderBook::new(trading_pair);

    assert_eq!(orderbook.get_active_orders_count().await, 0);
    assert_eq!(orderbook.get_current_price().await, None);

    let (bid, ask) = orderbook.get_best_bid_ask().await;
    assert_eq!(bid, None);
    assert_eq!(ask, None);
}

#[tokio::test]
async fn test_single_order() {
    let trading_pair = TradingPair::new("ETH", "USD");
    let orderbook = SimpleOrderBook::new(trading_pair.clone());

    let order = Order::new(1, trading_pair, OrderType::Buy, 3000.0, 1.0);
    orderbook.add_order(order).await;

    assert_eq!(orderbook.get_active_orders_count().await, 1);
    assert_eq!(orderbook.get_current_price().await, Some(3000.0));

    let (bid, ask) = orderbook.get_best_bid_ask().await;
    assert_eq!(bid, Some(3000.0));
    assert_eq!(ask, None);
}

#[tokio::test]
async fn test_bid_ask_spread() {
    let trading_pair = TradingPair::new("BTC", "USD");
    let orderbook = SimpleOrderBook::new(trading_pair.clone());

    let buy_order = Order::new(1, trading_pair.clone(), OrderType::Buy, 49900.0, 1.0);
    orderbook.add_order(buy_order).await;

    let sell_order = Order::new(2, trading_pair, OrderType::Sell, 50100.0, 1.0);
    orderbook.add_order(sell_order).await;

    assert_eq!(orderbook.get_active_orders_count().await, 2);
    assert_eq!(orderbook.get_current_price().await, Some(50000.0));

    let (bid, ask) = orderbook.get_best_bid_ask().await;
    assert_eq!(bid, Some(49900.0));
    assert_eq!(ask, Some(50100.0));
}

#[tokio::test]
async fn test_exact_match() {
    let trading_pair = TradingPair::new("ETH", "USD");
    let orderbook = SimpleOrderBook::new(trading_pair.clone());

    let buy_order = Order::new(1, trading_pair.clone(), OrderType::Buy, 3000.0, 2.0);
    orderbook.add_order(buy_order).await;
    assert_eq!(orderbook.get_active_orders_count().await, 1);

    let sell_order = Order::new(2, trading_pair, OrderType::Sell, 3000.0, 2.0);
    orderbook.add_order(sell_order).await;

    assert_eq!(orderbook.get_active_orders_count().await, 0);
    assert_eq!(orderbook.get_current_price().await, None);
}

#[tokio::test]
async fn test_partial_fill() {
    let trading_pair = TradingPair::new("BTC", "USD");
    let orderbook = SimpleOrderBook::new(trading_pair.clone());

    let sell_order = Order::new(1, trading_pair.clone(), OrderType::Sell, 50000.0, 3.0);
    orderbook.add_order(sell_order).await;
    assert_eq!(orderbook.get_active_orders_count().await, 1);

    let buy_order = Order::new(2, trading_pair, OrderType::Buy, 50000.0, 1.0);
    orderbook.add_order(buy_order).await;

    assert_eq!(orderbook.get_active_orders_count().await, 1);

    let (bid, ask) = orderbook.get_best_bid_ask().await;
    assert_eq!(bid, None);
    assert_eq!(ask, Some(50000.0));
}

#[tokio::test]
async fn test_price_time_priority() {
    let trading_pair = TradingPair::new("ETH", "USD");
    let orderbook = SimpleOrderBook::new(trading_pair.clone());

    let order1 = Order::new(1, trading_pair.clone(), OrderType::Buy, 3000.0, 1.0);
    let order2 = Order::new(2, trading_pair.clone(), OrderType::Buy, 3000.0, 1.0);
    let order3 = Order::new(3, trading_pair.clone(), OrderType::Buy, 3000.0, 1.0);

    orderbook.add_order(order1).await;
    orderbook.add_order(order2).await;
    orderbook.add_order(order3).await;

    assert_eq!(orderbook.get_active_orders_count().await, 3);

    let sell_order = Order::new(4, trading_pair, OrderType::Sell, 3000.0, 2.0);
    orderbook.add_order(sell_order).await;

    assert_eq!(orderbook.get_active_orders_count().await, 1);
}

#[tokio::test]
async fn test_multiple_price_levels() {
    let trading_pair = TradingPair::new("BTC", "USD");
    let orderbook = SimpleOrderBook::new(trading_pair.clone());

    for i in 0..5 {
        let price = 49000.0 + (i as f64 * 100.0); // 49000, 49100, 49200, 49300, 49400
        let order = Order::new(i, trading_pair.clone(), OrderType::Buy, price, 1.0);
        orderbook.add_order(order).await;
    }

    for i in 5..10 {
        let price = 50000.0 + ((i - 5) as f64 * 100.0); // 50000, 50100, 50200, 50300, 50400
        let order = Order::new(i, trading_pair.clone(), OrderType::Sell, price, 1.0);
        orderbook.add_order(order).await;
    }

    assert_eq!(orderbook.get_active_orders_count().await, 10);

    let (bid, ask) = orderbook.get_best_bid_ask().await;
    assert_eq!(bid, Some(49400.0));
    assert_eq!(ask, Some(50000.0));

    assert_eq!(orderbook.get_current_price().await, Some(49700.0));
}

#[tokio::test]
async fn test_concurrent_operations() {
    let trading_pair = TradingPair::new("TEST", "USD");
    let orderbook = Arc::new(SimpleOrderBook::new(trading_pair.clone()));

    let mut handles = Vec::new();

    for i in 0..100 {
        let orderbook_clone = orderbook.clone();
        let trading_pair_clone = trading_pair.clone();

        let handle = tokio::spawn(async move {
            let order = Order::new(
                i,
                trading_pair_clone,
                if i % 2 == 0 {
                    OrderType::Buy
                } else {
                    OrderType::Sell
                },
                1000.0 + (i as f64),
                1.0,
            );
            orderbook_clone.add_order(order).await;
        });

        handles.push(handle);
    }

    for handle in handles {
        handle.await.unwrap();
    }

    let active_orders = orderbook.get_active_orders_count().await;
    assert!(active_orders <= 100);

    let (bid, ask) = orderbook.get_best_bid_ask().await;
    if let (Some(bid), Some(ask)) = (bid, ask) {
        assert!(ask >= bid);
    }
}
