# RustOrderBooks

This project explores the basics of concurrency in Rust while experimenting with libraries like Tokio and Crossbeam.

It implements three versions of a basic order book, along with integration tests and Criterion benchmarks.

**SimpleOrderBook** (`simple.rs`)

- Mutex-based synchronisation.
- Uses `BTreeMap<Price, PriceLevel>` protected by `tokio::sync::Mutex`.
- Coarse-grained locking of entire order book during operations.

**ConcurrentOrderBook** (`concurrent.rs`)

- Fine-grained locking with reader-writer locks.
- Uses `Arc<RwLock<BTreeMap<OrderPrice, Arc<RwLock<PriceLevel>>>>>`.
- Uses two-level locking, with `RwLock` for price level map and individual `RwLock` per price level.

**LockFreeOrderBook** (`lockfree.rs`)

- Lock-free data structures (using crossbeam) with atomic operations.
- Uses `crossbeam_skiplist::SkipMap` with `AtomicPriceLevel`.
- Atomic operations and lock-free queues for price level with `crossbeam_queue::SegQueue`.

**Note**

This project was completed in December 2024 but was briefly cleaned up in September 2025 for readability and to pass GitHub Actions checks. It is no longer actively maintained.

Thank you for checking out this project! :)
