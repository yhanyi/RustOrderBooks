use crate::engine::engine::{start_engine, Message};
use crate::engine::models::{Order, OrderType, TradingPair};
use clap::{Parser, Subcommand};
use std::io::{self};
use std::str::FromStr;
use tokio::sync::mpsc;
use tracing::{info, span, Level};

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Cli {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    PlaceOrder {
        #[clap(short, long)]
        pair: String,
        #[clap(short, long)]
        order_type: String,
        #[clap(short, long)]
        price: f64,
        #[clap(short, long)]
        quantity: f64,
    },
    GetPrice {
        #[clap(short, long)]
        pair: String,
    },
    StartSimulation,
}

pub async fn run_cli() {
    let builder = metrics_exporter_prometheus::PrometheusBuilder::new();
    builder
        .install()
        .expect("Failed to install Prometheus recorder");
    let cli = Cli::parse();
    let engine_tx = start_engine();

    match cli.command {
        Commands::PlaceOrder {
            pair,
            order_type,
            price,
            quantity,
        } => {
            info!(
                pair = ?pair,
                order_type = ?order_type,
                price = price,
                quantity = quantity,
                "Processing order command"
            );
            let trading_pair = match TradingPair::from_str(&pair) {
                Ok(tp) => tp,
                Err(e) => {
                    println!("Error: {}", e);
                    return;
                }
            };
            let order = Order {
                id: 0, // The engine will assign the actual ID
                trading_pair,
                order_type: match order_type.as_str() {
                    "buy" => OrderType::Buy,
                    "sell" => OrderType::Sell,
                    _ => {
                        println!("Invalid order type. Use 'buy' or 'sell'.");
                        return;
                    }
                },
                price,
                quantity,
                timestamp: chrono::Utc::now(),
            };
            if engine_tx.send(Message::NewOrder(order)).await.is_ok() {
                println!("Order placed successfully");
            } else {
                println!("Failed to place order");
            }
        }
        Commands::GetPrice { pair } => {
            let span = span!(Level::INFO, "get_price", pair = ?pair);
            let _enter = span.enter();
            let trading_pair = match TradingPair::from_str(&pair) {
                Ok(tp) => tp,
                Err(e) => {
                    println!("Error: {}", e);
                    return;
                }
            };
            let (price_tx, mut price_rx) = mpsc::channel(1);
            if engine_tx
                .send(Message::GetPrice(trading_pair, price_tx))
                .await
                .is_ok()
            {
                if let Some(price) = price_rx.recv().await.unwrap() {
                    println!("Current price for {}: {}", pair, price);
                } else {
                    println!("Price not available");
                }
            } else {
                println!("Failed to get price");
            }
        }
        Commands::StartSimulation => {
            info!("Starting trading simulation");
            println!("Trading simulation started. Metrics available on stdout.");
            println!("Press Enter to exit.");
            let mut input = String::new();
            io::stdin().read_line(&mut input).unwrap();
        }
    }
}
