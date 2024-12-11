mod engine;
use engine::{
    engine::{start_engine, Message},
    models::{Order, OrderType, TradingPair},
};
use tokio;
use tracing::info;

#[tokio::main]
async fn main() {
    let engine_tx = start_engine();
    let trading_pair = TradingPair::from_string("BTC/USD").unwrap();
    let test_orders = vec![
        Order {
            id: 1,
            trading_pair: trading_pair.clone(),
            order_type: OrderType::Buy,
            price: 50000.0,
            quantity: 1.0,
            timestamp: chrono::Utc::now(),
        },
        Order {
            id: 2,
            trading_pair: trading_pair.clone(),
            order_type: OrderType::Sell,
            price: 50000.0,
            quantity: 0.5,
            timestamp: chrono::Utc::now(),
        },
    ];

    // Temporary nap to ensure engine is ready.
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    for order in test_orders {
        info!(
            order_id = order.id,
            order_type = ?order.order_type,
            "Sending test order"
        );
        engine_tx.send(Message::NewOrder(order)).await.unwrap();
    }

    engine_tx
        .send(Message::MatchOrders(trading_pair.clone()))
        .await
        .unwrap();

    let (price_tx, mut price_rx) = tokio::sync::mpsc::channel(1);
    engine_tx
        .send(Message::GetPrice(trading_pair.clone(), price_tx))
        .await
        .unwrap();

    if let Some(price) = price_rx.recv().await.unwrap() {
        info!(price = price, "Curent price received: {}", price);
    }

    // run_cli().await;
}
