use reqwest::Client;

use crate::cli::{Cli, Commands, EnvAction, CompressAction, SkillAction};
use crate::claude;
use crate::config::{self, AppConfig};
use crate::env_sync;
use crate::provider;
use crate::proxy;

pub async fn run(cli: Cli) -> anyhow::Result<()> {
    let command = cli.command.unwrap_or(default_command());
    match command {
        Commands::Init => cmd_init().await,
        Commands::Start { no_tray } => cmd_start(no_tray).await,
        Commands::Status => cmd_status(),
        Commands::List => cmd_list(),
        Commands::Use { provider } => cmd_use(&provider).await,
        Commands::Test => cmd_test().await,
        Commands::Doctor => cmd_doctor().await,
        Commands::Settings => cmd_settings().await,
        Commands::Env { action } => cmd_env(action),
        Commands::RestoreAnthropic => cmd_restore_anthropic(),
        Commands::RepairClaudeCode => cmd_repair_claude_code().await,
        Commands::Skill { action } => cmd_skill(action).await,
        Commands::SetupObsidian => cmd_setup_obsidian().await,
        Commands::Compression { action } => cmd_compress(action).await,
    }
}

fn default_command() -> Commands {
    #[cfg(windows)]
    {
        Commands::Start { no_tray: false }
    }
    #[cfg(not(windows))]
    {
        Commands::Start { no_tray: true }
    }
}

async fn cmd_init() -> anyhow::Result<()> {
    crate::paths::ensure_helper_dirs()?;
    let app = if crate::paths::helper_config_path()?.exists() {
        AppConfig::load()?
    } else {
        let app = AppConfig::default();
        app.save()?;
        app
    };
    claude::inject_proxy_config(&app)?;

    println!("✅ 初始化完成");
    println!("   配置目录: {}", crate::paths::helper_dir()?.display());
    println!("   当前模型: {} ({})", app.active, app.active_provider()?.name);
    println!("   代理地址: {}", app.proxy_base_url());
    println!();
    println!("下一步:");
    println!("  1. 右键托盘 → 🔑 设置 API Key（或 bull-doctor settings）");
    println!("  2. bull-doctor start    # Windows 默认带系统托盘");
    println!("  3. 完全退出并重新打开 Claude 桌面端（Code + Cowork 均走 Doctor Gateway）");
    Ok(())
}

async fn ensure_proxy_port_available(app: &AppConfig) -> anyhow::Result<()> {
    let addr = format!("{}:{}", app.proxy.host, app.proxy.port);
    let health_url = format!("http://{addr}/health");

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(1))
        .build()?;

    if let Ok(resp) = client.get(&health_url).send().await {
        if resp.status().is_success() {
            anyhow::bail!(
                "端口 {addr} 上已有 Bull Doctor 在运行。请在任务栏找到图标 → 退出后再启动，或直接使用托盘切换模型。"
            );
        }
    }

    match tokio::net::TcpListener::bind(&addr).await {
        Ok(listener) => {
            drop(listener);
            Ok(())
        }
        Err(_) => anyhow::bail!(
            "端口 {addr} 已被其他程序占用。请关闭占用该端口的程序。"
        ),
    }
}

async fn cmd_start(no_tray: bool) -> anyhow::Result<()> {
    let app = AppConfig::load()?;
    claude::inject_proxy_config(&app)?;
    // inject_proxy_config 已将同步后的配置写入磁盘，重新加载以获取最新 provider 元数据
    let app = AppConfig::load()?;

    #[cfg(windows)]
    if !no_tray {
        ensure_proxy_port_available(&app).await?;
        return crate::tray::run_with_proxy(app).await;
    }

    #[cfg(not(windows))]
    if !no_tray {
        println!("⚠️  系统托盘目前仅支持 Windows，将以 CLI 模式启动代理");
    }

    ensure_proxy_port_available(&app).await?;
    println!(
        "🚀 {} · {} · Ctrl+C 停止",
        app.proxy_base_url(),
        app.active_provider()?.name
    );
    proxy::start_server(app).await
}

fn cmd_status() -> anyhow::Result<()> {
    let app = AppConfig::load()?;
    let provider = app.active_provider()?;
    let key_status = match config::resolve_api_key(&provider.api_key_env) {
        Ok(_) => "已配置",
        Err(_) => "未配置",
    };

    println!("Bull Doctor 状态");
    println!("──────────────────────────────");
    println!("当前模型:   {} ({})", app.active, provider.name);
    println!("默认模型:   {}", provider.default_model);
    println!("代理地址:   {}", app.proxy_base_url());
    println!("上游地址:   {}", provider.base_url);
    println!("API Key:    {key_status} ({})", provider.api_key_env);
    println!(
        "Claude 配置: {}",
        if claude::claude_settings_uses_helper() {
            if claude::claude_proxy_port_matches(&app) {
                "已指向本地代理（端口一致）"
            } else {
                "已指向本地代理（⚠ 端口不一致，请重新同步）"
            }
        } else {
            "未配置（运行 bull-doctor init）"
        }
    );
    Ok(())
}

fn cmd_list() -> anyhow::Result<()> {
    let app = AppConfig::load()?;
    println!("可用模型预设:");
    for preset in provider::list_presets(&app) {
        let mark = if preset.id == app.active { "✓" } else { " " };
        println!(
            "  {mark} {:<10} {:<18} {}",
            preset.id, preset.default_model, preset.name
        );
    }
    Ok(())
}

async fn cmd_use(provider_id: &str) -> anyhow::Result<()> {
    let mut app = AppConfig::load()?;
    provider::get_preset(&app, provider_id)?;
    app.active = provider_id.to_string();
    app.save()?;
    claude::inject_proxy_config(&app)?;

    let provider = app.active_provider()?;
    println!("✅ 已切换到 {} ({})", provider.id, provider.name);
    println!("   默认模型: {}", provider.default_model);

    // 自动检测连接
    match config::resolve_api_key(&provider.api_key_env) {
        Ok(api_key) => {
            print!("🔍 检测连接... ");
            match crate::settings::test_api_key(provider, &api_key).await {
                Ok(()) => println!("✅ 连通"),
                Err(err) => println!("❌ 失败: {err:#}"),
            }
        }
        Err(_) => println!("⚠️  API Key 未配置，请先运行: bull-doctor env set {} <your-key>", provider.api_key_env),
    }

    if proxy::notify_running_proxy_reload(&app).await {
        println!("   代理已热更新");
    } else {
        println!("   代理未运行，下次 start 时生效");
    }
    Ok(())
}

async fn cmd_test() -> anyhow::Result<()> {
    let app = AppConfig::load()?;
    let provider = app.active_provider()?;
    let api_key = config::resolve_api_key(&provider.api_key_env)?;

    print!("正在测试 {} ... ", provider.name);
    match crate::settings::test_api_key(provider, &api_key).await {
        Ok(()) => {
            println!("✅ 成功");
            Ok(())
        }
        Err(err) => {
            println!("❌ 失败");
            Err(err)
        }
    }
}

async fn cmd_settings() -> anyhow::Result<()> {
    let app = AppConfig::load()?;
    let health_url = format!(
        "http://{}:{}/health",
        app.proxy.host, app.proxy.port
    );
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()?;

    match client.get(&health_url).send().await {
        Ok(resp) if resp.status().is_success() => {
            #[cfg(windows)]
            {
                crate::settings::open_settings_window(app.proxy.port);
                println!("✅ 已打开设置窗口");
                Ok(())
            }
            #[cfg(not(windows))]
            {
                let settings_url = format!("http://{}:{}/admin/settings", app.proxy.host, app.proxy.port);
                match open::that(&settings_url) {
                    Ok(()) => {
                        println!("✅ 已在浏览器中打开设置页: {settings_url}");
                        Ok(())
                    }
                    Err(err) => {
                        anyhow::bail!("无法打开浏览器: {err}。请手动访问 {settings_url}")
                    }
                }
            }
        }
        _ => anyhow::bail!(
            "本地代理未运行。请先运行 bull-doctor start，再右键托盘 → 🔑 设置 API Key"
        ),
    }
}

async fn cmd_doctor() -> anyhow::Result<()> {
    let mut ok = true;
    println!("Bull Doctor 诊断");
    println!("──────────────────────────────");

    if crate::paths::helper_dir()?.exists() {
        println!("✅ 配置目录存在");
    } else {
        println!("⚠️  配置目录不存在，运行 bull-doctor init");
        ok = false;
    }

    if claude::claude_settings_exists() {
        println!("✅ Claude Code settings.json 存在");
    } else {
        println!("⚠️  Claude Code settings.json 不存在，运行 bull-doctor init");
        ok = false;
    }

    if let Ok(key) = config::resolve_api_key("DEEPSEEK_API_KEY")
        .or_else(|_| config::resolve_api_key("ANTHROPIC_API_KEY"))
    {
        let preview = if key.len() > 8 {
            format!("{}...{}", &key[..4], &key[key.len() - 4..])
        } else {
            "***".into()
        };
        println!("✅ API Key 已配置 ({preview})");
    } else {
        println!("❌ API Key 未配置，请右键托盘 → 🔑 设置 API Key");
        ok = false;
    }

    #[cfg(windows)]
    if env_sync::windows_user_env_is_set("ANTHROPIC_API_KEY") {
        println!("✅ Windows 用户环境变量 ANTHROPIC_API_KEY 已设置");
    } else {
        println!("⚠️  Windows 用户环境变量 ANTHROPIC_API_KEY 未设置，请运行 init 后重启 Claude Code");
        ok = false;
    }

    if claude::claude_settings_uses_helper() {
        println!("✅ Claude Code 已指向本地代理");
    } else {
        println!("⚠️  Claude Code 尚未指向本地代理");
        ok = false;
    }

    let app = AppConfig::load()?;

    if claude::claude_settings_uses_helper() {
        match claude::read_helper_base_url() {
            Ok(url) if claude::claude_proxy_port_matches(&app) => {
                println!("✅ settings.json 代理地址与 config.json 一致 ({url})");
            }
            Ok(url) => {
                println!(
                    "❌ settings.json 代理地址不一致: {url} ≠ {}",
                    app.proxy_base_url()
                );
                println!(
                    "   期望端口 {}，请托盘「重新同步配置」后完全退出并重启 Claude Code",
                    config::DEFAULT_PORT
                );
                ok = false;
            }
            Err(_) => {
                println!("❌ 无法读取 settings.json 中的 ANTHROPIC_BASE_URL");
                println!("   运行 bull-doctor init 修复");
                ok = false;
            }
        }

        if claude::dual_surface_synced(&app) {
            let gateway = claude::read_desktop_gateway_base_url().unwrap_or_default();
            println!("✅ Code + Cowork 双通道已同步到 Doctor");
            println!("   Code  → {}", app.proxy_base_url());
            println!("   Cowork → {gateway}");
        } else {
            if !claude::desktop_gateway_matches(&app) {
                match claude::read_desktop_gateway_base_url() {
                    Ok(url) => {
                        println!(
                            "❌ Cowork Gateway 地址不一致: {url} ≠ {}",
                            format!("{}/claude-desktop", app.proxy_base_url())
                        );
                    }
                    Err(_) => println!("❌ Cowork Gateway 未配置"),
                }
            }
            if !claude::desktop_app_uses_third_party_mode() {
                println!("❌ Claude Desktop 未启用 3P 模式（deploymentMode=3p）");
            }
            println!("   请托盘「重新同步配置」（会自动退出 Claude 桌面端）");
            ok = false;
        }
    }

    let provider = app.active_provider()?;
    match config::resolve_api_key(&provider.api_key_env) {
        Ok(_) => println!("✅ 当前模型 Key 已配置 ({})", provider.api_key_env),
        Err(_) => {
            println!("❌ 当前模型 Key 未配置，请右键托盘 → 🔑 设置 API Key");
            ok = false;
        }
    }

    let health_url = format!(
        "http://{}:{}/health",
        app.proxy.host, app.proxy.port
    );
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()?;
    match client.get(&health_url).send().await {
        Ok(resp) if resp.status().is_success() => println!("✅ 本地代理正在运行"),
        _ => {
            println!("⚠️  本地代理未运行，运行: bull-doctor start");
            println!(
                "   默认代理地址: http://{}:{} （健康检查 /health）",
                app.proxy.host, config::DEFAULT_PORT
            );
            ok = false;
        }
    }

    match claude::ccd_binary::status() {
        Ok(claude::ccd_binary::CcdBinaryStatus::Ready { version, path }) => {
            println!("✅ Claude Code 组件已就绪 ({version})");
            println!("   {}", path.display());
        }
        Ok(claude::ccd_binary::CcdBinaryStatus::Missing {
            expected_version,
            root,
        }) => {
            println!("❌ Claude Code 组件未就绪（Desktop 会报 binary not available）");
            if let Some(version) = expected_version {
                println!("   需要版本: {version}");
            }
            println!("   目录: {}", root.display());
            println!("   修复: bull-doctor repair-claude-code");
            ok = false;
        }
        Err(err) => {
            println!("⚠️  无法检测 Claude Code 组件: {err:#}");
        }
    }

    if ok {
        println!();
        println!("一切正常，可以运行 Claude Code 了。");
    } else {
        println!();
        println!("发现一些问题，请按上方提示修复。");
    }
    Ok(())
}

fn cmd_env(action: EnvAction) -> anyhow::Result<()> {
    match action {
        EnvAction::Set { key, value } => {
            config::save_env_value(&key, &value)?;
            println!("✅ 已保存 {key}");
            Ok(())
        }
    }
}

fn cmd_restore_anthropic() -> anyhow::Result<()> {
    claude::restore_anthropic_official()?;
    crate::actions::kill_claude_desktop()?;
    println!("✅ 已恢复 Anthropic 官方 Claude Code 配置，并退出 Claude 桌面端");
    println!("   请重新打开 Claude 桌面端");
    Ok(())
}

async fn cmd_repair_claude_code() -> anyhow::Result<()> {
    claude::ccd_binary::repair_with_download().await
}

async fn cmd_skill(action: SkillAction) -> anyhow::Result<()> {
    match action {
        SkillAction::Install { repo } => {
            println!("正在从 GitHub 安装 Skill: {repo} ...");
            match crate::skills::install_skill(&repo).await {
                Ok(skill) => {
                    println!("✅ Skill 安装成功: {}", skill.id);
                    println!("   目录: {}", skill.directory);
                    println!("   已同步到 Claude Code skills 目录");
                    Ok(())
                }
                Err(err) => {
                    println!("❌ 安装失败: {err:#}");
                    Err(err)
                }
            }
        }
        SkillAction::Uninstall { skill_id } => {
            match crate::skills::uninstall_skill(&skill_id) {
                Ok(()) => {
                    println!("✅ Skill 已卸载: {skill_id}");
                    Ok(())
                }
                Err(err) => {
                    println!("❌ 卸载失败: {err:#}");
                    Err(err)
                }
            }
        }
        SkillAction::List => {
            match crate::skills::list_skills() {
                Ok(skills) => {
                    if skills.is_empty() {
                        println!("暂无已安装的 Skill");
                        println!();
                        println!("安装示例:");
                        println!("  bull-doctor skill install anthropics/skills");
                    } else {
                        println!("已安装的 Skills:");
                        for s in &skills {
                            println!("  • {} — {}", s.id, s.description);
                        }
                    }
                    Ok(())
                }
                Err(err) => {
                    println!("❌ 获取列表失败: {err:#}");
                    Err(err)
                }
            }
        }
        SkillAction::Sync => {
            match crate::skills::sync_all_to_tools() {
                Ok(()) => {
                    println!("✅ 所有 Skill 已同步到所有工具");
                    Ok(())
                }
                Err(err) => {
                    println!("❌ 同步失败: {err:#}");
                    Err(err)
                }
            }
        }
    }
}

async fn cmd_setup_obsidian() -> anyhow::Result<()> {
    use crate::paths;
    use crate::config::AppConfig;

    let app = AppConfig::load()?;
    let proxy_url = app.proxy_base_url();
    let provider = app.active_provider()?;

    println!("正在搜索 Obsidian vault 中的 Claudian 插件...");

    let candidates = paths::obsidian_vault_candidates();
    let mut found = false;

    for vault in &candidates {
        let plugin_dir = vault.join(".obsidian").join("plugins").join("claudian");
        if !plugin_dir.exists() {
            continue;
        }

        // Claudian 插件配置文件 data.json
        let data_path = plugin_dir.join("data.json");
        if !data_path.exists() {
            // 创建默认配置
            let default_config = serde_json::json!({
                "apiKey": "",
                "baseUrl": &proxy_url,
                "model": provider.default_model,
                "provider": "custom",
            });
            std::fs::write(&data_path, serde_json::to_string_pretty(&default_config)?)?;
            println!("✅ 已创建 Claudian 配置: {}", vault.display());
            println!("   Base URL: {proxy_url}");
            println!("   Model: {}", provider.default_model);
            println!("   请在 Obsidian 中打开 Claudian 插件设置，填入 API Key。");
            found = true;
        } else {
            // 更新现有配置
            let raw = std::fs::read_to_string(&data_path)?;
            if let Ok(mut config) = serde_json::from_str::<serde_json::Value>(&raw) {
                config["baseUrl"] = serde_json::Value::String(proxy_url.clone());
                config["provider"] = serde_json::Value::String("custom".into());
                if config.get("model").is_none() {
                    config["model"] = serde_json::Value::String(provider.default_model.clone());
                }
                std::fs::write(&data_path, serde_json::to_string_pretty(&config)?)?;
                println!("✅ 已更新 Claudian 配置: {}", vault.display());
                println!("   Base URL: {proxy_url}");
                found = true;
            }
        }
    }

    if !found {
        println!("⚠️  未找到 Obsidian vault 或 Claudian 插件");
        println!();
        println!("请确保:");
        println!("  1. Obsidian 已安装");
        println!("  2. 已安装 Claudian 插件（设置 → 第三方插件 → 浏览 → 搜索 claudian）");
        println!("  3. 至少打开过一次 Obsidian vault");
        println!();
        println!("手动配置方式:");
        println!("  在 Claudian 插件设置中:");
        println!("    Provider: custom");
        println!("    Base URL: {proxy_url}");
        println!("    API Key: （填入你的 API Key）");
    }

    Ok(())
}

async fn cmd_compress(action: CompressAction) -> anyhow::Result<()> {
    let handle = crate::compress::new_compress_handle();
    match action {
        CompressAction::Start { port } => {
            let app = AppConfig::load().unwrap_or_default();
            let p = if port != 8787 { port } else { app.compress.port };
            crate::compress::start_compress(p, app.proxy.port, &handle).await?;
            println!("✅ 压缩代理已启动");
            println!("   端口: {}", p);
        }
        CompressAction::Stop => {
            crate::compress::stop_compress(&handle).await?;
            println!("✅ 压缩代理已停止");
        }
        CompressAction::Status => {
            let app = AppConfig::load().unwrap_or_default();
            let status = crate::compress::compress_status(&handle, app.compress.port);
            let running = status.get("running").and_then(|v| v.as_bool()).unwrap_or(false);
            println!("Compression 模式: 嵌入式（内置在 Doctor 中）");
            println!("配置端口: {}", app.compress.port);
            println!("自动启动: {}", if app.compress.auto_start { "是" } else { "否" });
            println!("运行状态: {}", if running { "运行中" } else { "未运行" });
        }
    }
    Ok(())
}
