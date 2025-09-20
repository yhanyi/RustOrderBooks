use engine::models::{Order, OrderBook, OrderType, TradingPair};
use engine::orderbooks::concurrent::ConcurrentOrderBook;
use engine::orderbooks::lockfree::LockFreeOrderBook;
use engine::orderbooks::simple::SimpleOrderBook;
use std::sync::Arc;

// Helper to create a vector of all three orderbook implementations.
fn create_all_orderbooks(trading_pair: TradingPair) -> Vec<(String, Arc<dyn OrderBook>)> {
    vec![
        (
            "Simple".to_string(),
            Arc::new(SimpleOrderBook::new(trading_pair.clone())),
        ),
        (
            "Concurrent".to_string(),
            Arc::new(ConcurrentOrderBook::new(trading_pair.clone())),
        ),
        (
            "LockFree".to_string(),
            Arc::new(LockFreeOrderBook::new(trading_pair)),
        ),
    ]
}

// Helper to run a test on all implementations.
async fn test_all_implementations<F, Fut>(test_name: &str, test_fn: F)
where
    F: Fn(Arc<dyn OrderBook>) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    let trading_pair = TradingPair::new("TEST", "USD");
    let orderbooks = create_all_orderbooks(trading_pair);

    let mut handles = Vec::new();

    // Wrap the test_fn in an Arc to ensure it can be safely shared across threads.
    let test_fn = Arc::new(test_fn);

    for (name, orderbook) in orderbooks {
        // Clone Arc to be used in async block.
        let test_fn = test_fn.clone();
        // Clone test_name so it can live long enough.
        let test_name = test_name.to_string();

        let handle = tokio::spawn(async move {
            println!("Running {} test on {} implementation", test_name, name);
            test_fn(orderbook).await;
            println!("{} test passed on {} implementation", test_name, name);
        });

        handles.push(handle);
    }

    // Await all tests to complete.
    for handle in handles {
        handle.await.expect("Test task panicked");
    }

    println!("All implementations passed {} test", test_name);
}

#[tokio::test]
async fn test_empty_orderbook() {
    test_all_implementations("empty_orderbook", |orderbook| async move {
        assert_eq!(orderbook.get_active_orders_count().await, 0);
        assert_eq!(orderbook.get_current_price().await, None);

        let (bid, ask) = orderbook.get_best_bid_ask().await;
        assert_eq!(bid, None);
        assert_eq!(ask, None);
    })
    .await;
}

#[tokio::test]
async fn test_single_order() {
    test_all_implementations("single_order", |orderbook| async move {
        let trading_pair = TradingPair::new("ETH", "USD");
        let order = Order::new(1, trading_pair, OrderType::Buy, 3000.0, 1.0);
        orderbook.add_order(order).await;

        assert_eq!(orderbook.get_active_orders_count().await, 1);
        assert_eq!(orderbook.get_current_price().await, Some(3000.0));

        let (bid, ask) = orderbook.get_best_bid_ask().await;
        assert_eq!(bid, Some(3000.0));
        assert_eq!(ask, None);
    })
    .await;
}

#[tokio::test]
async fn test_bid_ask_spread() {
    test_all_implementations("bid_ask_spread", |orderbook| async move {
        let trading_pair = TradingPair::new("BTC", "USD");

        // Add buy order.
        let buy_order = Order::new(1, trading_pair.clone(), OrderType::Buy, 49900.0, 1.0);
        orderbook.add_order(buy_order).await;

        // Add sell order.
        let sell_order = Order::new(2, trading_pair, OrderType::Sell, 50100.0, 1.0);
        orderbook.add_order(sell_order).await;

        assert_eq!(orderbook.get_active_orders_count().await, 2);
        assert_eq!(orderbook.get_current_price().await, Some(50000.0));

        let (bid, ask) = orderbook.get_best_bid_ask().await;
        assert_eq!(bid, Some(49900.0));
        assert_eq!(ask, Some(50100.0));
    })
    .await;
}

#[tokio::test]
async fn test_exact_match() {
    test_all_implementations("exact_match", |orderbook| async move {
        let trading_pair = TradingPair::new("ETH", "USD");

        // Add buy order.
        let buy_order = Order::new(1, trading_pair.clone(), OrderType::Buy, 3000.0, 2.0);
        orderbook.add_order(buy_order).await;
        assert_eq!(orderbook.get_active_orders_count().await, 1);

        // Add exact matching sell order.
        let sell_order = Order::new(2, trading_pair, OrderType::Sell, 3000.0, 2.0);
        orderbook.add_order(sell_order).await;

        // Should have no orders left due to complete match.
        assert_eq!(orderbook.get_active_orders_count().await, 0);
        assert_eq!(orderbook.get_current_price().await, None);
    })
    .await;
}

#[tokio::test]
async fn test_partial_fill() {
    test_all_implementations("partial_fill", |orderbook| async move {
        let trading_pair = TradingPair::new("BTC", "USD");

        // Add large sell order.
        let sell_order = Order::new(1, trading_pair.clone(), OrderType::Sell, 50000.0, 3.0);
        orderbook.add_order(sell_order).await;
        assert_eq!(orderbook.get_active_orders_count().await, 1);

        // Add smaller buy order that partially fills.
        let buy_order = Order::new(2, trading_pair, OrderType::Buy, 50000.0, 1.0);
        orderbook.add_order(buy_order).await;

        // Should have 1 order left due to partial fill.
        assert_eq!(orderbook.get_active_orders_count().await, 1);

        let (bid, ask) = orderbook.get_best_bid_ask().await;
        assert_eq!(bid, None);
        assert_eq!(ask, Some(50000.0));
    })
    .await;
}

#[tokio::test]
async fn test_price_time_priority() {
    test_all_implementations("price_time_priority", |orderbook| async move {
        let trading_pair = TradingPair::new("ETH", "USD");

        // Add multiple buy orders at same price (should be FIFO).
        let order1 = Order::new(1, trading_pair.clone(), OrderType::Buy, 3000.0, 1.0);
        let order2 = Order::new(2, trading_pair.clone(), OrderType::Buy, 3000.0, 1.0);
        let order3 = Order::new(3, trading_pair.clone(), OrderType::Buy, 3000.0, 1.0);

        orderbook.add_order(order1).await;
        orderbook.add_order(order2).await;
        orderbook.add_order(order3).await;

        assert_eq!(orderbook.get_active_orders_count().await, 3);

        // Add sell order that matches 2 orders.
        let sell_order = Order::new(4, trading_pair, OrderType::Sell, 3000.0, 2.0);
        orderbook.add_order(sell_order).await;

        // Should have 1 buy order remaining.
        assert_eq!(orderbook.get_active_orders_count().await, 1);
    })
    .await;
}

#[tokio::test]
async fn test_multiple_price_levels() {
    test_all_implementations("multiple_price_levels", |orderbook| async move {
        let trading_pair = TradingPair::new("BTC", "USD");

        // Add buy orders at different prices.
        for i in 0..5 {
            let price = 49000.0 + (i as f64 * 100.0);
            let order = Order::new(i, trading_pair.clone(), OrderType::Buy, price, 1.0);
            orderbook.add_order(order).await;
        }

        // Add sell orders at different prices.
        for i in 5..10 {
            let price = 50000.0 + ((i - 5) as f64 * 100.0);
            let order = Order::new(i, trading_pair.clone(), OrderType::Sell, price, 1.0);
            orderbook.add_order(order).await;
        }

        assert_eq!(orderbook.get_active_orders_count().await, 10);

        let (bid, ask) = orderbook.get_best_bid_ask().await;
        assert_eq!(bid, Some(49400.0)); // Highest buy.
        assert_eq!(ask, Some(50000.0)); // Lowest sell.

        assert_eq!(orderbook.get_current_price().await, Some(49700.0));
    })
    .await;
}

#[tokio::test]
async fn test_price_improvement_matching() {
    test_all_implementations("price_improvement_matching", |orderbook| async move {
        let trading_pair = TradingPair::new("TEST", "USD");

        // Add sell order at 100.
        let sell_order = Order::new(1, trading_pair.clone(), OrderType::Sell, 100.0, 1.0);
        orderbook.add_order(sell_order).await;

        // Add buy order at 110.
        let buy_order = Order::new(2, trading_pair, OrderType::Buy, 110.0, 1.0);
        orderbook.add_order(buy_order).await;

        // Should have no orders left.
        assert_eq!(orderbook.get_active_orders_count().await, 0);
        assert_eq!(orderbook.get_current_price().await, None);
    })
    .await;
}

#[tokio::test]
async fn test_no_match_spread() {
    test_all_implementations("no_match_spread", |orderbook| async move {
        let trading_pair = TradingPair::new("SPREAD", "USD");

        // Add buy order at 90.
        let buy_order = Order::new(1, trading_pair.clone(), OrderType::Buy, 90.0, 1.0);
        orderbook.add_order(buy_order).await;

        // Add sell order at 110.
        let sell_order = Order::new(2, trading_pair, OrderType::Sell, 110.0, 1.0);
        orderbook.add_order(sell_order).await;

        // Should have both orders, no match.
        assert_eq!(orderbook.get_active_orders_count().await, 2);

        let (bid, ask) = orderbook.get_best_bid_ask().await;
        assert_eq!(bid, Some(90.0));
        assert_eq!(ask, Some(110.0));
        assert_eq!(orderbook.get_current_price().await, Some(100.0));
    })
    .await;
}

#[tokio::test]
async fn test_multiple_partial_fills() {
    test_all_implementations("multiple_partial_fills", |orderbook| async move {
        let trading_pair = TradingPair::new("MULTI", "USD");

        // Add multiple small sell orders.
        for i in 0..5 {
            let order = Order::new(i, trading_pair.clone(), OrderType::Sell, 100.0, 0.5);
            orderbook.add_order(order).await;
        }

        assert_eq!(orderbook.get_active_orders_count().await, 5);

        // Add large buy order that should match all sell orders.
        let buy_order = Order::new(10, trading_pair, OrderType::Buy, 100.0, 2.5);
        orderbook.add_order(buy_order).await;

        // All orders should be matched completely.
        assert_eq!(orderbook.get_active_orders_count().await, 0);
    })
    .await;
}

#[tokio::test]
async fn test_large_order_matching() {
    test_all_implementations("large_order_matching", |orderbook| async move {
        let trading_pair = TradingPair::new("LARGE", "USD");

        // Add large buy order.
        let buy_order = Order::new(1, trading_pair.clone(), OrderType::Buy, 50000.0, 100.0);
        orderbook.add_order(buy_order).await;

        // Add smaller sell orders that partially fill.
        for i in 0..3 {
            let order = Order::new(10 + i, trading_pair.clone(), OrderType::Sell, 50000.0, 20.0);
            orderbook.add_order(order).await;
        }

        // Should have 1 remaining buy order.
        assert_eq!(orderbook.get_active_orders_count().await, 1);

        let (bid, ask) = orderbook.get_best_bid_ask().await;
        assert_eq!(bid, Some(50000.0));
        assert_eq!(ask, None);
    })
    .await;
}

#[tokio::test]
async fn test_concurrent_operations() {
    test_all_implementations("concurrent_operations", |orderbook| async move {
        let trading_pair = TradingPair::new("CONCURRENT", "USD");

        let mut handles = Vec::new();

        // Spawn multiple tasks adding orders concurrently.
        for i in 0..20 {
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

        // Wait for all tasks to complete.
        for handle in handles {
            handle.await.unwrap();
        }

        // Verify orderbook state is consistent.
        let active_orders = orderbook.get_active_orders_count().await;
        assert!(active_orders <= 20); // Some orders might have matched.

        let (bid, ask) = orderbook.get_best_bid_ask().await;
        if let (Some(bid), Some(ask)) = (bid, ask) {
            assert!(ask >= bid);
        }
    })
    .await;
}

#[tokio::test]
async fn test_precision_edge_cases() {
    test_all_implementations("precision_edge_cases", |orderbook| async move {
        let trading_pair = TradingPair::new("PRECISION", "USD");

        // Test with very small quantities.
        let buy_order = Order::new(1, trading_pair.clone(), OrderType::Buy, 100.0, 0.001);
        orderbook.add_order(buy_order).await;

        let sell_order = Order::new(2, trading_pair, OrderType::Sell, 100.0, 0.001);
        orderbook.add_order(sell_order).await;

        // Should match completely despite small quantities.
        assert_eq!(orderbook.get_active_orders_count().await, 0);
    })
    .await;
}

#[tokio::test]
async fn test_price_levels_cleanup() {
    test_all_implementations("price_levels_cleanup", |orderbook| async move {
        let trading_pair = TradingPair::new("CLEANUP", "USD");

        // Add orders at same price level.
        for i in 0..3 {
            let order = Order::new(i, trading_pair.clone(), OrderType::Buy, 100.0, 1.0);
            orderbook.add_order(order).await;
        }

        assert_eq!(orderbook.get_active_orders_count().await, 3);

        // Match all orders at this price level.
        let sell_order = Order::new(10, trading_pair, OrderType::Sell, 100.0, 3.0);
        orderbook.add_order(sell_order).await;

        // Price level should be cleaned up.
        assert_eq!(orderbook.get_active_orders_count().await, 0);
        assert_eq!(orderbook.get_current_price().await, None);
    })
    .await;
}

#[tokio::test]
async fn test_simple_basic_operations() {
    let trading_pair = TradingPair::new("BTC", "USD");
    let orderbook = SimpleOrderBook::new(trading_pair.clone());

    assert_eq!(orderbook.get_active_orders_count().await, 0);
    assert_eq!(orderbook.get_current_price().await, None);

    let buy_order = Order::new(1, trading_pair.clone(), OrderType::Buy, 50000.0, 1.0);
    orderbook.add_order(buy_order).await;

    assert_eq!(orderbook.get_active_orders_count().await, 1);
    assert_eq!(orderbook.get_current_price().await, Some(50000.0));

    let sell_order = Order::new(2, trading_pair.clone(), OrderType::Sell, 50100.0, 0.5);
    orderbook.add_order(sell_order).await;

    assert_eq!(orderbook.get_active_orders_count().await, 2);
    assert_eq!(orderbook.get_current_price().await, Some(50050.0)); // Mid price
}

#[tokio::test]
async fn test_simple_order_matching() {
    let trading_pair = TradingPair::new("ETH", "USD");
    let orderbook = SimpleOrderBook::new(trading_pair.clone());

    let buy_order = Order::new(1, trading_pair.clone(), OrderType::Buy, 3000.0, 2.0);
    orderbook.add_order(buy_order).await;

    let sell_order = Order::new(2, trading_pair.clone(), OrderType::Sell, 3000.0, 1.0);
    orderbook.add_order(sell_order).await;

    assert_eq!(orderbook.get_active_orders_count().await, 1);
    let (bid, ask) = orderbook.get_best_bid_ask().await;
    assert_eq!(bid, Some(3000.0));
    assert_eq!(ask, None);
}

#[tokio::test]
async fn test_simple_partial_fills() {
    let trading_pair = TradingPair::new("BTC", "USD");
    let orderbook = SimpleOrderBook::new(trading_pair.clone());

    let sell_order = Order::new(1, trading_pair.clone(), OrderType::Sell, 50000.0, 2.0);
    orderbook.add_order(sell_order).await;

    let buy_order = Order::new(2, trading_pair.clone(), OrderType::Buy, 50000.0, 0.5);
    orderbook.add_order(buy_order).await;

    assert_eq!(orderbook.get_active_orders_count().await, 1);
    let (bid, ask) = orderbook.get_best_bid_ask().await;
    assert_eq!(bid, None);
    assert_eq!(ask, Some(50000.0));
}

#[tokio::test]
async fn test_concurrent_basic_operations() {
    let trading_pair = TradingPair::new("BTC", "USD");
    let orderbook = ConcurrentOrderBook::new(trading_pair.clone());

    assert_eq!(orderbook.get_active_orders_count().await, 0);
    assert_eq!(orderbook.get_current_price().await, None);

    let buy_order = Order::new(1, trading_pair.clone(), OrderType::Buy, 50000.0, 1.0);
    orderbook.add_order(buy_order).await;

    assert_eq!(orderbook.get_active_orders_count().await, 1);
    assert_eq!(orderbook.get_current_price().await, Some(50000.0));
}

#[tokio::test]
async fn test_concurrent_matching() {
    let trading_pair = TradingPair::new("ETH", "USD");
    let orderbook = ConcurrentOrderBook::new(trading_pair.clone());

    let buy_order = Order::new(1, trading_pair.clone(), OrderType::Buy, 3000.0, 2.0);
    orderbook.add_order(buy_order).await;

    let sell_order = Order::new(2, trading_pair.clone(), OrderType::Sell, 3000.0, 1.0);
    orderbook.add_order(sell_order).await;

    assert_eq!(orderbook.get_active_orders_count().await, 1);
    let (bid, ask) = orderbook.get_best_bid_ask().await;
    assert_eq!(bid, Some(3000.0));
    assert_eq!(ask, None);
}

#[tokio::test]
async fn test_lockfree_basic_operations() {
    let trading_pair = TradingPair::new("BTC", "USD");
    let orderbook = LockFreeOrderBook::new(trading_pair.clone());

    assert_eq!(orderbook.get_active_orders_count().await, 0);
    assert_eq!(orderbook.get_current_price().await, None);

    let buy_order = Order::new(1, trading_pair.clone(), OrderType::Buy, 50000.0, 1.0);
    orderbook.add_order(buy_order).await;

    assert_eq!(orderbook.get_active_orders_count().await, 1);
    assert_eq!(orderbook.get_current_price().await, Some(50000.0));
}

#[tokio::test]
async fn test_lockfree_matching() {
    let trading_pair = TradingPair::new("ETH", "USD");
    let orderbook = LockFreeOrderBook::new(trading_pair.clone());

    let buy_order = Order::new(1, trading_pair.clone(), OrderType::Buy, 3000.0, 2.0);
    orderbook.add_order(buy_order).await;

    let sell_order = Order::new(2, trading_pair.clone(), OrderType::Sell, 3000.0, 1.0);
    orderbook.add_order(sell_order).await;

    assert_eq!(orderbook.get_active_orders_count().await, 1);
    let (bid, ask) = orderbook.get_best_bid_ask().await;
    assert_eq!(bid, Some(3000.0));
    assert_eq!(ask, None);
}
