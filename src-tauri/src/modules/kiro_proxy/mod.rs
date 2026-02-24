pub mod account_pool;
pub mod auth;
pub mod event_stream;
pub mod kiro_api;
pub mod server;
pub mod service;
pub mod stats;
pub mod storage;
pub mod translator;
pub mod types;

use std::sync::Arc;

pub use service::KiroProxyService;

pub async fn shared_service() -> Arc<KiroProxyService> {
    service::service().await
}