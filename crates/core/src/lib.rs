//! Epicycle's financial domain: money, recurrence rules, subscriptions and the
//! calculations on them.
//!
//! Everything here is a pure function over immutable data. There is no I/O, no
//! storage, no UI and no clock: callers pass "today" explicitly, which keeps every
//! result reproducible in tests.

pub mod cashflow;
pub mod commitment;
pub mod money;
pub mod recurrence;
pub mod subscription;
