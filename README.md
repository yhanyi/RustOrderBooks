# RustOrderBooks

This project explorings different concurrency models in Rust while learning the languages and discovering useful Rust libraries.

It implements three versions of a basic order book, along with integrationn tests and benchmarks using Criterion.

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

This project was done in December 2024 but cleaned up recently for readability and testing, and is not being maintained.

Thank you for checking out the project! :)
