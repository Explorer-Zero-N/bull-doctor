use axum::extract::State;
use axum::response::{Html, IntoResponse, Json};
use axum::http::{header, HeaderMap, HeaderValue};
use serde::Deserialize;

use crate::claude;
use crate::config::{self, AppConfig};
use crate::provider;
use crate::proxy::{reload_config_in_state, request_tray_health_check, ProxyState};
use crate::settings;

const SETTINGS_HTML: &str = include_str!("page.html");
const BRAND_ICON_SVG: &str = include_str!("../../assets/brand-icon.svg");

pub async fn settings_page() -> Html<String> {
    Html(SETTINGS_HTML.replace("<!--BRAND_ICON-->", BRAND_ICON_SVG.trim()))
}

pub async fn brand_icon_svg() -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("image/svg+xml; charset=utf-8"),
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=86400"),
    );
    (headers, BRAND_ICON_SVG)
}

fn mask_key_preview(key: &str) -> String {
    if key.len() <= 8 {
        return "***".into();
    }
    format!("{}...{}", &key[..4], &key[key.len() - 4..])
}

pub async fn settings_bootstrap() -> impl IntoResponse {
    let app = match AppConfig::load() {
        Ok(a) => a,
        Err(err) => {
            return Json(serde_json::json!({
                "error": err.to_string(),
                "providers": [],
                "active": "",
            }))
            .into_response();
        }
    };

    let mut providers = Vec::new();
    for preset in provider::list_presets(&app) {
        let key_preview = config::resolve_api_key(&preset.api_key_env)
            .ok()
            .map(|k| mask_key_preview(&k));
        let supports_reasoning_effort =
            provider::chat_reasoning::provider_supports_reasoning_effort(preset);
        let reasoning_effort_options = if supports_reasoning_effort {
            provider::chat_reasoning::reasoning_effort_options_for(preset)
        } else {
            Vec::new()
        };
        providers.push(serde_json::json!({
            "id": preset.id,
            "name": preset.name,
            "signup_url": settings::signup_url(&preset.id),
            "key_hint": settings::key_hint(&preset.id),
            "key_configured": key_preview.is_some(),
            "key_preview": key_preview,
            "base_url": preset.base_url,
            "base_url_customized": preset.base_url_customized,
            "is_custom": preset.id == "custom",
            "supports_reasoning_effort": supports_reasoning_effort,
            "reasoning_effort_options": reasoning_effort_options,
            "default_model": preset.default_model,
            "reasoning_style": preset.reasoning_style,
        }));
    }

    // 构建工具列表
    let mut tools = Vec::new();
    for &tool_id in config::ALL_TOOLS {
        let tc = app.tool_config(tool_id);
        let tool_provider = app.active_provider_for(tool_id)
            .map(|p| p.name.clone())
            .unwrap_or_else(|_| "unknown".into());
        tools.push(serde_json::json!({
            "id": tool_id,
            "name": config::tool_display_name(tool_id),
            "enabled": tc.enabled,
            "active_provider": if tc.active_provider.is_empty() { &app.active } else { &tc.active_provider },
            "provider_name": tool_provider,
        }));
    }

    Json(serde_json::json!({
        "active": app.active,
        "model_reasoning_effort": app.normalized_model_reasoning_effort(),
        "providers": providers,
        "tools": tools,
        "compress_auto_start": app.compress.auto_start,
    }))
    .into_response()
}

#[derive(Deserialize)]
pub struct SettingsSaveBody {
    provider_id: String,
    #[serde(default)]
    api_key: String,
    #[serde(default)]
    base_url: String,
    #[serde(default)]
    model_reasoning_effort: Option<String>,
    #[serde(default)]
    model: String,
    /// 可选：将此 provider 设为此工具的默认
    #[serde(default)]
    tool_id: String,
    /// 更新 compress 自动启动设置（不更改 provider 时使用）
    #[serde(default)]
    compress_auto_start: Option<bool>,
}

pub async fn settings_save(
    State(state): State<ProxyState>,
    Json(body): Json<SettingsSaveBody>,
) -> impl IntoResponse {
    // 压缩设置（不涉及 provider 切换）
    if let Some(auto_start) = body.compress_auto_start {
        if body.provider_id.is_empty() {
            let mut app = AppConfig::load().unwrap_or_default();
            app.compress.auto_start = auto_start;
            if let Err(err) = app.save() {
                return Json(serde_json::json!({
                    "ok": false,
                    "message": format!("{err:#}"),
                }))
                .into_response();
            }
            return Json(serde_json::json!({
                "ok": true,
                "message": if auto_start { "已开启 Compression 自动启动" } else { "已关闭 Compression 自动启动" },
            }))
            .into_response();
        }
    }

    match save_api_key(
        &state,
        &body.provider_id,
        body.api_key.trim(),
        body.base_url.trim(),
        body.model_reasoning_effort.as_deref(),
        body.model.trim(),
        body.tool_id.trim(),
    )
    .await
    {
        Ok(message) => Json(serde_json::json!({
            "ok": true,
            "message": message,
        }))
        .into_response(),
        Err(err) => Json(serde_json::json!({
            "ok": false,
            "message": format!("{err:#}"),
        }))
        .into_response(),
    }
}

#[derive(Deserialize)]
pub struct SettingsTestBody {
    provider_id: String,
    #[serde(default)]
    api_key: String,
    #[serde(default)]
    base_url: String,
}
pub async fn settings_test(Json(body): Json<SettingsTestBody>) -> impl IntoResponse {
    let app = match AppConfig::load() {
        Ok(a) => a,
        Err(err) => {
            return Json(serde_json::json!({
                "ok": false,
                "message": err.to_string(),
            }))
            .into_response();
        }
    };

    let mut provider = match provider::get_preset(&app, &body.provider_id) {
        Ok(p) => p.clone(),
        Err(err) => {
            return Json(serde_json::json!({
                "ok": false,
                "message": err.to_string(),
            }))
            .into_response();
        }
    };

    if let Err(err) = apply_provider_base_url(&mut provider, body.base_url.trim()) {
        return Json(serde_json::json!({
            "ok": false,
            "message": format!("{err:#}"),
        }))
        .into_response();
    }

    let api_key = match resolve_key_for_request(&provider, body.api_key.trim()) {
        Ok(key) => key,
        Err(err) => {
            return Json(serde_json::json!({
                "ok": false,
                "message": format!("{err:#}"),
            }))
            .into_response();
        }
    };
    match test_api_key(&provider, &api_key).await {
        Ok(()) => Json(serde_json::json!({
            "ok": true,
            "message": format!("{} 连接成功！", provider.name),
        }))
        .into_response(),
        Err(err) => Json(serde_json::json!({
            "ok": false,
            "message": format!("{err:#}"),
        }))
        .into_response(),
    }
}

pub async fn settings_clear_all(State(state): State<ProxyState>) -> impl IntoResponse {
    match clear_all_settings(&state).await {
        Ok(message) => Json(serde_json::json!({
            "ok": true,
            "message": message,
        }))
        .into_response(),
        Err(err) => Json(serde_json::json!({
            "ok": false,
            "message": format!("{err:#}"),
        }))
        .into_response(),
    }
}

async fn clear_all_settings(state: &ProxyState) -> anyhow::Result<String> {
    let app = AppConfig::clear_all_settings()?;
    claude::inject_proxy_config(&app)?;
    reload_config_in_state(state).await?;
    state.request_log.clear().await;
    request_tray_health_check(state);
    Ok(
        "已清除所有 Doctor 配置（API Key、厂商选择、中转站地址）。请重新配置各工具的模型并重启。"
            .into(),
    )
}

async fn save_api_key(
    state: &ProxyState,
    provider_id: &str,
    api_key: &str,
    base_url: &str,
    model_reasoning_effort: Option<&str>,
    model: &str,
    tool_id: &str,
) -> anyhow::Result<String> {
    let mut app = AppConfig::load()?;
    provider::get_preset(&app, provider_id)?;
    let provider_entry = app
        .providers
        .get_mut(provider_id)
        .ok_or_else(|| anyhow::anyhow!("未知模型预设: {provider_id}"))?;

    apply_provider_base_url(provider_entry, base_url)?;

    let provider = provider_entry.clone();
    if let Some(effort) = model_reasoning_effort {
        if provider::chat_reasoning::provider_supports_reasoning_effort(&provider) {
            app.model_reasoning_effort = config::normalize_model_reasoning_effort(effort);
        }
    }
    if !api_key.is_empty() {
        config::save_env_value(&provider.api_key_env, api_key)?;
    } else if provider.id != "ollama"
        && provider.id != "lmstudio"
        && config::resolve_api_key(&provider.api_key_env).is_err()
    {
        anyhow::bail!("API Key 不能为空");
    }

    if !model.is_empty() {
        provider_entry.default_model = model.to_string();
        provider_entry.api_model = model.to_string();
    }

    // 如果指定了 tool_id，将该 provider 设为该工具的默认
    if !tool_id.is_empty() {
        app.ensure_tool_entries();
        let tool = app.tools.entry(tool_id.to_string()).or_default();
        tool.active_provider = provider_id.to_string();
    } else {
        // 没有 tool_id 则设为全局默认
        app.active = provider_id.to_string();
    }

    app.save()?;

    // 只在全局默认变更或 Claude Code 相关工具时同步 Claude Code 配置
    if tool_id.is_empty()
        || tool_id == config::TOOL_CLAUDE_CODE
        || tool_id == config::TOOL_CLAUDE_DESKTOP
    {
        claude::inject_proxy_config(&app)?;
    }

    // 自动同步 Hermes 和 OpenClaw 配置文件
    if tool_id == config::TOOL_HERMES || tool_id.is_empty() {
        let _ = crate::tool_configs::sync_hermes_config(&provider, model);
    }
    if tool_id == config::TOOL_OPENCLAW || tool_id.is_empty() {
        let _ = crate::tool_configs::sync_openclaw_config(&provider, model);
    }

    reload_config_in_state(state).await?;
    request_tray_health_check(state);

    if tool_id.is_empty() {
        Ok(format!(
            "已保存 {} 为全局默认。请完全退出并重新打开 Claude Code。",
            provider.name
        ))
    } else {
        let tool_name = config::tool_display_name(tool_id);
        Ok(format!(
            "已为 {} 设置 {} 为默认模型。重启 {} 生效。",
            tool_name, provider.name, tool_name
        ))
    }
}

fn builtin_base_url(provider_id: &str) -> String {
    provider::presets::builtin_presets()
        .into_iter()
        .find(|preset| preset.id == provider_id)
        .map(|preset| preset.base_url)
        .unwrap_or_default()
}

fn builtin_wire_api(provider_id: &str) -> String {
    provider::presets::builtin_presets()
        .into_iter()
        .find(|preset| preset.id == provider_id)
        .map(|preset| preset.wire_api.clone())
        .unwrap_or_else(|| "chat".into())
}

fn apply_provider_base_url(
    provider: &mut config::ProviderConfig,
    base_url: &str,
) -> anyhow::Result<()> {
    let default_url = builtin_base_url(&provider.id);

    if provider.id == "custom" {
        if base_url.is_empty() && provider.base_url.trim().is_empty() {
            anyhow::bail!("请填写 Base URL");
        }
        if !base_url.is_empty() {
            provider.base_url = config::validate_base_url(base_url)?;
        }
        provider.base_url_customized = true;
        provider.detect_wire_api_from_base_url();
        return Ok(());
    }

    if base_url.is_empty() {
        provider.base_url = default_url;
        provider.base_url_customized = false;
        provider.wire_api = builtin_wire_api(&provider.id);
        return Ok(());
    }

    provider.base_url = config::validate_base_url(base_url)?;
    provider.base_url_customized = provider.base_url != default_url;
    if provider.base_url_customized {
        provider.detect_wire_api_from_base_url();
    } else {
        provider.wire_api = builtin_wire_api(&provider.id);
    }
    Ok(())
}

fn resolve_key_for_request(
    provider: &config::ProviderConfig,
    api_key: &str,
) -> anyhow::Result<String> {
    if !api_key.is_empty() {
        return Ok(api_key.to_string());
    }
    config::resolve_api_key(&provider.api_key_env)
}

pub async fn settings_fetch_models(Json(body): Json<SettingsTestBody>) -> impl IntoResponse {
    let mut app = match AppConfig::load() {
        Ok(a) => a,
        Err(err) => {
            return Json(serde_json::json!({
                "ok": true,
                "message": err.to_string(),
                "builtin_only": true,
                "models": builtin_models_for_provider(&body.provider_id),
            }))
            .into_response();
        }
    };

    let mut provider = match provider::get_preset(&app, &body.provider_id) {
        Ok(p) => p.clone(),
        Err(err) => {
            return Json(serde_json::json!({
                "ok": true,
                "message": err.to_string(),
                "builtin_only": true,
                "models": builtin_models_for_provider(&body.provider_id),
            }))
            .into_response();
        }
    };

    if let Err(_err) = apply_provider_base_url(&mut provider, body.base_url.trim()) {
        // Even if base URL is invalid, return built-in models
        return Json(serde_json::json!({
            "ok": true,
            "models": builtin_models_for_provider(&provider.id),
            "builtin_only": true,
        }))
        .into_response();
    }

    let api_key = match resolve_key_for_request(&provider, body.api_key.trim()) {
        Ok(key) => key,
        Err(_err) => {
            // No API key, just return built-in models
            return Json(serde_json::json!({
                "ok": true,
                "models": builtin_models_for_provider(&provider.id),
                "builtin_only": true,
            }))
            .into_response();
        }
    };

    let builtin = builtin_models_for_provider(&provider.id);

    // Try to fetch from upstream API; on failure, just return built-in models
    let (models, fetched) = if let Ok(api_models) = fetch_models_from_upstream(&provider, &api_key).await {
        if !api_models.is_empty() {
            (merge_model_lists(builtin, api_models), true)
        } else {
            (builtin, false)
        }
    } else {
        (builtin, false)
    };

    // 成功从 API 获取到模型列表时，持久化到配置以便托盘菜单使用
    if fetched {
        let custom_models: Vec<config::CustomModelEntry> = models
            .iter()
            .map(|m| {
                let id = m
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_default();
                let display_name = m
                    .get("display_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&id)
                    .to_string();
                config::CustomModelEntry { id, display_name }
            })
            .collect();
        if let Some(provider_entry) = app.providers.get_mut(&body.provider_id) {
            provider_entry.custom_models = custom_models;
            let _ = app.save();
        }
    }

    Json(serde_json::json!({
        "ok": true,
        "models": models,
    }))
    .into_response()
}

fn builtin_models_for_provider(provider_id: &str) -> Vec<serde_json::Value> {
    provider::models::popular_models(provider_id)
        .iter()
        .map(|m| {
            serde_json::json!({
                "id": m.slug,
                "display_name": m.display_name,
                "api_model": m.api_model,
                "builtin": true,
            })
        })
        .collect()
}

fn merge_model_lists(
    mut builtin: Vec<serde_json::Value>,
    api_models: Vec<serde_json::Value>,
) -> Vec<serde_json::Value> {
    for api_m in api_models {
        let id = api_m
            .get("id")
            .or_else(|| api_m.get("name"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_default();
        if id.is_empty() || builtin.iter().any(|m| m.get("id").and_then(|v| v.as_str()) == Some(&id)) {
            continue;
        }
        builtin.push(serde_json::json!({
            "id": id,
            "display_name": api_m.get("display_name").and_then(|v| v.as_str()).unwrap_or(&id),
            "builtin": false,
        }));
    }
    builtin
}

async fn fetch_models_from_upstream(
    provider: &config::ProviderConfig,
    api_key: &str,
) -> anyhow::Result<Vec<serde_json::Value>> {
    let client = config::build_upstream_client(std::time::Duration::from_secs(15))?;

    if provider.uses_anthropic_upstream() {
        let url = format!(
            "{}/v1/models",
            provider.base_url.trim_end_matches('/')
        );
        let resp = client
            .get(&url)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("请求模型列表失败: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("获取模型列表失败 ({status})：{text}");
        }

        let data: serde_json::Value = resp.json().await?;
        let models = data
            .get("data")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        return Ok(models);
    }

    // Ollama uses /api/tags, everyone else uses /models
    let url = if provider.id == "ollama" {
        format!(
            "{}/api/tags",
            provider.base_url.trim_end_matches('/')
        )
    } else {
        format!(
            "{}/models",
            provider.base_url.trim_end_matches('/')
        )
    };

    // Ollama and LM Studio don't require authentication
    let resp = if provider.id == "ollama" || provider.id == "lmstudio" {
        client
            .get(&url)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("请求模型列表失败: {e}"))?
    } else {
        client
            .get(&url)
            .bearer_auth(api_key)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("请求模型列表失败: {e}"))?
    };

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("获取模型列表失败 ({status})：{text}");
    }

    let data: serde_json::Value = resp.json().await?;

    // Handle Ollama response format: {"models": [...]}
    if provider.id == "ollama" {
        let models = data
            .get("models")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|model| {
                let mut m = model;
                let name = m.get("name").cloned().unwrap_or_else(|| m.get("id").cloned().unwrap_or(serde_json::Value::Null));
                m["id"] = name.clone();
                m["display_name"] = name;
                m
            })
            .collect();
        return Ok(models);
    }

    let models = data
        .get("data")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    Ok(models)
}

pub async fn test_api_key(provider: &config::ProviderConfig, api_key: &str) -> anyhow::Result<()> {
    if api_key.is_empty() && provider.id != "ollama" && provider.id != "lmstudio" {
        anyhow::bail!("API Key 不能为空");
    }
    if provider.id == "custom" && provider.base_url.trim().is_empty() {
        anyhow::bail!("请填写 Base URL");
    }

    let client = config::build_upstream_client(std::time::Duration::from_secs(30))?;

    if provider.uses_anthropic_upstream() {
        let url = format!(
            "{}/v1/messages",
            provider.base_url.trim_end_matches('/')
        );
        let body = serde_json::json!({
            "model": provider.upstream_model(),
            "max_tokens": 8,
            "messages": [{"role": "user", "content": "ping"}]
        });
        let resp = client
            .post(url)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await?;
        if resp.status().is_success() {
            return Ok(());
        }
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("连接失败 ({status})：{text}");
    }

    // Ollama doesn't require API key for testing, just check if the server is reachable
    if provider.id == "ollama" {
        let url = format!(
            "{}/api/tags",
            provider.base_url.trim_end_matches('/')
        );
        let resp = client
            .get(&url)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("连接失败: {e}"))?;
        if resp.status().is_success() {
            return Ok(());
        }
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("连接失败 ({status})：{text}");
    }

    // LM Studio doesn't require API key for testing, just check if the server is reachable
    if provider.id == "lmstudio" {
        let url = format!(
            "{}/models",
            provider.base_url.trim_end_matches('/')
        );
        let resp = client
            .get(&url)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("连接失败: {e}"))?;
        if resp.status().is_success() {
            return Ok(());
        }
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("连接失败 ({status})：{text}");
    }

    let url = format!(
        "{}/chat/completions",
        provider.base_url.trim_end_matches('/')
    );
    let body = serde_json::json!({
        "model": provider.upstream_model(),
        "messages": [{"role": "user", "content": "ping"}],
        "max_tokens": 8
    });
    let resp = client
        .post(url)
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await?;

    if resp.status().is_success() {
        return Ok(());
    }

    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    anyhow::bail!("连接失败 ({status})：{text}")
}

pub async fn settings_check_config(State(_state): State<ProxyState>) -> impl IntoResponse {
    let app = AppConfig::load();

    let (active, default_model, api_model, model_env_values, settings_content) = match app {
        Ok(ref app) => {
            let active = app.active.clone();
            let (default_model, api_model) = app
                .active_provider()
                .map(|p| (p.default_model.clone(), p.upstream_model().to_string()))
                .unwrap_or_default();

            let settings_content: String = crate::paths::claude_settings_path()
                .ok()
                .and_then(|p| std::fs::read_to_string(p).ok())
                .unwrap_or_default();

            let parsed: Option<serde_json::Value> =
                serde_json::from_str(&settings_content).ok();

            let model_env_values: Vec<(String, String)> = parsed
                .as_ref()
                .and_then(|v| v.get("env"))
                .and_then(|v| v.as_object())
                .map(|env| {
                    let keys = [
                        "ANTHROPIC_MODEL",
                        "ANTHROPIC_DEFAULT_HAIKU_MODEL",
                        "ANTHROPIC_DEFAULT_SONNET_MODEL",
                        "ANTHROPIC_DEFAULT_OPUS_MODEL",
                        "ANTHROPIC_REASONING_MODEL",
                        "ANTHROPIC_BASE_URL",
                    ];
                    keys.iter()
                        .filter_map(|k| {
                            env.get(*k)
                                .and_then(|v| v.as_str())
                                .map(|v| (k.to_string(), v.to_string()))
                        })
                        .collect()
                })
                .unwrap_or_default();

            (active, default_model, api_model, model_env_values, settings_content)
        }
        Err(_) => (
            String::new(),
            String::new(),
            String::new(),
            Vec::new(),
            String::new(),
        ),
    };

    Json(serde_json::json!({
        "helper": {
            "active": active,
            "default_model": default_model,
            "api_model": api_model,
        },
        "claude_settings_env": model_env_values,
        "has_settings_file": !settings_content.is_empty(),
    }))
}

// ── Skills API ──

pub async fn settings_skills_bootstrap() -> impl IntoResponse {
    match crate::skills::list_skills() {
        Ok(skills) => Json(serde_json::json!({
            "ok": true,
            "skills": skills,
        }))
        .into_response(),
        Err(err) => Json(serde_json::json!({
            "ok": false,
            "error": format!("{err:#}"),
        }))
        .into_response(),
    }
}

#[derive(Deserialize)]
pub struct SkillInstallBody {
    repo: String,
}

pub async fn settings_skills_install(
    Json(body): Json<SkillInstallBody>,
) -> impl IntoResponse {
    match crate::skills::install_skill(&body.repo).await {
        Ok(skill) => Json(serde_json::json!({
            "ok": true,
            "skill": skill,
            "message": format!("Skill {} 安装成功", skill.id),
        }))
        .into_response(),
        Err(err) => Json(serde_json::json!({
            "ok": false,
            "error": format!("{err:#}"),
        }))
        .into_response(),
    }
}

#[derive(Deserialize)]
pub struct SkillInstallLocalBody {
    path: String,
}

pub async fn settings_skills_install_local(
    Json(body): Json<SkillInstallLocalBody>,
) -> impl IntoResponse {
    match crate::skills::install_local_skill(&body.path) {
        Ok(skill) => Json(serde_json::json!({
            "ok": true,
            "skill": skill,
            "message": format!("本地 Skill \"{}\" 安装成功", skill.name),
        }))
        .into_response(),
        Err(err) => Json(serde_json::json!({
            "ok": false,
            "error": format!("{err:#}"),
        }))
        .into_response(),
    }
}

#[derive(Deserialize)]
pub struct SkillUninstallBody {
    skill_id: String,
}

pub async fn settings_skills_uninstall(
    Json(body): Json<SkillUninstallBody>,
) -> impl IntoResponse {
    match crate::skills::uninstall_skill(&body.skill_id) {
        Ok(()) => Json(serde_json::json!({
            "ok": true,
            "message": format!("Skill {} 已卸载", body.skill_id),
        }))
        .into_response(),
        Err(err) => Json(serde_json::json!({
            "ok": false,
            "error": format!("{err:#}"),
        }))
        .into_response(),
    }
}

pub async fn settings_skills_sync() -> impl IntoResponse {
    match crate::skills::sync_all_to_tools() {
        Ok(()) => Json(serde_json::json!({
            "ok": true,
            "message": "所有 Skill 已同步到所有工具",
        }))
        .into_response(),
        Err(err) => Json(serde_json::json!({
            "ok": false,
            "error": format!("{err:#}"),
        }))
        .into_response(),
    }
}

/// 切换 Claude Code settings.json 中的 ANTHROPIC_BASE_URL。
fn switch_claude_base_url(url: &str) -> anyhow::Result<()> {
    let settings_path = crate::paths::claude_settings_path()?;
    if !settings_path.exists() {
        return Ok(());
    }
    let raw = std::fs::read_to_string(&settings_path)?;
    let mut root: serde_json::Value = serde_json::from_str(&raw)
        .unwrap_or_else(|_| serde_json::Value::Object(serde_json::Map::new()));

    let env = root
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("settings.json 根节点必须是对象"))?
        .entry("env")
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));

    let env_map = env
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("settings.json env 必须是对象"))?;

    env_map.insert(
        "ANTHROPIC_BASE_URL".into(),
        serde_json::Value::String(url.to_string()),
    );

    crate::config::write_atomic(&settings_path, &format!("{}\n", serde_json::to_string_pretty(&root)?))?;
    Ok(())
}

// ── Compression API ──

pub async fn compress_status(
    State(state): State<ProxyState>,
) -> impl IntoResponse {
    let app = AppConfig::load().unwrap_or_default();
    let port = app.compress.port;

    let mut status = crate::compress::compress_status(&state.compress_process, port);

    // 探测端口是否就绪
    if status.get("running").and_then(|v| v.as_bool()).unwrap_or(false) {
        let check_url = format!("http://127.0.0.1:{}/livez", port);
        let client = config::build_upstream_client(std::time::Duration::from_secs(2)).ok();
        if let Some(c) = client {
            match c.get(&check_url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    status["ready"] = serde_json::Value::Bool(true);
                }
                _ => {
                    // 端口还没就绪（ML 模型加载中），但不清理 handle
                    status["ready"] = serde_json::Value::Bool(false);
                }
            }
        }
    }

    Json(status).into_response()
}

pub async fn compress_start(
    State(state): State<ProxyState>,
) -> impl IntoResponse {
    let app = match AppConfig::load() {
        Ok(a) => a,
        Err(err) => {
            return Json(serde_json::json!({
                "ok": false,
                "message": format!("{err:#}"),
            }))
            .into_response();
        }
    };

    match crate::compress::start_compress(app.compress.port, app.proxy.port, &state.compress_process).await {
        Ok(()) => {
            crate::tool_configs::switch_all_tools_to_compress(app.compress.port);
            let _ = switch_claude_base_url(&format!("http://127.0.0.1:{}", app.compress.port));

            Json(serde_json::json!({
                "ok": true,
                "message": format!("压缩代理已启动，端口 {}。请重启 Claude Code 生效。", app.compress.port),
            }))
            .into_response()
        }
        Err(err) => Json(serde_json::json!({
            "ok": false,
            "message": format!("{err:#}"),
        }))
        .into_response(),
    }
}

pub async fn compress_stop(
    State(state): State<ProxyState>,
) -> impl IntoResponse {
    match crate::compress::stop_compress(&state.compress_process).await {
        Ok(()) => {
            let app = AppConfig::load().unwrap_or_default();
            // 恢复所有工具路由到 Helper
            crate::tool_configs::restore_all_tools_to_doctor(app.proxy.port);
            let _ = switch_claude_base_url(&format!("http://127.0.0.1:{}", app.proxy.port));

            Json(serde_json::json!({
                "ok": true,
                "message": "压缩代理已停止，所有工具请求已恢复直连 Doctor",
            }))
            .into_response()
        }
        Err(err) => Json(serde_json::json!({
            "ok": false,
            "message": format!("{err:#}"),
        }))
        .into_response(),
    }
}

/// 从 Compression 的 /metrics Prometheus 端点提取关键指标。
pub async fn compress_stats(
    State(state): State<ProxyState>,
) -> impl IntoResponse {
    let app = AppConfig::load().unwrap_or_default();
    let port = app.compress.port;

    if !crate::compress::is_running(&state.compress_process) {
        return Json(serde_json::json!({
            "ok": false,
            "running": false,
            "message": "压缩代理未运行",
        }))
        .into_response();
    }

    let client = match config::build_upstream_client(std::time::Duration::from_secs(3)) {
        Ok(c) => c,
        Err(_) => {
            return Json(serde_json::json!({ "ok": false, "running": true, "message": "创建客户端失败" }))
                .into_response();
        }
    };

    let url = format!("http://127.0.0.1:{}/metrics", port);
    match client.get(&url).send().await {
        Ok(resp) if resp.status().is_success() => {
            let text = resp.text().await.unwrap_or_default();
            let metrics = parse_prometheus_metrics(&text);
            Json(serde_json::json!({
                "ok": true,
                "running": true,
                "display_session": {
                    "requests": metrics.get("requests").copied().unwrap_or(0.0) as u64,
                    "tokens_saved": metrics.get("tokens_saved").copied().unwrap_or(0.0) as u64,
                    "total_input_tokens": metrics.get("input_tokens").copied().unwrap_or(0.0) as u64,
                    "savings_percent": metrics.get("savings_percent").copied().unwrap_or(0.0),
                    "compression_savings_usd": metrics.get("savings_usd").copied().unwrap_or(0.0),
                },
                "tokens": {
                    "input": metrics.get("input_tokens").copied().unwrap_or(0.0) as u64,
                    "saved": metrics.get("tokens_saved").copied().unwrap_or(0.0) as u64,
                },
                "latency": {
                    "average_ms": metrics.get("latency_avg_ms").copied().unwrap_or(0.0),
                },
                "overhead": {
                    "average_ms": metrics.get("overhead_avg_ms").copied().unwrap_or(0.0),
                },
            }))
            .into_response()
        }
        _ => Json(serde_json::json!({
            "ok": true,
            "running": true,
            "display_session": { "requests": 0, "tokens_saved": 0, "total_input_tokens": 0, "savings_percent": 0, "compression_savings_usd": 0 },
            "tokens": { "input": 0, "saved": 0 },
            "latency": { "average_ms": 0 },
            "overhead": { "average_ms": 0 },
        }))
        .into_response(),
    }
}

/// 解析 Prometheus 文本格式，提取关键指标。
fn parse_prometheus_metrics(text: &str) -> std::collections::HashMap<String, f64> {
    let mut map = std::collections::HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() { continue; }
        // 格式: metric_name{labels} value  或  metric_name value
        let (name_part, value_part) = if let Some(space_idx) = line.rfind(' ') {
            (&line[..space_idx], line[space_idx + 1..].trim())
        } else {
            continue;
        };
        let metric_name = if let Some(brace_idx) = name_part.find('{') {
            &name_part[..brace_idx]
        } else {
            name_part
        };
        if let Ok(val) = value_part.parse::<f64>() {
            // 映射已知的 compress 指标名到我们的 key
            let key = match metric_name {
                "compress_requests_total" => "requests",
                "compress_tokens_saved_total" => "tokens_saved",
                "compress_input_tokens_total" => "input_tokens",
                "compress_latency_ms_avg" => "latency_avg_ms",
                "compress_overhead_ms_avg" => "overhead_avg_ms",
                _ => continue,
            };
            map.insert(key.to_string(), val);
        }
    }
    // 计算压缩率
    let input = map.get("input_tokens").copied().unwrap_or(0.0);
    let saved = map.get("tokens_saved").copied().unwrap_or(0.0);
    if input > 0.0 && saved > 0.0 {
        map.insert("savings_percent".into(), saved / input);
        // 粗略估算费用节省（按 $3/1M tokens）
        map.insert("savings_usd".into(), saved * 3.0 / 1_000_000.0);
    }
    map
}
