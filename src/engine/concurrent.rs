use crate::engine::api::OrderBookEntry;
use crate::engine::models::{Order, OrderType, Trade, TradingPair};
use crate::engine::order_book::OrderBook;
use async_trait::async_trait;
use parking_lot::RwLock;
use std::cmp::Ordering;
use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;
use tokio::sync::mpsc;

#[derive(Debug, Clone, Copy, PartialEq)]
struct OrderPrice(f64);

impl Ord for OrderPrice {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.partial_cmp(&other.0).unwrap_or(Ordering::Equal)
    }
}

impl Eq for OrderPrice {}

impl PartialOrd for OrderPrice {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug)]
struct PriceLevel {
    orders: VecDeque<Order>,
    total_quantity: f64,
}

impl PriceLevel {
    fn new() -> Self {
        Self {
            orders: VecDeque::new(),
            total_quantity: 0.0,
        }
    }

    fn add_order(&mut self, order: Order) {
        self.total_quantity += order.quantity;
        self.orders.push_back(order);
    }

    fn try_match(&mut self, incoming_order: &Order, next_trade_id: u64) -> Option<Trade> {
        if self.orders.is_empty() {
            return None;
        }

        let resting_order = self.orders.front_mut()?;
        let match_quantity = f64::min(incoming_order.quantity, resting_order.quantity);

        if match_quantity <= 0.0 {
            return None;
        }

        resting_order.quantity -= match_quantity;
        self.total_quantity -= match_quantity;

        let resting_order = self.orders.front()?.clone();
        let match_quantity = f64::min(incoming_order.quantity, resting_order.quantity);

        if match_quantity <= 0.0 {
            return None;
        }

        let trade = Trade {
            id: next_trade_id,
            trading_pair: incoming_order.trading_pair.clone(),
            buy_order_id: if incoming_order.order_type == OrderType::Buy {
                incoming_order.id
            } else {
                resting_order.id
            },
            sell_order_id: if incoming_order.order_type == OrderType::Buy {
                resting_order.id
            } else {
                incoming_order.id
            },
            price: resting_order.price,
            quantity: match_quantity,
            timestamp: chrono::Utc::now(),
        };

        Some(trade)
    }
}

#[allow(dead_code)]
pub struct ConcurrentOrderBook {
    trading_pair: TradingPair,
    buy_levels: Arc<RwLock<BTreeMap<OrderPrice, Arc<RwLock<PriceLevel>>>>>,
    sell_levels: Arc<RwLock<BTreeMap<OrderPrice, Arc<RwLock<PriceLevel>>>>>,
    trade_tx: mpsc::UnboundedSender<Trade>,
    next_trade_id: Arc<std::sync::atomic::AtomicU64>,
}

impl ConcurrentOrderBook {
    #[allow(dead_code)]
    pub fn new(trading_pair: TradingPair) -> (Self, mpsc::UnboundedReceiver<Trade>) {
        let (trade_tx, trade_rx) = mpsc::unbounded_channel();

        (
            Self {
                trading_pair,
                buy_levels: Arc::new(RwLock::new(BTreeMap::new())),
                sell_levels: Arc::new(RwLock::new(BTreeMap::new())),
                trade_tx,
                next_trade_id: Arc::new(std::sync::atomic::AtomicU64::new(1)),
            },
            trade_rx,
        )
    }

    async fn process_order(&self, mut incoming_order: Order) -> Vec<Trade> {
        let mut trades = Vec::new();
        let (matching_levels, resting_levels) = match incoming_order.order_type {
            OrderType::Buy => (&self.sell_levels, &self.buy_levels),
            OrderType::Sell => (&self.buy_levels, &self.sell_levels),
        };

        {
            let levels = matching_levels.read();
            for level in levels.values() {
                if incoming_order.quantity <= 0.0 {
                    break;
                }

                let mut price_level = level.write();
                while incoming_order.quantity > 0.0 {
                    let trade_id = self
                        .next_trade_id
                        .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    match price_level.try_match(&incoming_order, trade_id) {
                        Some(trade) => {
                            incoming_order.quantity -= trade.quantity;
                            trades.push(trade);
                        }
                        None => break,
                    }
                }
            }
        }

        if incoming_order.quantity > 0.0 {
            let mut levels = resting_levels.write();
            let price_level = levels
                .entry(OrderPrice(incoming_order.price))
                .or_insert_with(|| Arc::new(RwLock::new(PriceLevel::new())));
            price_level.write().add_order(incoming_order);
        }

        trades
    }
}

#[async_trait]
impl OrderBook for ConcurrentOrderBook {
    async fn add_order(&self, order: Order) {
        let trades = self.process_order(order).await;
        for trade in trades {
            let _ = self.trade_tx.send(trade);
        }
    }

    async fn match_orders(&self) -> Vec<Trade> {
        Vec::new()
    }

    async fn get_current_price(&self) -> Option<f64> {
        let buy_orders = self.buy_levels.read();
        let sell_orders = self.sell_levels.read();

        match (buy_orders.keys().next_back(), sell_orders.keys().next()) {
            (Some(&OrderPrice(bid)), Some(&OrderPrice(ask))) => Some((bid + ask) / 2.0),
            (Some(&OrderPrice(bid)), None) => Some(bid),
            (None, Some(&OrderPrice(ask))) => Some(ask),
            (None, None) => Some(50000.0),
        }
    }

    async fn get_order_book(&self) -> (Vec<OrderBookEntry>, Vec<OrderBookEntry>) {
        let buy_levels = self.buy_levels.read();
        let sell_levels = self.sell_levels.read();

        let bids: Vec<OrderBookEntry> = buy_levels
            .iter()
            .rev()
            .map(|(&OrderPrice(price), level)| OrderBookEntry {
                price,
                quantity: level.read().total_quantity,
            })
            .collect();

        let asks: Vec<OrderBookEntry> = sell_levels
            .iter()
            .map(|(&OrderPrice(price), level)| OrderBookEntry {
                price,
                quantity: level.read().total_quantity,
            })
            .collect();

        (bids, asks)
    }

    async fn get_trade_history(&self) -> Vec<Trade> {
        Vec::new()
    }

    async fn get_active_orders_count(&self) -> usize {
        let buy_count: usize = self
            .buy_levels
            .read()
            .values()
            .map(|level| level.read().orders.len())
            .sum();

        let sell_count: usize = self
            .sell_levels
            .read()
            .values()
            .map(|level| level.read().orders.len())
            .sum();

        buy_count + sell_count
    }
}
