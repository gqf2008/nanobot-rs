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

/// 媒体类型枚举
#[derive(Debug, Clone)]
pub enum MediaType {
    Image,
    Audio,
    File,
}

/// 媒体消息结构
#[derive(Debug, Clone)]
pub struct Media {
    pub media_type: MediaType,
    pub path: Option<String>,
    pub url: Option<String>,
    pub name: Option<String>,
}

impl Media {
    pub fn new_image(path: Option<String>, url: Option<String>, name: Option<String>) -> Self {
        Self { media_type: MediaType::Image, path, url, name }
    }

    pub fn new_audio(path: Option<String>, url: Option<String>, name: Option<String>) -> Self {
        Self { media_type: MediaType::Audio, path, url, name }
    }

    pub fn new_file(path: Option<String>, url: Option<String>, name: Option<String>) -> Self {
        Self { media_type: MediaType::File, path, url, name }
    }
}

/// 通道 trait - 定义消息通道的基本接口
#[async_trait]
pub trait Channel: Send + Sync {
    /// 通道名称
    fn name(&self) -> &str;
    
    /// 启动通道服务
    async fn start(&self) -> Result<()>;
    
    /// 停止通道服务
    async fn stop(&self) -> Result<()>;
    
    /// 发送文本消息
    async fn send_message(
        &self,
        target: &str,
        content: &str,
    ) -> Result<()>;
    
    /// 发送媒体消息（可选实现）
    async fn send_media(
        &self,
        target: &str,
        media: &Media,
    ) -> Result<()> {
        Err(anyhow::anyhow!("{} 不支持发送媒体消息", self.name()))
    }
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
