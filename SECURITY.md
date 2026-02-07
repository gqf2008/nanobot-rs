# 安全加固指南

本文档描述了 nanobot-rs 的安全特性和配置建议。

## 工作区限制

### 文件工具白名单

配置文件中的 `tools.allowed_paths` 限制了文件工具可以访问的路径：

```toml
[tools]
allowed_paths = ["/home/user/workspace", "/tmp"]
```

默认允许的路径：
- `/home` - 用户主目录
- `/tmp` - 临时文件目录

### Shell 命令白名单

配置文件中的 `tools.shell_whitelist` 限制了可以执行的命令：

```toml
[tools]
shell_whitelist = ["echo", "cat", "ls", "pwd", "grep", "head", "tail"]
```

## 环境变量安全配置

### API Keys

所有 API Key 都通过环境变量配置，避免写入配置文件：

```bash
# LLM 提供商
export OPENROUTER_API_KEY="your-key"
export DEEPSEEK_API_KEY="your-key"
export MOONSHOT_API_KEY="your-key"
export OPENAI_API_KEY="your-key"
export ANTHROPIC_API_KEY="your-key"

# 通道
export TELEGRAM_BOT_TOKEN="your-token"
export DISCORD_BOT_TOKEN="your-token"
export FEISHU_APP_ID="your-id"
export FEISHU_APP_SECRET="your-secret"
export WHATSAPP_BRIDGE_URL="ws://localhost:3000"

# 工具
export SEARCH_API_KEY="your-key"
```

### 配置文件权限

确保配置文件权限正确：

```bash
chmod 600 ~/.nanobot/config.toml
```

## 通道安全

### Telegram
- 使用 `allowed_users` 限制可访问的用户 ID
- 使用 Webhook 模式时验证请求来源

### Discord
- 使用 `allowed_guilds` 限制可访问的服务器
- 使用 `allowed_channels` 限制可访问的频道
- 使用 `allowed_users` 限制可访问的用户

### 飞书
- 启用 `verify_signature` 验证请求签名
- 使用 `allowed_users` 限制可访问的用户

### WhatsApp
- 使用 `allowed_users` 限制可访问的手机号
- Bridge 连接使用本地 WebSocket，不暴露到公网

## LLM 提供商安全

### Moonshot/Kimi
- API Key 通过 `MOONSHOT_API_KEY` 环境变量配置
- 支持自定义 API Base URL
- kimi-k2.5 模型自动调整 temperature 为 1.0

### vLLM
- 支持本地部署，无需外部 API Key
- 可通过 `VLLM_API_KEY` 配置访问密钥（可选）
- 默认连接 `http://localhost:8000/v1`

## 最佳实践

1. **不要在代码中硬编码 API Key**
2. **定期轮换 API Key**
3. **使用白名单限制文件和命令访问**
4. **启用通道的用户白名单**
5. **定期检查日志中的异常访问**
6. **使用防火墙限制端口访问**
7. **保持依赖库更新**

## 安全更新

如有安全漏洞，请及时更新：

```bash
cargo update
cargo audit
```
