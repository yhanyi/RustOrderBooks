use metrics::{register_counter, register_gauge, register_histogram, Counter, Gauge, Histogram};

// TODO: Implement features and remove dead code
#[allow(dead_code)]
pub struct EngineMetrics {
    orders_processed: Counter,
    active_orders: Gauge,
    order_processing_duration: Histogram,
    match_processing_duration: Histogram,
    current_price: Gauge,
}

impl Default for EngineMetrics {
    fn default() -> Self {
        Self::new()
    }
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
