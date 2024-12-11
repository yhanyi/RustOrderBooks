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

    fn try_match(&mut self, quantity: f64) -> Option<(Vec<Trade>, f64)> {
        if self.total_quantity == 0.0 {
            return None;
        }

        let mut matched_quantity = 0.0;
        let trades = Vec::new();

        while matched_quantity < quantity && !self.orders.is_empty() {
            let order = self.orders.front_mut().unwrap();
            let available = order.quantity;
            let match_qty = (quantity - matched_quantity).min(available);

            matched_quantity += match_qty;
            self.total_quantity -= match_qty;
            order.quantity -= match_qty;

            if order.quantity == 0.0 {
                self.orders.pop_front();
            }
        }

        if matched_quantity > 0.0 {
            Some((trades, matched_quantity))
        } else {
            None
        }
    }
}

#[allow(dead_code)]
pub struct ConcurrentOrderBook {
    trading_pair: TradingPair,
    // Use RwLock for the price level maps to allow concurrent reads
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

    async fn process_order(&self, order: Order) -> Vec<Trade> {
        let mut trades = Vec::new();
        let order_price = OrderPrice(order.price);

        let (matching_levels, resting_levels) = match order.order_type {
            OrderType::Buy => (&self.sell_levels, &self.buy_levels),
            OrderType::Sell => (&self.buy_levels, &self.sell_levels),
        };

        let mut remaining_qty = order.quantity;

        {
            let levels = matching_levels.read();
            for (&price, level) in levels.iter() {
                if remaining_qty <= 0.0 {
                    break;
                }

                if (order.order_type == OrderType::Buy && price.0 <= order.price)
                    || (order.order_type == OrderType::Sell && price.0 >= order.price)
                {
                    let mut level = level.write();
                    if let Some((mut new_trades, matched_qty)) = level.try_match(remaining_qty) {
                        remaining_qty -= matched_qty;
                        trades.append(&mut new_trades);
                    }
                }
            }
        }

        if remaining_qty > 0.0 {
            let mut remaining_order = order.clone();
            remaining_order.quantity = remaining_qty;

            let mut levels = resting_levels.write();
            let level = levels
                .entry(order_price)
                .or_insert_with(|| Arc::new(RwLock::new(PriceLevel::new())));

            level.write().add_order(remaining_order);
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
