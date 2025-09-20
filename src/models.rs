use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum OrderType {
    Buy,
    Sell,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TradingPair {
    pub base: String,
    pub quote: String,
}

impl TradingPair {
    pub fn new(base: impl Into<String>, quote: impl Into<String>) -> Self {
        Self {
            base: base.into(),
            quote: quote.into(),
        }
    }

    pub fn from_string(s: &str) -> Result<Self, String> {
        let parts: Vec<&str> = s.split('/').collect();
        if parts.len() != 2 {
            return Err("Invalid trading pair format. Use BASE/QUOTE".to_string());
        }
        Ok(TradingPair {
            base: parts[0].to_string(),
            quote: parts[1].to_string(),
        })
    }
}

impl fmt::Display for TradingPair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.base, self.quote)
    }
}

#[derive(Debug, Clone)]
pub struct Order {
    pub id: u64,
    pub trading_pair: TradingPair,
    pub order_type: OrderType,
    pub price: f64,
    pub quantity: f64,
}

impl Order {
    pub fn new(
        id: u64,
        trading_pair: TradingPair,
        order_type: OrderType,
        price: f64,
        quantity: f64,
    ) -> Self {
        Self {
            id,
            trading_pair,
            order_type,
            price,
            quantity,
        }
    }
}

// Minimal orderbook trait.
#[async_trait]
pub trait OrderBook: Send + Sync {
    // Adds an order to the book.
    async fn add_order(&self, order: Order);
    // Get the current mid price (average of best bid/ask)
    async fn get_current_price(&self) -> Option<f64>;
    // Get the best bid/ask prices.
    async fn get_best_bid_ask(&self) -> (Option<f64>, Option<f64>);
    // Get number of active orders.
    async fn get_active_orders_count(&self) -> usize;
}

// HTTP API types.
#[derive(Debug, Serialize, Deserialize)]
pub struct PlaceOrderRequest {
    pub trading_pair: String,
    pub order_type: String, // "buy" or "sell"
    pub price: f64,
    pub quantity: f64,
}

#[derive(Debug, Serialize)]
pub struct PlaceOrderResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct PriceResponse {
    pub trading_pair: String,
    pub price: Option<f64>,
    pub bid: Option<f64>,
    pub ask: Option<f64>,
}

impl PlaceOrderRequest {
    pub fn to_order(&self, id: u64) -> Result<Order, String> {
        let trading_pair = TradingPair::from_string(&self.trading_pair)?;

        let order_type = match self.order_type.to_lowercase().as_str() {
            "buy" => OrderType::Buy,
            "sell" => OrderType::Sell,
            _ => return Err(format!("Invalid order type: {}", self.order_type)),
        };

        if self.price <= 0.0 || self.quantity <= 0.0 {
            return Err("Price and quantity must be positive".to_string());
        }

        Ok(Order::new(
            id,
            trading_pair,
            order_type,
            self.price,
            self.quantity,
        ))
    }
}
