use crate::engine::api::OrderBookEntry;
use crate::engine::models::{Order, Trade, TradingPair};
use async_trait::async_trait;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use tokio::sync::Mutex;
use tracing::{info, instrument};

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

#[async_trait]
pub trait OrderBook: Send + Sync {
    async fn add_order(&self, order: Order);
    #[allow(dead_code)]
    async fn match_orders(&self) -> Vec<Trade>;
    async fn get_current_price(&self) -> Option<f64>;
    async fn get_order_book(&self) -> (Vec<OrderBookEntry>, Vec<OrderBookEntry>);
    async fn get_trade_history(&self) -> Vec<Trade>;
    #[allow(dead_code)]
    async fn get_active_orders_count(&self) -> usize;
    #[allow(dead_code)]
    async fn cleanup(&self) {}
}

pub struct SimpleOrderBook {
    trading_pair: TradingPair,
    buy_orders: Mutex<BTreeMap<OrderPrice, Vec<Order>>>,
    sell_orders: Mutex<BTreeMap<OrderPrice, Vec<Order>>>,
    trade_history: Mutex<Vec<Trade>>,
}

impl SimpleOrderBook {
    pub fn new(trading_pair: TradingPair) -> Self {
        SimpleOrderBook {
            trading_pair,
            buy_orders: Mutex::new(BTreeMap::new()),
            sell_orders: Mutex::new(BTreeMap::new()),
            trade_history: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl OrderBook for SimpleOrderBook {
    #[instrument(skip(self))]
    async fn add_order(&self, order: Order) {
        let start = std::time::Instant::now();
        let orders = match order.order_type {
            crate::engine::models::OrderType::Buy => &self.buy_orders,
            crate::engine::models::OrderType::Sell => &self.sell_orders,
        };

        let mut orders = orders.lock().await;
        orders
            .entry(OrderPrice(order.price))
            .or_insert_with(Vec::new)
            .push(order);

        info!(
            duration_ms = ?start.elapsed().as_millis(),
            "Order added to order book."
        );
    }

    async fn match_orders(&self) -> Vec<Trade> {
        let mut buy_orders = self.buy_orders.lock().await;
        let mut sell_orders = self.sell_orders.lock().await;
        let mut trades = Vec::new();

        loop {
            let buy_max = buy_orders
                .keys()
                .next_back()
                .map(|&OrderPrice(price)| price);
            let sell_min = sell_orders.keys().next().map(|&OrderPrice(price)| price);

            match (buy_max, sell_min) {
                (Some(buy_price), Some(sell_price)) if buy_price >= sell_price => {
                    let buy_list = buy_orders.get_mut(&OrderPrice(buy_price)).unwrap();
                    let sell_list = sell_orders.get_mut(&OrderPrice(sell_price)).unwrap();

                    let mut i = 0;
                    let mut j = 0;

                    while i < buy_list.len() && j < sell_list.len() {
                        let buy = &mut buy_list[i];
                        let sell = &mut sell_list[j];
                        let trade_quantity = buy.quantity.min(sell.quantity);

                        trades.push(Trade {
                            id: (trades.len() as u64) + 1,
                            trading_pair: self.trading_pair.clone(),
                            buy_order_id: buy.id,
                            sell_order_id: sell.id,
                            price: sell_price,
                            quantity: trade_quantity,
                            timestamp: chrono::Utc::now(),
                        });

                        buy.quantity -= trade_quantity;
                        sell.quantity -= trade_quantity;

                        if buy.quantity == 0.0 {
                            i += 1;
                        }
                        if sell.quantity == 0.0 {
                            j += 1;
                        }
                    }

                    buy_list.drain(0..i);
                    sell_list.drain(0..j);

                    if buy_list.is_empty() {
                        buy_orders.remove(&OrderPrice(buy_price));
                    }
                    if sell_list.is_empty() {
                        sell_orders.remove(&OrderPrice(sell_price));
                    }
                }
                _ => {
                    break;
                }
            }
        }

        let mut history = self.trade_history.lock().await;
        for trade in &trades {
            history.push(trade.clone());
        }

        trades
    }

    async fn get_trade_history(&self) -> Vec<Trade> {
        let history = self.trade_history.lock().await;
        let mut result = Vec::new();
        for trade in history.iter() {
            result.push(trade.clone());
        }
        result
    }

    async fn get_current_price(&self) -> Option<f64> {
        info!("Getting current price from order book");
        let buy_orders = self.buy_orders.lock().await;
        let sell_orders = self.sell_orders.lock().await;

        let price = match (buy_orders.keys().next_back(), sell_orders.keys().next()) {
            (Some(&OrderPrice(bid)), Some(&OrderPrice(ask))) => {
                info!("Found bid and ask prices");
                Some((bid + ask) / 2.0)
            }
            (Some(&OrderPrice(bid)), None) => {
                info!("Found only bid price");
                Some(bid)
            }
            (None, Some(&OrderPrice(ask))) => {
                info!("Found only ask price");
                Some(ask)
            }
            (None, None) => {
                info!("No orders found, returning default price");
                Some(50000.0)
            }
        };
        info!("Returning price: {:?}", price);
        price
    }

    async fn get_order_book(&self) -> (Vec<OrderBookEntry>, Vec<OrderBookEntry>) {
        let buy_orders = self.buy_orders.lock().await;
        let sell_orders = self.sell_orders.lock().await;

        let bids: Vec<OrderBookEntry> = buy_orders
            .iter()
            .rev()
            .map(|(&OrderPrice(price), orders)| {
                let quantity: f64 = orders.iter().map(|order| order.quantity).sum();
                OrderBookEntry { price, quantity }
            })
            .collect();

        let asks: Vec<OrderBookEntry> = sell_orders
            .iter()
            .map(|(&OrderPrice(price), orders)| {
                let quantity: f64 = orders.iter().map(|order| order.quantity).sum();
                OrderBookEntry { price, quantity }
            })
            .collect();

        (bids, asks)
    }

    async fn get_active_orders_count(&self) -> usize {
        let buy_count = self
            .buy_orders
            .lock()
            .await
            .values()
            .map(|orders| orders.len())
            .sum::<usize>();

        let sell_count = self
            .sell_orders
            .lock()
            .await
            .values()
            .map(|orders| orders.len())
            .sum::<usize>();

        buy_count + sell_count
    }
}
