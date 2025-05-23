# RustOrderBooks

Project that attempts to implement and benchmark three different order book architectures for trading.

1. **SimpleOrderBook** - Single-threaded with mutex-based synchronisation
2. **ConcurrentOrderBook** - Multi-threaded with fine-grained read-write locks
3. **LockFreeOrderBook** - Lock-free implementation using atomic operations

## Key Components

### Core Engine

- `models.rs`- Core data structures (Order, Trade, TradingPair)
- `core.rs` - Message-driven engine that orchestrates order processing
- `api.rs` - REST API server for order placement and market data
- `order_book.rs` SimpleOrderBook implementation and OrderBook trait
- `concurrent.rs` - ConcurrentOrderBook with hierarchical locking
- `lockfree.rs` - LockFreeOrderBook using atomic operations

## Benchmarking

- `mod.rs` - Main stress testing orchestration
- `simulator.rs` - Market participant simulators
- `metrics.rs` - Performance metrics collection
- `test/order_book_benchmarks.rs` - Runs all implementations through identical stress tests

## Architecture

### SimpleOrderBook

- Uses `BTreeMap<Price, Vec<Order>>` for price-time priority
- Single Mutex protects entire order book
- Simple but limited scalability

### ConcurrentOrderBook

- Hierarchical locking: outer `RwLock` for price levels, inner `RwLock` per level
- Allows concurrent access to different price levels
- Balances performance with complexity

### LockFreeOrderBook

- Uses `SkipMap` for concurrent price level access
- `SegQueue` for lock-free FIFO order queues within price levels
- Atomic operations for all shared state
- Maximum concurrency but highest complexity

Thanks for checking out the project!
