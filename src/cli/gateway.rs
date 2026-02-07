//! gateway 命令 - 启动网关服务

use anyhow::Result;
use std::sync::Arc;
use tracing::{info, warn};

use crate::agent::Agent;
use crate::channel::ChannelManager;
use crate::config::Config;

pub async fn run(config: Config, channel: Option<String>) -> Result<()> {
    info!("启动 Nanobot Gateway...");

    // 创建 Agent
    let agent = Arc::new(Agent::new(config.clone()).await?);

    let mut manager = ChannelManager::new();

    // 确定要启动的通道
    let channels_to_start: Vec<String> = if let Some(ch) = channel {
        vec![ch]
    } else {
        // 默认启动所有已配置的通道
        let mut channels = Vec::new();
        
        if config.channel.telegram.bot_token.is_some() {
            channels.push("telegram".to_string());
        }
        
        channels
    };

    if channels_to_start.is_empty() {
        warn!("没有配置任何通道，请先配置");
        return Ok(());
    }

    // 注册并启动通道
    for channel_name in channels_to_start {
        info!("注册通道: {}", channel_name);
        
        match crate::channel::ChannelFactory::create(&channel_name, &config, agent.clone()
        ) {
            Ok(channel) => {
                manager.register(channel);
            }
            Err(e) => {
                warn!("无法创建通道 '{}': {}", channel_name, e);
            }
        }
    }

    // 启动所有通道
    manager.start_all().await?;

    Ok(())
}
