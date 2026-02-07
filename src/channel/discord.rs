//! Discord 通道实现
//!
//! 使用 serenity 库与 Discord API 交互

use anyhow::{Context, Result};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use crate::channel::Channel;
use crate::config::DiscordConfig;

/// Discord 通道
pub struct DiscordChannel {
    config: DiscordConfig,
    agent: Arc<crate::agent::Agent>,
    /// 运行状态
    running: RwLock<bool>,
}

impl DiscordChannel {
    /// 创建新的 Discord 通道
    pub fn new(
        config: DiscordConfig,
        agent: Arc<crate::agent::Agent>,
    ) -> Result<Self> {
        // 验证配置
        if config.bot_token.is_none() {
            anyhow::bail!("Discord Bot Token 未配置");
        }

        Ok(Self {
            config,
            agent,
            running: RwLock::new(false),
        })
    }

    /// 检查服务器是否在白名单中
    fn is_guild_allowed(&self, guild_id: u64) -> bool {
        if self.config.allowed_guilds.is_empty() {
            return true;
        }
        self.config.allowed_guilds.contains(&guild_id)
    }

    /// 检查频道是否在白名单中
    fn is_channel_allowed(&self, channel_id: u64) -> bool {
        if self.config.allowed_channels.is_empty() {
            return true;
        }
        self.config.allowed_channels.contains(&channel_id)
    }

    /// 检查用户是否在白名单中
    fn is_user_allowed(&self, user_id: u64) -> bool {
        if self.config.allowed_users.is_empty() {
            return true;
        }
        self.config.allowed_users.contains(&user_id)
    }

    /// 分割长消息（Discord 限制 2000 字符）
    fn split_message(content: &str, max_length: usize) -> Vec<String> {
        if content.len() <= max_length {
            return vec![content.to_string()];
        }

        let mut chunks = Vec::new();
        let mut start = 0;

        while start < content.len() {
            let end = (start + max_length).min(content.len());
            let chunk = &content[start..end];

            // 尝试在换行处分割
            let split_pos = if end < content.len() {
                chunk.rfind('\n').map(|pos| start + pos + 1)
                    .or_else(|| chunk.rfind(' ').map(|pos| start + pos + 1))
                    .unwrap_or(end)
            } else {
                end
            };

            chunks.push(content[start..split_pos].to_string());
            start = split_pos;
        }

        chunks
    }
}

#[async_trait]
impl Channel for DiscordChannel {
    fn name(&self) -> &str {
        "discord"
    }

    async fn start(&self) -> Result<()> {
        info!("启动 Discord Bot...");

        // TODO: 使用 serenity 实现完整的 Discord Bot
        // 1. 创建 serenity Client
        // 2. 设置事件处理器
        // 3. 连接到 Discord Gateway
        // 4. 启动消息监听循环

        // 由于 serenity 的复杂性，这里提供一个简化的实现框架
        // 实际使用时需要完整实现 serenity 的事件处理器

        info!("Discord Bot 已启动（简化模式）");
        *self.running.write().await = true;

        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        info!("停止 Discord Bot...");
        *self.running.write().await = false;
        info!("Discord Bot 已停止");
        Ok(())
    }

    async fn send_message(
        &self,
        target: &str,
        content: &str,
    ) -> Result<()> {
        info!("发送 Discord 消息到 {}: {}", target, content);

        // 解析 target 为 channel_id
        let channel_id: u64 = target
            .parse()
            .context("无效的 Discord Channel ID")?;

        // 检查白名单
        if !self.is_channel_allowed(channel_id) {
            anyhow::bail!("频道 {} 不在白名单中", channel_id);
        }

        // 分割长消息
        let chunks = Self::split_message(content, 2000);

        // TODO: 使用 serenity 发送消息
        for (i, chunk) in chunks.iter().enumerate() {
            info!("发送消息块 {}/{}: {}", i + 1, chunks.len(), chunk);
        }

        Ok(())
    }
}

// ============== Serenity 实现框架 ==============
// 以下代码展示了如何使用 serenity 实现完整的 Discord Bot
// 实际使用时需要取消注释并完善

/*
use serenity::async_trait as serenity_async_trait;
use serenity::model::application::interaction::{Interaction, InteractionResponseType};
use serenity::model::channel::Message;
use serenity::model::gateway::Ready;
use serenity::model::id::{ChannelId, GuildId, UserId};
use serenity::prelude::*;

struct DiscordHandler {
    agent: Arc<crate::agent::Agent>,
    config: DiscordConfig,
}

#[serenity_async_trait]
impl EventHandler for DiscordHandler {
    async fn message(&self,
        ctx: Context,
        msg: Message,
    ) {
        // 忽略自己的消息
        if msg.author.bot {
            return;
        }

        // 检查白名单
        if let Some(guild_id) = msg.guild_id {
            if !self.is_guild_allowed(guild_id.0) {
                return;
            }
        }

        if !self.is_channel_allowed(msg.channel_id.0) {
            return;
        }

        if !self.is_user_allowed(msg.author.id.0) {
            return;
        }

        // 处理消息
        info!("收到 Discord 消息: {}", msg.content);

        // 调用 Agent 处理
        match self.agent.chat(&msg.content).await {
            Ok(response) => {
                // 发送响应
                let chunks = DiscordChannel::split_message(&response.content, 2000);
                for chunk in chunks {
                    if let Err(e) = msg.channel_id.say(&ctx.http, chunk).await {
                        error!("发送消息失败: {}", e);
                    }
                }
            }
            Err(e) => {
                error!("Agent 处理失败: {}", e);
                let _ = msg.channel_id.say(&ctx.http, "处理消息时出错").await;
            }
        }
    }

    async fn ready(&self,
        _ctx: Context,
        ready: Ready,
    ) {
        info!("Discord Bot 已连接: {}#{}", ready.user.name, ready.user.discriminator);
    }

    async fn interaction_create(&self,
        ctx: Context,
        interaction: Interaction,
    ) {
        if let Interaction::ApplicationCommand(command) = interaction {
            info!("收到 Slash Command: {}", command.data.name);

            let content = match command.data.name.as_str() {
                "help" => "可用命令:\n/help - 显示帮助\n/clear - 清空上下文\n/status - 查看状态".to_string(),
                "clear" => {
                    // TODO: 清空会话上下文
                    "上下文已清空".to_string()
                }
                "status" => {
                    // TODO: 返回状态信息
                    "Bot 运行正常".to_string()
                }
                _ => "未知命令".to_string(),
            };

            if let Err(e) = command
                .create_interaction_response(&ctx.http,
                    |response| {
                        response
                            .kind(InteractionResponseType::ChannelMessageWithSource)
                            .interaction_response_data(|message| message.content(content))
                    },
                )
                .await
            {
                error!("响应命令失败: {}", e);
            }
        }
    }
}

impl DiscordHandler {
    fn is_guild_allowed(&self, guild_id: u64) -> bool {
        if self.config.allowed_guilds.is_empty() {
            return true;
        }
        self.config.allowed_guilds.contains(&guild_id)
    }

    fn is_channel_allowed(&self, channel_id: u64) -> bool {
        if self.config.allowed_channels.is_empty() {
            return true;
        }
        self.config.allowed_channels.contains(&channel_id)
    }

    fn is_user_allowed(&self, user_id: u64) -> bool {
        if self.config.allowed_users.is_empty() {
            return true;
        }
        self.config.allowed_users.contains(&user_id)
    }
}
*/

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_message() {
        let content = "a".repeat(2500);
        let chunks = DiscordChannel::split_message(&content, 2000);
        assert!(chunks.len() > 1);
        assert!(chunks[0].len() <= 2000);
    }

    #[test]
    fn test_split_message_short() {
        let content = "Hello, World!";
        let chunks = DiscordChannel::split_message(content, 2000);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], content);
    }
}
