//! Generals.io Socket.IO client schema.
//!
//! Models client-visible application events with `sioc` payload derives.
//! Named payload fields are positional arguments; nested Serde structs model
//! object arguments. Source evidence and unresolved variants are maintained in
//! the project's generals-protocol skill.

pub mod client;
pub mod constants;
pub mod prelude;
pub mod server;

#[cfg(test)]
mod protocol_tests;
