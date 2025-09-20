use crate::models::{Order, OrderBook, OrderType, TradingPair};
use async_trait::async_trait;
use crossbeam_skiplist::SkipMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

const PRECISION: u64 = 1_000_000;

#[derive(Clone)]
struct OrderMicroQuantity {
    order: Order,
    micro: u64,
}

struct AtomicPriceLevel {
    total_quantity: AtomicU64,
    order_count: AtomicUsize,
    head: crossbeam_queue::SegQueue<OrderMicroQuantity>,
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
        // Convert f64 to u64 for atomic operations
        let micro = (order.quantity * PRECISION as f64) as u64;
        self.total_quantity.fetch_add(micro, Ordering::AcqRel);
        self.order_count.fetch_add(1, Ordering::AcqRel);
        self.head.push(OrderMicroQuantity { order, micro });
    }

    fn try_match(&self, quantity_needed: f64) -> Option<(Order, f64)> {
        if self.order_count.load(Ordering::Acquire) == 0 {
            return None;
        }

        if let Some(mut order) = self.head.pop() {
            let quantity_needed = (quantity_needed * PRECISION as f64) as u64;
            let match_micro = u64::min(order.micro, quantity_needed);
            let match_quantity = match_micro as f64 / PRECISION as f64;

            self.total_quantity.fetch_sub(match_micro, Ordering::AcqRel);
            order.micro -= match_micro;
            order.order.quantity = order.micro as f64 / PRECISION as f64;

            if order.micro > 0 {
                // Put back the partially filled order.
                self.head.push(order.clone());
            } else {
                self.order_count.fetch_sub(1, Ordering::AcqRel);
                #[cfg(debug_assertions)]
                println!("Order {} fully filled", order.order.id);
            }

            Some((order.order, match_quantity))
        } else {
            None
        }
    }

    fn get_total_quantity(&self) -> f64 {
        let micro = self.total_quantity.load(Ordering::Acquire);
        micro as f64 / PRECISION as f64
    }

    fn get_order_count(&self) -> usize {
        self.order_count.load(Ordering::Acquire)
    }
}

pub struct LockFreeOrderBook {
    _trading_pair: TradingPair,
    buy_levels: SkipMap<u64, AtomicPriceLevel>,
    sell_levels: SkipMap<u64, AtomicPriceLevel>,
}

impl LockFreeOrderBook {
    pub fn new(trading_pair: TradingPair) -> Self {
        Self {
            _trading_pair: trading_pair,
            buy_levels: SkipMap::new(),
            sell_levels: SkipMap::new(),
        }
    }

    async fn process_order(&self, mut incoming_order: Order) {
        let (matching_levels, resting_levels) = match incoming_order.order_type {
            OrderType::Buy => (&self.sell_levels, &self.buy_levels),
            OrderType::Sell => (&self.buy_levels, &self.sell_levels),
        };

        // Convert price to bits for comparison.
        let order_price_bits = incoming_order.price.to_bits();

        // Try matching with existing orders.
        let mut matched_something = true;
        while incoming_order.quantity > 0.0 && matched_something {
            matched_something = false;

            // Collect matching price levels first to avoid iterator lifetime issues.
            let matching_prices: Vec<u64> = match incoming_order.order_type {
                OrderType::Buy => matching_levels
                    .iter()
                    .take_while(|entry| *entry.key() <= order_price_bits)
                    .map(|entry| *entry.key())
                    .collect(),
                OrderType::Sell => matching_levels
                    .iter()
                    .rev()
                    .take_while(|entry| *entry.key() >= order_price_bits)
                    .map(|entry| *entry.key())
                    .collect(),
            };

            for price_bits in matching_prices {
                if incoming_order.quantity <= 0.0 {
                    break;
                }

                if let Some(level_entry) = matching_levels.get(&price_bits) {
                    if let Some((_resting_order, match_quantity)) =
                        level_entry.value().try_match(incoming_order.quantity)
                    {
                        incoming_order.quantity -= match_quantity;
                        matched_something = true;
                        // Might be able to match more.
                        if match_quantity > 0.0 {
                            // Restart matching loop.
                            break;
                        }
                    }
                }
            }
        }

        // Add remaining orders to book.
        if incoming_order.quantity > 0.0 {
            let price_level =
                resting_levels.get_or_insert(order_price_bits, AtomicPriceLevel::new());
            price_level.value().add_order(incoming_order);
        }
    }
}

// LockFreeOrderBook implements the OrderBook trait.
#[async_trait]
impl OrderBook for LockFreeOrderBook {
    async fn add_order(&self, order: Order) {
        self.process_order(order).await;
    }

    async fn get_current_price(&self) -> Option<f64> {
        let (bid, ask) = self.get_best_bid_ask().await;
        match (bid, ask) {
            (Some(bid), Some(ask)) => Some((bid + ask) / 2.0),
            (Some(bid), None) => Some(bid),
            (None, Some(ask)) => Some(ask),
            (None, None) => None,
        }
    }

    async fn get_best_bid_ask(&self) -> (Option<f64>, Option<f64>) {
        let best_bid = self
            .buy_levels
            .iter()
            .next_back()
            .filter(|entry| entry.value().get_order_count() > 0)
            .map(|e| f64::from_bits(*e.key()));

        let best_ask = self
            .sell_levels
            .iter()
            .next()
            .filter(|entry| entry.value().get_order_count() > 0)
            .map(|e| f64::from_bits(*e.key()));

        (best_bid, best_ask)
    }

    async fn get_active_orders_count(&self) -> usize {
        let buy_count: usize = self
            .buy_levels
            .iter()
            .map(|entry| entry.value().get_order_count())
            .sum();

        let sell_count: usize = self
            .sell_levels
            .iter()
            .map(|entry| entry.value().get_order_count())
            .sum();

        buy_count + sell_count
    }
}
