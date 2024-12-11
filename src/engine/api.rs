use crate::engine::engine::Message;
use crate::engine::models::{Order, OrderType, TradingPair};
use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::info;

// API Types
#[derive(Debug, Deserialize)]
pub struct PlaceOrderRequest {
    trading_pair: String,
    order_type: String,
    price: f64,
    quantity: f64,
}

#[derive(Debug, Serialize)]
pub struct PlaceOrderResponse {
    order_id: u64,
    status: String,
}

#[derive(Debug, Serialize)]
pub struct PriceResponse {
    trading_pair: String,
    price: Option<f64>,
    timestamp: String,
}

#[derive(Clone)]
pub struct AppState {
    engine_tx: mpsc::Sender<Message>,
}

pub async fn run_api_server(engine_tx: mpsc::Sender<Message>) {
    let state = AppState { engine_tx };

    let app = Router::new()
        .route("/order", post(place_order))
        .route("/price/:base/:quote", get(get_price))
        .route("/health", get(health_check))
        .with_state(state);

    info!("Starting API server on 0.0.0.0:3000");

    axum::Server::bind(&"0.0.0.0:3000".parse().unwrap())
        .serve(app.into_make_service())
        .await
        .unwrap();
}

// Handler functions
async fn place_order(
    State(state): State<AppState>,
    Json(request): Json<PlaceOrderRequest>,
) -> Json<PlaceOrderResponse> {
    info!(
        trading_pair = ?request.trading_pair,
        order_type = ?request.order_type,
        price = request.price,
        quantity = request.quantity,
        "Received order request"
    );

    let trading_pair = TradingPair::from_string(&request.trading_pair).unwrap();
    let order_type = match request.order_type.to_lowercase().as_str() {
        "buy" => OrderType::Buy,
        "sell" => OrderType::Sell,
        _ => panic!("Invalid order type"),
    };

    let order = Order {
        id: 0, // Engine will assign ID
        trading_pair,
        order_type,
        price: request.price,
        quantity: request.quantity,
        timestamp: chrono::Utc::now(),
    };

    if state.engine_tx.send(Message::NewOrder(order)).await.is_ok() {
        Json(PlaceOrderResponse {
            order_id: 0, // In a real system, we'd return the actual ID
            status: "accepted".to_string(),
        })
    } else {
        Json(PlaceOrderResponse {
            order_id: 0,
            status: "failed".to_string(),
        })
    }
}

async fn get_price(
    State(state): State<AppState>,
    Path((base, quote)): Path<(String, String)>,
) -> Json<PriceResponse> {
    info!("Getting price for pair: {}/{}", base, quote);
    let trading_pair = format!("{}/{}", base, quote);
    let trading_pair_parsed = match TradingPair::from_string(&trading_pair) {
        Ok(pair) => pair,
        Err(_) => {
            eprintln!("Failed to parse trading pair");
            return Json(PriceResponse {
                trading_pair,
                price: None,
                timestamp: chrono::Utc::now().to_rfc3339(),
            });
        }
    };

    let (price_tx, mut price_rx) = mpsc::channel(1);

    match state
        .engine_tx
        .send(Message::GetPrice(trading_pair_parsed, price_tx))
        .await
    {
        Ok(_) => {
            info!("GetPrice message sent, awaiting response");
            match price_rx.recv().await {
                Some(price) => {
                    info!("Received price response: {:?}", price);
                    let response = Json(PriceResponse {
                        trading_pair,
                        price,
                        timestamp: chrono::Utc::now().to_rfc3339(),
                    });
                    info!("Sending response to client: {:?}", response);
                    response
                }
                None => {
                    eprintln!("Price channel closed unexpectedly");
                    Json(PriceResponse {
                        trading_pair,
                        price: None,
                        timestamp: chrono::Utc::now().to_rfc3339(),
                    })
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to send price request: {}", e);
            Json(PriceResponse {
                trading_pair,
                price: None,
                timestamp: chrono::Utc::now().to_rfc3339(),
            })
        }
    }
}

async fn health_check() -> &'static str {
    "OK"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_price_response_serialization() {
        let response = PriceResponse {
            trading_pair: "BTC/USD".to_string(),
            price: Some(50000.0),
            timestamp: chrono::Utc::now().to_rfc3339(),
        };

        let serialized = serde_json::to_string(&response).unwrap();
        println!("Serialized response: {}", serialized);
        assert!(serialized.contains("trading_pair"));
        assert!(serialized.contains("price"));
    }
}
