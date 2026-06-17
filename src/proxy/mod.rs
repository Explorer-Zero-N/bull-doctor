use std::sync::{Arc, RwLock as StdRwLock};

use axum::{
    body::Body,
    extract::State,
    http::{HeaderMap, HeaderValue, Method, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::{any, get, post},
    Json, Router,
};
use reqwest::Client;
use tokio::sync::RwLock;
use tower_http::trace::TraceLayer;
use tracing::{info, warn};

use crate::claude::desktop_gateway::{
    build_inference_models, request_uses_desktop_roles, rewrite_model_field, rewrite_request_model,
    DESKTOP_ROLE_SONNET,
};
use crate::config::{self, AppConfig, ProviderConfig};
use serde_json::Value;

mod anthropic_to_chat;
mod chat_to_anthropic;
mod logged_stream;
mod message_repair;
mod reasoning_options;
mod sse;
mod upstream_retry;
use anthropic_to_chat::{convert_anthropic_to_chat_with_options, ConvertOptions};
use chat_to_anthropic::{
    anthropic_stream_preamble, convert_chat_json_to_anthropic, AnthropicSseTranslator,
};
use logged_stream::LoggingByteStream;
use crate::logs::{logs_bootstrap, logs_clear, logs_page};
use crate::stats::{stats_bootstrap, stats_page};
use crate::request_log::{
    extract_model_from_body, parse_usage_from_json, PendingRequest, RequestLogStore,
};
use crate::settings::{
    brand_icon_svg, settings_bootstrap, settings_check_config, settings_clear_all,
    settings_fetch_models, settings_page, settings_save, settings_test,
    settings_skills_bootstrap, settings_skills_install, settings_skills_install_local, settings_skills_uninstall,
    settings_skills_sync,
    compress_status, compress_start, compress_stop, compress_stats,
};

type TrayHealthCheckHook = Arc<dyn Fn() + Send + Sync>;

#[derive(Clone)]
pub struct ProxyState {
    pub config: Arc<RwLock<AppConfig>>,
    /// 非流式上游请求（总超时见 `DEFAULT_UPSTREAM_REQUEST_TIMEOUT_SECS`）。
    pub client: Client,
    /// 流式上游请求（无总超时，读空闲超时见 `DEFAULT_UPSTREAM_STREAM_READ_IDLE_TIMEOUT_SECS`）。
    pub streaming_client: Client,
    pub request_log: RequestLogStore,
    tray_health_check: Arc<StdRwLock<Option<TrayHealthCheckHook>>>,
    /// Compression 子进程句柄
    pub compress_process: crate::compress::CompressHandle,
}

pub fn spawn_server(config: AppConfig) -> anyhow::Result<Arc<ProxyState>> {
    let (client, streaming_client) = config::build_proxy_upstream_clients()
        .expect("failed to build upstream HTTP clients");
    let state = Arc::new(ProxyState {
        config: Arc::new(RwLock::new(config.clone())),
        client,
        streaming_client,
        request_log: RequestLogStore::new(),
        tray_health_check: Arc::new(StdRwLock::new(None)),
        compress_process: crate::compress::new_compress_handle(),
    });
    let addr = format!("{}:{}", config.proxy.host, config.proxy.port);
    let serve_state = state.clone();
    tokio::spawn(async move {
        if let Err(err) = run_listener(serve_state, &addr).await {
            tracing::error!("代理异常退出: {err:#}");
        }
    });
    Ok(state)
}

pub async fn notify_running_proxy_reload(app: &AppConfig) -> bool {
    let url = format!(
        "http://{}:{}/admin/reload",
        app.proxy.host, app.proxy.port
    );
    let client = match Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
    {
        Ok(c) => c,
        Err(_) => return false,
    };
    match client.post(&url).send().await {
        Ok(resp) => resp.status().is_success(),
        Err(_) => false,
    }
}

pub async fn reload_config_in_state(state: &ProxyState) -> anyhow::Result<AppConfig> {
    let app = AppConfig::load()?;
    let mut cfg = state.config.write().await;
    *cfg = app.clone();
    Ok(app)
}

pub fn register_tray_health_check(state: &Arc<ProxyState>, hook: TrayHealthCheckHook) {
    if let Ok(mut slot) = state.tray_health_check.write() {
        *slot = Some(hook);
    }
}

pub fn request_tray_health_check(state: &ProxyState) {
    let hook = state
        .tray_health_check
        .read()
        .ok()
        .and_then(|slot| slot.clone());
    if let Some(hook) = hook {
        hook();
    }
}

pub async fn start_server(config: AppConfig) -> anyhow::Result<()> {
    let addr = format!("{}:{}", config.proxy.host, config.proxy.port);
    let (client, streaming_client) = config::build_proxy_upstream_clients()?;
    let state = ProxyState {
        config: Arc::new(RwLock::new(config.clone())),
        client,
        streaming_client,
        request_log: RequestLogStore::new(),
        tray_health_check: Arc::new(StdRwLock::new(None)),
        compress_process: crate::compress::new_compress_handle(),
    };

    // Compression 自动启动
    if config.compress.auto_start {
        let hr_handle = state.compress_process.clone();
        let compress_port = config.compress.port;
        let helper_port = config.proxy.port;
        tokio::spawn(async move {
            if let Err(e) = crate::compress::start_compress(compress_port, helper_port, &hr_handle).await {
                tracing::error!("Compression 自动启动失败: {e}");
            }
        });
    }

    run_listener(Arc::new(state), &addr).await
}

async fn run_listener(state: Arc<ProxyState>, addr: &str) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/health", get(health))
        .route("/admin/reload", post(admin_reload))
        .route("/admin/settings", get(settings_page))
        .route("/admin/brand-icon.svg", get(brand_icon_svg))
        .route("/admin/settings/bootstrap", get(settings_bootstrap))
        .route("/admin/settings/save", post(settings_save))
        .route("/admin/settings/clear-all", post(settings_clear_all))
        .route("/admin/settings/test", post(settings_test))
        .route("/admin/settings/fetch-models", post(settings_fetch_models))
        .route("/admin/settings/check-config", get(settings_check_config))
        .route("/admin/logs", get(logs_page))
        .route("/admin/logs/bootstrap", get(logs_bootstrap))
        .route("/admin/logs/clear", post(logs_clear))
        .route("/admin/stats", get(stats_page))
        .route("/admin/stats/bootstrap", get(stats_bootstrap))
        // Skills 管理 API
        .route("/admin/skills/bootstrap", get(settings_skills_bootstrap))
        .route("/admin/skills/install", post(settings_skills_install))
        .route("/admin/skills/install-local", post(settings_skills_install_local))
        .route("/admin/skills/uninstall", post(settings_skills_uninstall))
        .route("/admin/skills/sync", post(settings_skills_sync))
        // 上下文压缩代理管理
        .route("/admin/compress/status", get(compress_status))
        .route("/admin/compress/start", post(compress_start))
        .route("/admin/compress/stop", post(compress_stop))
        .route("/admin/compress/stats", get(compress_stats))
        .route("/v1/models", get(list_models))
        .route("/v1/messages", post(proxy_messages))
        .route("/claude-desktop/v1/models", get(list_desktop_models))
        .route("/claude-desktop/v1/messages", post(proxy_desktop_messages))
        // OpenClaw / Hermes 子路径代理
        .route("/openclaw/v1/chat/completions", post(proxy_openclaw))
        .route("/openclaw/v1/models", get(list_openclaw_models))
        .route("/hermes/v1/chat/completions", post(proxy_hermes))
        .route("/hermes/v1/models", get(list_hermes_models))
        .fallback(any(catch_all))
        .layer(TraceLayer::new_for_http())
        .with_state(state.as_ref().clone());

    info!("Bull Doctor 代理已启动: http://{addr}");

    let listener = tokio::net::TcpListener::bind(addr).await.map_err(|e| {
        anyhow::anyhow!("无法绑定端口 {addr}: {e}。请检查端口是否被占用。")
    })?;

    axum::serve(listener, app).await?;
    Ok(())
}

async fn admin_reload(State(state): State<ProxyState>) -> impl IntoResponse {
    match reload_config_in_state(&state).await {
        Ok(app) => {
            let provider_name = app
                .active_provider()
                .map(|p| p.name.clone())
                .unwrap_or_else(|_| "unknown".into());
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "ok",
                    "active": app.active,
                    "provider": provider_name,
                })),
            )
                .into_response()
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": err.to_string() })),
        )
            .into_response(),
    }
}

async fn health(State(state): State<ProxyState>) -> impl IntoResponse {
    let config = state.config.read().await;
    let active = config.active.clone();
    let provider = config
        .active_provider()
        .map(|p| p.name.clone())
        .unwrap_or_else(|_| "unknown".into());
    axum::Json(serde_json::json!({
        "status": "ok",
        "active": active,
        "provider": provider,
    }))
}

fn gateway_models_payload(provider: &config::ProviderConfig) -> serde_json::Value {
    let models = build_inference_models(provider)
        .into_iter()
        .map(|entry: serde_json::Value| {
            let id = entry
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or(DESKTOP_ROLE_SONNET);
            let display = entry
                .get("labelOverride")
                .and_then(|v| v.as_str())
                .unwrap_or(id);
            serde_json::json!({
                "id": id,
                "display_name": display,
                "type": "model",
            })
        })
        .collect::<Vec<_>>();

    serde_json::json!({ "data": models })
}

async fn list_models(State(state): State<ProxyState>) -> impl IntoResponse {
    gateway_models_response(&state).await
}

async fn list_desktop_models(State(state): State<ProxyState>) -> impl IntoResponse {
    gateway_models_response(&state).await
}

async fn gateway_models_response(state: &ProxyState) -> Response {
    let config = state.config.read().await;
    let provider = match config.active_provider() {
        Ok(p) => p,
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                axum::Json(serde_json::json!({ "error": err.to_string() })),
            )
                .into_response();
        }
    };

    // For Ollama, fetch models directly from the API
    if provider.id == "ollama" {
        match fetch_ollama_models(&state.streaming_client, provider).await {
            Ok(models) => axum::Json(serde_json::json!({ "data": models })).into_response(),
            Err(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(serde_json::json!({ "error": err.to_string() })),
            )
                .into_response(),
        }
    } else {
        axum::Json(gateway_models_payload(provider)).into_response()
    }
}

async fn proxy_messages(
    State(state): State<ProxyState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    proxy_anthropic_inner(&state, headers, body, false, config::TOOL_CLAUDE_CODE).await
}

async fn proxy_desktop_messages(
    State(state): State<ProxyState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    proxy_anthropic_inner(&state, headers, body, true, config::TOOL_CLAUDE_DESKTOP).await
}

/// OpenClaw / Hermes 的 Chat Completions 透传代理
async fn proxy_openclaw(
    State(state): State<ProxyState>,
    _headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    proxy_chat_passthrough(&state, _headers, body, config::TOOL_OPENCLAW).await
}

async fn proxy_hermes(
    State(state): State<ProxyState>,
    _headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    proxy_chat_passthrough(&state, _headers, body, config::TOOL_HERMES).await
}

async fn list_openclaw_models(State(state): State<ProxyState>) -> impl IntoResponse {
    gateway_models_response_for(&state, config::TOOL_OPENCLAW).await
}

async fn list_hermes_models(State(state): State<ProxyState>) -> impl IntoResponse {
    gateway_models_response_for(&state, config::TOOL_HERMES).await
}

async fn gateway_models_response_for(state: &ProxyState, tool_id: &str) -> Response {
    let config = state.config.read().await;
    let provider = match config.active_provider_for(tool_id) {
        Ok(p) => p.clone(),
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                axum::Json(serde_json::json!({ "error": err.to_string() })),
            )
                .into_response();
        }
    };
    if provider.id == "ollama" {
        match fetch_ollama_models(&state.streaming_client, &provider).await {
            Ok(models) => axum::Json(serde_json::json!({ "data": models })).into_response(),
            Err(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(serde_json::json!({ "error": err.to_string() })),
            )
                .into_response(),
        }
    } else {
        axum::Json(gateway_models_payload(&provider)).into_response()
    }
}

async fn proxy_anthropic_inner(
    state: &ProxyState,
    headers: HeaderMap,
    body: axum::body::Bytes,
    map_desktop_roles: bool,
    tool_id: &str,
) -> Response {
    let config = state.config.read().await.clone();
    let provider = match config.active_provider_for(tool_id) {
        Ok(p) => p.clone(),
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": err.to_string() })),
            )
                .into_response();
        }
    };

    let body = if map_desktop_roles || request_uses_desktop_roles(&body) {
        match rewrite_request_model(&body, &provider) {
            Ok(rewritten) => axum::body::Bytes::from(rewritten),
            Err(err) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "type": "error",
                        "error": {
                            "type": "invalid_request_error",
                            "message": format!("模型映射失败: {err}")
                        }
                    })),
                )
                    .into_response();
            }
        }
    } else {
        // 非桌面角色请求：始终注入当前 provider 的 upstream model，
        // 确保托盘切换模型后无需重启 Claude Code 即可生效。
        match rewrite_model_field(&body, provider.upstream_model()) {
            Ok(rewritten) => axum::body::Bytes::from(rewritten),
            Err(err) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "type": "error",
                        "error": {
                            "type": "invalid_request_error",
                            "message": format!("模型注入失败: {err}")
                        }
                    })),
                )
                    .into_response();
            }
        }
    };

    if provider.uses_anthropic_upstream() {
        return forward_anthropic_request(state, &provider, headers, body).await;
    }

    let app_cfg = state.config.read().await;
    let upstream_model = extract_model_from_body(&body, provider.upstream_model());
    let convert_options = ConvertOptions {
        provider: Some(&provider),
        model_reasoning_effort: &app_cfg.normalized_model_reasoning_effort(),
        tool_output_max_chars: app_cfg.tool_output_max_chars,
    };
    drop(app_cfg);

    match convert_anthropic_to_chat_with_options(&body, &upstream_model, convert_options) {
        Ok(chat_body) => {
            forward_chat_as_anthropic(state, &provider, headers, chat_body.into()).await
        }
        Err(err) => {
            warn!("Anthropic 请求转换失败: {err}");
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "type": "error",
                    "error": {
                        "type": "invalid_request_error",
                        "message": format!("Anthropic 请求转换失败: {err}")
                    }
                })),
            )
                .into_response()
        }
    }
}

/// OpenClaw / Hermes 的 Chat Completions 透传代理。
/// 接收标准 OpenAI Chat Completions 格式，直接转发到上游 provider，不做格式转换。
async fn proxy_chat_passthrough(
    state: &ProxyState,
    _headers: HeaderMap,
    body: axum::body::Bytes,
    tool_id: &str,
) -> Response {
    let config = state.config.read().await.clone();
    let provider = match config.active_provider_for(tool_id) {
        Ok(p) => p.clone(),
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": err.to_string() })),
            )
                .into_response();
        }
    };

    let started = std::time::Instant::now();
    let model = extract_model_from_body(&body, provider.upstream_model());
    let stream = body
        .get(..)
        .and_then(|slice| serde_json::from_slice::<serde_json::Value>(slice).ok())
        .and_then(|v| v.get("stream").and_then(|s| s.as_bool()))
        .unwrap_or(false);

    let pending = PendingRequest {
        provider_id: provider.id.clone(),
        provider_name: provider.name.clone(),
        model: model.clone(),
        path: "/v1/chat/completions".into(),
        stream,
        started,
        status: 0,
    };

    let api_key = match config::resolve_api_key(&provider.api_key_env) {
        Ok(key) => key,
        Err(err) => return auth_error_response(state, pending, err.to_string()).await,
    };

    let target = format!(
        "{}/chat/completions",
        provider.base_url.trim_end_matches('/')
    );

    forward_upstream_with_retry(
        state,
        stream,
        Method::POST,
        &target,
        body.to_vec(),
        UpstreamAuth::Bearer(api_key),
        pending,
    )
    .await
}

async fn catch_all(
    State(state): State<ProxyState>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let path = uri.path();
    if !path.starts_with("/v1/") {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    }
    let config = state.config.read().await.clone();
    let provider = match config.active_provider() {
        Ok(p) => p.clone(),
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": err.to_string() })),
            )
                .into_response();
        }
    };
    if provider.uses_anthropic_upstream() {
        let upstream_path = path.trim_start_matches('/');
        return forward_anthropic_request_path(&state, &provider, upstream_path, method, headers, body)
            .await;
    }
    (StatusCode::NOT_FOUND, "not found").into_response()
}

async fn forward_anthropic_request(
    state: &ProxyState,
    provider: &config::ProviderConfig,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    forward_anthropic_request_path(state, provider, "v1/messages", Method::POST, headers, body)
        .await
}

async fn forward_anthropic_request_path(
    state: &ProxyState,
    provider: &config::ProviderConfig,
    upstream_path: &str,
    method: Method,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let started = std::time::Instant::now();
    let model = extract_model_from_body(&body, provider.upstream_model());
    let stream = body
        .get(..)
        .and_then(|slice| serde_json::from_slice::<serde_json::Value>(slice).ok())
        .and_then(|v| v.get("stream").and_then(|s| s.as_bool()))
        .unwrap_or(false);

    let pending_base = PendingRequest {
        provider_id: provider.id.clone(),
        provider_name: provider.name.clone(),
        model,
        path: format!("/{upstream_path}"),
        stream,
        started,
        status: 0,
    };

    let api_key = match config::resolve_api_key(&provider.api_key_env) {
        Ok(key) => key,
        Err(err) => return auth_error_response(state, pending_base, err.to_string()).await,
    };

    let target = format!(
        "{}/{}",
        provider.base_url.trim_end_matches('/'),
        upstream_path.trim_start_matches('/')
    );

    let version = anthropic_version_header(&headers);
    forward_upstream_with_retry(
        state,
        stream,
        method,
        &target,
        body.to_vec(),
        UpstreamAuth::Anthropic {
            api_key,
            version,
        },
        pending_base,
    )
    .await
}

async fn forward_chat_as_anthropic(
    state: &ProxyState,
    provider: &config::ProviderConfig,
    _headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let started = std::time::Instant::now();
    let model = extract_model_from_body(&body, provider.upstream_model());
    let stream = body
        .get(..)
        .and_then(|slice| serde_json::from_slice::<serde_json::Value>(slice).ok())
        .and_then(|v| v.get("stream").and_then(|s| s.as_bool()))
        .unwrap_or(false);

    let pending_base = PendingRequest {
        provider_id: provider.id.clone(),
        provider_name: provider.name.clone(),
        model: model.clone(),
        path: "/v1/chat/completions".into(),
        stream,
        started,
        status: 0,
    };

    let api_key = match config::resolve_api_key(&provider.api_key_env) {
        Ok(key) => key,
        Err(err) => return auth_error_response(state, pending_base, err.to_string()).await,
    };

    // Ollama 特殊处理：不使用 Bearer token，且需要特殊的格式转换
    let (target, body) = if provider.id == "ollama" {
        let body = convert_anthropic_to_ollama(&body, &model);
        (
            format!("{}{}", provider.base_url.trim_end_matches('/'), "/api/chat"),
            body
        )
    } else {
        (
            format!(
                "{}/chat/completions",
                provider.base_url.trim_end_matches('/')
            ),
            body.to_vec()
        )
    };

    let response = forward_upstream_with_retry(
        state,
        stream,
        Method::POST,
        &target,
        body,
        if provider.id == "ollama" {
            UpstreamAuth::None
        } else {
            UpstreamAuth::Bearer(api_key)
        },
        pending_base,
    )
    .await;

    // Ollama 响应需要特殊转换
    if provider.id == "ollama" {
        convert_ollama_response_to_anthropic(response, &model).await
    } else {
        convert_chat_response_to_anthropic(response, &model).await
    }
}

enum UpstreamAuth {
    Anthropic { api_key: String, version: String },
    Bearer(String),
    None,
}

async fn forward_upstream_with_retry(
    state: &ProxyState,
    stream_request: bool,
    method: Method,
    target: &str,
    body: Vec<u8>,
    auth: UpstreamAuth,
    pending_base: PendingRequest,
) -> Response {
    let upstream_client = if stream_request {
        &state.streaming_client
    } else {
        &state.client
    };

    for attempt in 0..upstream_retry::MAX_UPSTREAM_ATTEMPTS {
        let mut request = upstream_client.request(method.clone(), target);
        request = request.header("Content-Type", "application/json");
        match &auth {
            UpstreamAuth::Anthropic { api_key, version } => {
                request = request.header("x-api-key", api_key);
                request = request.header("anthropic-version", version);
            }
            UpstreamAuth::Bearer(api_key) => {
                request = request.header("Authorization", format!("Bearer {api_key}"));
            }
            UpstreamAuth::None => {
                // Ollama 不需要认证
            }
        }
        if !body.is_empty() {
            request = request.body(body.clone());
        }

        match request.send().await {
            Ok(resp) => {
                if upstream_retry::is_retryable_upstream_status(resp.status())
                    && attempt + 1 < upstream_retry::MAX_UPSTREAM_ATTEMPTS
                {
                    let delay =
                        upstream_retry::retry_delay_from_headers(resp.headers(), attempt);
                    warn!(
                        "上游返回 {}，{:?} 后重试 ({}/{})",
                        resp.status(),
                        delay,
                        attempt + 2,
                        upstream_retry::MAX_UPSTREAM_ATTEMPTS
                    );
                    tokio::time::sleep(delay).await;
                    continue;
                }
                return finish_upstream_response(resp, pending_base, state).await;
            }
            Err(err) => {
                if upstream_retry::is_retryable_upstream_error(&err)
                    && attempt + 1 < upstream_retry::MAX_UPSTREAM_ATTEMPTS
                {
                    let delay = upstream_retry::retry_backoff(attempt);
                    warn!(
                        "上游连接失败，{:?} 后重试 ({}/{}): {target} -> {err}",
                        delay,
                        attempt + 2,
                        upstream_retry::MAX_UPSTREAM_ATTEMPTS
                    );
                    tokio::time::sleep(delay).await;
                    continue;
                }

                warn!("上游请求失败: {target} -> {err}");
                let mut pending = pending_base;
                pending.status = StatusCode::BAD_GATEWAY.as_u16();
                let entry = state.request_log.finalize(pending, None);
                state.request_log.push(entry).await;
                return (
                    StatusCode::BAD_GATEWAY,
                    Json(serde_json::json!({
                        "type": "error",
                        "error": {
                            "type": "api_error",
                            "message": format!("上游请求失败: {err}")
                        }
                    })),
                )
                    .into_response();
            }
        }
    }

    unreachable!("upstream retry loop must return inside");
}

/// Fetch models directly from Ollama API
async fn fetch_ollama_models(client: &Client, provider: &ProviderConfig) -> anyhow::Result<Vec<serde_json::Value>> {
    let url = format!("{}/api/tags", provider.base_url.trim_end_matches('/'));

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to fetch Ollama models: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("Failed to fetch Ollama models ({status}): {text}");
    }

    let data: serde_json::Value = resp.json().await?;
    let models = data
        .get("models")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|model| {
            let mut m = model;
            m["id"] = m.get("name").unwrap_or(&Value::String("unknown".into())).clone();
            m["display_name"] = m.get("name").unwrap_or(&Value::String("unknown".into())).clone();
            m["type"] = "model".into();
            m
        })
        .collect();

    Ok(models)
}

async fn finish_upstream_response(
    resp: reqwest::Response,
    pending_base: PendingRequest,
    state: &ProxyState,
) -> Response {
    let status = resp.status();
    let mut pending = pending_base;
    pending.status = status.as_u16();
    let mut response_headers = HeaderMap::new();
    let mut is_sse = false;
    for (name, value) in resp.headers() {
        if name == reqwest::header::TRANSFER_ENCODING {
            continue;
        }
        if name == reqwest::header::CONTENT_TYPE {
            if let Ok(v) = value.to_str() {
                if v.to_ascii_lowercase().contains("text/event-stream") {
                    is_sse = true;
                }
            }
        }
        if let Ok(v) = HeaderValue::from_bytes(value.as_bytes()) {
            response_headers.insert(name, v);
        }
    }

    if is_sse {
        pending.stream = true;
        response_headers.remove(reqwest::header::CONTENT_LENGTH);
        let stream = LoggingByteStream::new(
            resp.bytes_stream(),
            pending,
            state.request_log.clone(),
        );
        let body = Body::from_stream(stream);
        (status, response_headers, body).into_response()
    } else {
        let bytes = resp.bytes().await.unwrap_or_default();
        let usage = serde_json::from_slice::<serde_json::Value>(&bytes)
            .ok()
            .and_then(|value| parse_usage_from_json(&value));
        let entry = state.request_log.finalize(pending, usage);
        state.request_log.push(entry).await;
        (status, response_headers, Body::from(bytes)).into_response()
    }
}

async fn auth_error_response(
    state: &ProxyState,
    mut pending: PendingRequest,
    message: String,
) -> Response {
    pending.status = StatusCode::UNAUTHORIZED.as_u16();
    let entry = state.request_log.finalize(pending, None);
    state.request_log.push(entry).await;
    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({
            "type": "error",
            "error": {
                "type": "authentication_error",
                "message": message
            }
        })),
    )
        .into_response()
}

async fn convert_chat_response_to_anthropic(response: Response, model: &str) -> Response {
    let status = response.status();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();

    if !status.is_success() {
        return response;
    }

    if content_type.contains("text/event-stream") {
        use async_stream::stream;
        use futures_util::TryStreamExt;

        let (mut parts, body) = response.into_parts();
        let message_id = format!("msg_{}", uuid::Uuid::new_v4());
        let preamble = anthropic_stream_preamble(model, &message_id);
        let upstream_stream = body.into_data_stream().map_err(std::io::Error::other);

        let translated = stream! {
            yield Ok::<_, std::io::Error>(axum::body::Bytes::from(preamble));
            let mut buffer = String::new();
            let mut utf8_remainder = Vec::new();
            let mut translator = AnthropicSseTranslator::new(&message_id);
            futures_util::pin_mut!(upstream_stream);
            while let Some(chunk) = upstream_stream.try_next().await? {
                sse::append_utf8_safe(&mut buffer, &mut utf8_remainder, &chunk);
                while let Some(block) = sse::take_sse_block(&mut buffer) {
                    let converted = translator.convert_event(&format!("{block}\n\n")).join("");
                    if !converted.is_empty() {
                        yield Ok(axum::body::Bytes::from(converted));
                    }
                }
            }
            sse::flush_utf8_remainder(&mut buffer, &mut utf8_remainder);
            if !buffer.trim().is_empty() {
                let converted = translator.convert_event(&format!("{}\n\n", buffer.trim())).join("");
                if !converted.is_empty() {
                    yield Ok(axum::body::Bytes::from(converted));
                }
            }
        };

        parts.headers.remove(reqwest::header::CONTENT_LENGTH);
        parts.headers.insert(
            reqwest::header::CONTENT_TYPE,
            HeaderValue::from_static("text/event-stream; charset=utf-8"),
        );
        parts.headers.insert(
            reqwest::header::CACHE_CONTROL,
            HeaderValue::from_static("no-cache"),
        );
        return Response::from_parts(parts, Body::from_stream(translated)).into_response();
    }

    let body = response.into_body();
    let bytes = match axum::body::to_bytes(body, usize::MAX).await {
        Ok(bytes) => bytes,
        Err(err) => {
            warn!("读取上游响应失败: {err}");
            return (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({
                    "type": "error",
                    "error": {
                        "type": "api_error",
                        "message": format!("读取上游响应失败: {err}")
                    }
                })),
            )
                .into_response();
        }
    };

    match convert_chat_json_to_anthropic(&bytes) {
        Ok(converted) => (
            status,
            [(reqwest::header::CONTENT_TYPE.as_str(), "application/json")],
            converted,
        )
            .into_response(),
        Err(err) => {
            warn!("Chat 响应转换 Anthropic 失败: {err}");
            (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({
                    "type": "error",
                    "error": {
                        "type": "api_error",
                        "message": format!("Chat 响应转换 Anthropic 失败: {err}")
                    }
                })),
            )
                .into_response()
        }
    }
}

fn anthropic_version_header(headers: &HeaderMap) -> String {
    headers
        .get("anthropic-version")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("2023-06-01")
        .to_string()
}

/// 将 Anthropic 格式转换为 Ollama 格式
fn convert_anthropic_to_ollama(body: &[u8], model: &str) -> Vec<u8> {
    use serde_json::{Map, Value};

    let value: Value = serde_json::from_slice(body).unwrap_or_default();

    let mut ollama_body = Map::new();
    ollama_body.insert("model".into(), Value::String(model.to_string()));

    let mut ollama_messages: Vec<Value> = Vec::new();

    if let Some(messages) = value.get("messages").and_then(|v| v.as_array()) {
        for msg in messages {
            if let Some(role) = msg.get("role").and_then(|v| v.as_str()) {
                let mut ollama_msg = Map::new();
                ollama_msg.insert("role".into(), Value::String(role.to_string()));

                if let Some(content) = msg.get("content") {
                    if content.is_array() {
                        // 如果是数组，合并成字符串
                        let content_str = content
                            .as_array()
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|c| c.as_object())
                                    .filter_map(|o| o.get("text").and_then(|t| t.as_str()))
                                    .collect::<Vec<_>>()
                                    .join("\n\n")
                            })
                            .unwrap_or_default();
                        ollama_msg.insert("content".into(), Value::String(content_str));
                    } else {
                        ollama_msg.insert("content".into(), content.clone());
                    }
                }

                ollama_messages.push(Value::Object(ollama_msg));
            }
        }
    }

    ollama_body.insert("messages".into(), Value::Array(ollama_messages));

    serde_json::to_vec(&Value::Object(ollama_body)).unwrap_or_default()
}

/// 将 Ollama 响应转换为 Anthropic 格式
async fn convert_ollama_response_to_anthropic(response: Response, model: &str) -> Response {
    let status = response.status();
    let _content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();

    if !status.is_success() {
        return response;
    }

    let body = response.into_body();
    let bytes = match axum::body::to_bytes(body, usize::MAX).await {
        Ok(bytes) => bytes,
        Err(err) => {
            warn!("读取 Ollama 响应失败: {err}");
            return (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({
                    "type": "error",
                    "error": {
                        "type": "api_error",
                        "message": format!("读取 Ollama 响应失败: {err}")
                    }
                })),
            )
                .into_response();
        }
    };

    match convert_ollama_to_anthropic(&bytes, model) {
        Ok(converted) => (
            status,
            [(reqwest::header::CONTENT_TYPE.as_str(), "application/json")],
            converted,
        )
            .into_response(),
        Err(err) => {
            warn!("Ollama 响应转换 Anthropic 失败: {err}");
            (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({
                    "type": "error",
                    "error": {
                        "type": "api_error",
                        "message": format!("Ollama 响应转换 Anthropic 失败: {err}")
                    }
                })),
            )
                .into_response()
        }
    }
}

/// 将 Ollama 响应转换为 Anthropic 格式
fn convert_ollama_to_anthropic(body: &[u8], model: &str) -> anyhow::Result<Vec<u8>> {
    use serde_json::{json, Value};

    let value: Value = serde_json::from_slice(body).unwrap_or_default();

    // Ollama 响应格式示例：
    // {
    //   "model": "llama3.1",
    //   "created_at": "2024-12-01T00:00:00Z",
    //   "message": {
    //     "role": "assistant",
    //     "content": "Hello! How can I help you today?"
    //   },
    //   "done": true
    // }

    let mut response = json!({
        "id": format!("msg_{}", uuid::Uuid::new_v4()),
        "type": "message",
        "role": "assistant",
        "content": [],
        "model": model,
        "stop_reason": "end_turn",
        "stop_sequence": null,
        "usage": {
            "input_tokens": 0,
            "output_tokens": 0,
            "cache_creation_input_tokens": 0,
            "cache_read_input_tokens": 0
        }
    });

    if let Some(message_obj) = value.get("message").and_then(|v| v.as_object()) {
        if let Some(role) = message_obj.get("role").and_then(|v| v.as_str()) {
            response["role"] = role.into();
        }
        if let Some(content) = message_obj.get("content").and_then(|v| v.as_str()) {
            response["content"] = json!([{"type": "text", "text": content}]);
        }
    }

    // 如果是流式响应，Ollama 没有 done: true，这里只是处理完整的响应
    if let Some(done) = value.get("done").and_then(|v| v.as_bool()) {
        if done {
            response["stop_reason"] = "end_turn".into();
        }
    }

    serde_json::to_vec(&response).map_err(|err| {
        anyhow::anyhow!("Failed to serialize Ollama response: {}", err)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ProviderConfig;
    use serde_json::Value;

    fn deepseek_provider() -> ProviderConfig {
        ProviderConfig {
            id: "deepseek".into(),
            name: "DeepSeek".into(),
            base_url: "https://api.deepseek.com/v1".into(),
            api_key_env: "DEEPSEEK_API_KEY".into(),
            default_model: "deepseek-v4-pro".into(),
            api_model: "deepseek-v4-pro".into(),
            wire_api: "chat".into(),
            base_url_customized: false,
            custom_models: vec![],
            reasoning_style: String::new(),
        }
    }

    fn qwen_provider() -> ProviderConfig {
        ProviderConfig {
            id: "qwen".into(),
            name: "千问".into(),
            base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1".into(),
            api_key_env: "DASHSCOPE_API_KEY".into(),
            default_model: "qwen3.7-max".into(),
            api_model: "qwen3.7-max".into(),
            wire_api: "chat".into(),
            base_url_customized: false,
            custom_models: vec![],
            reasoning_style: String::new(),
        }
    }

    fn convert_anthropic(body: &[u8], provider: &ProviderConfig, tool_output_max_chars: usize) -> Value {
        let out = convert_anthropic_to_chat_with_options(
            body,
            provider.upstream_model(),
            ConvertOptions {
                provider: Some(provider),
                model_reasoning_effort: "medium",
                tool_output_max_chars,
            },
        )
        .unwrap();
        serde_json::from_slice(&out).unwrap()
    }

    #[test]
    fn injects_reasoning_placeholder_only_for_thinking_providers() {
        let body = br#"{"model":"deepseek-v4-pro","messages":[
            {"role":"user","content":"run"},
            {"role":"assistant","content":[{"type":"tool_use","id":"call_1","name":"a","input":{}}]},
            {"role":"user","content":[{"type":"tool_result","tool_use_id":"call_1","content":"ok"}]}
        ]}"#;
        let deepseek = convert_anthropic(body, &deepseek_provider(), 0);
        let qwen = convert_anthropic(body, &qwen_provider(), 0);
        let assistant = deepseek["messages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m.get("role") == Some(&Value::String("assistant".into())))
            .unwrap();
        assert_eq!(assistant["reasoning_content"], "tool call");
        let qwen_assistant = qwen["messages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m.get("role") == Some(&Value::String("assistant".into())))
            .unwrap();
        assert!(qwen_assistant.get("reasoning_content").is_none());
    }

    #[test]
    fn backfill_inherits_reasoning_from_earlier_assistant_message() {
        let body = br#"{"model":"deepseek-v4-pro","messages":[
            {"role":"assistant","content":[{"type":"thinking","thinking":"Plan the patch."},{"type":"text","text":"go"}]},
            {"role":"user","content":"apply"},
            {"role":"assistant","content":[{"type":"tool_use","id":"call_1","name":"apply_patch","input":{}}]},
            {"role":"user","content":[{"type":"tool_result","tool_use_id":"call_1","content":"ok"}]}
        ]}"#;
        let chat = convert_anthropic(body, &deepseek_provider(), 0);
        let msgs = chat["messages"].as_array().unwrap();
        let tool_assistant = msgs
            .iter()
            .filter(|m| m.get("role") == Some(&Value::String("assistant".into())))
            .nth(1)
            .unwrap();
        assert_eq!(tool_assistant["reasoning_content"], "Plan the patch.");
    }

    #[test]
    fn leaves_tool_output_intact_when_truncation_disabled() {
        let long_output = "a".repeat(500);
        let body = format!(
            r#"{{
            "model": "deepseek-v4-pro",
            "messages": [
                {{"role": "assistant", "content": [{{"type": "tool_use", "id": "call_1", "name": "grep", "input": {{}}}}]}},
                {{"role": "user", "content": [{{"type": "tool_result", "tool_use_id": "call_1", "content": {}}}]}}
            ]
        }}"#,
            serde_json::to_string(&long_output).unwrap()
        );
        let chat = convert_anthropic(body.as_bytes(), &deepseek_provider(), 0);
        let tool = chat["messages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m.get("role") == Some(&Value::String("tool".into())))
            .unwrap();
        assert_eq!(tool["content"], long_output);
    }

    #[test]
    fn backfills_tool_responses_when_assistant_tool_calls_trail_history() {
        let body = br#"{"model":"qwen3.7-max","messages":[
            {"role":"user","content":"run tools"},
            {"role":"assistant","content":[{"type":"tool_use","id":"call_1","name":"a","input":{}},{"type":"tool_use","id":"call_2","name":"b","input":{}}]}
        ]}"#;
        let chat = convert_anthropic(body, &qwen_provider(), 0);
        let msgs = chat["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 4);
        assert_eq!(msgs[2]["tool_call_id"], "call_1");
        assert_eq!(msgs[3]["tool_call_id"], "call_2");
    }

    #[test]
    fn injects_stream_options_include_usage_for_streaming_requests() {
        let body = br#"{"model":"qwen3.7-max","stream":true,"messages":[{"role":"user","content":"hi"}]}"#;
        let chat = convert_anthropic(body, &qwen_provider(), 0);
        assert_eq!(chat["stream_options"]["include_usage"], true);
    }

    #[test]
    fn strips_tool_choice_when_tools_are_absent() {
        let body = br#"{
            "model": "qwen3.7-max",
            "stream": false,
            "tool_choice": {"type": "auto"},
            "messages": [{"role": "user", "content": "hi"}]
        }"#;
        let chat = convert_anthropic(body, &qwen_provider(), 0);
        assert!(chat.get("tool_choice").is_none());
        assert!(chat.get("parallel_tool_calls").is_none());
    }

    #[test]
    fn maps_default_reasoning_effort_for_deepseek_at_transform_stage() {
        let body = br#"{"model":"deepseek-v4-pro","messages":[{"role":"user","content":"hi"}]}"#;
        let chat = convert_anthropic(body, &deepseek_provider(), 0);
        assert_eq!(chat["reasoning_effort"], "high");
        assert_eq!(chat["thinking"]["type"], "enabled");
        assert!(chat.get("reasoning").is_none());
    }

    #[test]
    fn multi_round_tool_conversation_stays_valid_for_upstream() {
        let body = br#"{
            "model": "deepseek-v4-pro",
            "messages": [
                {"role":"user","content":"scan project"},
                {"role":"assistant","content":[
                    {"type":"thinking","thinking":"Need README first."},
                    {"type":"tool_use","id":"call_1","name":"read_file","input":{"path":"README.md"}}
                ]},
                {"role":"user","content":[
                    {"type":"tool_result","tool_use_id":"call_1","content":[{"type":"text","text":"README title"}]}
                ]},
                {"role":"assistant","content":[
                    {"type":"tool_use","id":"call_2","name":"grep","input":{"pattern":"todo"}}
                ]},
                {"role":"user","content":[
                    {"type":"tool_result","tool_use_id":"call_2","content":[{"type":"text","text":"src/main.rs:1"}]}
                ]},
                {"role":"user","content":"summarize"},
                {"role":"assistant","content":[{"type":"text","text":"Done."}]}
            ]
        }"#;
        let chat = convert_anthropic(body, &deepseek_provider(), 0);
        let msgs = chat["messages"].as_array().unwrap();

        assert_eq!(msgs[0]["role"], "user");
        assert_eq!(msgs[1]["reasoning_content"], "Need README first.");
        assert_eq!(msgs[1]["tool_calls"][0]["function"]["name"], "read_file");
        assert_eq!(msgs[2]["role"], "tool");
        assert_eq!(msgs[2]["tool_call_id"], "call_1");
        assert_eq!(msgs[2]["content"], "README title");
        assert_eq!(msgs[3]["tool_calls"][0]["function"]["name"], "grep");
        assert_eq!(msgs[3]["reasoning_content"], "tool call");
        assert_eq!(msgs[4]["tool_call_id"], "call_2");
        assert_eq!(msgs[5]["role"], "user");
        assert_eq!(msgs[6]["content"], "Done.");
    }

    #[test]
    fn multi_round_tool_conversation_truncates_long_output_when_enabled() {
        let long_output = "HEAD".to_string() + &"x".repeat(200) + "TAIL";
        let body = format!(
            r#"{{
            "model": "deepseek-v4-pro",
            "messages": [
                {{"role":"user","content":"run"}},
                {{"role":"assistant","content":[{{"type":"tool_use","id":"call_1","name":"grep","input":{{}}}}]}},
                {{"role":"user","content":[{{"type":"tool_result","tool_use_id":"call_1","content":{}}}]}}
            ]
        }}"#,
            serde_json::to_string(&long_output).unwrap()
        );
        let chat = convert_anthropic(body.as_bytes(), &deepseek_provider(), 80);
        let tool = chat["messages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m.get("role") == Some(&Value::String("tool".into())))
            .unwrap();
        let content = tool["content"].as_str().unwrap();
        assert!(content.contains("HEAD"));
        assert!(content.contains("TAIL"));
        assert!(content.contains("truncated"));
        assert!(content.chars().count() < long_output.chars().count());
    }

    #[test]
    fn test_convert_anthropic_to_ollama() {
        use super::*;
        use serde_json::Value;

        // Test Anthropic to Ollama conversion
        let body = br#"{
            "model": "llama3.1",
            "messages": [
                {"role": "user", "content": "Hello"},
                {"role": "assistant", "content": "Hi there!"},
                {"role": "user", "content": "How are you?"}
            ]
        }"#;

        let ollama_body = convert_anthropic_to_ollama(body, "llama3.1");
        let ollama: Value = serde_json::from_slice(&ollama_body).unwrap();

        assert_eq!(ollama["model"], "llama3.1");
        assert_eq!(ollama["messages"].as_array().unwrap().len(), 3);
        assert_eq!(ollama["messages"][0]["role"], "user");
        assert_eq!(ollama["messages"][0]["content"], "Hello");
        assert_eq!(ollama["messages"][1]["role"], "assistant");
        assert_eq!(ollama["messages"][1]["content"], "Hi there!");
    }

    #[test]
    fn test_convert_ollama_to_anthropic() {
        use super::*;
        use serde_json::Value;

        // Test Ollama to Anthropic conversion
        let ollama_response = r#"{
            "model": "llama3.1",
            "created_at": "2024-12-01T00:00:00Z",
            "message": {
                "role": "assistant",
                "content": "Hello! How can I help you today?"
            },
            "done": true
        }"#;

        let anthropic_body = convert_ollama_to_anthropic(ollama_response.as_bytes(), "llama3.1").unwrap();
        let anthropic: Value = serde_json::from_slice(&anthropic_body).unwrap();

        assert_eq!(anthropic["role"], "assistant");
        assert_eq!(anthropic["content"][0]["type"], "text");
        assert_eq!(anthropic["content"][0]["text"], "Hello! How can I help you today?");
        assert_eq!(anthropic["model"], "llama3.1");
    }
}
