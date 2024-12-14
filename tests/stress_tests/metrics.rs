use std::sync::Arc;
use std::time::Duration;
use tracing::info;

#[derive(Clone)]
pub struct TestMetrics {
    pub orders_processed: Arc<std::sync::atomic::AtomicUsize>,
    pub trades_executed: Arc<std::sync::atomic::AtomicUsize>,
    pub price_checks: Arc<std::sync::atomic::AtomicUsize>,
    pub total_latency: Arc<std::sync::atomic::AtomicU64>,
    pub max_latency: Arc<std::sync::atomic::AtomicU64>,
    pub contention_count: Arc<std::sync::atomic::AtomicUsize>,
    pub order_queue_depth: Arc<std::sync::atomic::AtomicUsize>,
}

impl TestMetrics {
    pub fn new() -> Self {
        Self {
            orders_processed: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            trades_executed: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            price_checks: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            total_latency: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            max_latency: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            contention_count: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            order_queue_depth: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }

    pub fn record_latency(&self, latency: Duration) {
        self.total_latency.fetch_add(
            latency.as_micros() as u64,
            std::sync::atomic::Ordering::Relaxed,
        );

        let mut current_max = self.max_latency.load(std::sync::atomic::Ordering::Relaxed);
        loop {
            if latency.as_micros() as u64 <= current_max {
                break;
            }
            match self.max_latency.compare_exchange(
                current_max,
                latency.as_micros() as u64,
                std::sync::atomic::Ordering::Relaxed,
                std::sync::atomic::Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => current_max = actual,
            }
        }
    }

    pub fn report(&self, duration: Duration) -> String {
        let orders = self
            .orders_processed
            .load(std::sync::atomic::Ordering::Relaxed);
        let trades = self
            .trades_executed
            .load(std::sync::atomic::Ordering::Relaxed);
        let prices = self.price_checks.load(std::sync::atomic::Ordering::Relaxed);
        let total_latency = self
            .total_latency
            .load(std::sync::atomic::Ordering::Relaxed);
        let max_latency = self.max_latency.load(std::sync::atomic::Ordering::Relaxed);
        let avg_latency = if orders > 0 {
            total_latency as f64 / orders as f64
        } else {
            0.0
        };
        let contention_events = self
            .contention_count
            .load(std::sync::atomic::Ordering::Relaxed);
        let avg_queue_depth = self
            .order_queue_depth
            .load(std::sync::atomic::Ordering::Relaxed);

        let report = format!(
            "\n=== Stress Test Results ===\n\
            Duration: {:?}\n\
            Total orders processed: {}\n\
            Total trades executed: {}\n\
            Total price checks: {}\n\
            Orders/second: {:.2}\n\
            Trades/second: {:.2}\n\
            Price checks/second: {:.2}\n\
            Average latency: {:.2} μs\n\
            Maximum latency: {} μs\n\
            Trade/Order ratio: {:.2}%\n\
            Contention events: {}\n\
            Average queue depth: {}\n",
            duration,
            orders,
            trades,
            prices,
            orders as f64 / duration.as_secs_f64(),
            trades as f64 / duration.as_secs_f64(),
            prices as f64 / duration.as_secs_f64(),
            avg_latency,
            max_latency,
            (trades as f64 / orders as f64) * 100.0,
            contention_events,
            avg_queue_depth,
        );

        info!("{}", report);
        report
    }
}
