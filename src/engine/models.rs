use chrono::{DateTime, Utc};
use std::str::FromStr;

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
    #[allow(dead_code)]
    pub fn new(base: String, quote: String) -> Self {
        TradingPair { base, quote }
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

impl FromStr for TradingPair {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        TradingPair::from_string(s)
    }
}

#[derive(Debug, Clone)]
pub struct Order {
    pub id: u64,
    pub trading_pair: TradingPair,
    pub order_type: OrderType,
    pub price: f64,
    pub quantity: f64,
    #[allow(dead_code)]
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct Trade {
    pub id: u64,
    pub trading_pair: TradingPair,
    #[allow(dead_code)]
    pub buy_order_id: u64,
    #[allow(dead_code)]
    pub sell_order_id: u64,
    pub price: f64,
    pub quantity: f64,
    pub timestamp: DateTime<Utc>,
}
