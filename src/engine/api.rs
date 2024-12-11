use crate::engine::core::Message;
use crate::engine::models::{Order, OrderType, TradingPair};
use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::info;

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

#[derive(Debug, Serialize)]
pub struct OrderBookEntry {
    pub price: f64,
    pub quantity: f64,
}

#[derive(Debug, Serialize)]
pub struct OrderBookResponse {
    trading_pair: String,
    bids: Vec<OrderBookEntry>,
    asks: Vec<OrderBookEntry>,
    timestamp: String,
}

#[derive(Debug, Serialize)]
pub struct TradeResponse {
    pub id: u64,
    pub trading_pair: String,
    pub price: f64,
    pub quantity: f64,
    pub timestamp: String,
}

#[derive(Debug, Serialize)]
pub struct TradeHistoryResponse {
    pub trading_pair: String,
    pub trades: Vec<TradeResponse>,
}

#[derive(Clone)]
pub struct AppState {
    pub engine_tx: mpsc::Sender<Message>,
}

impl AppState {
    #[allow(dead_code)]
    pub fn new(engine_tx: mpsc::Sender<Message>) -> Self {
        Self { engine_tx }
    }
}

pub async fn run_api_server(engine_tx: mpsc::Sender<Message>) {
    let state = AppState {
        engine_tx: engine_tx.clone(),
    };

    let app = Router::new()
        .route("/order", post(place_order))
        .route("/price/:base/:quote", get(get_price))
        .route("/health", get(health_check))
        .route("/trades/:base/:quote", get(get_trade_history))
        .route("/orderbook/:base/:quote", get(get_order_book))
        .with_state(state);

    info!("Starting API server on 0.0.0.0:3000");

    let server =
        axum::Server::bind(&"0.0.0.0:3000".parse().unwrap()).serve(app.into_make_service());

    tokio::select! {
        result = server => {
            if let Err(e) = result {
                eprintln!("Server error: {}", e);
            }
            info!("Server stopped");

        }
        _ = tokio::signal::ctrl_c() => {
            info!("Shutdown signal received, stopping API server.");
            if let Err(e) = engine_tx.send(Message::Shutdown).await {
                info!("Engine already shut down: {}", e);
            } else {
                info!("Sent shutdown signal to engine");
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
    }
}

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
        id: 0,
        trading_pair,
        order_type,
        price: request.price,
        quantity: request.quantity,
        timestamp: chrono::Utc::now(),
    };

    if state.engine_tx.send(Message::NewOrder(order)).await.is_ok() {
        Json(PlaceOrderResponse {
            order_id: 0,
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

async fn get_order_book(
    State(state): State<AppState>,
    Path((base, quote)): Path<(String, String)>,
) -> Json<OrderBookResponse> {
    info!("Getting order book for pair: {}/{}", base, quote);
    let trading_pair = format!("{}/{}", base, quote);

    let trading_pair_parsed = match TradingPair::from_string(&trading_pair) {
        Ok(pair) => pair,
        Err(_) => {
            return Json(OrderBookResponse {
                trading_pair,
                bids: vec![],
                asks: vec![],
                timestamp: chrono::Utc::now().to_rfc3339(),
            });
        }
    };

    let (book_tx, mut book_rx) = mpsc::channel(1);

    match state
        .engine_tx
        .send(Message::GetOrderBook(trading_pair_parsed, book_tx))
        .await
    {
        Ok(_) => match book_rx.recv().await {
            Some((bids, asks)) => Json(OrderBookResponse {
                trading_pair,
                bids,
                asks,
                timestamp: chrono::Utc::now().to_rfc3339(),
            }),
            None => Json(OrderBookResponse {
                trading_pair,
                bids: vec![],
                asks: vec![],
                timestamp: chrono::Utc::now().to_rfc3339(),
            }),
        },
        Err(_) => Json(OrderBookResponse {
            trading_pair,
            bids: vec![],
            asks: vec![],
            timestamp: chrono::Utc::now().to_rfc3339(),
        }),
    }
}

async fn get_trade_history(
    State(state): State<AppState>,
    Path((base, quote)): Path<(String, String)>,
) -> Json<TradeHistoryResponse> {
    info!("Getting trade history for pair: {}/{}", base, quote);
    let trading_pair = format!("{}/{}", base, quote);

    let trading_pair_parsed = match TradingPair::from_string(&trading_pair) {
        Ok(pair) => pair,
        Err(_) => {
            return Json(TradeHistoryResponse {
                trading_pair,
                trades: vec![],
            });
        }
    };

    let (history_tx, mut history_rx) = mpsc::channel(1);

    match state
        .engine_tx
        .send(Message::GetTradeHistory(trading_pair_parsed, history_tx))
        .await
    {
        Ok(_) => match history_rx.recv().await {
            Some(trades) => {
                let trade_responses: Vec<TradeResponse> = trades
                    .into_iter()
                    .map(|trade| TradeResponse {
                        id: trade.id,
                        trading_pair: format!(
                            "{}/{}",
                            trade.trading_pair.base, trade.trading_pair.quote
                        ),
                        price: trade.price,
                        quantity: trade.quantity,
                        timestamp: trade.timestamp.to_rfc3339(),
                    })
                    .collect();

                Json(TradeHistoryResponse {
                    trading_pair,
                    trades: trade_responses,
                })
            }
            None => Json(TradeHistoryResponse {
                trading_pair,
                trades: vec![],
            }),
        },
        Err(_) => Json(TradeHistoryResponse {
            trading_pair,
            trades: vec![],
        }),
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

// TODO: Fix this
#[allow(dead_code)]
pub fn create_test_app(state: AppState) -> Router {
    Router::new()
        .route("/order", post(place_order))
        .route("/price/:base/:quote", get(get_price))
        .route("/orderbook/:base/:quote", get(get_order_book))
        .route("/trades/:base/:quote", get(get_trade_history))
        .route("/health", get(health_check))
        .with_state(state)
}
