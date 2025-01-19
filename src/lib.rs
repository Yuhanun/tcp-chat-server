#![warn(
    clippy::pedantic,
    clippy::suspicious,
    clippy::perf,
    clippy::style,
    clippy::nursery,
    clippy::expect_used,
    clippy::unwrap_used
)]
#![allow(
    clippy::missing_errors_doc,
    // tokio::select !{} does this internally...
    clippy::redundant_pub_crate
)]
pub mod decoder;
pub mod encoder;
pub mod models;
pub mod server;
