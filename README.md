# 🤖 Nanobot - Rust 实现

超轻量级个人 AI Agent 的 Rust 复刻版本。

## 功能特性

- **🧠 多 LLM 提供商** - 支持 OpenRouter、DeepSeek、OpenAI、Anthropic
- **📡 多通道集成** - 支持 Telegram Bot（可扩展 Discord、Slack 等）
- **🔧 工具系统** - Shell 命令、文件读写、Web 搜索
- **💾 持久化内存** - SQLite 存储对话历史和长期记忆
- **⚙️ 灵活配置** - TOML 配置文件 + 环境变量覆盖
- **🚀 简单易用** - 类似原版 nanobot 的 CLI 体验

## 快速开始

### 1. 克隆并构建

```bash
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
export OPENROUTER_API_KEY="your-openrouter-api-key"
export DEEPSEEK_API_KEY="your-deepseek-api-key"
export TELEGRAM_BOT_TOKEN="your-telegram-bot-token"
```

### 4. 运行

```bash
# 查看状态
cargo run -- status

# 启动交互式对话
cargo run -- agent

# 启动 Telegram Bot
cargo run -- gateway --channel telegram
```

## CLI 命令

| 命令 | 描述 |
|------|------|
| `nanobot agent` | 启动交互式 AI 对话 |
| `nanobot gateway` | 启动网关服务（Telegram Bot） |
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

[channel.telegram]
bot_token = "your-bot-token"
allowed_users = []  # 留空表示允许所有用户

[memory]
db_path = "/home/user/.nanobot/memory.db"
max_memories = 1000

[tools]
shell_whitelist = ["echo", "cat", "ls", "pwd", "git"]
allowed_paths = ["/home", "/tmp"]
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

## 项目结构

```
src/
├── main.rs           # 入口点，CLI 解析
├── agent/            # Agent 核心（对话循环）
│   └── mod.rs
├── llm/              # LLM 提供商
│   ├── mod.rs
│   ├── openrouter.rs
│   └── deepseek.rs
├── channel/          # 消息通道
│   ├── mod.rs
│   └── telegram.rs
├── tools/            # 工具系统
│   ├── mod.rs
│   ├── shell.rs
│   ├── file.rs
│   └── web.rs
├── memory/           # 内存系统
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

## 测试

```bash
# 运行单元测试
cargo test

# 运行特定测试
cargo test test_name

# 带日志输出测试
cargo test -- --nocapture
```

## 许可证

MIT

## 致谢

原版 [nanobot](https://github.com/danielmiessler/nanobot) 的灵感来源
