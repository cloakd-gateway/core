pub mod client;
pub mod failover;

pub use client::UpstreamClient;
#[allow(unused_imports)]
pub use failover::{is_failover_status, ForwardResult};
