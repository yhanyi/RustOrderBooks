use crate::models::{OrderBook, PlaceOrderRequest, PlaceOrderResponse, PriceResponse, TradingPair};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use once_cell::sync::Lazy;
use rand::Rng;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{RwLock, Semaphore};
use tokio::task;

// Max limit on concurrent order processing.
static ORDER_PROCESSING_SEMAPHORE: Lazy<Semaphore> = Lazy::new(|| Semaphore::new(20));

// Global order ID counter.
static ORDER_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
pub struct AppState {
    orderbooks: Arc<RwLock<HashMap<TradingPair, Arc<dyn OrderBook>>>>,
    orderbook_factory: Arc<dyn Fn(TradingPair) -> Arc<dyn OrderBook> + Send + Sync>,
}

impl AppState {
    pub fn new<F>(factory: F) -> Self
    where
        F: Fn(TradingPair) -> Arc<dyn OrderBook> + Send + Sync + 'static,
    {
        Self {
            orderbooks: Arc::new(RwLock::new(HashMap::new())),
            orderbook_factory: Arc::new(factory),
        }
    }

    async fn get_or_create_orderbook(&self, trading_pair: TradingPair) -> Arc<dyn OrderBook> {
        // Try to get an existing orderbook (read lock).
        {
            let books = self.orderbooks.read().await;
            if let Some(book) = books.get(&trading_pair) {
                return book.clone();
            }
        }

        // Create new orderbook (write lock).
        let mut books = self.orderbooks.write().await;

        // Double check in case it was created while waiting.
        if let Some(book) = books.get(&trading_pair) {
            return book.clone();
        }

        let new_book = (self.orderbook_factory)(trading_pair.clone());
        books.insert(trading_pair, new_book.clone());
        new_book
    }
}

pub async fn create_server<F>(factory: F) -> Router
where
    F: Fn(TradingPair) -> Arc<dyn OrderBook> + Send + Sync + 'static,
{
    let state = AppState::new(factory);

    Router::new()
        .route("/health", get(health_check))
        .route("/order", post(place_order))
        .route("/price/:base/:quote", get(get_price))
        .with_state(state)
}

pub async fn start_server<F>(port: u16, factory: F)
where
    F: Fn(TradingPair) -> Arc<dyn OrderBook> + Send + Sync + 'static,
{
    let app = create_server(factory).await;
    let address = format!("0.0.0.0:{}", port);

    println!("Starting orderbook server on {}", address);
    println!("Endpoints:");
    println!("  GET  /health");
    println!("  POST /order");
    println!("  GET  /price/:base/:quote");

    let listener = tokio::net::TcpListener::bind(&address)
        .await
        .expect("Failed to bind to address");

    axum::serve(listener, app).await.expect("Server error");
}

async fn health_check() -> &'static str {
    "OK"
}

// Order placement and matching.
async fn place_order(
    State(state): State<AppState>,
    Json(request): Json<PlaceOrderRequest>,
) -> Result<Json<PlaceOrderResponse>, StatusCode> {
    // Validate request format first.
    let order_id = ORDER_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    let order = match request.to_order(order_id) {
        Ok(order) => order,
        Err(err) => {
            return Ok(Json(PlaceOrderResponse {
                success: false,
                message: err,
            }));
        }
    };

    let trading_pair = order.trading_pair.clone();

    // Acquire semaphore.
    let _permit = ORDER_PROCESSING_SEMAPHORE
        .acquire()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;

    // Execute order processing in blocking task.
    let orderbook = state.get_or_create_orderbook(trading_pair).await;
    let result = task::spawn_blocking(move || {
        simulate_cpu_latency();
        order
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Add order to orderbook.
    orderbook.add_order(result).await;

    Ok(Json(PlaceOrderResponse {
        success: true,
        message: "Order placed successfully".to_string(),
    }))
}

// Get price query.
async fn get_price(
    State(state): State<AppState>,
    Path((base, quote)): Path<(String, String)>,
) -> Json<PriceResponse> {
    let trading_pair_str = format!("{}/{}", base, quote);

    let trading_pair = match TradingPair::from_string(&trading_pair_str) {
        Ok(pair) => pair,
        Err(_) => {
            return Json(PriceResponse {
                trading_pair: trading_pair_str,
                price: None,
                bid: None,
                ask: None,
            });
        }
    };

    // Get orderbook.
    let orderbook = state.get_or_create_orderbook(trading_pair.clone()).await;
    let (bid, ask) = orderbook.get_best_bid_ask().await;
    let mid_price = orderbook.get_current_price().await;

    simulate_io_latency().await;

    Json(PriceResponse {
        trading_pair: trading_pair_str,
        price: mid_price,
        bid,
        ask,
    })
}

// Simulate CPU-intensive latency.
fn simulate_cpu_latency() {
    let mut sum = 0u64;
    for i in 0..1000 {
        sum = sum.wrapping_add(i * i);
    }
    std::hint::black_box(sum);
}

// Simulate I/O latency.
async fn simulate_io_latency() {
    let delay_ms = rand::thread_rng().gen_range(1..10);
    tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orderbooks::simple::SimpleOrderBook;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_health_endpoint() {
        let app = create_server(|pair| Arc::new(SimpleOrderBook::new(pair))).await;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_price_endpoint() {
        let app = create_server(|pair| Arc::new(SimpleOrderBook::new(pair))).await;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/price/BTC/USD")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }
}
