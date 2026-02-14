//! Message Tool - 发送消息到聊天通道
//!
//! 允许 Agent 通过工具向用户发送消息

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

use crate::channel::Channel;

/// 消息工具配置
#[derive(Debug, Clone)]
pub struct MessageToolConfig {
    pub default_channel: String,
    pub default_chat_id: String,
}

impl Default for MessageToolConfig {
    fn default() -> Self {
        Self {
            default_channel: String::new(),
            default_chat_id: String::new(),
        }
    }
}

/// 消息工具
#[derive(Clone)]
pub struct MessageTool {
    /// 通道管理器引用
    channels: Vec<Arc<dyn crate::channel::Channel>>,
    /// 默认通道
    default_channel: String,
    /// 默认聊天 ID
    default_chat_id: String,
}

impl MessageTool {
    pub fn new(channels: Vec<Arc<dyn crate::channel::Channel>>) -> Self {
        Self {
            channels,
            default_channel: String::new(),
            default_chat_id: String::new(),
        }
    }

    /// 设置当前上下文
    pub fn set_context(&mut self, channel: &str, chat_id: &str) {
        self.default_channel = channel.to_string();
        self.default_chat_id = chat_id.to_string();
    }
}

#[async_trait]
impl crate::tools::Tool for MessageTool {
    fn definition(&self) -> &crate::tools::ToolDef {
        &crate::tools::ToolDef {
            name: "message".to_string(),
            description: "Send a message to the user. Use this when you want to communicate something to the user on the chat platform.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "content": {
                        "type": "string",
                        "description": "The message content to send"
                    },
                    "channel": {
                        "type": "string", 
                        "description": "Optional: target channel (telegram, discord, feishu, whatsapp)"
                    },
                    "chat_id": {
                        "type": "string",
                        "description": "Optional: target chat/user ID"
                    }
                },
                "required": ["content"]
            }),
        }
    }

    async fn execute(&self, args: Value, _ctx: &crate::tools::ToolContext) -> Result<crate::tools::ToolResult> {
        let content = args.get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'content' parameter"))?;

        let channel = args.get("channel")
            .and_then(|v| v.as_str())
            .unwrap_or(&self.default_channel);

        let chat_id = args.get("chat_id")
            .and_then(|v| v.as_str())
            .unwrap_or(&self.default_chat_id);

        if channel.is_empty() || chat_id.is_empty() {
            return Ok(crate::tools::ToolResult::error(
                "No target channel/chat specified"
            ));
        }

        // 查找目标通道
        let target_channel = if channel.is_empty() {
            self.channels.first()
        } else {
            self.channels.iter().find(|c| c.name() == channel)
        };

        match target_channel {
            Some(ch) => {
                match ch.send_message(chat_id, content).await {
                    Ok(_) => Ok(crate::tools::ToolResult::success(
                        format!("Message sent to {}:{}", channel, chat_id)
                    )),
                    Err(e) => Ok(crate::tools::ToolResult::error(
                        format!("Failed to send message: {}", e)
                    )),
                }
            }
            None => Ok(crate::tools::ToolResult::error(
                format!("Channel '{}' not found", channel)
            )),
        }
    }
}
