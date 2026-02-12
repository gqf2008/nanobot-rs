//! Telegram Bot 通道实现
//! 
//! 使用 teloxide 库与 Telegram API 交互

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use std::sync::Arc;
use teloxide::dispatching::{HandlerExt, UpdateFilterExt};
use teloxide::prelude::*;
use teloxide::types::{Message, ParseMode, Update};
use teloxide::utils::command::BotCommands;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

use crate::channel::Channel;
use crate::config::TelegramConfig;

/// Telegram Bot 命令
#[derive(BotCommands, Clone, Debug)]
#[command(rename_rule = "lowercase", description = "可用命令:")]
enum Command {
    #[command(description = "显示帮助信息")]
    Help,
    #[command(description = "开始对话")]
    Start,
    #[command(description = "清空对话上下文")]
    Clear,
    #[command(description = "查看当前状态")]
    Status,
}

/// Telegram 通道
pub struct TelegramChannel {
    config: TelegramConfig,
    bot: Bot,
    agent: Arc<crate::agent::Agent>,
    running: RwLock<bool>,
}

impl TelegramChannel {
    pub fn new(
        config: TelegramConfig,
        agent: Arc<crate::agent::Agent>,
    ) -> Result<Self> {
        let token = config.bot_token.as_ref()
            .ok_or_else(|| anyhow!("Telegram Bot Token 未配置"))?;

        let bot = Bot::new(token);

        Ok(Self {
            config,
            bot,
            agent,
            running: RwLock::new(false),
        })
    }

    /// 检查用户是否有权限
    fn is_allowed(&self,
        user_id: i64,
    ) -> bool {
        if self.config.allowed_users.is_empty() {
            return true; // 未配置白名单，允许所有用户
        }
        self.config.allowed_users.contains(&user_id)
    }

    /// 处理命令
    async fn handle_command(
        &self,
        bot: Bot,
        msg: Message,
        cmd: Command,
    ) -> Result<()> {
        let text = match cmd {
            Command::Help => {
                "🤖 *Nanobot 帮助*\n\n\
                    可用命令:\n\
                    /help - 显示此帮助\n\
                    /start - 开始对话\n\
                    /clear - 清空对话上下文\n\
                    /status - 查看状态\n\n\
                    直接发送消息即可与 AI 对话。".to_string()
            }
            Command::Start => {
                "👋 你好！我是 Nanobot，你的个人 AI 助手。\n\n直接发送消息即可开始对话。".to_string()
            }
            Command::Clear => {
                self.agent.clear_context().await;
                "🧹 对话上下文已清空。".to_string()
            }
            Command::Status => {
                let ctx_len = self.agent.context_length().await;
                let session_id = self.agent.session_id().await;
                format!(
                    "📊 *状态信息*\n\n\
                    会话 ID: `{}`\n\
                    上下文消息数: {}\n\
                    提供商: {}\n\
                    模型: {}",
                    session_id,
                    ctx_len,
                    "deepseek",
                    "deepseek-chat"
                )
            }
        };

        bot.send_message(msg.chat.id, text)
            .parse_mode(ParseMode::MarkdownV2)
            .await?;

        Ok(())
    }

    /// 处理文本消息
    async fn handle_message(
        &self,
        bot: Bot,
        msg: Message,
    ) -> Result<()> {
        let user_id = msg.from()
            .map(|u| u.id.0 as i64)
            .unwrap_or(0);

        // 检查权限
        if !self.is_allowed(user_id) {
            warn!("用户 {} 尝试访问但被拒绝", user_id);
            bot.send_message(msg.chat.id, "⛔ 你无权使用此 Bot。")
                .await?;
            return Ok(());
        }

        // 获取消息文本
        let text = msg.text()
            .ok_or_else(|| anyhow!("消息没有文本内容"))?;

        // 显示"正在输入"状态
        bot.send_chat_action(msg.chat.id, teloxide::types::ChatAction::Typing)
            .await?;

        // 设置会话 ID 为 telegram:chat_id，这样重启后能记住对话
        let session_key = format!("telegram:{}", msg.chat.id.0);
        self.agent.set_session_id(&session_key).await;

        // 调用 Agent
        match self.agent.chat(text).await {
            Ok(response) => {
                // 转义 Markdown 特殊字符
                let escaped = Self::escape_markdown(&response.content);
                
                // 分段发送长消息
                for chunk in Self::split_message(&escaped, 4096) {
                    bot.send_message(msg.chat.id, chunk)
                        .parse_mode(ParseMode::MarkdownV2)
                        .await?;
                }
            }
            Err(e) => {
                error!("Agent 错误: {}", e);
                bot.send_message(msg.chat.id, format!("❌ 错误: {}", e))
                    .await?;
            }
        }

        Ok(())
    }

    /// 转义 Markdown 特殊字符
    fn escape_markdown(text: &str) -> String {
        let special_chars = ['_', '*', '[', ']', '(', ')', '~', '`', '>', '#', '+', '-', '=', '|', '{', '}', '.', '!'];
        let mut result = String::with_capacity(text.len() * 2);
        
        for ch in text.chars() {
            if special_chars.contains(&ch) {
                result.push('\\');
            }
            result.push(ch);
        }
        
        result
    }

    /// 分割长消息
    fn split_message(text: &str, max_len: usize) -> Vec<String> {
        if text.len() <= max_len {
            return vec![text.to_string()];
        }

        let mut chunks = Vec::new();
        let mut start = 0;

        while start < text.len() {
            let end = (start + max_len).min(text.len());
            // 尝试在换行处分割
            let split_pos = if end < text.len() {
                text[start..end].rfind('\n').map(|pos| start + pos + 1).unwrap_or(end)
            } else {
                end
            };
            
            chunks.push(text[start..split_pos].to_string());
            start = split_pos;
        }

        chunks
    }
}

#[async_trait]
impl Channel for TelegramChannel {
    fn name(&self) -> &str {
        "telegram"
    }

    async fn start(&self) -> Result<()> {
        info!("启动 Telegram Bot...");

        let bot = self.bot.clone();
        let agent = self.agent.clone();
        let config = self.config.clone();
        let channel = Arc::new(TelegramChannel {
            config,
            bot: bot.clone(),
            agent,
            running: RwLock::new(true),
        });

        // 设置命令
        bot.set_my_commands(Command::bot_commands()).await?;

        info!("Telegram Bot 已启动，正在监听消息...");

        // 为每个分支克隆 channel
        let channel_cmd = channel.clone();
        let channel_msg = channel.clone();

        // 启动消息处理
        let handler = Update::filter_message()
            .branch(
                dptree::entry()
                    .filter_command::<Command>()
                    .endpoint(move |bot: Bot, msg: Message, cmd: Command| {
                        let channel = channel_cmd.clone();
                        async move {
                            if let Err(e) = channel.handle_command(bot, msg, cmd).await {
                                error!("处理命令错误: {}", e);
                            }
                            Ok::<(), anyhow::Error>(())
                        }
                    }),
            )
            .branch(
                dptree::endpoint(move |bot: Bot, msg: Message| {
                    let channel = channel_msg.clone();
                    async move {
                        if let Err(e) = channel.handle_message(bot, msg).await {
                            error!("处理消息错误: {}", e);
                        }
                        Ok::<(), anyhow::Error>(())
                    }
                }),
            );

        Dispatcher::builder(bot, handler)
            .enable_ctrlc_handler()
            .build()
            .dispatch()
            .await;

        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        info!("停止 Telegram Bot...");
        *self.running.write().await = false;
        Ok(())
    }

    async fn send_message(
        &self,
        target: &str,
        content: &str,
    ) -> Result<()> {
        let chat_id: i64 = target.parse()
            .context("无效的 chat ID")?;
        
        self.bot.send_message(ChatId(chat_id), content)
            .await?;
        
        Ok(())
    }
}

use teloxide::dispatching::Dispatcher;
use teloxide::dptree;
