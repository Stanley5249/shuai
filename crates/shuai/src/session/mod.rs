//! Connection lifecycle: queue, one concrete match, then rematch or requeue.
//!
//! [`Session`] owns the connection and the only `SocketReceiver`. Queue, gameplay,
//! and rematch borrow the receiver in turn, so no event is forwarded or read twice.
//! Gameplay is a plain async function passed to [`Session::run`], not a trait, and
//! receives those borrows in a [`MatchContext`].
//!
//! Gameplay reads events with [`next_event`], which applies one connection and
//! decoding policy to every phase. A normal gameplay return means `GameOver` was
//! observed. Everything else is a [`MatchError`], which tells Session whether the
//! match can be left safely.

mod context;
mod error;
mod events;
mod room;
mod run;
mod surrender;

pub use context::MatchContext;
pub use error::{LifecycleError, MatchError};
pub use events::{SignalError, next_event};
pub use room::QueueMode;
pub use run::{Session, cancel_on_ctrl_c};
pub use surrender::SurrenderOnce;

#[cfg(test)]
mod tests;
