use crate::engine::models::{Order, PriceUpdate, TradingPair};
use crate::engine::order_book::{OrderBook, SimpleOrderBook};
use std::collections::HashMap;
use tokio::sync::mpsc;

pub enum Message {
    NewOrder(Order),
    PriceUpdate(PriceUpdate),
    MatchOrders(TradingPair),
    GetPrice(TradingPair, mpsc::Sender<Option<f64>>),
    Shutdown,
}

pub struct Engine {
    order_books: HashMap<TradingPair, Box<dyn OrderBook>>,
}

impl Engine {
    pub fn new() -> Self {
        Engine {
            order_books: HashMap::new(),
        }
    }

    pub async fn run(&mut self, mut rx: mpsc::Receiver<Message>) {
        while let Some(message) = rx.recv().await {
            match message {
                Message::NewOrder(order) => {
                    let order_book = self
                        .order_books
                        .entry(order.trading_pair.clone())
                        .or_insert_with(|| {
                            Box::new(SimpleOrderBook::new(order.trading_pair.clone()))
                        });
                    order_book.add_order(order).await;
                }
                Message::PriceUpdate(update) => {
                    println!("Price update: {:?}", update);
                }
                Message::MatchOrders(trading_pair) => {
                    if let Some(order_book) = self.order_books.get(&trading_pair) {
                        let trades = order_book.match_orders().await;
                        println!("Executed trades for {:?}: {:?}", trading_pair, trades);
                    }
                }
                Message::GetPrice(trading_pair, response_tx) => {
                    let price = if let Some(order_book) = self.order_books.get(&trading_pair) {
                        order_book.get_current_price().await
                    } else {
                        None
                    };
                    if let Err(e) = response_tx.send(price).await {
                        eprintln!("Failed to send price: {:?}", e);
                    }
                }
                Message::Shutdown => {
                    break;
                }
            }
        }
    }
}

pub fn start_engine() -> mpsc::Sender<Message> {
    let (tx, rx) = mpsc::channel(100);

    tokio::spawn(async move {
        let mut engine = Engine::new();
        engine.run(rx).await;
    });

    tx
}
