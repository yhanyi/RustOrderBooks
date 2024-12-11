use crate::engine::api::OrderBookEntry;
use crate::engine::models::{Order, PriceUpdate, TradingPair};
use crate::engine::order_book::{OrderBook, SimpleOrderBook};
use metrics::{register_counter, register_gauge, register_histogram, Counter, Gauge, Histogram};
use std::collections::HashMap;
use std::sync::Once;
use tokio::sync::mpsc;
use tracing::info;

static INIT: Once = Once::new();

pub enum Message {
    NewOrder(Order),
    PriceUpdate(PriceUpdate),
    MatchOrders(TradingPair),
    GetPrice(TradingPair, mpsc::Sender<Option<f64>>),
    GetOrderBook(
        TradingPair,
        mpsc::Sender<(Vec<OrderBookEntry>, Vec<OrderBookEntry>)>,
    ),
    Shutdown,
}

pub struct EngineMetrics {
    orders_processed: Counter,
    active_orders: Gauge,
    order_processing_duration: Histogram,
    match_processing_duration: Histogram,
    current_price: Gauge,
}

impl EngineMetrics {
    pub fn new() -> Self {
        Self {
            orders_processed: register_counter!("trading_engine_orders_processed_total"),
            active_orders: register_gauge!("trading_engine_active_orders"),
            order_processing_duration: register_histogram!(
                "trading_engine_order_processing_duration_ms"
            ),
            match_processing_duration: register_histogram!(
                "trading_engine_match_processing_duration_ms"
            ),
            current_price: register_gauge!("trading_engine_current_price"),
        }
    }
}

pub struct Engine {
    order_books: HashMap<TradingPair, Box<dyn OrderBook>>,
    metrics: EngineMetrics,
}

impl Engine {
    pub fn new() -> Self {
        INIT.call_once(|| {
            tracing_subscriber::fmt()
                .with_target(false)
                .with_thread_ids(true)
                .with_level(true)
                .with_file(true)
                .with_line_number(true)
                .with_env_filter("info")
                .init();

            let builder = metrics_exporter_prometheus::PrometheusBuilder::new();
            builder
                .install()
                .expect("Failed to install Prometheus recorder.");
        });

        Engine {
            order_books: HashMap::new(),
            metrics: EngineMetrics::new(),
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

        info!("Attempting to send price {:?} through channel", price);
        if let Err(e) = response_tx.send(price).await {
            eprintln!("Failed to send price response: {}", e);
        } else {
            info!("Successfully sent price through channel");
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

    pub async fn run(&mut self, mut rx: mpsc::Receiver<Message>) {
        info!("Starting engine");
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
                Message::GetOrderBook(trading_pair, response_tx) => {
                    self.process_get_order_book(trading_pair, response_tx).await;
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
                    self.process_get_price(trading_pair, response_tx).await;
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
