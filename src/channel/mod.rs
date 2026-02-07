//! 通道模块 - 支持多平台集成
//! 
//! 目前支持 Telegram Bot、Discord、飞书、WhatsApp

use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;

pub mod discord;
pub mod feishu;
pub mod telegram;
pub mod whatsapp;

/// 通道 trait - 定义消息通道的基本接口
#[async_trait]
pub trait Channel: Send + Sync {
    /// 通道名称
    fn name(&self) -> &str;
    
    /// 启动通道服务
    async fn start(&self) -> Result<()>;
    
    /// 停止通道服务
    async fn stop(&self) -> Result<()>;
    
    /// 发送消息
    async fn send_message(
        &self,
        target: &str,
        content: &str,
    ) -> Result<()>;
}

/// 通道工厂
pub struct ChannelFactory;

impl ChannelFactory {
    /// 创建通道实例
    pub fn create(
        name: &str,
        config: &crate::config::Config,
        agent: Arc<crate::agent::Agent>,
    ) -> Result<Arc<dyn Channel>> {
        match name {
            "telegram" => {
                let channel = telegram::TelegramChannel::new(
                    config.channel.telegram.clone(),
                    agent,
                )?;
                Ok(Arc::new(channel))
            }
            "discord" => {
                let channel = discord::DiscordChannel::new(
                    config.channel.discord.clone(),
                    agent,
                )?;
                Ok(Arc::new(channel))
            }
            "feishu" => {
                let channel = feishu::FeishuChannel::new(
                    config.channel.feishu.clone(),
                    agent,
                )?;
                Ok(Arc::new(channel))
            }
            "whatsapp" => {
                let channel = whatsapp::WhatsAppChannel::new(
                    config.channel.whatsapp.clone(),
                    agent,
                )?;
                Ok(Arc::new(channel))
            }
            _ => Err(anyhow::anyhow!("未知的通道: {}", name)),
        }
    }
}

/// 通道管理器
pub struct ChannelManager {
    channels: Vec<Arc<dyn Channel>>,
}

impl ChannelManager {
    pub fn new() -> Self {
        Self {
            channels: Vec::new(),
        }
    }

    /// 注册通道
    pub fn register(&mut self, channel: Arc<dyn Channel>) {
        self.channels.push(channel);
    }

    /// 启动所有通道
    pub async fn start_all(&self) -> Result<()> {
        for channel in &self.channels {
            info!("启动通道: {}", channel.name());
            channel.start().await?;
        }
        Ok(())
    }

    /// 停止所有通道
    pub async fn stop_all(&self) -> Result<()> {
        for channel in &self.channels {
            info!("停止通道: {}", channel.name());
            channel.stop().await?;
        }
        Ok(())
    }
}

use tracing::info;
