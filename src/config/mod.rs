use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::paths;

#[allow(dead_code)]
pub const PROVIDER_ID: &str = "bull-doctor";
pub const DUMMY_ENV_KEY: &str = "BULL_DOCTOR_DUMMY_KEY";
pub const DEFAULT_HOST: &str = "127.0.0.1";
/// 本地代理固定端口；Claude Code 通过 ANTHROPIC_BASE_URL=http://127.0.0.1:25573 访问 Helper。
pub const DEFAULT_PORT: u16 = 25573;
pub const DEFAULT_MODEL_REASONING_EFFORT: &str = "medium";
/// 上游 tool 消息内容最大字符数；0 表示不截断（默认，优先稳定性）。
pub const DEFAULT_TOOL_OUTPUT_MAX_CHARS: usize = 0;
/// 上游连接阶段超时（秒）。
pub const DEFAULT_UPSTREAM_CONNECT_TIMEOUT_SECS: u64 = 30;
/// 流式响应读空闲超时：连续这么久没有新 chunk 则断开（秒）。
pub const DEFAULT_UPSTREAM_STREAM_READ_IDLE_TIMEOUT_SECS: u64 = 600;
/// 非流式请求总超时（秒）；仅用于一次性等待完整响应。
pub const DEFAULT_UPSTREAM_REQUEST_TIMEOUT_SECS: u64 = 600;

const VALID_MODEL_REASONING_EFFORTS: &[&str] =
    &["none", "minimal", "low", "medium", "high", "xhigh", "max"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    pub host: String,
    pub port: u16,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            host: DEFAULT_HOST.to_string(),
            port: DEFAULT_PORT,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub api_key_env: String,
    /// Claude Code settings / 托盘菜单中展示的模型 slug
    pub default_model: String,
    /// 实际上游 API 的 model 名称（可与 default_model 不同）
    #[serde(default)]
    pub api_model: String,
    /// anthropic = 上游原生 Anthropic API；chat = OpenAI Chat Completions（由代理转换）
    pub wire_api: String,
    /// 用户是否在设置页改过 Base URL（为 true 时 sync 不再覆盖为官方默认）。
    #[serde(default)]
    pub base_url_customized: bool,
    /// API 获取的自定义模型列表；非空时在托盘菜单中替换内置 popular_models。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub custom_models: Vec<CustomModelEntry>,
    /// 显式指定推理格式（deepseek / qwen / zhipu / kimi / minimax / mimo / openrouter）。
    /// 为空时对 custom provider 自动根据 base_url 推断。
    #[serde(default)]
    pub reasoning_style: String,
}

/// 存储从上游 API 获取的模型条目（用于托盘菜单展示）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomModelEntry {
    pub id: String,
    pub display_name: String,
}

impl ProviderConfig {
    pub fn upstream_model(&self) -> &str {
        if self.api_model.trim().is_empty() {
            &self.default_model
        } else {
            &self.api_model
        }
    }

    pub fn uses_anthropic_upstream(&self) -> bool {
        self.wire_api == "anthropic"
    }

    /// 根据 Base URL 自动修正 wire_api（以 /anthropic 结尾 → anthropic）。
    pub fn detect_wire_api_from_base_url(&mut self) {
        if self.base_url.trim().ends_with("/anthropic") {
            self.wire_api = "anthropic".into();
        }
    }

    pub fn catalog_display_name(&self) -> String {
        match (self.id.as_str(), self.default_model.as_str()) {
            ("deepseek", "deepseek-v4-flash") => "DeepSeek V4 Flash".into(),
            ("deepseek", "deepseek-v4-pro") => "DeepSeek V4 Pro".into(),
            ("qwen", "qwen3.7-max") => "千问 3.7 Max".into(),
            ("qwen", "qwen3.7-plus") => "千问 3.7 Plus".into(),
            ("zhipu", "glm-5") => "GLM-5".into(),
            ("zhipu", "glm-5.1") => "GLM-5.1".into(),
            ("zhipu", "glm-4.7") => "GLM-4.7".into(),
            ("kimi", "kimi-k2.6") => "Kimi K2.6".into(),
            ("minimax", "minimax-m3") => "MiniMax M3".into(),
            ("mimo", "mimo-v2.5-pro") => "MiMo V2.5 Pro".into(),
            ("mimo", "mimo-v2.5") => "MiMo V2.5".into(),
            ("custom", "claude-opus-4-8") => "Claude Opus 4.8".into(),
            ("custom", "claude-opus-4-7") => "Claude Opus 4.7".into(),
            ("custom", "claude-sonnet-4-6") => "Claude Sonnet 4.6".into(),
            _ => self.name.clone(),
        }
    }
}

fn default_model_reasoning_effort() -> String {
    DEFAULT_MODEL_REASONING_EFFORT.to_string()
}

fn default_tool_output_max_chars() -> usize {
    DEFAULT_TOOL_OUTPUT_MAX_CHARS
}

pub const DEFAULT_COMPRESS_PORT: u16 = 8787;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressConfig {
    /// 压缩代理监听端口
    #[serde(default = "default_compress_port")]
    pub port: u16,
    /// Doctor 启动时自动启动 Compression
    #[serde(default)]
    pub auto_start: bool,
}

fn default_compress_port() -> u16 {
    DEFAULT_COMPRESS_PORT
}

impl Default for CompressConfig {
    fn default() -> Self {
        Self {
            port: DEFAULT_COMPRESS_PORT,
            auto_start: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolConfig {
    /// 是否启用该工具的代理（关闭则工具直连官方 API）
    #[serde(default = "default_tool_enabled")]
    pub enabled: bool,
    /// 该工具当前使用的 provider ID
    #[serde(default)]
    pub active_provider: String,
}

fn default_tool_enabled() -> bool {
    true
}

impl Default for ToolConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            active_provider: String::new(),
        }
    }
}

pub const TOOL_CLAUDE_CODE: &str = "claude-code";
pub const TOOL_CLAUDE_DESKTOP: &str = "claude-desktop";
pub const TOOL_OPENCLAW: &str = "openclaw";
pub const TOOL_HERMES: &str = "hermes";

pub const ALL_TOOLS: &[&str] = &[TOOL_CLAUDE_CODE, TOOL_CLAUDE_DESKTOP, TOOL_OPENCLAW, TOOL_HERMES];

pub fn tool_display_name(tool_id: &str) -> &'static str {
    match tool_id {
        TOOL_CLAUDE_CODE => "Claude Code",
        TOOL_CLAUDE_DESKTOP => "Claude Desktop",
        TOOL_OPENCLAW => "OpenClaw",
        TOOL_HERMES => "Hermes",
        _ => "Unknown",
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub proxy: ProxyConfig,
    pub active: String,
    pub providers: HashMap<String, ProviderConfig>,
    /// 每个工具的独立配置（provider、启用状态）
    #[serde(default)]
    pub tools: HashMap<String, ToolConfig>,
    /// 默认推理档位，映射为各厂商 Chat API 的 thinking / reasoning 参数。
    #[serde(default = "default_model_reasoning_effort")]
    pub model_reasoning_effort: String,
    /// 发往上游前截断超长 `role: tool` 文本（head+tail）；0 = 关闭。
    #[serde(default = "default_tool_output_max_chars")]
    pub tool_output_max_chars: usize,
    /// 设置窗口上次关闭时的宽度（像素），None 则用默认值
    #[serde(default)]
    pub settings_window_width: Option<u32>,
    /// 设置窗口上次关闭时的高度（像素），None 则用默认值
    #[serde(default)]
    pub settings_window_height: Option<u32>,
    /// 上下文压缩代理配置
    #[serde(default)]
    pub compress: CompressConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        let mut providers = HashMap::new();
        for preset in crate::provider::presets::builtin_presets() {
            providers.insert(preset.id.clone(), preset);
        }
        Self {
            proxy: ProxyConfig::default(),
            active: "deepseek".to_string(),
            providers,
            tools: HashMap::new(),
            model_reasoning_effort: default_model_reasoning_effort(),
            tool_output_max_chars: default_tool_output_max_chars(),
            settings_window_width: None,
            settings_window_height: None,
            compress: CompressConfig::default(),
        }
    }
}

pub fn normalize_model_reasoning_effort(value: &str) -> String {
    let effort = value.trim().to_ascii_lowercase();
    if VALID_MODEL_REASONING_EFFORTS.contains(&effort.as_str()) {
        effort
    } else {
        DEFAULT_MODEL_REASONING_EFFORT.to_string()
    }
}

impl AppConfig {
    pub fn load() -> anyhow::Result<Self> {
        let path = paths::helper_config_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = std::fs::read_to_string(&path)?;
        let mut app: AppConfig = serde_json::from_str(&raw)?;
        crate::provider::sync_builtin_presets(&mut app);
        app.ensure_tool_entries();
        Ok(app)
    }

    pub fn save(&self) -> anyhow::Result<()> {
        paths::ensure_helper_dirs()?;
        let path = paths::helper_config_path()?;
        let raw = serde_json::to_string_pretty(self)?;
        write_atomic(&path, &raw)
    }

    pub fn clear_all_settings() -> anyhow::Result<Self> {
        let current = Self::load().unwrap_or_default();
        let mut app = Self::default();
        app.proxy.host = current.proxy.host.clone();
        app.proxy.port = current.proxy.port;
        app.save()?;

        let env_path = paths::helper_env_path()?;
        if env_path.exists() {
            std::fs::remove_file(env_path)?;
        }
        Ok(app)
    }

    pub fn active_provider(&self) -> anyhow::Result<&ProviderConfig> {
        self.providers
            .get(&self.active)
            .ok_or_else(|| anyhow::anyhow!("未找到当前模型预设: {}", self.active))
    }

    /// 获取指定工具的 provider；若该工具未独立配置则回退到全局 active。
    pub fn active_provider_for(&self, tool_id: &str) -> anyhow::Result<&ProviderConfig> {
        // 先确定 provider_id（需要 clone 避免 borrow checker 冲突）
        let provider_id = self
            .tools
            .get(tool_id)
            .and_then(|t| {
                if t.active_provider.is_empty() {
                    None
                } else {
                    Some(t.active_provider.clone())
                }
            })
            .unwrap_or_else(|| self.active.clone());
        self.providers
            .get(&provider_id)
            .ok_or_else(|| anyhow::anyhow!("未找到工具 {tool_id} 的模型预设: {provider_id}"))
    }

    /// 获取指定工具的 ToolConfig（不存在则返回默认）。
    pub fn tool_config(&self, tool_id: &str) -> ToolConfig {
        self.tools.get(tool_id).cloned().unwrap_or_default()
    }

    /// 确保所有工具都有默认条目（兼容旧配置迁移）。
    pub fn ensure_tool_entries(&mut self) {
        for &id in ALL_TOOLS {
            self.tools.entry(id.to_string()).or_default();
        }
    }

    pub fn normalized_model_reasoning_effort(&self) -> String {
        normalize_model_reasoning_effort(&self.model_reasoning_effort)
    }

    /// Claude Code 的 ANTHROPIC_BASE_URL 不带 /v1 后缀。
    pub fn proxy_base_url(&self) -> String {
        format!("http://{}:{}", self.proxy.host, self.proxy.port)
    }
}

pub fn normalize_base_url(url: &str) -> String {
    url.trim().trim_end_matches('/').to_string()
}

pub fn validate_base_url(url: &str) -> anyhow::Result<String> {
    let normalized = normalize_base_url(url);
    if normalized.is_empty() {
        anyhow::bail!("请填写 Base URL");
    }
    if !normalized.starts_with("http://") && !normalized.starts_with("https://") {
        anyhow::bail!("Base URL 需以 http:// 或 https:// 开头");
    }
    Ok(normalized)
}

pub fn build_upstream_client(timeout: std::time::Duration) -> anyhow::Result<reqwest::Client> {
    reqwest::Client::builder()
        .no_proxy()
        .connect_timeout(std::time::Duration::from_secs(
            DEFAULT_UPSTREAM_CONNECT_TIMEOUT_SECS,
        ))
        .timeout(timeout)
        .build()
        .map_err(Into::into)
}

/// 流式上游客户端：不设总超时，靠读空闲超时检测僵死连接。
pub fn build_upstream_streaming_client() -> anyhow::Result<reqwest::Client> {
    reqwest::Client::builder()
        .no_proxy()
        .connect_timeout(std::time::Duration::from_secs(
            DEFAULT_UPSTREAM_CONNECT_TIMEOUT_SECS,
        ))
        .read_timeout(std::time::Duration::from_secs(
            DEFAULT_UPSTREAM_STREAM_READ_IDLE_TIMEOUT_SECS,
        ))
        .tcp_keepalive(std::time::Duration::from_secs(60))
        .build()
        .map_err(Into::into)
}

pub fn build_proxy_upstream_clients() -> anyhow::Result<(reqwest::Client, reqwest::Client)> {
    let request_timeout =
        std::time::Duration::from_secs(DEFAULT_UPSTREAM_REQUEST_TIMEOUT_SECS);
    Ok((
        build_upstream_client(request_timeout)?,
        build_upstream_streaming_client()?,
    ))
}

pub fn write_atomic(path: &std::path::Path, content: &str) -> anyhow::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, content)?;
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    std::fs::rename(tmp, path)?;
    Ok(())
}

pub fn load_env_file() -> anyhow::Result<HashMap<String, String>> {
    let path = paths::helper_env_path()?;
    let mut map = HashMap::new();
    if !path.exists() {
        return Ok(map);
    }
    for line in std::fs::read_to_string(path)?.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            map.insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    Ok(map)
}

pub fn save_env_value(key: &str, value: &str) -> anyhow::Result<()> {
    save_env_values(&[(key, value)])
}

/// 批量写入多个环境变量，只做一次文件读写（比多次调用 save_env_value 快得多）。
pub fn save_env_values(pairs: &[(&str, &str)]) -> anyhow::Result<()> {
    paths::ensure_helper_dirs()?;
    let path = paths::helper_env_path()?;
    let mut map = load_env_file()?;
    for (key, value) in pairs {
        map.insert(key.to_string(), value.to_string());
    }
    let mut lines: Vec<String> = map
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect();
    lines.sort();
    write_atomic(&path, &format!("{}\n", lines.join("\n")))
}

pub fn resolve_api_key(env_key: &str) -> anyhow::Result<String> {
    if let Ok(value) = std::env::var(env_key) {
        if !value.trim().is_empty() {
            return Ok(value.trim().to_string());
        }
    }
    let file_env = load_env_file()?;
    file_env
        .get(env_key)
        .cloned()
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "未找到 API Key，请先运行: bull-doctor env set {env_key} <your-key>"
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_tool_output_max_chars_is_disabled() {
        assert_eq!(
            AppConfig::default().tool_output_max_chars,
            DEFAULT_TOOL_OUTPUT_MAX_CHARS
        );
    }

    #[test]
    fn proxy_upstream_clients_build_successfully() {
        let (regular, streaming) = build_proxy_upstream_clients().unwrap();
        let _ = regular;
        let _ = streaming;
    }

    #[test]
    fn default_proxy_uses_fixed_port() {
        let config = ProxyConfig::default();
        assert_eq!(config.host, DEFAULT_HOST);
        assert_eq!(config.port, DEFAULT_PORT);
        assert_eq!(
            AppConfig::default().proxy_base_url(),
            format!("http://{DEFAULT_HOST}:{DEFAULT_PORT}")
        );
    }

    #[test]
    fn default_model_reasoning_effort_is_medium() {
        assert_eq!(
            AppConfig::default().model_reasoning_effort,
            DEFAULT_MODEL_REASONING_EFFORT
        );
    }

    #[test]
    fn normalize_model_reasoning_effort_falls_back_to_medium() {
        assert_eq!(normalize_model_reasoning_effort("HIGH"), "high");
        assert_eq!(normalize_model_reasoning_effort("bogus"), "medium");
    }

    #[test]
    fn default_upstream_timeout_constants_are_sane() {
        assert!(DEFAULT_UPSTREAM_CONNECT_TIMEOUT_SECS > 0);
        assert!(DEFAULT_UPSTREAM_STREAM_READ_IDLE_TIMEOUT_SECS > 0);
        assert!(
            DEFAULT_UPSTREAM_REQUEST_TIMEOUT_SECS >= DEFAULT_UPSTREAM_STREAM_READ_IDLE_TIMEOUT_SECS
        );
    }
}
