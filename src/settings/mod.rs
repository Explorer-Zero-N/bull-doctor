//! 可视化 API Key 设置窗口（Windows）。

mod api;

#[cfg(windows)]
mod window;

pub use api::{brand_icon_svg, settings_bootstrap, settings_check_config, settings_clear_all, settings_fetch_models, settings_page, settings_save, settings_test, test_api_key, settings_skills_bootstrap, settings_skills_install, settings_skills_install_local, settings_skills_uninstall, settings_skills_sync, compress_status, compress_start, compress_stop, compress_stats};

#[cfg(windows)]
pub use window::{
    close_settings_window, focus_settings_window,
    open_settings_on_loop, open_settings_window, SettingsWindow,
};

#[cfg(not(windows))]
pub fn open_settings_window(_proxy_port: u16) {
    eprintln!("设置窗口目前仅支持 Windows，请使用: bull-doctor env set DEEPSEEK_API_KEY sk-xxx");
}

pub fn signup_url(provider_id: &str) -> &'static str {
    match provider_id {
        "deepseek" => "https://platform.deepseek.com/",
        "qwen" => "https://dashscope.aliyun.com/",
        "kimi" | "moonshot" => "https://platform.moonshot.cn/",
        "zhipu" => "https://www.bigmodel.cn/",
        "minimax" => "https://platform.minimaxi.com/",
        "mimo" => "https://platform.xiaomimimo.com/",
        "custom" => "",
        _ => "",
    }
}

pub fn key_hint(provider_id: &str) -> &'static str {
    match provider_id {
        "deepseek" => "粘贴 sk- 开头的 Key",
        "qwen" => "粘贴 sk- 开头的 Key（DashScope）",
        "zhipu" => "粘贴 . 开头的 Key（智谱 API Key）",
        "kimi" => "粘贴 sk- 开头的 Key（Moonshot）",
        "minimax" => "粘贴 eyJ 开头的 Key（MiniMax JWT）",
        "mimo" => "粘贴 Bearer 开头的 Key 或直接填写 Token",
        "ollama" => "Ollama 本地服务无需 Key",
        "lmstudio" => "LM Studio 本地服务无需 Key",
        "custom" => "粘贴 API Key（格式由中转站决定）",
        _ => "粘贴 API Key",
    }
}
