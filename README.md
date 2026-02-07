# 🤖 Nanobot - Rust 实现

超轻量级个人 AI Agent 的 Rust 复刻版本。

## 功能特性

- **🧠 多 LLM 提供商** - 支持 OpenRouter、DeepSeek、Moonshot/Kimi、vLLM、OpenAI、Anthropic
- **📡 多通道集成** - 支持 Telegram、Discord、飞书(Lark/Feishu)、WhatsApp
- **🔧 工具系统** - Shell 命令、文件读写、Web 搜索
- **💾 Markdown 内存** - 使用 Markdown 文件存储对话历史和长期记忆（与 Python 版本兼容）
- **⚙️ 灵活配置** - TOML 配置文件 + 环境变量覆盖
- **🚀 简单易用** - 类似原版 nanobot 的 CLI 体验
- **🔒 安全加固** - 工作区限制、白名单控制、环境变量安全配置

## 快速开始

### 1. 克隆并构建

```bash
git clone https://github.com/gqf2008/nanobot-rs.git
cd nanobot-rs
cargo build --release
```

### 2. 初始化配置

```bash
# 创建配置文件
cargo run -- init

# 或使用指定路径
cargo run -- init --config /path/to/config.toml
```

### 3. 配置 API Key

编辑 `~/.nanobot/config.toml` 或设置环境变量：

```bash
# LLM 提供商
export OPENROUTER_API_KEY="your-openrouter-api-key"
export DEEPSEEK_API_KEY="your-deepseek-api-key"
export MOONSHOT_API_KEY="your-moonshot-api-key"

# 通道
export TELEGRAM_BOT_TOKEN="your-telegram-bot-token"
export DISCORD_BOT_TOKEN="your-discord-bot-token"
export FEISHU_APP_ID="your-feishu-app-id"
export FEISHU_APP_SECRET="your-feishu-app-secret"
```

### 4. 运行

```bash
# 查看状态
cargo run -- status

# 启动交互式对话
cargo run -- agent

# 启动 Telegram Bot
cargo run -- gateway --channel telegram

# 启动 Discord Bot
cargo run -- gateway --channel discord

# 启动飞书 Bot
cargo run -- gateway --channel feishu
```

## CLI 命令

| 命令 | 描述 |
|------|------|
| `nanobot agent` | 启动交互式 AI 对话 |
| `nanobot gateway` | 启动网关服务（Bot） |
| `nanobot status` | 查看系统状态 |
| `nanobot init` | 初始化配置文件 |
| `nanobot tool <name>` | 直接执行工具 |

## 配置文件示例

```toml
[agent]
system_prompt = "你是一个有帮助的 AI 助手。"
max_context = 20
default_provider = "openrouter"
default_model = "openrouter/optimus-alpha"

[llm.openrouter]
api_key = "your-api-key"
base_url = "https://openrouter.ai/api/v1"
default_model = "openrouter/optimus-alpha"
timeout_secs = 60

[llm.deepseek]
api_key = "your-api-key"
base_url = "https://api.deepseek.com"
default_model = "deepseek-chat"
timeout_secs = 60

[llm.moonshot]
api_key = "your-moonshot-api-key"
base_url = "https://api.moonshot.cn/v1"
default_model = "moonshot-v1-8k"
timeout_secs = 60

[llm.vllm]
# 本地 vLLM 部署
api_key = ""
base_url = "http://localhost:8000/v1"
default_model = "default"
timeout_secs = 60

[channel.telegram]
bot_token = "your-bot-token"
allowed_users = []  # 留空表示允许所有用户

[channel.discord]
bot_token = "your-discord-bot-token"
application_id = "your-application-id"
allowed_guilds = []  # 允许的服务器
allowed_channels = []  # 允许的频道
allowed_users = []  # 允许的用户

[channel.feishu]
app_id = "your-app-id"
app_secret = "your-app-secret"
allowed_users = []  # 允许的用户 Open ID

[channel.whatsapp]
bridge_url = "ws://localhost:3000"  # WhatsApp Bridge WebSocket 地址
allowed_users = []  # 允许的手机号

[memory]
# Memory 工作目录（用于存储 Markdown 记忆文件）
workspace_path = "/home/user/.nanobot"
max_memories = 1000

[tools]
shell_whitelist = ["echo", "cat", "ls", "pwd", "git"]
allowed_paths = ["/home/user/workspace", "/tmp"]
search_api_key = "your-brave-search-key"
```

## 工具列表

| 工具名 | 描述 |
|--------|------|
| `shell` | 执行系统命令（需白名单） |
| `read_file` | 读取文件内容 |
| `write_file` | 写入文件 |
| `list_dir` | 列出目录内容 |
| `web_search` | Web 搜索（需要 Brave API Key） |

## Memory 系统

与 Python 版本兼容的 Markdown 文件格式：

### 日常笔记
`~/.nanobot/memory/2026-02-07.md`
```markdown
# 2026-02-07

## 12:30 - User
Hello, how are you?

## 12:31 - Assistant
I'm doing well, thank you!
```

### 长期记忆
`~/.nanobot/memory/MEMORY.md`
```markdown
# Long-term Memory

## Important Facts
- **User name**: Gao
- **Preferred language**: Chinese

## Preferences
- **Programming language**: Rust
```

### 对话历史
`~/.nanobot/memory/conversations/{session_id}.md`
```markdown
# Conversation: test-session

## 2026-02-07 12:30:00
**user**: Hello

## 2026-02-07 12:30:05
**assistant**: Hi there!
```

## 项目结构

```
src/
├── main.rs           # 入口点，CLI 解析
├── agent/            # Agent 核心（对话循环）
│   └── mod.rs
├── llm/              # LLM 提供商
│   ├── mod.rs
│   ├── openrouter.rs
│   ├── deepseek.rs
│   ├── moonshot.rs   # Moonshot/Kimi
│   └── vllm.rs       # 本地 vLLM
├── channel/          # 消息通道
│   ├── mod.rs
│   ├── telegram.rs
│   ├── discord.rs
│   ├── feishu.rs     # 飞书/Lark
│   └── whatsapp.rs   # WhatsApp (WebSocket Bridge)
├── tools/            # 工具系统
│   ├── mod.rs
│   ├── shell.rs
│   ├── file.rs
│   └── web.rs
├── memory/           # Markdown 内存系统
│   └── mod.rs
├── cron/             # 定时任务
│   └── mod.rs
├── bus/              # 事件总线
│   └── mod.rs
├── session/          # 会话管理
│   └── mod.rs
├── config/           # 配置管理
│   └── mod.rs
├── cli/              # CLI 命令实现
│   ├── mod.rs
│   ├── agent.rs
│   ├── gateway.rs
│   ├── init.rs
│   ├── status.rs
│   └── tool.rs
└── error.rs          # 错误类型
```

## 扩展开发

### 添加新的 LLM 提供商

1. 在 `src/llm/` 创建新的 provider 文件
2. 实现 `LlmProvider` trait
3. 在 `LlmProviderFactory` 中注册

### 添加新的工具

1. 在 `src/tools/` 创建新的工具文件
2. 实现 `Tool` trait
3. 在 `ToolRegistry::default_with_config` 中注册

### 添加新的通道

1. 在 `src/channel/` 创建新的通道文件
2. 实现 `Channel` trait
3. 在 `ChannelFactory` 中注册

## 安全加固

详见 [SECURITY.md](SECURITY.md)

- 工作区限制
- 文件工具白名单
- Shell 命令白名单
- 通道用户白名单
- 环境变量安全配置

## 测试

```bash
# 运行单元测试
cargo test

# 运行特定测试
cargo test test_name

# 带日志输出测试
cargo test -- --nocapture
```

## 分支管理

采用 GitHub 分支管理风格：

- `main` - 主分支（生产就绪）
- `develop` - 开发分支
- `feature/*` - 功能分支
- `hotfix/*` - 紧急修复分支
- `release/*` - 发布分支

## 许可证

MIT

## 致谢

原版 [nanobot](https://github.com/HKUDS/nanobot) 的灵感来源
