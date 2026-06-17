use std::path::PathBuf;

pub fn helper_dir() -> anyhow::Result<PathBuf> {
    let dir = dirs::home_dir()
        .ok_or_else(|| anyhow::anyhow!("无法定位用户主目录"))?
        .join(".bull-doctor");
    Ok(dir)
}

pub fn helper_config_path() -> anyhow::Result<PathBuf> {
    Ok(helper_dir()?.join("config.json"))
}

pub fn helper_env_path() -> anyhow::Result<PathBuf> {
    Ok(helper_dir()?.join(".env"))
}

pub fn helper_backups_dir() -> anyhow::Result<PathBuf> {
    Ok(helper_dir()?.join("backups"))
}

pub fn helper_logs_dir() -> anyhow::Result<PathBuf> {
    Ok(helper_dir()?.join("logs"))
}

pub fn helper_request_log_path() -> anyhow::Result<PathBuf> {
    Ok(helper_dir()?.join("request-log.sqlite"))
}

/// Claude Code 用户配置目录，默认 ~/.claude，可通过 CLAUDE_CONFIG_DIR 覆盖。
pub fn claude_config_dir() -> anyhow::Result<PathBuf> {
    std::env::var("CLAUDE_CONFIG_DIR")
        .ok()
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".claude")))
        .ok_or_else(|| anyhow::anyhow!("无法定位 Claude Code 配置目录"))
}

pub fn claude_settings_path() -> anyhow::Result<PathBuf> {
    Ok(claude_config_dir()?.join("settings.json"))
}

pub fn ensure_helper_dirs() -> anyhow::Result<()> {
    std::fs::create_dir_all(helper_dir()?)?;
    std::fs::create_dir_all(helper_backups_dir()?)?;
    std::fs::create_dir_all(helper_logs_dir()?)?;
    std::fs::create_dir_all(skills_dir()?)?;
    Ok(())
}

pub fn skills_dir() -> anyhow::Result<PathBuf> {
    Ok(helper_dir()?.join("skills"))
}

pub fn skills_config_path() -> anyhow::Result<PathBuf> {
    Ok(helper_dir()?.join("skills.json"))
}

/// Claude Code 的 skills 安装目录
pub fn claude_code_skills_dir() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join(".claude").join("skills"))
        .unwrap_or_else(|| PathBuf::from(".claude/skills"))
}

/// Hermes 的 skills 安装目录
pub fn hermes_skills_dir() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join(".hermes").join("skills"))
        .unwrap_or_else(|| PathBuf::from(".hermes/skills"))
}

/// 返回所有支持 skills 的工具目录
pub fn all_skills_dirs() -> Vec<(&'static str, PathBuf)> {
    let dirs = vec![
        ("Claude Code", claude_code_skills_dir()),
        ("Hermes", hermes_skills_dir()),
    ];
    // Claude Desktop 共享 Claude Code 的 skills 目录，无需重复
    dirs
}

/// Obsidian vault 配置目录候选列表
pub fn obsidian_vault_candidates() -> Vec<PathBuf> {
    let home = dirs::home_dir();
    let mut candidates = Vec::new();
    if let Some(ref h) = home {
        // 常见 Obsidian vault 位置
        candidates.push(h.join("Documents").join("Obsidian"));
        candidates.push(h.join("Obsidian"));
        candidates.push(h.join("Documents").join("obsidian"));
        candidates.push(h.join("obsidian"));
        // 扫描 Documents 下任何 .obsidian 目录
        if let Ok(entries) = std::fs::read_dir(h.join("Documents")) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() && path.join(".obsidian").exists() {
                    candidates.push(path);
                }
            }
        }
    }
    candidates
}
