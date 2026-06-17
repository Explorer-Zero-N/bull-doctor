use std::sync::{Arc, RwLock as StdRwLock};

use serde_json::json;

/// Compression 嵌入式服务器句柄
pub struct CompressProcess {
    pub port: u16,
    shutdown: Arc<tokio::sync::Notify>,
}

/// Compression 句柄，线程安全，跟随 Doctor 生命周期。
pub type CompressHandle = Arc<StdRwLock<Option<CompressProcess>>>;

pub fn new_compress_handle() -> CompressHandle {
    Arc::new(StdRwLock::new(None))
}

/// 启动嵌入式 压缩代理服务器（在当前 tokio runtime 内）。
pub async fn start_compress(
    port: u16,
    helper_port: u16,
    handle: &CompressHandle,
) -> anyhow::Result<()> {
    {
        let guard = handle.read().unwrap();
        if guard.is_some() {
            anyhow::bail!("压缩代理已在运行中");
        }
    }

    let listen: std::net::SocketAddr = ([127, 0, 0, 1], port).into();
    let upstream = url::Url::parse(&format!("http://127.0.0.1:{}", helper_port))
        .map_err(|e| anyhow::anyhow!("无效的上游地址: {e}"))?;

    let mut config = headroom_proxy::Config::for_test(upstream);
    config.listen = listen;
    config.compression = true;
    config.compression_mode = headroom_proxy::config::CompressionMode::LiveZone;
    config.rewrite_host = true;
    config.strip_internal_headers = headroom_proxy::config::StripInternalHeaders::Enabled;

    let shutdown = headroom_proxy::spawn_server(config)
        .map_err(|e| anyhow::anyhow!("启动 Compression 失败: {e}"))?;

    {
        let mut guard = handle.write().unwrap();
        *guard = Some(CompressProcess { port, shutdown });
    }

    tracing::info!("压缩代理已启动，端口 {}", port);
    Ok(())
}

/// 停止 压缩代理。
pub async fn stop_compress(handle: &CompressHandle) -> anyhow::Result<()> {
    let info = {
        let mut guard = handle.write().unwrap();
        guard
            .take()
            .ok_or_else(|| anyhow::anyhow!("Compression 未在运行"))?
    };

    info.shutdown.notify_one();
    tracing::info!("压缩代理已停止（端口 {}）", info.port);
    Ok(())
}

/// 检查 compress 是否正在运行。
pub fn is_running(handle: &CompressHandle) -> bool {
    let guard = handle.read().unwrap();
    guard.is_some()
}

/// 获取 compress 状态。
pub fn compress_status(handle: &CompressHandle, port: u16) -> serde_json::Value {
    let guard = handle.read().unwrap();
    match guard.as_ref() {
        Some(info) => json!({
            "running": true,
            "port": info.port,
            "ready": true,
        }),
        None => json!({
            "running": false,
            "port": port,
            "ready": false,
        }),
    }
}
