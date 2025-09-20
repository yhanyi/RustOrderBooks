use crate::models::{Order, OrderBook, OrderType, TradingPair};
use async_trait::async_trait;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use tokio::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq)]
struct Price(f64);

impl Eq for Price {}

impl PartialOrd for Price {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        if self.0.is_nan() && other.0.is_nan() {
            Some(Ordering::Equal)
        } else if self.0.is_nan() {
            Some(Ordering::Greater)
        } else if other.0.is_nan() {
            Some(Ordering::Less)
        } else {
            self.0.partial_cmp(&other.0)
        }
    }
}

impl Ord for Price {
    fn cmp(&self, other: &Self) -> Ordering {
        if self.0.is_nan() && other.0.is_nan() {
            Ordering::Equal
        } else if self.0.is_nan() {
            Ordering::Greater
        } else if other.0.is_nan() {
            Ordering::Less
        } else {
            self.0.partial_cmp(&other.0).unwrap_or(Ordering::Equal)
        }
    }
}

impl From<f64> for Price {
    fn from(price: f64) -> Self {
        Price(price)
    }
}

#[derive(Debug)]
struct PriceLevel {
    orders: Vec<Order>,
    total_quantity: f64,
}

impl PriceLevel {
    fn new() -> Self {
        Self {
            orders: Vec::new(),
            total_quantity: 0.0,
        }
    }

    fn add_order(&mut self, order: Order) {
        self.total_quantity += order.quantity;
        self.orders.push(order);
    }

    fn is_empty(&self) -> bool {
        self.orders.is_empty()
    }

    // Match against current price level, returns (matched_quantity, remaining_orders).
    fn match_order(&mut self, incoming_quantity: f64) -> f64 {
        let mut matched_quantity = 0.0;
        let mut remaining_quantity = incoming_quantity;

        while remaining_quantity > 0.0 && !self.orders.is_empty() {
            let resting_order = &mut self.orders[0];
            let match_qty = f64::min(remaining_quantity, resting_order.quantity);

            matched_quantity += match_qty;
            remaining_quantity -= match_qty;
            resting_order.quantity -= match_qty;
            self.total_quantity -= match_qty;

            // Remove order if fully filled.
            if resting_order.quantity <= 0.0 {
                let _filled_order = self.orders.remove(0);
                #[cfg(debug_assertions)]
                println!("Order {} fully filled", _filled_order.id);
            }
        }

        matched_quantity
    }
}

pub struct SimpleOrderBook {
    // Temporarily unused.
    _trading_pair: TradingPair,
    // Highest price first.
    buy_orders: Mutex<BTreeMap<Price, PriceLevel>>,
    // Lowest price first.
    sell_orders: Mutex<BTreeMap<Price, PriceLevel>>,
}

impl SimpleOrderBook {
    pub fn new(trading_pair: TradingPair) -> Self {
        Self {
            _trading_pair: trading_pair,
            buy_orders: Mutex::new(BTreeMap::new()),
            sell_orders: Mutex::new(BTreeMap::new()),
        }
    }

    async fn process_order(&self, order: Order) {
        match order.order_type {
            OrderType::Buy => self.process_buy_order(order).await,
            OrderType::Sell => self.process_sell_order(order).await,
        }
    }

    async fn process_buy_order(&self, mut buy_order: Order) {
        let mut sell_orders = self.sell_orders.lock().await;

        // Match against existing sell orders from lowest price first.
        let mut prices_to_remove = Vec::new();

        for (price, level) in sell_orders.iter_mut() {
            if buy_order.quantity <= 0.0 {
                break;
            }

            // Only match if buy price >= sell price.
            if buy_order.price >= price.0 {
                let matched_qty = level.match_order(buy_order.quantity);
                buy_order.quantity -= matched_qty;

                if level.is_empty() {
                    prices_to_remove.push(*price);
                }
            } else {
                // No more matching possible.
                break;
            }
        }

        // Clean up empty price levels.
        for price in prices_to_remove {
            sell_orders.remove(&price);
        }

        drop(sell_orders);

        // Add remaining quantity to buy side.
        if buy_order.quantity > 0.0 {
            let mut buy_orders = self.buy_orders.lock().await;
            buy_orders
                .entry(Price::from(buy_order.price))
                .or_insert_with(PriceLevel::new)
                .add_order(buy_order);
        }
    }

    async fn process_sell_order(&self, mut sell_order: Order) {
        let mut buy_orders = self.buy_orders.lock().await;

        // Match against existing buy orders from highest price first.
        let mut prices_to_remove = Vec::new();

        for (price, level) in buy_orders.iter_mut().rev() {
            if sell_order.quantity <= 0.0 {
                break;
            }

            // Only match if sell price <= buy price.
            if sell_order.price <= price.0 {
                let matched_qty = level.match_order(sell_order.quantity);
                sell_order.quantity -= matched_qty;

                if level.is_empty() {
                    prices_to_remove.push(*price);
                }
            } else {
                // No more matching possible.
                break;
            }
        }

        // Clean up empty price levels.
        for price in prices_to_remove {
            buy_orders.remove(&price);
        }

        drop(buy_orders);

        // Add remaining quantity to sell side.
        if sell_order.quantity > 0.0 {
            let mut sell_orders = self.sell_orders.lock().await;
            sell_orders
                .entry(Price::from(sell_order.price))
                .or_insert_with(PriceLevel::new)
                .add_order(sell_order);
        }
    }
}

// SimpleOrderBook implements the OrderBook trait.
#[async_trait]
impl OrderBook for SimpleOrderBook {
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
        let buy_orders = self.buy_orders.lock().await;
        let sell_orders = self.sell_orders.lock().await;

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
        let buy_orders = self.buy_orders.lock().await;
        let sell_orders = self.sell_orders.lock().await;

        let buy_count: usize = buy_orders.values().map(|level| level.orders.len()).sum();

        let sell_count: usize = sell_orders.values().map(|level| level.orders.len()).sum();

        buy_count + sell_count
    }
}
