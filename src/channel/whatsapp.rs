//! WhatsApp 通道实现
//! 
//! 通过 WebSocket 连接到 Node.js Bridge（使用 whatsapp-web.js 或 @whiskeysockets/baileys）
//! Bridge 负责处理 WhatsApp Web 协议，Rust 端通过 WebSocket 与之通信

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::RwLock;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};
use tracing::{error, info, warn};

use crate::channel::Channel;
use crate::config::WhatsAppConfig;

/// WebSocket 消息类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
enum BridgeMessage {
    #[serde(rename = "message")]
    Message {
        sender: String,
        content: String,
        #[serde(rename = "messageId")]
        message_id: Option<String>,
        timestamp: Option<i64>,
        #[serde(rename = "isGroup")]
        is_group: Option<bool>,
    },
    #[serde(rename = "status")]
    Status { status: String },
    #[serde(rename = "qr")]
    Qr { qr: String },
    #[serde(rename = "error")]
    Error { error: String },
}

/// 发送到 Bridge 的消息
#[derive(Debug, Clone, Serialize)]
struct SendMessage {
    #[serde(rename = "type")]
    msg_type: String,
    to: String,
    text: String,
}

/// WhatsApp 通道
pub struct WhatsAppChannel {
    config: WhatsAppConfig,
    agent: Arc<crate::agent::Agent>,
    ws_stream: RwLock<Option<WebSocketStream<MaybeTlsStream<TcpStream>>>>,
    connected: RwLock<bool>,
    running: Arc<RwLock<bool>>,
}

impl WhatsAppChannel {
    pub fn new(
        config: WhatsAppConfig,
        agent: Arc<crate::agent::Agent>,
    ) -> Result<Self> {
        if config.bridge_url.is_none() {
            return Err(anyhow!("WhatsApp Bridge URL 未配置"));
        }

        Ok(Self {
            config,
            agent,
            ws_stream: RwLock::new(None),
            connected: RwLock::new(false),
            running: Arc::new(RwLock::new(false)),
        })
    }

    /// 检查用户是否有权限
    fn is_allowed(&self, phone_number: &str) -> bool {
        if self.config.allowed_users.is_empty() {
            return true; // 未配置白名单，允许所有用户
        }
        self.config.allowed_users.contains(&phone_number.to_string())
    }

    /// 处理来自 Bridge 的消息
    async fn handle_bridge_message(
        &self,
        raw: &str,
    ) -> Result<()> {
        let msg: BridgeMessage = serde_json::from_str(raw)
            .with_context(|| format!("解析 Bridge 消息失败: {}", raw))?;

        match msg {
            BridgeMessage::Message { sender, content, message_id: _, timestamp: _, is_group: _ } => {
                // 提取手机号（sender 格式通常是: <phone>@s.whatsapp.net）
                let phone_number = sender.split('@').next().unwrap_or(&sender);
                
                // 检查权限
                if !self.is_allowed(phone_number) {
                    warn!("用户 {} 尝试访问但被拒绝", phone_number);
                    return Ok(());
                }

                info!("收到 WhatsApp 消息 from={}: {}", phone_number, content);

                // 处理语音消息
                let content = if content == "[Voice Message]" {
                    "[语音消息: 暂不支持转录]".to_string()
                } else {
                    content
                };

                // 调用 Agent
                match self.agent.chat(&content).await {
                    Ok(response) => {
                        // 发送回复
                        if let Err(e) = self.send_message_internal(&sender, &response.content).await {
                            error!("发送 WhatsApp 消息失败: {}", e);
                        }
                    }
                    Err(e) => {
                        error!("Agent 错误: {}", e);
                        let _ = self.send_message_internal(&sender, &format!("❌ 错误: {}", e)).await;
                    }
                }
            }
            BridgeMessage::Status { status } => {
                info!("WhatsApp 状态更新: {}", status);
                match status.as_str() {
                    "connected" => {
                        *self.connected.write().await = true;
                    }
                    "disconnected" => {
                        *self.connected.write().await = false;
                    }
                    _ => {}
                }
            }
            BridgeMessage::Qr { qr: _ } => {
                info!("WhatsApp QR 码已生成，请在手机上扫描登录");
                // 可以在这里添加 QR 码显示逻辑
            }
            BridgeMessage::Error { error } => {
                error!("WhatsApp Bridge 错误: {}", error);
            }
        }

        Ok(())
    }

    /// 内部发送消息方法
    async fn send_message_internal(
        &self,
        to: &str,
        content: &str,
    ) -> Result<()> {
        let mut ws = self.ws_stream.write().await;
        
        if let Some(ref mut stream) = *ws {
            let msg = SendMessage {
                msg_type: "send".to_string(),
                to: to.to_string(),
                text: content.to_string(),
            };

            let json = serde_json::to_string(&msg)?;
            stream.send(tokio_tungstenite::tungstenite::Message::Text(json)).await?;
            Ok(())
        } else {
            Err(anyhow!("WebSocket 未连接"))
        }
    }

    /// 连接到 Bridge 并处理消息
    async fn connect_and_run(&self) -> Result<()> {
        let bridge_url = self.config.bridge_url.as_ref()
            .ok_or_else(|| anyhow!("Bridge URL 未配置"))?;

        info!("连接到 WhatsApp Bridge: {}", bridge_url);

        let (ws_stream, _) = connect_async(bridge_url).await
            .with_context(|| format!("无法连接到 WhatsApp Bridge: {}", bridge_url))?;

        *self.ws_stream.write().await = Some(ws_stream);
        *self.running.write().await = true;

        info!("已连接到 WhatsApp Bridge");

        // 分离读写
        let (mut write, mut read) = {
            let mut ws = self.ws_stream.write().await;
            ws.take().unwrap().split()
        };

        // 启动心跳任务
        let running = self.running.clone();
        let heartbeat_handle: tokio::task::JoinHandle<()> = tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
            loop {
                interval.tick().await;
                let ping = serde_json::json!({ "type": "ping" });
                let msg = tokio_tungstenite::tungstenite::Message::Text(ping.to_string());
                if let Err(e) = write.send(msg).await {
                    error!("发送心跳失败: {}", e);
                    break;
                }
            }
        });

        // 处理接收到的消息
        while let Some(msg) = read.next().await {
            match msg {
                Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                    if let Err(e) = self.handle_bridge_message(&text).await {
                        error!("处理 Bridge 消息错误: {}", e);
                    }
                }
                Ok(tokio_tungstenite::tungstenite::Message::Close(_)) => {
                    info!("WhatsApp Bridge 连接已关闭");
                    break;
                }
                Ok(_) => {} // 忽略其他消息类型
                Err(e) => {
                    error!("WebSocket 错误: {}", e);
                    break;
                }
            }
        }

        *self.running.write().await = false;
        heartbeat_handle.abort();
        *self.connected.write().await = false;

        Ok(())
    }
}

#[async_trait]
impl Channel for WhatsAppChannel {
    fn name(&self) -> &str {
        "whatsapp"
    }

    async fn start(&self) -> Result<()> {
        info!("启动 WhatsApp 通道...");

        if self.config.auto_reconnect {
            // 自动重连循环
            loop {
                match self.connect_and_run().await {
                    Ok(_) => {
                        if !*self.running.read().await {
                            break;
                        }
                    }
                    Err(e) => {
                        error!("WhatsApp 连接错误: {}", e);
                    }
                }

                if !*self.running.read().await {
                    break;
                }

                warn!("{} 秒后尝试重连...", self.config.reconnect_interval_secs);
                tokio::time::sleep(tokio::time::Duration::from_secs(
                    self.config.reconnect_interval_secs
                )).await;
            }
        } else {
            self.connect_and_run().await?;
        }

        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        info!("停止 WhatsApp 通道...");
        *self.running.write().await = false;
        *self.connected.write().await = false;

        let mut ws = self.ws_stream.write().await;
        if let Some(ref mut stream) = *ws {
            let _ = stream.close(None).await;
        }
        *ws = None;

        Ok(())
    }

    async fn send_message(
        &self,
        target: &str,
        content: &str,
    ) -> Result<()> {
        // 确保目标格式正确（添加 @s.whatsapp.net 后缀）
        let to = if target.contains('@') {
            target.to_string()
        } else {
            format!("{}@s.whatsapp.net", target)
        };

        self.send_message_internal(&to, content).await
    }
}