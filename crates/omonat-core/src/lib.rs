//! Omonat core: lot ledger, Policy Engine (I1–I13), shartnomalar, taqsimot, hash chain.
//!
//! Sof domen mantig'i — IO yo'q. Server va store shu crate ustiga quriladi.

pub mod builders;
pub mod chain;
pub mod contract;
pub mod engine;
pub mod event;
pub mod model;
pub mod money;
pub mod policy;
pub mod state;
pub mod trace;
pub mod views;

pub use engine::{CommandError, Decision, Engine, Outcome};
pub use event::Event;
pub use money::Money;

use chrono::{DateTime, FixedOffset, NaiveDate, Utc};

/// Toshkent vaqti (UTC+05:00).
pub fn tashkent() -> FixedOffset {
    FixedOffset::east_opt(5 * 3600).expect("valid offset")
}

/// Hodisa vaqtining Toshkent bo'yicha sanasi (to'lov muddatlari va kunlik anchor shu bo'yicha).
pub fn local_date(at: DateTime<Utc>) -> NaiveDate {
    at.with_timezone(&tashkent()).date_naive()
}
