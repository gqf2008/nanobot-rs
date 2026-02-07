//! 配置系统
//! 
//! 支持 TOML 配置文件和环境变量覆盖

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 主配置结构
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    /// Agent 配置
    #[serde(default)]
    pub agent: AgentConfig,
    
    /// LLM 提供商配置
    #[serde(default)]
    pub llm: LlmConfig,
    
    /// 通道配置
    #[serde(default)]
    pub channel: ChannelConfig,
    
    /// 内存系统配置
    #[serde(default)]
    pub memory: MemoryConfig,
    
    /// 工具配置
    #[serde(default)]
    pub tools: ToolsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// 系统提示词
    #[serde(default = "default_system_prompt")]
    pub system_prompt: String,
    /// 最大上下文消息数
    #[serde(default = "default_max_context")]
    pub max_context: usize,
    /// 默认 LLM 提供商
    #[serde(default = "default_provider")]
    pub default_provider: String,
    /// 默认模型
    #[serde(default = "default_model")]
    pub default_model: String,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            system_prompt: default_system_prompt(),
            max_context: default_max_context(),
            default_provider: default_provider(),
            default_model: default_model(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
pub struct LlmConfig {
    /// OpenRouter 配置
    #[serde(default)]
    pub openrouter: ProviderConfig,
    /// DeepSeek 配置
    #[serde(default)]
    pub deepseek: ProviderConfig,
    /// Moonshot 配置
    #[serde(default)]
    pub moonshot: ProviderConfig,
    /// vLLM 配置
    #[serde(default)]
    pub vllm: ProviderConfig,
    /// OpenAI 配置
    #[serde(default)]
    pub openai: ProviderConfig,
    /// Anthropic 配置
    #[serde(default)]
    pub anthropic: ProviderConfig,
}


#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderConfig {
    /// API Key
    pub api_key: Option<String>,
    /// 基础 URL（用于自定义端点）
    pub base_url: Option<String>,
    /// 默认模型
    pub default_model: Option<String>,
    /// 超时时间（秒）
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
pub struct ChannelConfig {
    /// Telegram 配置
    #[serde(default)]
    pub telegram: TelegramConfig,
    /// Discord 配置
    #[serde(default)]
    pub discord: DiscordConfig,
    /// 飞书配置
    #[serde(default)]
    pub feishu: FeishuConfig,
    /// WhatsApp 配置
    #[serde(default)]
    pub whatsapp: WhatsAppConfig,
}


#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TelegramConfig {
    /// Bot Token
    pub bot_token: Option<String>,
    /// 允许的用户 ID 列表
    #[serde(default)]
    pub allowed_users: Vec<i64>,
    /// Webhook URL（可选）
    pub webhook_url: Option<String>,
}

/// Discord 配置
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DiscordConfig {
    /// Bot Token
    pub bot_token: Option<String>,
    /// Application ID
    pub application_id: Option<u64>,
    /// 允许的服务器 ID 列表
    #[serde(default)]
    pub allowed_guilds: Vec<u64>,
    /// 允许的频道 ID 列表
    #[serde(default)]
    pub allowed_channels: Vec<u64>,
    /// 允许的用户 ID 列表
    #[serde(default)]
    pub allowed_users: Vec<u64>,
    /// 默认前缀
    #[serde(default = "default_prefix")]
    pub prefix: String,
    /// Webhook URL（可选）
    pub webhook_url: Option<String>,
    /// 是否启用 Slash Command
    #[serde(default = "default_true")]
    pub enable_slash_commands: bool,
}

/// 飞书配置
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FeishuConfig {
    /// App ID
    pub app_id: Option<String>,
    /// App Secret
    pub app_secret: Option<String>,
    /// Verification Token
    pub verification_token: Option<String>,
    /// Encrypt Key
    pub encrypt_key: Option<String>,
    /// 允许的用户 Open ID 列表
    #[serde(default)]
    pub allowed_users: Vec<String>,
    /// 允许的用户 Open ID 列表（别名）
    #[serde(default)]
    pub allowed_open_ids: Vec<String>,
    /// 允许的群 Chat ID 列表
    #[serde(default)]
    pub allowed_chats: Vec<String>,
    /// 是否验证请求签名
    #[serde(default = "default_true")]
    pub verify_signature: bool,
    /// 消息卡片模板 ID
    pub card_template_id: Option<String>,
}

/// WhatsApp 配置
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WhatsAppConfig {
    /// WebSocket Bridge URL
    pub bridge_url: Option<String>,
    /// 允许的用户手机号列表
    #[serde(default)]
    pub allowed_users: Vec<String>,
    /// 自动重连间隔（秒）
    #[serde(default = "default_reconnect_interval")]
    pub reconnect_interval_secs: u64,
    /// 是否自动重连
    #[serde(default = "default_true")]
    pub auto_reconnect: bool,
}

fn default_reconnect_interval() -> u64 {
    5
}

fn default_prefix() -> String {
    "!".to_string()
}

fn default_true() -> bool {
    true
}

/// 内存系统配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    /// 工作目录路径（用于存储 Markdown 记忆文件）
    #[serde(default = "default_workspace_path")]
    pub workspace_path: PathBuf,
    /// 最大记忆条数
    #[serde(default = "default_max_memories")]
    pub max_memories: usize,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            workspace_path: default_workspace_path(),
            max_memories: default_max_memories(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsConfig {
    /// Shell 命令白名单
    #[serde(default)]
    pub shell_whitelist: Vec<String>,
    /// 允许的文件路径
    #[serde(default)]
    pub allowed_paths: Vec<String>,
    /// Web 搜索 API Key
    pub search_api_key: Option<String>,
}

impl Default for ToolsConfig {
    fn default() -> Self {
        Self {
            shell_whitelist: vec!["echo".to_string(), "cat".to_string(), "ls".to_string()],
            allowed_paths: vec!["/home".to_string(), "/tmp".to_string()],
            search_api_key: None,
        }
    }
}

// 默认值函数
fn default_system_prompt() -> String {
    "你是一个有帮助的 AI 助手。你可以使用工具来完成用户的请求。".to_string()
}

fn default_max_context() -> usize {
    20
}

fn default_provider() -> String {
    "openrouter".to_string()
}

fn default_model() -> String {
    "openrouter/optimus-alpha".to_string()
}

fn default_timeout() -> u64 {
    60
}

fn default_workspace_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    home.join(".nanobot")
}

fn default_max_memories() -> usize {
    1000
}

impl Config {
    /// 加载配置文件
    pub fn load(path: Option<&str>) -> Result<Self> {
        let config_path = if let Some(p) = path {
            PathBuf::from(p)
        } else {
            Self::default_config_path()?
        };

        if !config_path.exists() {
            anyhow::bail!("配置文件不存在: {}", config_path.display());
        }

        let content = std::fs::read_to_string(&config_path)
            .with_context(|| format!("读取配置文件失败: {}", config_path.display()))?;
        
        let mut config: Config = toml::from_str(&content)
            .with_context(|| "解析配置文件失败")?;

        // 环境变量覆盖
        config.apply_env_overrides();

        Ok(config)
    }

    /// 保存配置文件
    pub fn save(&self, path: Option<&str>) -> Result<()> {
        let config_path = if let Some(p) = path {
            PathBuf::from(p)
        } else {
            Self::default_config_path()?
        };

        // 确保目录存在
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(self)?;
        std::fs::write(&config_path, content)?;

        Ok(())
    }

    /// 默认配置文件路径
    pub fn default_config_path() -> Result<PathBuf> {
        let home = dirs::home_dir()
            .context("无法获取家目录")?;
        Ok(home.join(".nanobot").join("config.toml"))
    }

    /// 应用环境变量覆盖
    fn apply_env_overrides(&mut self) {
        // LLM API Keys
        if let Ok(key) = std::env::var("OPENROUTER_API_KEY") {
            self.llm.openrouter.api_key = Some(key);
        }
        if let Ok(key) = std::env::var("DEEPSEEK_API_KEY") {
            self.llm.deepseek.api_key = Some(key);
        }
        if let Ok(key) = std::env::var("MOONSHOT_API_KEY") {
            self.llm.moonshot.api_key = Some(key);
        }
        if let Ok(url) = std::env::var("VLLM_BASE_URL") {
            self.llm.vllm.base_url = Some(url);
        }
        if let Ok(key) = std::env::var("VLLM_API_KEY") {
            self.llm.vllm.api_key = Some(key);
        }
        if let Ok(key) = std::env::var("OPENAI_API_KEY") {
            self.llm.openai.api_key = Some(key);
        }
        if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
            self.llm.anthropic.api_key = Some(key);
        }
        
        // Telegram
        if let Ok(token) = std::env::var("TELEGRAM_BOT_TOKEN") {
            self.channel.telegram.bot_token = Some(token);
        }
        
        // Discord
        if let Ok(token) = std::env::var("DISCORD_BOT_TOKEN") {
            self.channel.discord.bot_token = Some(token);
        }
        if let Ok(app_id) = std::env::var("DISCORD_APPLICATION_ID") {
            if let Ok(id) = app_id.parse::<u64>() {
                self.channel.discord.application_id = Some(id);
            }
        }
        
        // Feishu
        if let Ok(app_id) = std::env::var("FEISHU_APP_ID") {
            self.channel.feishu.app_id = Some(app_id);
        }
        if let Ok(app_secret) = std::env::var("FEISHU_APP_SECRET") {
            self.channel.feishu.app_secret = Some(app_secret);
        }
        if let Ok(verification_token) = std::env::var("FEISHU_VERIFICATION_TOKEN") {
            self.channel.feishu.verification_token = Some(verification_token);
        }
        if let Ok(encrypt_key) = std::env::var("FEISHU_ENCRYPT_KEY") {
            self.channel.feishu.encrypt_key = Some(encrypt_key);
        }
        
        // WhatsApp
        if let Ok(bridge_url) = std::env::var("WHATSAPP_BRIDGE_URL") {
            self.channel.whatsapp.bridge_url = Some(bridge_url);
        }
        
        // 搜索 API
        if let Ok(key) = std::env::var("SEARCH_API_KEY") {
            self.tools.search_api_key = Some(key);
        }
    }

    /// 生成示例配置
    pub fn example() -> Self {
        Self {
            agent: AgentConfig {
                system_prompt: "你是一个有帮助的 AI 助手。".to_string(),
                max_context: 20,
                default_provider: "openrouter".to_string(),
                default_model: "openrouter/optimus-alpha".to_string(),
            },
            llm: LlmConfig {
                openrouter: ProviderConfig {
                    api_key: Some("your-openrouter-api-key".to_string()),
                    base_url: Some("https://openrouter.ai/api/v1".to_string()),
                    default_model: Some("openrouter/optimus-alpha".to_string()),
                    timeout_secs: 60,
                },
                deepseek: ProviderConfig {
                    api_key: Some("your-deepseek-api-key".to_string()),
                    base_url: Some("https://api.deepseek.com".to_string()),
                    default_model: Some("deepseek-chat".to_string()),
                    timeout_secs: 60,
                },
                moonshot: ProviderConfig {
                    api_key: Some("your-moonshot-api-key".to_string()),
                    base_url: Some("https://api.moonshot.cn/v1".to_string()),
                    default_model: Some("moonshot-v1-8k".to_string()),
                    timeout_secs: 60,
                },
                vllm: ProviderConfig {
                    api_key: Some("".to_string()),
                    base_url: Some("http://localhost:8000/v1".to_string()),
                    default_model: Some("default".to_string()),
                    timeout_secs: 60,
                },
                openai: ProviderConfig::default(),
                anthropic: ProviderConfig::default(),
            },
            channel: ChannelConfig {
                telegram: TelegramConfig {
                    bot_token: Some("your-telegram-bot-token".to_string()),
                    allowed_users: vec![],
                    webhook_url: None,
                },
                discord: DiscordConfig {
                    bot_token: Some("your-discord-bot-token".to_string()),
                    application_id: Some(1234567890123456789),
                    allowed_guilds: vec![],
                    allowed_channels: vec![],
                    allowed_users: vec![],
                    prefix: "!".to_string(),
                    webhook_url: None,
                    enable_slash_commands: true,
                },
                feishu: FeishuConfig {
                    app_id: Some("cli_xxxxxxxxxxxxxxxx".to_string()),
                    app_secret: Some("xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx".to_string()),
                    verification_token: Some("xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx".to_string()),
                    encrypt_key: Some("xxxxxxxxxxxxxxxx".to_string()),
                    allowed_users: vec![],
                    allowed_open_ids: vec![],
                    allowed_chats: vec![],
                    verify_signature: true,
                    card_template_id: None,
                },
                whatsapp: WhatsAppConfig {
                    bridge_url: Some("ws://localhost:3000".to_string()),
                    allowed_users: vec![],
                    reconnect_interval_secs: 5,
                    auto_reconnect: true,
                },
            },
            memory: MemoryConfig {
                workspace_path: default_workspace_path(),
                max_memories: 1000,
            },
            tools: ToolsConfig {
                shell_whitelist: vec!["echo".to_string(), "cat".to_string(), "ls".to_string(), "pwd".to_string()],
                allowed_paths: vec!["/home".to_string(), "/tmp".to_string()],
                search_api_key: Some("your-search-api-key".to_string()),
            },
        }
    }
}
