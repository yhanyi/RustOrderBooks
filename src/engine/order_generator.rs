use crate::engine::engine::Message;
use crate::engine::models::{Order, OrderType, TradingPair};
use rand::rngs::StdRng;
use rand::Rng;
use rand::SeedableRng;
use tokio::time::{interval, Duration};

pub async fn run_order_generator(
    tx: tokio::sync::mpsc::Sender<Message>,
    trading_pair: TradingPair,
) {
    let mut interval = interval(Duration::from_millis(100));
    let mut order_id = 0;
    let mut rng = StdRng::from_entropy();

    loop {
        interval.tick().await;

        order_id += 1;
        let order_type = if rng.gen_bool(0.5) {
            OrderType::Buy
        } else {
            OrderType::Sell
        };
        let price = rng.gen_range(90.0..=110.0);
        let quantity = rng.gen_range(0.1..=10.0);

        let order = Order {
            id: order_id,
            trading_pair: trading_pair.clone(),
            order_type,
            price,
            quantity,
            timestamp: chrono::Utc::now(),
        };

        if tx.send(Message::NewOrder(order)).await.is_err() {
            break;
        }
    }
}
