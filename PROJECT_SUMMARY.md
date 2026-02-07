# Nanobot-rs 项目总结

## 项目概述

这是一个用 Rust 实现的超轻量级个人 AI Agent，复刻了原版 nanobot 的核心功能。

## 已实现功能

### 1. ✅ 模块架构
- 清晰的模块化设计
- 各模块职责分明，易于扩展

### 2. ✅ LLM 提供商 (2个已实现)
- **OpenRouter** - 统一的 API 访问多个模型
- **DeepSeek** - 高性价比中文 LLM
- 框架支持 OpenAI、Anthropic（待配置 API Key）

### 3. ✅ 核心 Agent 循环
- LLM ↔ 工具执行的完整循环
- 支持工具调用（function calling）
- 上下文管理和限制
- 最大迭代次数保护

### 4. ✅ 工具系统 (5个工具)
- `shell` - 执行系统命令（白名单机制）
- `read_file` - 读取文件
- `write_file` - 写入文件
- `list_dir` - 列出目录
- `web_search` - Web 搜索（Brave Search）

### 5. ✅ Telegram 通道
- 完整的 Telegram Bot 实现
- 支持命令：/help, /start, /clear, /status
- 用户白名单控制
- Markdown 格式化支持
- 长消息自动分段

### 6. ✅ 配置系统
- TOML 配置文件
- 环境变量覆盖
- 合理的默认值

### 7. ✅ 内存系统
- SQLite 持久化存储
- 对话历史记录
- 键值对记忆
- 记忆搜索功能

### 8. ✅ CLI 命令
- `nanobot agent` - 交互式对话
- `nanobot gateway` - 启动网关服务
- `nanobot status` - 查看系统状态
- `nanobot init` - 初始化配置
- `nanobot tool <name>` - 直接执行工具

## 项目结构

```
nanobot-rs/
├── Cargo.toml              # 项目配置
├── Makefile                # 构建脚本
├── README.md               # 使用说明
├── config.example.toml     # 配置示例
├── .env.example            # 环境变量示例
├── .gitignore             # Git 忽略文件
├── .github/
│   └── workflows/
│       └── ci.yml         # CI/CD 配置
└── src/
    ├── main.rs            # 入口点
    ├── error.rs           # 错误类型
    ├── tests.rs           # 测试模块
    ├── agent/             # Agent 核心
    │   └── mod.rs
    ├── llm/               # LLM 提供商
    │   ├── mod.rs
    │   ├── openrouter.rs
    │   └── deepseek.rs
    ├── channel/           # 消息通道
    │   ├── mod.rs
    │   └── telegram.rs
    ├── tools/             # 工具系统
    │   ├── mod.rs
    │   ├── shell.rs
    │   ├── file.rs
    │   └── web.rs
    ├── memory/            # 内存系统
    │   └── mod.rs
    ├── config/            # 配置管理
    │   └── mod.rs
    └── cli/               # CLI 命令
        ├── mod.rs
        ├── agent.rs
        ├── gateway.rs
        ├── init.rs
        ├── status.rs
        └── tool.rs
```

## 技术栈

| 组件 | 库 |
|------|-----|
| 异步运行时 | tokio |
| HTTP 客户端 | reqwest |
| CLI 框架 | clap |
| 序列化 | serde |
| 数据库 | sqlx (SQLite) |
| Telegram Bot | teloxide |
| 配置解析 | toml |
| 日志 | tracing |
| 错误处理 | anyhow, thiserror |

## 代码特点

1. **清晰的 trait 定义**
   - `LlmProvider` - LLM 提供商接口
   - `Tool` - 工具接口
   - `Channel` - 通道接口

2. **安全性考虑**
   - Shell 命令白名单
   - 文件操作路径限制
   - 工具超时机制

3. **可扩展性**
   - 通过 trait 轻松添加新的 LLM 提供商
   - 通过 trait 轻松添加新的工具
   - 通过 trait 轻松添加新的通道

4. **良好的错误处理**
   - 自定义错误类型
   - 详细的错误信息
   - 优雅的降级处理

## 待扩展功能

1. **更多 LLM 提供商**
   - Groq
   - Gemini
   - 本地模型（Ollama）

2. **更多通道**
   - Discord
   - WhatsApp
   - Slack
   - 飞书

3. **更多工具**
   - GitHub API
   - 天气查询
   - tmux 控制
   - 代码编辑器集成

4. **定时任务**
   - Cron 调度支持

5. **Webhook 支持**
   - 用于生产环境部署

## 使用示例

```bash
# 构建
make build

# 初始化配置
make init

# 编辑配置
vim ~/.nanobot/config.toml

# 运行交互式对话
make agent

# 启动 Telegram Bot
export TELEGRAM_BOT_TOKEN="xxx"
make gateway
```

## 关键代码片段

### Agent 对话循环
```rust
async fn run_loop(&self) -> Result<AgentResponse> {
    loop {
        // 1. 调用 LLM
        let response = provider.chat(request).await?;
        
        // 2. 检查工具调用
        if let Some(tool_calls) = &response.message.tool_calls {
            // 3. 执行工具
            for call in tool_calls {
                let result = tools.execute(call).await?;
                // 4. 添加结果到上下文
                context.add_tool_result(result);
            }
            // 5. 继续循环
            continue;
        }
        
        // 6. 返回最终结果
        return Ok(response);
    }
}
```

### 工具注册
```rust
impl ToolRegistry {
    pub fn default_with_config(config: &Config) -> Self {
        let mut registry = Self::new();
        registry.register(ShellTool);
        registry.register(ReadFileTool);
        registry.register(WriteFileTool);
        registry.register(ListDirTool);
        registry.register(WebSearchTool::new(key));
        registry
    }
}
```

## 总结

这个项目完整实现了 nanobot 的核心功能，代码结构清晰、注释完善、易于扩展。可以作为构建个人 AI Agent 的基础框架。
