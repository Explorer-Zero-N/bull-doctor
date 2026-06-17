# Bull Doctor 使用手册

> 版本 v0.3.0 | 2026-06-17

---

## 目录

1. [简介](#1-简介)
2. [快速开始](#2-快速开始)
3. [支持的厂商](#3-支持的厂商)
4. [支持的工具](#4-支持的工具)
5. [CLI 命令详解](#5-cli-命令详解)
6. [设置窗口使用指南](#6-设置窗口使用指南)
7. [托盘菜单使用指南](#7-托盘菜单使用指南)
8. [Skills 管理](#8-skills-管理)
9. [Headroom 上下文压缩](#9-headroom-上下文压缩)
10. [统计面板](#10-统计面板)
11. [请求日志](#11-请求日志)
12. [Obsidian Claudian 集成](#12-obsidian-claudian-集成)
13. [OpenClaw / Hermes 集成](#13-openclaw--hermes-集成)
14. [配置文件说明](#14-配置文件说明)
15. [常见问题](#15-常见问题)
16. [故障排查](#16-故障排查)

---

## 1. 简介

Bull Doctor 是一个轻量级本地代理程序，运行在 `http://127.0.0.1:25573`。

**它能做什么：**
- 让 Claude Code CLI / Claude Desktop 使用 DeepSeek、千问、智谱等国产大模型
- 支持 LM Studio、Ollama 等本地模型，自动优化 KV 缓存
- 让 OpenClaw、Hermes 等多个 AI 编程工具共享同一个代理，各自独立选择模型
- 一键切换厂商和模型，无需重启 Claude Code
- 统计 API 调用次数、Token 用量、费用估算
- 从 GitHub 或本地文件夹安装和管理 Skills
- 集成 Headroom 上下文压缩代理，一键启动/停止
- 自动配置 Obsidian Claudian 插件

**工作原理：**
```
Claude Code ─→ http://127.0.0.1:25573/v1/messages ─→ 代理转换格式 ─→ 上游厂商 API
OpenClaw   ─→ http://127.0.0.1:25573/openclaw/...  ─→ 代理透传    ─→ 上游厂商 API
Hermes     ─→ http://127.0.0.1:25573/hermes/...    ─→ 代理透传    ─→ 上游厂商 API
```

---

## 2. 快速开始

### 2.1 安装

```bash
# 下载对应平台的发布包，解压到任意目录
# 或从源码编译：
cargo build --release
```

### 2.2 三步上手

```bash
# 第一步：初始化配置（自动写入 Claude Code settings.json）
cargo run -- init

# 第二步：设置 API Key（以 DeepSeek 为例）
bull-doctor env set DEEPSEEK_API_KEY sk-xxxxxxxxxxxxxxxx

# 第三步：启动代理
cargo run -- start

# 之后正常使用 claude 命令即可
```

**Windows 用户**：`start` 命令会启动系统托盘，托盘图标在任务栏右下角。启动时会自动弹出设置窗口。

### 2.3 切换厂商

```bash
# 列出所有可用厂商
bull-doctor list

# 切换到千问
bull-doctor use qwen

# 切换到智谱
bull-doctor use zhipu

# 切换到本地 Ollama
bull-doctor use ollama
```

---

## 3. 支持的厂商

| ID | 名称 | 默认模型 | API Key 环境变量 | Key 格式 |
|----|------|---------|-----------------|---------|
| `deepseek` | DeepSeek | deepseek-v4-pro | `DEEPSEEK_API_KEY` | `sk-` 开头 |
| `qwen` | 千问 | qwen3.7-max | `DASHSCOPE_API_KEY` | `sk-` 开头 |
| `zhipu` | 智谱 | glm-5.1 | `ZHIPU_API_KEY` | `.` 开头 |
| `kimi` | Kimi | kimi-k2.6 | `MOONSHOT_API_KEY` | `sk-` 开头 |
| `minimax` | MiniMax | minimax-m3 | `MINIMAX_API_KEY` | `eyJ` 开头（JWT） |
| `mimo` | 小米 MiMo | mimo-v2.5-pro | `MIMO_API_KEY` | Bearer Token |
| `ollama` | Ollama | llama3.1 | `OLLAMA_API_KEY` | 无需 Key |
| `lmstudio` | LM Studio | local-model | `LMSTUDIO_API_KEY` | 无需 Key |
| `custom` | 中转站 | claude-opus-4-8 | `CUSTOM_API_KEY` | 自定义 |

### 3.1 设置 API Key

```bash
# 方式一：CLI 命令
bull-doctor env set DEEPSEEK_API_KEY sk-xxxxxxxx
bull-doctor env set DASHSCOPE_API_KEY sk-xxxxxxxx

# 方式二：设置窗口（启动代理后）
bull-doctor settings
# 在打开的窗口中粘贴 Key，点击「保存」
```

### 3.2 使用 Ollama 本地模型

```bash
# 确保 Ollama 正在运行
ollama serve

# 切换到 Ollama
bull-doctor use ollama

# Ollama 默认使用 http://127.0.0.1:11434（标准端口）
# 如需修改端口，在设置窗口中修改 Base URL
```

### 3.2.1 使用 LM Studio 本地模型

LM Studio 提供完全兼容 OpenAI 的本地推理服务器，支持加载 GGUF 等格式模型。

```bash
# 1. 启动 LM Studio，加载模型，点击「Start Server」
#    默认端口 http://127.0.0.1:1234

# 2. 切换到 LM Studio
bull-doctor use lmstudio

# LM Studio 不需要 API Key
# 在设置窗口中可点击「获取列表」自动拉取 LM Studio 中加载的模型
```

**KV 缓存优化（重要）：** Bull Doctor 会自动在 Claude Code 的 settings.json 中写入 `CLAUDE_CODE_ATTRIBUTION_HEADER: "0"`，禁用 Claude Code 附加的变动计费头。这保证了每次请求的 prompt 前缀一致，让本地模型可以正确复用 KV 缓存，避免浪费大量 token、显著提升响应速度。

### 3.3 自定义中转站

```bash
# 切换到中转站模式
bull-doctor use custom

# 在设置窗口中填写中转站地址和 API Key
```

---

## 4. 支持的工具

每个工具可以**独立选择厂商和模型**，互不干扰。

| 工具 ID | 工具名称 | 代理路径 | 请求格式 |
|---------|---------|---------|---------|
| `claude-code` | Claude Code | `/v1/messages` | Anthropic Messages |
| `claude-desktop` | Claude Desktop | `/claude-desktop/v1/messages` | Anthropic Messages |
| `openclaw` | OpenClaw | `/openclaw/v1/chat/completions` | Chat Completions |
| `hermes` | Hermes | `/hermes/v1/chat/completions` | Chat Completions |

### 4.1 为不同工具配置不同厂商

在设置窗口顶部选择工具标签，然后为该工具选择厂商。

例如：
- Claude Code → DeepSeek（写代码）
- OpenClaw → 千问（长文本处理）
- Hermes → LM Studio（本地免费推理）

所有工具可以**同时运行**，不会冲突。代理通过 URL 路径区分请求来源。

### 4.2 OpenClaw / Hermes 配置

OpenClaw 和 Hermes 需要在各自的配置文件中设置：

**OpenClaw** (`~/.openclaw/config.yaml` 或类似)：
```yaml
baseUrl: http://127.0.0.1:25573/openclaw/v1
apiKey: your-api-key
```

**Hermes** (`~/.hermes/config.yaml` 或类似)：
```yaml
base_url: http://127.0.0.1:25573/hermes/v1
api_key: your-api-key
```

---

## 5. CLI 命令详解

### 5.1 `init` — 初始化配置

```bash
bull-doctor init
```

执行操作：
1. 创建配置目录 `~/.bull-doctor/`
2. 生成默认配置文件 `config.json`
3. 修改 Claude Code 的 `~/.claude/settings.json`，指向本地代理
4. 同步 Claude Desktop 网关配置（macOS/Windows）

### 5.2 `start` — 启动代理

```bash
bull-doctor start          # Windows：带系统托盘
bull-doctor start --no-tray # Windows：纯命令行模式
# macOS/Linux：默认命令行模式
```

启动后：
- 代理监听 `http://127.0.0.1:25573`
- Windows 托盘图标出现在任务栏
- 自动弹出设置窗口

### 5.3 `list` — 列出可用厂商

```bash
bull-doctor list
```

输出示例：
```
可用模型预设:
  ✓ deepseek   deepseek-v4-pro    DeepSeek
    qwen       qwen3.7-max        千问
    zhipu      glm-5.1            智谱
    kimi       kimi-k2.6          Kimi
    minimax    minimax-m3         MiniMax
    mimo       mimo-v2.5-pro      小米 MiMo
    ollama     llama3.1           Ollama
    lmstudio   local-model        LM Studio
    custom     claude-opus-4-8    中转站
```

`✓` 标记当前使用的厂商。

### 5.4 `use` — 切换厂商

```bash
bull-doctor use <provider-id>
```

示例：
```bash
bull-doctor use qwen       # 切换到千问
bull-doctor use deepseek   # 切换到 DeepSeek
bull-doctor use ollama     # 切换到 Ollama
bull-doctor use lmstudio   # 切换到 LM Studio
```

切换后自动检测连接是否可用，如果代理正在运行则热更新无需重启。

### 5.5 `test` — 测试连接

```bash
bull-doctor test
```

测试当前厂商的 API Key 是否有效、网络是否可达。

### 5.6 `status` — 查看状态

```bash
bull-doctor status
```

显示：
- 当前厂商和模型
- 代理地址和上游地址
- API Key 配置状态
- Claude Code 配置同步状态

### 5.7 `doctor` — 一键诊断

```bash
bull-doctor doctor
```

逐项检查：
- 配置目录是否存在
- Claude Code settings.json 是否存在
- API Key 是否已配置
- Windows 环境变量是否同步
- Claude Code 是否指向本地代理
- Claude Desktop 双通道是否同步
- 代理是否正在运行
- Claude Code 组件是否就绪

### 5.8 `settings` — 打开设置窗口

```bash
bull-doctor settings
```

- **Windows**：打开原生设置窗口（WebView2）
- **macOS/Linux**：自动打开浏览器访问设置页

### 5.9 `env set` — 设置 API Key

```bash
bull-doctor env set <环境变量名> <API-Key>
```

示例：
```bash
bull-doctor env set DEEPSEEK_API_KEY sk-xxxxxxxx
bull-doctor env set DASHSCOPE_API_KEY sk-xxxxxxxx
bull-doctor env set ZHIPU_API_KEY .xxxxxxxx
```

API Key 存储在 `~/.bull-doctor/.env`，不会提交到版本控制。

### 5.10 `skill` — Skills 管理

```bash
# 安装 Skill
bull-doctor skill install <owner/repo>
bull-doctor skill install <owner/repo>:<branch>
bull-doctor skill install <owner/repo>/<subdir>

# 列出已安装
bull-doctor skill list

# 卸载
bull-doctor skill uninstall <skill-id>

# 同步到 Claude Code
bull-doctor skill sync
```

详见 [Skills 管理](#8-skills-管理)。

### 5.11 `setup-obsidian` — 配置 Obsidian

```bash
bull-doctor setup-obsidian
```

自动扫描 Obsidian vault，找到 Claudian 插件并写入代理配置。

### 5.12 `headroom` — Headroom 上下文压缩代理管理

```bash
# 启动 Headroom（默认端口 8787）
bull-doctor headroom start

# 指定端口启动
bull-doctor headroom start --port 9090

# 停止 Headroom
bull-doctor headroom stop

# 查看状态
bull-doctor headroom status
```

Headroom 是一个上下文压缩代理，运行在独立端口，可压缩 LLM 请求中的工具输出、日志、文件内容等，可减少 Token 消耗（实际节省比例取决于工具输出、日志等内容的占比）。

详见 [Headroom 上下文压缩](#9-headroom-上下文压缩)。

### 5.13 `restore-anthropic` — 恢复官方配置

```bash
bull-doctor restore-anthropic
```

恢复 Claude Code 为 Anthropic 官方 API 配置，并退出 Claude Desktop。

### 5.14 `repair-claude-code` — 修复 Claude Code 组件

```bash
bull-doctor repair-claude-code
```

下载并安装 Claude Code CLI 组件到 Claude Desktop 缺失的位置。

---

## 6. 设置窗口使用指南

### 6.1 打开方式

- 启动代理自动弹出
- 托盘右键 → 「🔑 设置 API Key」
- CLI：`bull-doctor settings`

### 6.2 窗口功能

窗口可以**自由调整大小**，关闭时的尺寸会被记住，下次打开恢复。

**顶部 — 工具选择栏：**
- 点击选择要配置的工具（Claude Code / Claude Desktop / OpenClaw / Hermes）
- 「全局默认」— 不对特定工具做特殊配置时使用的默认厂商

**厂商选择：**
- 点击厂商芯片切换
- 蓝色高亮 = 当前选中
- 「当前」标记 = 代理正在使用的厂商

**思考强度（部分厂商支持）：**
- 关闭 / 低 / 中 / 高 / 最高
- DeepSeek、千问等支持推理强度的厂商会显示此选项

**模型：**
- 输入框可手动填写模型名
- 「获取列表」按钮从上游 API 获取可用模型列表
- 点击列表中的模型名自动填入

**Base URL：**
- 显示厂商默认 API 地址
- 可改为镜像/代理地址
- 中转站模式下必须填写

**API Key：**
- 密码输入框，可切换显示/隐藏
- 已保存时显示掩码预览
- 粘贴新 Key 可覆盖
- 每个厂商有专属的提示文案（如 DeepSeek 提示 `sk-` 开头）
- Ollama、LM Studio 无需 Key

**底部操作：**
- 「测试连接」— 验证 API Key 和网络
- 「保存」— 保存配置并热更新代理
- 「清除所有配置」— 重置全部设置

### 6.3 Headroom 上下文压缩卡

设置页中 Skills 管理上方有 Headroom 管理区域：

- **状态指示**：绿色圆点 = 运行中（显示端口和 PID），灰色圆点 = 已停止
- **启动 / 停止按钮**：一键切换 Headroom 代理的运行状态
- **自动启动开关**：开启后每次 Bull Doctor 启动时自动启动 Headroom
- Headroom 运行在独立端口（默认 8787），不干扰代理本身

### 6.4 Skills 管理卡

设置页下方有 Skills 管理区域：

**从 GitHub 安装：**
- 输入 GitHub 仓库地址（如 `anthropics/skills`）→ 点击「安装」
- 支持指定分支和子目录（如 `owner/repo:branch` 或 `owner/repo/subdir`）

**从本地文件夹安装：**
- 输入本地文件夹的完整路径（如 `D:\my-skills\my-skill`）→ 点击「安装」
- 文件夹会被复制到 Bull Doctor 的 skills 目录并同步到 Claude Code
- 适用于从 GitHub 下载的 skill 包、自制 skill、或无法直接 git clone 的环境

**已安装列表：**
- 显示 Skill ID + 描述
- 每个 Skill 有「卸载」按钮
- 「同步到 Claude Code」一键同步

---

## 7. 托盘菜单使用指南

### 7.1 托盘菜单项

| 菜单项 | 功能 |
|--------|------|
| 🔑 设置 API Key | 打开设置窗口 |
| 📊 请求日志 | 打开请求日志窗口 |
| 📈 统计面板 | 打开统计面板 |
| 选择模型 | 子菜单：当前厂商的可用模型 |
| 切换模型 | 子菜单：所有厂商 + 模型组合 |
| 重新同步配置 | 重新写入 Claude Code settings.json |
| 检测连接 | 检测当前 API Key 是否有效 |
| 诊断环境 | 运行 `doctor` 诊断 |
| 修复 Claude Code | 下载缺失的 Claude Code 组件 |
| 退出 | 关闭代理和托盘 |

### 7.2 模型切换

托盘菜单中「选择模型」显示当前厂商下的模型变体（如 DeepSeek 的 V4 Pro / V4 Flash）。

「切换模型」显示所有厂商及其模型列表，点击即可切换，代理自动热更新。

### 7.3 托盘提示

鼠标悬停在托盘图标上显示：
- 当前厂商名称
- 连接状态（已连接 / 未检测）

---

## 8. Skills 管理

Skills 是 Claude Code 的扩展能力模块，可从 GitHub 仓库安装或从本地文件夹导入。

### 8.1 从 GitHub 安装 Skill

```bash
# 从 GitHub 安装（默认 main 分支）
bull-doctor skill install anthropics/skills

# 指定分支
bull-doctor skill install some-org/skills:dev

# 指定子目录（仓库中的某个子目录作为 skill）
bull-doctor skill install anthropics/skills/my-skill
```

安装过程：
1. `git clone --depth=1` 到 `~/.bull-doctor/skills/`
2. 创建符号链接到 `~/.claude/skills/`（Windows 不支持符号链接时自动复制）
3. 记录到 `~/.bull-doctor/skills.json`

### 8.2 从本地文件夹安装 Skill

在设置窗口的「从本地文件夹安装」输入框中填写路径，或直接拖入文件夹路径：

```
D:\my-skills\awesome-skill
C:\Users\you\Downloads\skill-pack
```

安装过程：
1. 验证路径是否存在且为文件夹
2. 复制到 `~/.bull-doctor/skills/local_<目录名>/`
3. 同步到 `~/.claude/skills/`
4. 记录到 `~/.bull-doctor/skills.json`

**适用场景：**
- 从 GitHub 手动下载的 skill 包（无法访问 GitHub 时）
- 自己编写的自定义 skill
- 企业内网环境中无法 git clone 的情况

### 8.2 管理已安装的 Skill

```bash
# 查看列表
bull-doctor skill list

# 卸载
bull-doctor skill uninstall anthropics/skills

# 同步（恢复符号链接/重新复制到 Claude Code 目录）
bull-doctor skill sync
```

### 8.4 可视化 Skill 管理

在设置窗口中直接操作：

**从 GitHub 安装：**
1. 输入 `owner/repo` → 点击「安装」

**从本地安装：**
1. 输入本地文件夹路径 → 点击「安装」

**管理已安装的 Skill：**
1. 在列表中查看已安装的 Skill（GitHub 安装的显示 `owner/repo`，本地安装的显示 `local:目录名`）
2. 点击「卸载」移除
3. 点击「同步到 Claude Code」批量同步

---

## 9. Headroom 上下文压缩

[Headroom](https://github.com/chopratejas/headroom) 是一个开源的上下文压缩代理，可在 LLM 请求到达模型之前压缩工具输出、日志、文件内容、搜索结果等，减少 Token 消耗，同时保留准确性和回答质量。实际节省比例取决于工具输出、日志、搜索结果等内容的占比。

### 9.1 前置条件

```bash
# 安装 Headroom（需要 Python 3.10+）
pip install headroom[proxy]
```

Bull Doctor 会自动检测 `headroom` 命令是否可用。如未检测到，启动按钮会显示为禁用状态。

### 9.2 启动方式

**方式一：设置窗口（推荐）**

1. 打开设置窗口
2. 找到「Headroom 上下文压缩」卡片
3. 点击「启动」按钮
4. 可勾选「自动启动」让 Bull Doctor 启动时一并启动 Headroom

**方式二：CLI 命令**

```bash
bull-doctor headroom start          # 默认 8787 端口
bull-doctor headroom start --port 9090  # 自定义端口
bull-doctor headroom stop           # 停止
bull-doctor headroom status         # 查看状态
```

**方式三：配置自动启动**

在 `~/.bull-doctor/config.json` 中设置：

```json
{
  "headroom": {
    "port": 8787,
    "auto_start": true
  }
}
```

### 9.3 工作原理

Headroom 作为独立代理运行在本地端口（默认 8787），与 Bull Doctor 代理（25573）互不干扰：

```
                    ┌─ Bull Doctor (25573) ─→ 上游厂商 API（正常代理）
工具 ─→ 用户配置 ──┤
                    └─ Headroom (8787) ─→ 压缩上下文 ─→ LLM（压缩代理）
```

- **不使用 Headroom**：工具直接通过代理发送完整上下文
- **使用 Headroom**：工具先通过 Headroom 压缩上下文，再发送给 LLM

### 9.4 压缩能力

Headroom 根据内容类型自动选择最佳压缩算法：

| 内容类型 | 压缩方式 | 典型节省 |
|---------|---------|---------|
| JSON / API 响应 | SmartCrusher 统计去重 | 70-92% |
| 代码文件 | AST 感知压缩（tree-sitter） | 40-60% |
| 日志 / 终端输出 | 重复模式检测 | 80-95% |
| Diff / 补丁 | 去除无变更上下文 | 60-80% |
| 搜索结果 / RAG | 相关性排序筛选 | 50-90% |
| 自然语言文本 | ML 模型压缩（ModernBERT） | 40-70% |

### 9.5 CCR（压缩-缓存-检索）机制

Headroom 压缩数据后会在本地存储原始数据。如果 LLM 发现压缩后的内容不足以回答问题，它可以通过 CCR 机制按需检索原始未压缩数据。这保证了压缩不会丢失关键信息。

### 9.6 注意事项

- Headroom 独立于 Bull Doctor 运行，Bull Doctor 退出时 Headroom 也会停止
- 如需 Headroom 持续运行，可直接在终端执行 `headroom proxy --port 8787`
- Headroom 的 KV 缓存优化与 Bull Doctor 的 `CLAUDE_CODE_ATTRIBUTION_HEADER: "0"` 互补

---

## 10. 统计面板

### 9.1 打开方式

- 托盘右键 → 「📈 统计面板」
- 或直接访问 `http://127.0.0.1:25573/admin/stats`

### 9.2 面板内容

**概览卡片：**
- 总请求数
- 成功率
- 平均响应时间
- 总 Token 数
- 总费用（估算）
- 当前厂商

**24 小时趋势图：**
- 柱状图显示每小时的请求量
- 时间为北京时间

**最近请求记录表：**
| 列 | 说明 |
|----|------|
| 时间 | 北京时间 |
| 模型 | 上游模型名 |
| 路径 | API 路径 |
| 状态 | HTTP 状态码 |
| 耗时 | 毫秒 |
| Token | 输入+输出 Token |
| 费用 | 估算费用（元） |

**自动刷新**：每 5 秒自动刷新数据。

---

## 11. 请求日志

### 10.1 打开方式

- 托盘右键 → 「📊 请求日志」
- 或直接访问 `http://127.0.0.1:25573/admin/logs`

### 10.2 日志内容

显示最近的 API 请求记录，包含：
- 请求时间
- 工具（Claude Code / OpenClaw / Hermes）
- 厂商和模型
- 请求路径
- HTTP 状态码
- 响应耗时
- Token 用量

日志持久化存储在 `~/.bull-doctor/request-log.sqlite`，保留最近 300 条。

---

## 12. Obsidian Claudian 集成

### 11.1 自动配置

```bash
bull-doctor setup-obsidian
```

程序自动：
1. 扫描 Documents 目录下的 Obsidian vault
2. 查找 Claudian 插件的 `data.json`
3. 写入代理地址和模型配置

### 11.2 手动配置

在 Claudian 插件设置中：

| 字段 | 值 |
|------|----|
| Provider | `custom` |
| Base URL | `http://127.0.0.1:25573` |
| API Key | 你的厂商 API Key |

### 11.3 前置条件

1. 已安装 Obsidian
2. 已安装 Claudian 插件（社区插件 → 搜索 claudian）
3. 已安装 Claude Code CLI（`claude` 命令可用）
4. Bull Doctor 已运行 `bull-doctor init`

---

## 13. OpenClaw / Hermes 集成

### 12.1 在设置窗口中配置

1. 打开设置窗口
2. 顶部工具栏选择「OpenClaw」或「Hermes」
3. 选择厂商、配置 API Key
4. 保存

### 12.2 在工具中配置

**OpenClaw** 的配置文件中设置：
```
baseUrl: http://127.0.0.1:25573/openclaw/v1
apiKey: （与配置的厂商 API Key 相同）
```

**Hermes** 的配置文件中设置：
```
base_url: http://127.0.0.1:25573/hermes/v1
api_key: （与配置的厂商 API Key 相同）
```

### 12.3 多工具同时运行

Claude Code、OpenClaw、Hermes 可以同时运行，共用同一个代理端口：
- 每个工具配不同的厂商 → 各自走不同上游
- 每个工具配相同的厂商 → 共享 API 额度
- 配置保存在 `config.json` 的 `tools` 字段中

---

## 14. 配置文件说明

### 13.1 文件位置

| 文件 | 路径 | 说明 |
|------|------|------|
| 主配置 | `~/.bull-doctor/config.json` | 厂商、工具、代理配置 |
| 环境变量 | `~/.bull-doctor/.env` | API Key 存储 |
| Skills 记录 | `~/.bull-doctor/skills.json` | 已安装的 Skills |
| Skills 目录 | `~/.bull-doctor/skills/` | Skills 文件存放 |
| 请求日志 | `~/.bull-doctor/request-log.sqlite` | SQLite 持久化日志 |
| 备份目录 | `~/.bull-doctor/backups/` | Claude Code settings.json 备份 |
| 日志目录 | `~/.bull-doctor/logs/` | 代理运行日志 |

### 13.2 `config.json` 结构

```json
{
  "proxy": {
    "host": "127.0.0.1",
    "port": 25573
  },
  "active": "deepseek",
  "providers": {
    "deepseek": {
      "id": "deepseek",
      "name": "DeepSeek",
      "base_url": "https://api.deepseek.com/anthropic",
      "api_key_env": "DEEPSEEK_API_KEY",
      "default_model": "deepseek-v4-pro",
      "api_model": "deepseek-v4-pro",
      "wire_api": "anthropic",
      "base_url_customized": false,
      "custom_models": [],
      "reasoning_style": ""
    }
  },
  "tools": {
    "claude-code": {
      "enabled": true,
      "active_provider": ""
    },
    "openclaw": {
      "enabled": true,
      "active_provider": "qwen"
    }
  },
  "model_reasoning_effort": "medium",
  "tool_output_max_chars": 0,
  "settings_window_width": 600,
  "settings_window_height": 700,
  "headroom": {
    "port": 8787,
    "auto_start": false
  }
}
```

### 13.3 关键字段说明

| 字段 | 说明 |
|------|------|
| `proxy.host` | 代理监听地址，默认 127.0.0.1 |
| `proxy.port` | 代理监听端口，默认 25573 |
| `active` | 全局默认厂商 ID |
| `providers` | 所有厂商配置 |
| `tools` | 每个工具的独立配置 |
| `tools.<id>.enabled` | 是否启用该工具的代理 |
| `tools.<id>.active_provider` | 该工具的厂商（空则用全局 `active`） |
| `model_reasoning_effort` | 默认推理档位，映射为各厂商 Chat API 的 thinking / reasoning 参数。 |
| `tool_output_max_chars` | tool 输出截断长度（0=不截断） |
| `reasoning_style` | 中转站推理格式（deepseek/qwen/zhipu/kimi/minimax/mimo/openrouter） |
| `headroom.port` | Headroom 代理端口，默认 8787 |
| `headroom.auto_start` | Bull Doctor 启动时自动启动 Headroom，默认 false |

### 13.4 写入 Claude Code settings.json 的环境变量

Bull Doctor 保存配置时会自动在 `~/.claude/settings.json` 的 `env` 中写入以下键值对，**不会覆盖**用户已有的其他配置项：

| 环境变量 | 说明 |
|---------|------|
| `ANTHROPIC_BASE_URL` | 指向本地代理 `http://127.0.0.1:25573` |
| `ANTHROPIC_API_KEY` | 代理认证 token |
| `ANTHROPIC_AUTH_TOKEN` | 代理认证 token（兼容） |
| `ANTHROPIC_MODEL` | 当前厂商的默认模型 |
| `ANTHROPIC_DEFAULT_HAIKU_MODEL` | Flash 层级模型 |
| `ANTHROPIC_DEFAULT_SONNET_MODEL` | Flash 层级模型 |
| `ANTHROPIC_DEFAULT_OPUS_MODEL` | Pro 层级模型 |
| `ANTHROPIC_REASONING_MODEL` | 推理模型 |
| `CLAUDE_CODE_ATTRIBUTION_HEADER` | 设为 `"0"`，禁用变动计费头以优化本地模型 KV 缓存 |
| `ENABLE_TOOL_SEARCH` | 启用工具搜索 |
| `CLAUDE_CODE_ENABLE_GATEWAY_MODEL_DISCOVERY` | 启用网关模型发现 |

---

## 15. 常见问题

### Q1：Claude Code 启动后还是走 Anthropic 官方 API？

运行 `bull-doctor init` 重新同步配置，确保 `~/.claude/settings.json` 中的 `ANTHROPIC_BASE_URL` 指向 `http://127.0.0.1:25573`。

### Q2：代理启动报「端口已被占用」？

```bash
# 查找占用端口的进程
netstat -ano | findstr 25573

# 或关闭已运行的 Bull Doctor
taskkill /F /IM bull-doctor.exe
```

### Q3：API Key 保存后仍提示未配置？

检查环境变量名是否正确：
- DeepSeek → `DEEPSEEK_API_KEY`
- 千问 → `DASHSCOPE_API_KEY`
- 智谱 → `ZHIPU_API_KEY`

用 `bull-doctor env set <变量名> <key>` 保存，不要手动编辑 .env 文件。

### Q4：Ollama 连接失败？

1. 确保 Ollama 正在运行：`ollama serve`
2. 检查端口：默认 `http://127.0.0.1:11434`
3. 确保已下载模型：`ollama pull llama3.1`
4. Ollama 不需要 API Key，跳过 Key 配置
5. 在设置窗口中点击「获取列表」应能正确获取本地已安装的模型

### Q5：多个工具能同时用吗？

可以。Claude Code、OpenClaw、Hermes 可以同时运行，都指向 `127.0.0.1:25573`。
- 每个工具可以选不同的厂商
- 选同一厂商则共享 API 额度
- 代理自动按路径区分请求来源

### Q6：切换厂商后需要重启 Claude Code 吗？

不需要。代理支持热更新：`use` 命令切换后，新请求立即生效。如果 Claude Code 已打开，需要 `Ctrl+C` 退出后重新运行 `claude`，但不需要重启代理。

### Q7：如何恢复到 Anthropic 官方？

```bash
bull-doctor restore-anthropic
```

这会恢复 `~/.claude/settings.json` 并退出 Claude Desktop。

### Q8：中转站如何使用？

1. `bull-doctor use custom`
2. 在设置窗口填写中转站 Base URL
3. 在设置窗口粘贴中转站 API Key
4. 如果中转站格式不标准，可设置 `reasoning_style` 字段

### Q9：LM Studio 连接失败？

1. 确保 LM Studio 正在运行，且已点击「Start Server」启动本地服务
2. 检查端口：默认 `http://127.0.0.1:1234`（在 LM Studio 左下角显示）
3. 确保模型已加载到 LM Studio 中
4. LM Studio 不需要 API Key，跳过 Key 配置
5. 在设置窗口中点击「获取列表」应能自动拉取 LM Studio 中加载的模型

### Q10：本地模型（Ollama / LM Studio）响应很慢？

最常见原因是 KV 缓存未命中。Bull Doctor 会自动在 settings.json 中写入 `CLAUDE_CODE_ATTRIBUTION_HEADER: "0"` 禁用 Claude Code 的变动计费头。如果仍然很慢：

1. 重新保存一次设置（触发 settings.json 重新写入，确保 attribution header 已禁用）
2. 检查模型是否适合你的硬件（显存/内存是否足够）
3. 减小上下文窗口（如 `num_ctx` 参数）可减少 KV 缓存占用

### Q11：从本地安装的 Skill 如何卸载？

在设置窗口的 Skills 列表中找到对应的 `local:目录名` 条目，点击「卸载」即可。卸载会同时删除 Bull Doctor 和 Claude Code 中的 skill 文件。

### Q12：Headroom 启动失败，提示"未找到 headroom 命令"？

需要先安装 Headroom：

```bash
pip install headroom[proxy]
```

安装后在终端运行 `headroom --version` 确认安装成功。如果仍然找不到，检查 Python Scripts 目录是否在 PATH 中。

### Q13：Headroom 启动后立刻停止了？

可能原因：
1. 端口被占用 — 检查 `netstat -ano | findstr 8787`
2. Python 环境问题 — 在终端手动运行 `headroom proxy --port 8787` 查看错误输出
3. 依赖缺失 — 重新安装 `pip install headroom[proxy]`

### Q14：Headroom 和 Bull Doctor 的关系是什么？

两者是独立的本地代理，互不干扰：
- **Bull Doctor（25573）**：转换请求格式，让 Claude Code 使用第三方厂商 API
- **Headroom（8787）**：压缩请求上下文，减少 Token 消耗

可以同时运行，也可以单独使用。Headroom 的压缩对所有 LLM 请求生效（不限于通过 Bull Doctor 的请求）。

---

## 16. 故障排查

### 15.1 完整诊断

```bash
bull-doctor doctor
```

逐项检查所有配置是否正确。如果发现问题，按提示修复。

### 15.2 查看代理运行日志

```bash
# macOS/Linux
tail -f ~/.bull-doctor/logs/*.log

# Windows
type %USERPROFILE%\.bull-doctor\logs\
```

### 15.3 重置所有配置

在设置窗口中点击「清除所有配置」，或在终端中：

```bash
# 备份
cp ~/.bull-doctor/config.json ~/.bull-doctor/config.json.bak
cp ~/.bull-doctor/.env ~/.bull-doctor/.env.bak

# 删除
rm ~/.bull-doctor/config.json
rm ~/.bull-doctor/.env

# 重新初始化
bull-doctor init
```

### 15.4 常见错误码

| 错误 | 原因 | 解决 |
|------|------|------|
| 401 Unauthorized | API Key 错误 | 检查 Key 是否正确粘贴 |
| 403 Forbidden | 无权限/额度不足 | 检查厂商账户余额 |
| 502 Bad Gateway | 上游连接失败 | 检查网络/Base URL |
| 504 Gateway Timeout | 上游响应超时 | 检查厂商服务状态 |
| `connection refused` | 代理未启动 | `bull-doctor start` |

### 15.5 Claude Code 组件缺失

Claude Desktop 提示 "binary not available"：

```bash
bull-doctor repair-claude-code
```

这会自动下载对应版本的 Claude Code CLI 到正确位置。

---

## 附录：端口说明

| 端口 | 服务 |
|------|------|
| 25573 | Bull Doctor 代理（默认） |
| 8787 | Headroom 上下文压缩代理（默认） |
| 11434 | Ollama API（默认） |
| 1234 | LM Studio 本地服务器（默认） |

## 附录：环境变量

| 变量 | 说明 |
|------|------|
| `CLAUDE_CONFIG_DIR` | Claude Code 配置目录（覆盖默认 `~/.claude`） |
| `DEEPSEEK_API_KEY` | DeepSeek API Key |
| `DASHSCOPE_API_KEY` | 千问 API Key |
| `ZHIPU_API_KEY` | 智谱 API Key |
| `MOONSHOT_API_KEY` | Kimi API Key |
| `MINIMAX_API_KEY` | MiniMax API Key |
| `MIMO_API_KEY` | 小米 MiMo API Key |
| `OLLAMA_API_KEY` | Ollama（无需设置） |
| `LMSTUDIO_API_KEY` | LM Studio（无需设置） |
| `CUSTOM_API_KEY` | 中转站 API Key |

---

> 更多帮助：运行 `bull-doctor --help` 查看所有命令
