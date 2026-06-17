use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "bull-doctor",
    about = "Bull Doctor - 轻量代理，让 Claude Code 使用 DeepSeek 等国产大模型",
    version
)]
pub struct Cli {
    /// 省略子命令时，Windows 默认执行 start（启动托盘与代理）
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// 初始化配置并写入 Claude Code settings.json
    Init,
    /// 启动本地代理（Windows 默认显示系统托盘）
    Start {
        #[arg(long, help = "不显示托盘，仅用命令行模式")]
        no_tray: bool,
    },
    /// 查看当前状态
    Status,
    /// 列出可用模型预设
    List,
    /// 切换到指定模型
    Use {
        provider: String,
    },
    /// 测试当前模型连通性
    Test,
    /// 一键诊断环境
    Doctor,
    /// 打开 API Key 设置窗口
    Settings,
    /// 设置 API Key（命令行，高级）
    Env {
        #[command(subcommand)]
        action: EnvAction,
    },
    /// 恢复 Anthropic 官方配置
    RestoreAnthropic,
    /// 修复 Claude Desktop 缺失的 Claude Code 组件（claude.exe）
    RepairClaudeCode,
    /// Skills 管理
    Skill {
        #[command(subcommand)]
        action: SkillAction,
    },
    /// 一键配置 Obsidian Claudian 插件
    SetupObsidian,
    /// 上下文压缩代理管理
    Compression {
        #[command(subcommand)]
        action: CompressAction,
    },
}

#[derive(Subcommand, Debug)]
pub enum SkillAction {
    /// 从 GitHub 安装 Skill
    Install {
        /// owner/repo 格式，如 anthropics/skills
        repo: String,
    },
    /// 卸载 Skill
    Uninstall {
        /// Skill ID，如 anthropics/skills
        skill_id: String,
    },
    /// 列出已安装的 Skill
    List,
    /// 同步所有 Skill 到 Claude Code
    Sync,
}

#[derive(Subcommand, Debug)]
pub enum CompressAction {
    /// 启动 压缩代理
    Start {
        #[arg(long, default_value = "8787", help = "代理监听端口")]
        port: u16,
    },
    /// 停止 压缩代理
    Stop,
    /// 查看 Compression 状态
    Status,
}

#[derive(Subcommand, Debug)]
pub enum EnvAction {
    /// 保存 API Key 到 ~/.bull-doctor/.env
    Set {
        key: String,
        value: String,
    },
}
