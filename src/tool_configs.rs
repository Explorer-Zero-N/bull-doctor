use std::path::PathBuf;

use crate::config::ProviderConfig;

/// 当 compress代理 启动时，将所有工具的 API 地址改为指向 compress代理 端口。
/// 这样所有工具的请求都会经过 compress代理 压缩后再转发到 Helper。
pub fn switch_all_tools_to_compress(compress_port: u16) {
    let base = format!("http://127.0.0.1:{}", compress_port);
    // Hermes
    if let Ok(path) = hermes_config_path() {
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                let updated = update_yaml_field(&content, "base_url", &format!("{}/hermes/v1", base));
                let _ = crate::config::write_atomic(&path, &updated);
            }
        }
    }
    // OpenClaw
    if let Ok(path) = openclaw_config_path() {
        if path.exists() {
            if let Ok(raw) = std::fs::read_to_string(&path) {
                if let Ok(mut root) = serde_json::from_str::<serde_json::Value>(&raw) {
                    if let Some(helper) = root.pointer_mut("/models/providers/helper") {
                        if let Some(obj) = helper.as_object_mut() {
                            obj.insert("baseUrl".into(), serde_json::json!(format!("{}/openclaw/v1", base)));
                        }
                    }
                    if let Ok(out) = serde_json::to_string_pretty(&root) {
                        let _ = crate::config::write_atomic(&path, &format!("{}\n", out));
                    }
                }
            }
        }
    }
    tracing::info!("已将所有工具路由切换到 compress代理 (端口 {})", compress_port);
}

/// 当 compress代理 停止时，将所有工具的 API 地址恢复为指向 Helper 端口。
pub fn restore_all_tools_to_doctor(helper_port: u16) {
    let base = format!("http://127.0.0.1:{}", helper_port);
    // Hermes
    if let Ok(path) = hermes_config_path() {
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                let updated = update_yaml_field(&content, "base_url", &format!("{}/hermes/v1", base));
                let _ = crate::config::write_atomic(&path, &updated);
            }
        }
    }
    // OpenClaw
    if let Ok(path) = openclaw_config_path() {
        if path.exists() {
            if let Ok(raw) = std::fs::read_to_string(&path) {
                if let Ok(mut root) = serde_json::from_str::<serde_json::Value>(&raw) {
                    if let Some(helper) = root.pointer_mut("/models/providers/helper") {
                        if let Some(obj) = helper.as_object_mut() {
                            obj.insert("baseUrl".into(), serde_json::json!(format!("{}/openclaw/v1", base)));
                        }
                    }
                    if let Ok(out) = serde_json::to_string_pretty(&root) {
                        let _ = crate::config::write_atomic(&path, &format!("{}\n", out));
                    }
                }
            }
        }
    }
    tracing::info!("已将所有工具路由恢复到 Helper (端口 {})", helper_port);
}

/// 同步 Hermes 配置文件（config.yaml）。
pub fn sync_hermes_config(provider: &ProviderConfig, model: &str) -> anyhow::Result<()> {
    let config_path = hermes_config_path()?;
    if !config_path.exists() {
        tracing::debug!("Hermes 配置文件不存在，跳过同步: {}", config_path.display());
        return Ok(());
    }

    let content = std::fs::read_to_string(&config_path)?;
    let proxy_url = format!("http://127.0.0.1:25573/hermes/v1");
    let model_name = if model.is_empty() { &provider.default_model } else { model };

    let updated = update_yaml_field(&content, "base_url", &proxy_url);
    let updated = update_yaml_field(&updated, "default", model_name);
    let updated = update_yaml_field(&updated, "provider", &provider.id);

    crate::config::write_atomic(&config_path, &updated)?;
    tracing::info!("已同步 Hermes 配置: model={}, base_url={}", model_name, proxy_url);
    Ok(())
}

/// 同步 OpenClaw 配置文件（openclaw.json）。
/// 设置模型 provider + 将 Helper skills 目录加入 extraDirs。
pub fn sync_openclaw_config(provider: &ProviderConfig, model: &str) -> anyhow::Result<()> {
    let config_path = openclaw_config_path()?;
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut root: serde_json::Value = if config_path.exists() {
        let raw = std::fs::read_to_string(&config_path)?;
        serde_json::from_str(&raw).unwrap_or_else(|_| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    let proxy_url = "http://127.0.0.1:25573/openclaw/v1";
    let model_name = if model.is_empty() { &provider.default_model } else { model };

    let obj = root
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("openclaw.json 根节点必须是对象"))?;

    // 设置 models.providers.helper
    let providers = obj
        .entry("models")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("openclaw.json models 必须是对象"))?
        .entry("providers")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("openclaw.json providers 必须是对象"))?;

    providers.insert(
        "helper".into(),
        serde_json::json!({
            "baseUrl": proxy_url,
            "defaultModel": model_name,
        }),
    );

    // 将 Helper 的 skills 目录加入 skills.load.extraDirs
    let helper_skills_dir = crate::paths::skills_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    if !helper_skills_dir.is_empty() {
        let skills_load = obj
            .entry("skills")
            .or_insert_with(|| serde_json::json!({}))
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("openclaw.json skills 必须是对象"))?
            .entry("load")
            .or_insert_with(|| serde_json::json!({}))
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("openclaw.json skills.load 必须是对象"))?;

        let extra_dirs = skills_load
            .entry("extraDirs")
            .or_insert_with(|| serde_json::json!([]))
            .as_array_mut()
            .ok_or_else(|| anyhow::anyhow!("openclaw.json skills.load.extraDirs 必须是数组"))?;

        let dir_value = serde_json::Value::String(helper_skills_dir.clone());
        if !extra_dirs.contains(&dir_value) {
            extra_dirs.push(dir_value);
        }
    }

    let raw = serde_json::to_string_pretty(&root)?;
    crate::config::write_atomic(&config_path, &format!("{}\n", raw))?;
    tracing::info!("已同步 OpenClaw 配置: model={}, base_url={}", model_name, proxy_url);
    Ok(())
}

fn hermes_config_path() -> anyhow::Result<PathBuf> {
    // Windows: %LOCALAPPDATA%\hermes\config.yaml
    // Unix: ~/.config/hermes/config.yaml
    #[cfg(windows)]
    {
        let base = std::env::var("LOCALAPPDATA")
            .map(PathBuf::from)
            .or_else(|_| dirs::data_local_dir().ok_or_else(|| anyhow::anyhow!("无法定位 LOCALAPPDATA")))?;
        Ok(base.join("hermes").join("config.yaml"))
    }
    #[cfg(not(windows))]
    {
        Ok(dirs::config_dir()
            .ok_or_else(|| anyhow::anyhow!("无法定位配置目录"))?
            .join("hermes")
            .join("config.yaml"))
    }
}

fn openclaw_config_path() -> anyhow::Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("无法定位用户主目录"))?;
    Ok(home.join(".openclaw").join("openclaw.json"))
}

/// 简单的 YAML 字段更新（按行匹配 `key: value` 模式）。
fn update_yaml_field(content: &str, key: &str, value: &str) -> String {
    let escaped_value = if value.contains(' ') || value.contains(':') || value.contains('#') {
        format!("'{}'", value.replace('\'', "''"))
    } else {
        value.to_string()
    };

    let mut result = String::new();
    let mut found = false;

    for line in content.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with(&format!("{}:", key)) && !found {
            // 保持原始缩进
            let indent = &line[..line.len() - trimmed.len()];
            result.push_str(&format!("{}{}: {}\n", indent, key, escaped_value));
            found = true;
        } else {
            result.push_str(line);
            result.push('\n');
        }
    }

    // 如果没找到，在末尾添加
    if !found {
        result.push_str(&format!("{}: {}\n", key, escaped_value));
    }

    result
}
