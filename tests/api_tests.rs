mod common;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use common::{create_test_app, create_test_channel};
use engine::engine::api::AppState;
use serde_json::json;
use tower::ServiceExt;

#[tokio::test]
async fn test_health_check() {
    let state = AppState {
        engine_tx: create_test_channel(),
    };
    let app = create_test_app(state);

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
    let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
    assert_eq!(&body[..], b"OK");
}

#[tokio::test]
async fn test_place_order() {
    let state = AppState {
        engine_tx: create_test_channel(),
    };
    let app = create_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/order")
                .method("POST")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "trading_pair": "BTC/USD",
                        "order_type": "buy",
                        "price": 50000.0,
                        "quantity": 1.0
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
    let response: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(response["status"], "accepted");
}
