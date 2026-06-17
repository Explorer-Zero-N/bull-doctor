//! headroom-proxy library: transparent reverse proxy in front of the Python
//! Headroom proxy. Used by both `main.rs` and the integration tests.

pub mod bedrock;
pub mod cache_stabilization;
pub mod compression;
pub mod config;
pub mod error;
pub mod handlers;
pub mod headers;
pub mod health;
pub mod observability;
pub mod proxy;
pub mod responses_items;
pub mod sse;
pub mod vertex;
pub mod websocket;

pub use config::Config;
pub use error::ProxyError;
pub use proxy::{build_app, AppState};

use std::sync::Arc;

/// 在当前 tokio runtime 中启动 headroom-proxy 作为后台任务。
/// 返回一个 `Notify` 用于通知关闭。
pub fn spawn_server(
    config: Config,
) -> Result<Arc<tokio::sync::Notify>, ProxyError> {
    let state = AppState::new(config.clone())?;
    let app = build_app(state).into_make_service_with_connect_info::<std::net::SocketAddr>();
    let listen = config.listen;
    let shutdown = Arc::new(tokio::sync::Notify::new());
    let shutdown_clone = shutdown.clone();

    tokio::spawn(async move {
        let listener = match tokio::net::TcpListener::bind(listen).await {
            Ok(l) => l,
            Err(e) => {
                tracing::error!("Headroom 监听失败: {e}");
                return;
            }
        };

        tracing::info!("Headroom 代理已启动: http://{}", listen);

        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                shutdown_clone.notified().await;
                tracing::info!("Headroom 代理收到关闭信号");
            })
            .await
            .ok();
    });

    Ok(shutdown)
}
