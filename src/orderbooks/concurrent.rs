use crate::models::{Order, OrderBook, OrderType, TradingPair};
use async_trait::async_trait;
use parking_lot::RwLock;
use std::cmp::Ordering;
use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

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

    fn is_empty(&self) -> bool {
        self.orders.is_empty()
    }

    fn try_match(&mut self, incoming_quantity: f64) -> f64 {
        let mut matched_quantity = 0.0;
        let mut remaining_quantity = incoming_quantity;

        while remaining_quantity > 0.0 && !self.orders.is_empty() {
            let resting_order = &mut self.orders[0];
            let match_qty = f64::min(remaining_quantity, resting_order.quantity);

            matched_quantity += match_qty;
            remaining_quantity -= match_qty;
            resting_order.quantity -= match_qty;
            self.total_quantity -= match_qty;

            if resting_order.quantity <= 0.0 {
                let _filled_order = self.orders.pop_front();
                #[cfg(debug_assertions)]
                if let Some(order) = _filled_order {
                    println!("Order {} fully filled", order.id);
                }
            }
        }

        matched_quantity
    }
}

pub struct ConcurrentOrderBook {
    _trading_pair: TradingPair,
    buy_levels: Arc<RwLock<BTreeMap<OrderPrice, Arc<RwLock<PriceLevel>>>>>,
    sell_levels: Arc<RwLock<BTreeMap<OrderPrice, Arc<RwLock<PriceLevel>>>>>,
}

impl ConcurrentOrderBook {
    pub fn new(trading_pair: TradingPair) -> Self {
        Self {
            _trading_pair: trading_pair,
            buy_levels: Arc::new(RwLock::new(BTreeMap::new())),
            sell_levels: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }

    async fn process_order(&self, mut incoming_order: Order) {
        let (matching_levels, resting_levels) = match incoming_order.order_type {
            OrderType::Buy => (&self.sell_levels, &self.buy_levels),
            OrderType::Sell => (&self.buy_levels, &self.sell_levels),
        };

        // Try to match with existing orders.
        {
            let levels = matching_levels.read();
            let mut levels_to_remove = Vec::new();

            for (price, level) in levels.iter() {
                if incoming_order.quantity <= 0.0 {
                    break;
                }

                // Match at this price level.
                let can_match = match incoming_order.order_type {
                    OrderType::Buy => incoming_order.price >= price.0,
                    OrderType::Sell => incoming_order.price <= price.0,
                };

                if can_match {
                    let mut price_level = level.write();
                    let matched_qty = price_level.try_match(incoming_order.quantity);
                    incoming_order.quantity -= matched_qty;

                    if price_level.is_empty() {
                        levels_to_remove.push(*price);
                    }
                } else {
                    // No more matching possible.
                    break;
                }
            }

            // Release read lock.
            drop(levels);

            // Remove empty price levels.
            if !levels_to_remove.is_empty() {
                let mut levels = matching_levels.write();
                for price in levels_to_remove {
                    levels.remove(&price);
                }
            }
        }

        // Add remaining quantity to the book.
        if incoming_order.quantity > 0.0 {
            let mut levels = resting_levels.write();
            let price_level = levels
                .entry(OrderPrice(incoming_order.price))
                .or_insert_with(|| Arc::new(RwLock::new(PriceLevel::new())));
            price_level.write().add_order(incoming_order);
        }
    }
}

#[async_trait]
impl OrderBook for ConcurrentOrderBook {
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
        let buy_orders = self.buy_levels.read();
        let sell_orders = self.sell_levels.read();

        let best_bid = buy_orders
            .iter()
            .next_back() // Highest price.
            .map(|(price, _)| price.0);

        let best_ask = sell_orders
            .iter()
            .next() // Lowest price.
            .map(|(price, _)| price.0);

        (best_bid, best_ask)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_concurrent_basic_operations() {
        let trading_pair = TradingPair::new("BTC", "USD");
        let orderbook = ConcurrentOrderBook::new(trading_pair.clone());

        assert_eq!(orderbook.get_active_orders_count().await, 0);
        assert_eq!(orderbook.get_current_price().await, None);

        let buy_order = Order::new(1, trading_pair.clone(), OrderType::Buy, 50000.0, 1.0);
        orderbook.add_order(buy_order).await;

        assert_eq!(orderbook.get_active_orders_count().await, 1);
        assert_eq!(orderbook.get_current_price().await, Some(50000.0));
    }

    #[tokio::test]
    async fn test_concurrent_matching() {
        let trading_pair = TradingPair::new("ETH", "USD");
        let orderbook = ConcurrentOrderBook::new(trading_pair.clone());

        let buy_order = Order::new(1, trading_pair.clone(), OrderType::Buy, 3000.0, 2.0);
        orderbook.add_order(buy_order).await;

        let sell_order = Order::new(2, trading_pair.clone(), OrderType::Sell, 3000.0, 1.0);
        orderbook.add_order(sell_order).await;

        assert_eq!(orderbook.get_active_orders_count().await, 1);
        let (bid, ask) = orderbook.get_best_bid_ask().await;
        assert_eq!(bid, Some(3000.0));
        assert_eq!(ask, None);
    }
}
