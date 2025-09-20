use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use rustorderbooks::models::{Order, OrderBook, OrderType, TradingPair};
use rustorderbooks::orderbooks::simple::SimpleOrderBook;
use std::sync::Arc;
use tokio::runtime::Runtime;

fn create_orders(num_orders: usize, trading_pair: TradingPair) -> Vec<Order> {
    (0..num_orders)
        .map(|i| {
            Order::new(
                i as u64,
                trading_pair.clone(),
                if i % 2 == 0 {
                    OrderType::Buy
                } else {
                    OrderType::Sell
                },
                50000.0 + (i as f64 % 100.0),
                1.0,
            )
        })
        .collect()
}

fn bench_order_processing(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let trading_pair = TradingPair::new("BTC", "USD");

    let mut group = c.benchmark_group("order_processing");

    for size in [100, 1000, 10000].iter() {
        group.bench_with_input(
            BenchmarkId::new("simple_orderbook", size),
            size,
            |b, &size| {
                b.iter(|| {
                    rt.block_on(async {
                        let orderbook = Arc::new(SimpleOrderBook::new(trading_pair.clone()));
                        let orders = create_orders(size, trading_pair.clone());

                        for order in orders {
                            orderbook.add_order(black_box(order)).await;
                        }

                        black_box(orderbook.get_active_orders_count().await);
                    })
                });
            },
        );
    }

    group.finish();
}

fn bench_price_queries(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let trading_pair = TradingPair::new("ETH", "USD");

    let orderbook = rt.block_on(async {
        let orderbook = Arc::new(SimpleOrderBook::new(trading_pair.clone()));
        let orders = create_orders(1000, trading_pair.clone());

        for order in orders {
            orderbook.add_order(order).await;
        }

        orderbook
    });

    c.bench_function("price_query", |b| {
        b.iter(|| {
            rt.block_on(async {
                let price = orderbook.get_current_price().await;
                black_box(price);
            })
        });
    });

    c.bench_function("bid_ask_query", |b| {
        b.iter(|| {
            rt.block_on(async {
                let (bid, ask) = orderbook.get_best_bid_ask().await;
                black_box((bid, ask));
            })
        });
    });
}

fn bench_concurrent_orders(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let trading_pair = TradingPair::new("BTC", "USD");

    c.bench_function("concurrent_order_placement", |b| {
        b.iter(|| {
            rt.block_on(async {
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
                            50000.0 + (i as f64 % 10.0),
                            1.0,
                        );
                        orderbook_clone.add_order(order).await;
                    });

                    handles.push(handle);
                }

                for handle in handles {
                    handle.await.unwrap();
                }

                black_box(orderbook.get_active_orders_count().await);
            })
        });
    });
}

fn bench_matching_scenarios(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let trading_pair = TradingPair::new("ETH", "USD");

    let mut group = c.benchmark_group("matching_scenarios");

    // Orders that do not match.
    group.bench_function("no_matching", |b| {
        b.iter(|| {
            rt.block_on(async {
                let orderbook = Arc::new(SimpleOrderBook::new(trading_pair.clone()));

                for i in 0..100 {
                    let order = Order::new(
                        i,
                        trading_pair.clone(),
                        OrderType::Buy,
                        2900.0 + i as f64,
                        1.0,
                    );
                    orderbook.add_order(black_box(order)).await;
                }

                for i in 100..200 {
                    let order = Order::new(
                        i,
                        trading_pair.clone(),
                        OrderType::Sell,
                        3100.0 + (i - 100) as f64,
                        1.0,
                    );
                    orderbook.add_order(black_box(order)).await;
                }

                black_box(orderbook.get_active_orders_count().await);
            })
        });
    });

    // Orders that match immediately.
    group.bench_function("immediate_matching", |b| {
        b.iter(|| {
            rt.block_on(async {
                let orderbook = Arc::new(SimpleOrderBook::new(trading_pair.clone()));

                // Add some resting orders first
                for i in 0..50 {
                    let order = Order::new(i, trading_pair.clone(), OrderType::Buy, 3000.0, 1.0);
                    orderbook.add_order(order).await;
                }

                // Now add matching sell orders
                for i in 50..100 {
                    let order = Order::new(i, trading_pair.clone(), OrderType::Sell, 3000.0, 1.0);
                    orderbook.add_order(black_box(order)).await;
                }

                black_box(orderbook.get_active_orders_count().await);
            })
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_order_processing,
    bench_price_queries,
    bench_concurrent_orders,
    bench_matching_scenarios
);
criterion_main!(benches);
