pub mod account_pool;
pub mod api;
pub mod auth;
pub mod server;
pub mod service;
pub mod stats;
pub mod storage;
pub mod translator;
pub mod types;

use std::sync::Arc;

pub use service::AntigravityProxyService;

pub async fn shared_service() -> Arc<AntigravityProxyService> {
    service::service().await
}
