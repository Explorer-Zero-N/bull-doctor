# Bull Doctor

> 让 **Claude Code 桌面端，Claude code CLI，Openclaw，Hermes Agent** 一键切换到 DeepSeek、通义千问、智谱、Kimi、MiniMax 等国产大模型，一键安装，管理skill，压缩缓存节省token的本地代理工具。

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Release](https://img.shields.io/github/v/release/Explorer-Zero-N/bull-doctor)](https://github.com/Explorer-Zero-N/bull-doctor/releases)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS-lightgrey)]()

基于 [claude-code-helper](https://github.com/xqnode/claude-code-helper)（[xqnode](https://github.com/xqnode)）fork 修改，保留原始 MIT 许可，详见 [licenses/](licenses/)。

---

## ✨ 功能亮点

- **零门槛**：双击安装 → 自动配置 → 托盘右键选模型 → 开始用 Claude Code，全程不需要打开终端
- **多厂商支持**：DeepSeek、千问、智谱、Kimi、MiniMax、小米 MiMo、Ollama、LM Studio、自定义中转站
- **本地模型友好**：Ollama / LM Studio 一键接入，自动优化 KV 缓存（禁用变动计费头）
- **多工具共享**：Claude Code、Claude Desktop、OpenClaw、Hermes 可同时使用，各自独立选厂商
- **Headroom 上下文压缩**：集成 [Headroom](https://github.com/chopratejas/headroom) 代理，压缩工具输出、日志等冗余内容，减少 Token 消耗
- **可视化管理**：系统托盘菜单 + WebView2 设置窗口 + 统计面板 + 请求日志
- **一键诊断**：`bull-doctor doctor` 自动检查配置、API Key、代理、Claude Code 组件
- **Skills 管理**：从 GitHub 或本地文件夹安装 Claude Code Skills

---

## 🏗️ 工作原理

```
Claude Code ─→ http://127.0.0.1:25573/v1/messages ─→ 代理转换格式 ─→ 上游大模型 API
Claude Desktop ─→ /claude-desktop/v1/messages ─→ ─────────────────→ 上游大模型 API
OpenClaw   ─→ /openclaw/v1/chat/completions  ─→ 代理透传 ────────→ 上游大模型 API
Hermes     ─→ /hermes/v1/chat/completions    ─→ 代理透传 ────────→ 上游大模型 API
```

Bull Doctor 在本地 `127.0.0.1:25573` 启动代理服务：
1. **自动写入** `~/.claude/settings.json` 的 `env` 块（`ANTHROPIC_BASE_URL`、模型映射等）
2. **接收** Claude Code 的 Anthropic Messages API 请求
3. **转发**到上游 API：DeepSeek 走原生 Anthropic API，其他厂商自动转换为 OpenAI Chat Completions 格式

---

## 📦 支持的厂商

| ID | 名称 | 默认模型 | API Key 环境变量 | 备注 |
|----|------|---------|-----------------|------|
| `deepseek` | DeepSeek | deepseek-v4-pro | `DEEPSEEK_API_KEY` | 原生 Anthropic API |
| `qwen` | 千问 | qwen3.7-max | `DASHSCOPE_API_KEY` | Chat Completions 转换 |
| `zhipu` | 智谱 | glm-5.1 | `ZHIPU_API_KEY` | Chat Completions 转换 |
| `kimi` | Kimi | kimi-k2.6 | `MOONSHOT_API_KEY` | Chat Completions 转换 |
| `minimax` | MiniMax | minimax-m3 | `MINIMAX_API_KEY` | Chat Completions 转换 |
| `mimo` | 小米 MiMo | mimo-v2.5-pro | `MIMO_API_KEY` | Chat Completions 转换 |
| `ollama` | Ollama | llama3.1 | 无需 Key | 本地推理，自动 KV 缓存优化 |
| `lmstudio` | LM Studio | local-model | 无需 Key | 本地推理，自动 KV 缓存优化 |
| `custom` | 自定义中转站 | claude-opus-4-8 | `CUSTOM_API_KEY` | 支持多种推理格式 |

---

## 🚀 快速开始

### 第 1 步：下载安装

去 [GitHub Releases](https://github.com/Explorer-Zero-N/bull-doctor/releases) 下载：

| 平台 | 安装版 | 便携版 |
|------|--------|--------|
| **Windows** | `BullDoctor-x.x.x-Setup.exe` — 双击一路下一步 | `BullDoctor-x.x.x-win64.zip` — 解压后双击 `bull-doctor.exe` |
| **macOS** | `BullDoctor-x.x.x-macos.dmg` — 拖入应用程序 | `BullDoctor-x.x.x-macos.app.zip` — 解压后运行 `Bull Doctor.app` |

> macOS 当前以 CLI 代理模式运行（无菜单栏托盘）；Windows 支持完整托盘与设置窗口。

### 第 2 步：填 API Key

首次启动后，右键托盘 → **设置…** → 选择厂商 → 粘贴 API Key → 保存。

也可以用命令行：
```bash
bull-doctor env set DEEPSEEK_API_KEY sk-xxxxxxxxxxxxxxxx
```

### 第 3 步：重启 Claude Code

**完全退出** Claude 桌面端后重新打开（或托盘 → **重新同步配置**，会自动退出 Claude）。

---

## 📋 CLI 命令

| 命令 | 说明 |
|------|------|
| `bull-doctor` | Windows 默认启动托盘 + 代理 |
| `bull-doctor init` | 初始化并写入 Claude Code 配置 |
| `bull-doctor start` | 启动代理（`--no-tray` 仅 CLI 模式） |
| `bull-doctor list` | 列出可用厂商 |
| `bull-doctor use <id>` | 切换厂商（如 `deepseek`、`qwen`、`ollama`） |
| `bull-doctor test` | 测试上游 API 连通性 |
| `bull-doctor doctor` | 一键诊断所有配置 |
| `bull-doctor settings` | 打开设置窗口 |
| `bull-doctor env set KEY value` | 命令行保存 API Key |
| `bull-doctor headroom start/stop` | 管理 Headroom 上下文压缩代理 |
| `bull-doctor skill install <repo>` | 安装 Claude Code Skill |
| `bull-doctor restore-anthropic` | 恢复 Anthropic 官方配置 |
| `bull-doctor repair-claude-code` | 修复 Claude Code CLI 组件 |

完整命令文档见 [docs/USER_MANUAL.md](docs/USER_MANUAL.md)。

---

## 🗂️ 配置目录

| 路径 | 用途 |
|------|------|
| `~/.bull-doctor/config.json` | 厂商、代理端口、工具配置 |
| `~/.bull-doctor/.env` | API Key 存储 |
| `~/.bull-doctor/skills/` | 已安装的 Skills |
| `~/.bull-doctor/request-log.sqlite` | 请求日志（SQLite） |
| `~/.bull-doctor/backups/` | settings.json 自动备份 |
| `~/.claude/settings.json` | Claude Code 用户配置（自动注入） |

---

## 🔧 开发

### 环境要求

- Rust stable（1.80+）
- Windows 10/11（完整功能：托盘 + WebView2 设置窗口）
- macOS 12+（CLI 代理模式）

### 从源码构建

```bash
# 初始化配置
cargo run -- init

# 启动代理（带托盘）
cargo run -- start

# 仅 CLI 模式
cargo run -- start --no-tray

# 运行测试
cargo test
```

### 打包发布

```powershell
# Windows — ZIP + Inno Setup 安装包
.\scripts\build-all.bat

# macOS — Universal .app + DMG（在 Mac 上）
./scripts/build-macos-release.sh
```

### 自动发版

推送 `v*` 标签触发 GitHub Actions 自动构建四端产物并发布：

```bash
git tag v0.3.0
git push origin v0.3.0
```

详见 [RELEASE.md](RELEASE.md)。

---

## 📊 项目结构

```
bull-doctor/
├── src/                    # 主程序源码
│   ├── main.rs             # 入口
│   ├── cli.rs              # CLI 参数定义
│   ├── commands.rs         # 命令分发
│   ├── proxy/              # 核心代理（Anthropic ↔ Chat Completions 转换）
│   ├── provider/           # 厂商预设、模型列表
│   ├── claude/             # Claude Code settings.json 注入
│   ├── settings/           # WebView2 设置窗口
│   ├── logs/               # 请求日志窗口
│   ├── stats/              # 统计面板
│   ├── skills/             # Skills 管理
│   └── tray.rs             # 系统托盘
├── crates/
│   ├── headroom-core/      # Headroom 压缩引擎
│   └── headroom-proxy/     # Headroom 代理服务
├── installer/              # 安装器配置（Inno Setup / macOS Info.plist）
├── scripts/                # 构建与发版脚本
├── assets/                 # 图标、截图
└── docs/                   # 文档（用户手册、本地开发指南）
```

---

## 📖 文档

- [用户手册](docs/USER_MANUAL.md) — 完整使用指南，包含所有 CLI 命令、设置窗口、Skills 管理、Headroom 压缩等
- [本地开发](docs/local-dev.md) — 开发环境搭建、调试、发布构建
- [发版说明](RELEASE.md) — GitHub Actions 自动发版、本地构建、版本规则
- [更新日志](CHANGELOG.md) — 版本变更记录

---

## 🙏 致谢

- [claude-code-helper](https://github.com/xqnode/claude-code-helper) by [xqnode](https://github.com/xqnode) — 本项目的基础
- [codex-helper](https://github.com/xqnode/codex-helper) — 原始架构来源
- [Headroom](https://github.com/chopratejas/headroom) — 上下文压缩引擎

---

## 📄 License

MIT — 详见 [LICENSE](LICENSE)。

本项目 fork 自 [claude-code-helper](https://github.com/xqnode/claude-code-helper)，原始许可保留在 [licenses/LICENSE.claude-code-helper](licenses/LICENSE.claude-code-helper)。
