use crate::engine::api::OrderBookEntry;
use crate::engine::models::{Order, OrderType, Trade, TradingPair};
use async_trait::async_trait;
use crossbeam_skiplist::SkipMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use tokio::sync::mpsc;

struct AtomicPriceLevel {
    total_quantity: AtomicU64,
    order_count: AtomicUsize,
    head: crossbeam_queue::SegQueue<Order>,
}

impl AtomicPriceLevel {
    fn new() -> Self {
        Self {
            total_quantity: AtomicU64::new(0),
            order_count: AtomicUsize::new(0),
            head: crossbeam_queue::SegQueue::new(),
        }
    }

    fn add_order(&self, order: Order) {
        // Convert f64 to u64 bits for atomic operations
        let quantity_bits = order.quantity.to_bits();
        self.total_quantity
            .fetch_add(quantity_bits, Ordering::AcqRel);
        self.order_count.fetch_add(1, Ordering::AcqRel);
        self.head.push(order);
    }

    fn try_match(&self, quantity_needed: f64) -> Option<(Order, f64)> {
        if self.order_count.load(Ordering::Acquire) == 0 {
            return None;
        }

        if let Some(mut order) = self.head.pop() {
            let match_quantity = f64::min(order.quantity, quantity_needed);
            let quantity_bits = match_quantity.to_bits();
            self.total_quantity
                .fetch_sub(quantity_bits, Ordering::AcqRel);

            order.quantity -= match_quantity;
            if order.quantity > 0.0 {
                self.head.push(order.clone());
            } else {
                self.order_count.fetch_sub(1, Ordering::AcqRel);
            }

            Some((order, match_quantity))
        } else {
            None
        }
    }

    fn get_total_quantity(&self) -> f64 {
        f64::from_bits(self.total_quantity.load(Ordering::Acquire))
    }
}

pub struct LockFreeOrderBook {
    trading_pair: TradingPair,
    buy_levels: SkipMap<u64, AtomicPriceLevel>, // Price converted to u64 bits
    sell_levels: SkipMap<u64, AtomicPriceLevel>,
    trade_tx: mpsc::UnboundedSender<Trade>,
    next_trade_id: AtomicU64,
}

impl LockFreeOrderBook {
    #[allow(dead_code)]
    pub fn new(trading_pair: TradingPair) -> (Self, mpsc::UnboundedReceiver<Trade>) {
        let (trade_tx, trade_rx) = mpsc::unbounded_channel();

        (
            Self {
                trading_pair,
                buy_levels: SkipMap::new(),
                sell_levels: SkipMap::new(),
                trade_tx,
                next_trade_id: AtomicU64::new(1),
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

        // Convert price to bits for comparison
        let order_price_bits = incoming_order.price.to_bits();

        // Try matching with existing orders
        while incoming_order.quantity > 0.0 {
            let matched = match incoming_order.order_type {
                OrderType::Buy => matching_levels
                    .iter()
                    .take_while(|entry| entry.key() <= &order_price_bits)
                    .next(),
                OrderType::Sell => matching_levels
                    .iter()
                    .rev()
                    .take_while(|entry| entry.key() >= &order_price_bits)
                    .next(),
            };

            if let Some(level_entry) = matched {
                if let Some((resting_order, match_quantity)) =
                    level_entry.value().try_match(incoming_order.quantity)
                {
                    incoming_order.quantity -= match_quantity;

                    let trade = Trade {
                        id: self.next_trade_id.fetch_add(1, Ordering::AcqRel),
                        trading_pair: self.trading_pair.clone(),
                        price: resting_order.price,
                        quantity: match_quantity,
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
                        timestamp: chrono::Utc::now(),
                    };
                    trades.push(trade);
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        // Add remaining order to book
        if incoming_order.quantity > 0.0 {
            let price_level =
                resting_levels.get_or_insert(order_price_bits, AtomicPriceLevel::new());
            price_level.value().add_order(incoming_order);
        }

        trades
    }
}

#[async_trait]
impl crate::engine::order_book::OrderBook for LockFreeOrderBook {
    async fn add_order(&self, order: Order) {
        let trades = self.process_order(order).await;
        for trade in trades {
            let _ = self.trade_tx.send(trade);
        }
    }

    async fn match_orders(&self) -> Vec<Trade> {
        Vec::new() // Real-time matching is done in process_order
    }

    async fn get_current_price(&self) -> Option<f64> {
        let best_bid = self
            .buy_levels
            .iter()
            .next_back()
            .map(|e| f64::from_bits(*e.key()));
        let best_ask = self
            .sell_levels
            .iter()
            .next()
            .map(|e| f64::from_bits(*e.key()));

        match (best_bid, best_ask) {
            (Some(bid), Some(ask)) => Some((bid + ask) / 2.0),
            (Some(bid), None) => Some(bid),
            (None, Some(ask)) => Some(ask),
            (None, None) => Some(50000.0),
        }
    }

    async fn get_order_book(&self) -> (Vec<OrderBookEntry>, Vec<OrderBookEntry>) {
        let bids: Vec<OrderBookEntry> = self
            .buy_levels
            .iter()
            .map(|entry| OrderBookEntry {
                price: f64::from_bits(*entry.key()),
                quantity: entry.value().get_total_quantity(),
            })
            .collect();

        let asks: Vec<OrderBookEntry> = self
            .sell_levels
            .iter()
            .map(|entry| OrderBookEntry {
                price: f64::from_bits(*entry.key()),
                quantity: entry.value().get_total_quantity(),
            })
            .collect();

        (bids, asks)
    }

    async fn get_trade_history(&self) -> Vec<Trade> {
        Vec::new() // Would need separate storage for trade history
    }

    async fn get_active_orders_count(&self) -> usize {
        let buy_count: usize = self
            .buy_levels
            .iter()
            .map(|entry| entry.value().order_count.load(Ordering::Acquire))
            .sum();

        let sell_count: usize = self
            .sell_levels
            .iter()
            .map(|entry| entry.value().order_count.load(Ordering::Acquire))
            .sum();

        buy_count + sell_count
    }
}
