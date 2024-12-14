use crate::engine::api::OrderBookEntry;
use crate::engine::models::{Order, Trade, TradingPair};
use crate::engine::order_book::{OrderBook, SimpleOrderBook};
use std::collections::HashMap;
use tokio::sync::mpsc;
use tracing::info;

pub enum Message {
    NewOrder(Order),
    GetPrice(TradingPair, mpsc::Sender<Option<f64>>),
    GetOrderBook(
        TradingPair,
        mpsc::Sender<(Vec<OrderBookEntry>, Vec<OrderBookEntry>)>,
    ),
    GetTradeHistory(TradingPair, mpsc::Sender<Vec<Trade>>),
    Shutdown,
}

pub struct Engine {
    order_books: HashMap<TradingPair, Box<dyn OrderBook>>,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine {
    pub fn new() -> Self {
        tracing_subscriber::fmt()
            .with_target(false)
            .with_thread_ids(true)
            .with_level(true)
            .with_file(true)
            .with_line_number(true)
            .with_env_filter("info")
            .init();

        Engine {
            order_books: HashMap::new(),
        }
    }

    async fn process_get_price(
        &mut self,
        trading_pair: TradingPair,
        response_tx: mpsc::Sender<Option<f64>>,
    ) {
        info!("Processing get_price request for {:?}", trading_pair);

        let price = if let Some(order_book) = self.order_books.get(&trading_pair) {
            info!("Found existing order book");
            let price = order_book.get_current_price().await;
            info!("Got price from existing order book: {:?}", price);
            price
        } else {
            info!("Creating new order book");
            let order_book = SimpleOrderBook::new(trading_pair.clone());
            let price = order_book.get_current_price().await;
            info!("Got price from new order book: {:?}", price);
            self.order_books.insert(trading_pair, Box::new(order_book));
            price
        };

        if let Err(e) = response_tx.send(price).await {
            eprintln!("Failed to send price response: {}", e);
        }
    }

    async fn process_get_order_book(
        &mut self,
        trading_pair: TradingPair,
        response_tx: mpsc::Sender<(Vec<OrderBookEntry>, Vec<OrderBookEntry>)>,
    ) {
        if let Some(order_book) = self.order_books.get(&trading_pair) {
            let (bids, asks) = order_book.get_order_book().await;
            let _ = response_tx.send((bids, asks)).await;
        } else {
            let _ = response_tx.send((vec![], vec![])).await;
        }
    }

    async fn process_get_trade_history(
        &mut self,
        trading_pair: TradingPair,
        response_tx: mpsc::Sender<Vec<Trade>>,
    ) {
        if let Some(order_book) = self.order_books.get(&trading_pair) {
            let trades = order_book.get_trade_history().await;
            let _ = response_tx.send(trades).await;
        } else {
            let _ = response_tx.send(vec![]).await;
        }
    }

    pub async fn run(&mut self, mut rx: mpsc::Receiver<Message>) {
        info!("Starting engine.");
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
                Message::GetPrice(trading_pair, response_tx) => {
                    self.process_get_price(trading_pair, response_tx).await;
                }
                Message::GetOrderBook(trading_pair, response_tx) => {
                    self.process_get_order_book(trading_pair, response_tx).await;
                }
                Message::GetTradeHistory(trading_pair, response_tx) => {
                    self.process_get_trade_history(trading_pair, response_tx)
                        .await;
                }
                Message::Shutdown => {
                    info!("Received shutdown signal.");
                    break;
                }
            }
        }
        info!("Engine stopped.");
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
