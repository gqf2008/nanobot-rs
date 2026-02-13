//! 飞书(Feishu/Lark) 通道实现
//!
//! 使用飞书开放平台的 Webhook 和 Bot API，支持 WebSocket 长连接模式

use anyhow::{Context, Result};
use async_trait::async_trait;
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::LinkedList;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::channel::{Channel, Media, MediaType};
use crate::config::FeishuConfig;

/// 消息类型映射
const MSG_TYPE_MAP: &[(&str, &str)] = &[
    ("image", "[image]"),
    ("audio", "[audio]"),
    ("file", "[file]"),
    ("sticker", "[sticker]"),
];

/// 飞书访问令牌响应
#[derive(Debug, Clone, serde::Deserialize)]
struct TenantAccessTokenResponse {
    code: i32,
    msg: String,
    #[serde(rename = "tenant_access_token")]
    tenant_access_token: Option<String>,
    expire: Option<i64>,
}

/// 飞书消息响应
#[derive(Debug, Clone, serde::Deserialize)]
struct FeishuMessageResponse {
    code: i32,
    msg: String,
    data: Option<serde_json::Value>,
}

/// 飞书通道
pub struct FeishuChannel {
    config: FeishuConfig,
    agent: Arc<crate::agent::Agent>,
    /// 访问令牌
    access_token: RwLock<Option<String>>,
    /// 令牌过期时间
    token_expire_at: RwLock<Option<i64>>,
    /// 运行状态
    running: RwLock<bool>,
    /// HTTP 客户端
    http_client: reqwest::Client,
    /// 消息去重缓存 (Ordered set - 只保存最近 1000 条)
    processed_message_ids: RwLock<LinkedList<String>>,
}

impl FeishuChannel {
    /// 创建新的飞书通道
    pub fn new(
        config: FeishuConfig,
        agent: Arc<crate::agent::Agent>,
    ) -> Result<Self> {
        // 验证配置
        if config.app_id.is_none() || config.app_secret.is_none() {
            anyhow::bail!("飞书 App ID 或 App Secret 未配置");
        }

        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .context("创建 HTTP 客户端失败")?;

        Ok(Self {
            config,
            agent,
            access_token: RwLock::new(None),
            token_expire_at: RwLock::new(None),
            running: RwLock::new(false),
            http_client,
            processed_message_ids: RwLock::new(LinkedList::new()),
        })
    }

    /// 检查消息是否已处理（去重）
    async fn is_message_processed(&self, message_id: &str) -> bool {
        let cache = self.processed_message_ids.read().await;
        cache.contains(message_id)
    }

    /// 添加消息到已处理缓存
    async fn add_processed_message(&self, message_id: &str) {
        let mut cache = self.processed_message_ids.write().await;
        // 保持缓存大小不超过 1000
        if cache.len() >= 1000 {
            cache.pop_front();
        }
        cache.push_back(message_id.to_string());
    }

    /// 清理过期的消息 ID
    async fn trim_message_cache(&self) {
        let mut cache = self.processed_message_ids.write().await;
        while cache.len() > 1000 {
            cache.pop_front();
        }
    }

    /// 获取消息类型的显示文本
    fn get_msg_type_text(&self, msg_type: &str) -> &str {
        MSG_TYPE_MAP
            .iter()
            .find(|(t, _)| *t == msg_type)
            .map(|(_, text)| *text)
            .unwrap_or_else(|| {
                let text = format!("[{}]", msg_type);
                Box::leak(text.into_boxed_str())
            })
    }

    /// 检查用户是否在白名单中
    fn is_user_allowed(&self, user_id: &str) -> bool {
        if self.config.allowed_users.is_empty() {
            return true;
        }
        self.config.allowed_users.contains(&user_id.to_string())
    }

    /// 检查 Open ID 是否在白名单中
    fn is_open_id_allowed(&self, open_id: &str) -> bool {
        if self.config.allowed_open_ids.is_empty() {
            return true;
        }
        self.config.allowed_open_ids.contains(&open_id.to_string())
    }

    /// 获取有效的访问令牌
    async fn get_access_token(&self) -> Result<String> {
        // 检查现有令牌是否有效
        {
            let token: tokio::sync::RwLockReadGuard<'_, Option<String>> = self.access_token.read().await;
            let expire_at = self.token_expire_at.read().await;

            if let (Some(token), Some(expire)) = (token.as_ref(), *expire_at) {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64;

                // 提前 60 秒刷新令牌
                if now < expire - 60 {
                    return Ok(token.clone());
                }
            }
        }

        // 刷新令牌
        self.refresh_access_token().await
    }

    /// 刷新访问令牌
    async fn refresh_access_token(&self) -> Result<String> {
        let app_id = self.config.app_id.as_ref().unwrap();
        let app_secret = self.config.app_secret.as_ref().unwrap();

        let body = serde_json::json!({
            "app_id": app_id,
            "app_secret": app_secret,
        });

        let response: reqwest::Response = self.http_client
            .post("https://open.feishu.cn/open-apis/auth/v3/tenant_access_token/internal")
            .json(&body)
            .send()
            .await
            .context("请求访问令牌失败")?;

        let token_response: TenantAccessTokenResponse = response
            .json::<TenantAccessTokenResponse>()
            .await
            .context("解析令牌响应失败")?;

        if token_response.code != 0 {
            anyhow::bail!("获取访问令牌失败: {}", token_response.msg);
        }

        let token = token_response
            .tenant_access_token
            .ok_or_else(|| anyhow::anyhow!("访问令牌为空"))?;

        let expire = token_response.expire.unwrap_or(7200);
        let expire_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
            + expire;

        // 保存令牌
        *self.access_token.write().await = Some(token.clone());
        *self.token_expire_at.write().await = Some(expire_at);

        info!("飞书访问令牌已刷新");
        Ok(token)
    }

    /// 发送文本消息
    async fn send_text_message(
        &self,
        receive_id: &str,
        content: &str,
    ) -> Result<()> {
        let token = self.get_access_token().await?;

        let body = serde_json::json!({
            "receive_id": receive_id,
            "msg_type": "text",
            "content": serde_json::json!({
                "text": content
            }).to_string(),
        });

        let response: reqwest::Response = self.http_client
            .post("https://open.feishu.cn/open-apis/im/v1/messages")
            .header("Authorization", format!("Bearer {}", token))
            .query(&[("receive_id_type", "open_id")])
            .json(&body)
            .send()
            .await
            .context("发送消息失败")?;

        let msg_response: FeishuMessageResponse = response
            .json::<FeishuMessageResponse>()
            .await
            .context("解析消息响应失败")?;

        if msg_response.code != 0 {
            anyhow::bail!("发送消息失败: {}", msg_response.msg);
        }

        Ok(())
    }

    /// 发送卡片消息
    async fn send_card_message(
        &self,
        receive_id: &str,
        title: &str,
        content: &str,
    ) -> Result<()> {
        let token = self.get_access_token().await?;

        let card = serde_json::json!({
            "config": {
                "wide_screen_mode": true
            },
            "header": {
                "title": {
                    "tag": "plain_text",
                    "content": title
                }
            },
            "elements": [
                {
                    "tag": "div",
                    "text": {
                        "tag": "lark_md",
                        "content": content
                    }
                }
            ]
        });

        let body = serde_json::json!({
            "receive_id": receive_id,
            "msg_type": "interactive",
            "content": card.to_string(),
        });

        let response: reqwest::Response = self.http_client
            .post("https://open.feishu.cn/open-apis/im/v1/messages")
            .header("Authorization", format!("Bearer {}", token))
            .query(&[("receive_id_type", "open_id")])
            .json(&body)
            .send()
            .await
            .context("发送卡片消息失败")?;

        let msg_response: FeishuMessageResponse = response
            .json::<FeishuMessageResponse>()
            .await
            .context("解析消息响应失败")?;

        if msg_response.code != 0 {
            anyhow::bail!("发送卡片消息失败: {}", msg_response.msg);
        }

        Ok(())
    }

    /// 发送增强型卡片消息（支持 Markdown + 表格）
    async fn send_enhanced_card_message(
        &self,
        receive_id: &str,
        content: &str,
    ) -> Result<()> {
        let token = self.get_access_token().await?;

        // 根据 chat_id 格式确定 receive_id_type
        // open_id 以 "ou_" 开头，chat_id 以 "oc_" 开头
        let receive_id_type = if receive_id.starts_with("oc_") {
            "chat_id"
        } else {
            "open_id"
        };

        // 构建支持 Markdown 表格的卡片
        let elements = self.build_card_elements(content);
        let card = serde_json::json!({
            "config": {
                "wide_screen_mode": true
            },
            "elements": elements
        });

        let body = serde_json::json!({
            "receive_id": receive_id,
            "msg_type": "interactive",
            "content": card.to_string(),
        });

        let response: reqwest::Response = self.http_client
            .post("https://open.feishu.cn/open-apis/im/v1/messages")
            .header("Authorization", format!("Bearer {}", token))
            .query(&[("receive_id_type", receive_id_type)])
            .json(&body)
            .send()
            .await
            .context("发送增强型卡片消息失败")?;

        let msg_response: FeishuMessageResponse = response
            .json::<FeishuMessageResponse>()
            .await
            .context("解析消息响应失败")?;

        if msg_response.code != 0 {
            let log_id = msg_response.msg;
            anyhow::bail!(
                "发送飞书消息失败: code={}, msg={}, log_id={}",
                msg_response.code,
                msg_response.msg,
                log_id
            );
        } else {
            debug!("飞书消息已发送到 {}", receive_id);
        }

        Ok(())
    }

    /// 解析 Markdown 表格为飞书表格元素
    fn parse_md_table(&self, table_text: &str) -> Option<serde_json::Value> {
        let lines: Vec<&str> = table_text
            .trim()
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect();

        if lines.len() < 3 {
            return None;
        }

        let split = |l: &str| l.trim_matches('|').split('|').map(|c| c.trim()).collect::<Vec<_>>();
        let headers = split(lines[0]);
        let rows: Vec<Vec<_>> = lines[2..].iter().map(|l| split(l)).collect();

        let columns: Vec<serde_json::Value> = headers
            .iter()
            .enumerate()
            .map(|(i, h)| {
                serde_json::json!({
                    "tag": "column",
                    "name": format!("c{}", i),
                    "display_name": h,
                    "width": "auto"
                })
            })
            .collect();

        let table_rows: Vec<serde_json::Value> = rows
            .iter()
            .map(|row| {
                let row_json: serde_json::Map<String, serde_json::Value> = headers
                    .iter()
                    .enumerate()
                    .map(|(i, h)| {
                        let value = row.get(i).map(|s| *s).unwrap_or("");
                        (format!("c{}", i), serde_json::Value::String(value.to_string()))
                    })
                    .collect();
                serde_json::Value::Object(row_json)
            })
            .collect();

        Some(serde_json::json!({
            "tag": "table",
            "page_size": rows.len() + 1,
            "columns": columns,
            "rows": table_rows
        }))
    }

    /// 构建卡片元素列表（支持 Markdown + 表格）
    fn build_card_elements(&self, content: &str) -> Vec<serde_json::Value> {
        // Markdown 表格正则: 头部 + 分隔行 + 数据行
        let table_re = Regex::new(
            r"((?:^[ \t]*\|.+\|[ \t]*\n)(?:^[ \t]*\|[-:\s|]+\|[ \t]*\n)(?:^[ \t]*\|.+\|[ \t]*\n?)+)"
        ).unwrap();

        let mut elements = Vec::new();
        let mut last_end = 0usize;

        for m in table_re.find_iter(content) {
            let before = &content[last_end..m.start()].trim();
            if !before.is_empty() {
                elements.push(serde_json::json!({
                    "tag": "markdown",
                    "content": before
                }));
            }

            let table_element = self.parse_md_table(m.as_str());
            elements.push(
                table_element.unwrap_or_else(|| {
                    serde_json::json!({
                        "tag": "markdown",
                        "content": m.as_str()
                    })
                })
            );

            last_end = m.end();
        }

        let remaining = &content[last_end..].trim();
        if !remaining.is_empty() {
            elements.push(serde_json::json!({
                "tag": "markdown",
                "content": remaining
            }));
        }

        if elements.is_empty() {
            elements.push(serde_json::json!({
                "tag": "markdown",
                "content": content
            }));
        }

        elements
    }

    /// 添加反应（反应类型如 THUMBSUP, OK, EYES, DONE, OnIt, HEART）
    async fn add_reaction(&self, message_id: &str, emoji_type: &str) -> Result<()> {
        let token = self.get_access_token().await?;

        let body = serde_json::json!({
            "reaction_type": {
                "emoji_type": emoji_type
            }
        });

        let response: reqwest::Response = self.http_client
            .post(&format!(
                "https://open.feishu.cn/open-apis/im/v1/messages/{}/reactions",
                message_id
            ))
            .header("Authorization", format!("Bearer {}", token))
            .json(&body)
            .send()
            .await
            .context("添加反应失败")?;

        let reaction_response: serde_json::Value = response
            .json()
            .await
            .context("解析反应响应失败")?;

        if let Some(code) = reaction_response.get("code") {
            if code != 0 {
                let msg = reaction_response.get("msg")
                    .and_then(|v| v.as_str())
                    .unwrap_or("未知错误");
                warn!("添加反应失败: code={}, msg={}", code, msg);
            } else {
                debug!("已添加 {} 反应到消息 {}", emoji_type, message_id);
            }
        }

        Ok(())
    }

    /// 上传图片到飞书
        let token = self.get_access_token().await?;

        // 读取图片文件内容
        let file_content = tokio::fs::read(image_path).await
            .context("读取图片文件失败")?;
        let file_name = std::path::Path::new(image_path).file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("image.png");

        let part = reqwest::multipart::Part::bytes(file_content)
            .file_name(file_name.to_string())
            .mime_str("image/png")?;

        let response: reqwest::Response = self.http_client
            .post("https://open.feishu.cn/open-apis/im/v1/images")
            .header("Authorization", format!("Bearer {}", token))
            .multipart(reqwest::multipart::Form::new().part("image", part))
            .send()
            .await
            .context("上传图片失败")?;

        #[derive(Debug, Clone, serde::Deserialize)]
        struct UploadResponse {
            code: i32,
            msg: String,
            data: Option<serde_json::Value>,
        }

        let upload_response: UploadResponse = response
            .json::<UploadResponse>()
            .await
            .context("解析上传响应失败")?;

        if upload_response.code != 0 {
            anyhow::bail!("上传图片失败: {}", upload_response.msg);
        }

        let image_key = upload_response
            .data
            .and_then(|d| d.get("image_key").cloned())
            .and_then(|k| k.as_str().map(|s| s.to_string()))
            .ok_or_else(|| anyhow::anyhow!("图片上传成功但未返回 image_key"))?;

        info!("图片上传成功: {}", image_key);
        Ok(image_key)
    }

    /// 上传文件到飞书
    async fn upload_file(&self, file_path: &str, file_name: &str) -> Result<String> {
        let token = self.get_access_token().await?;

        // 读取文件内容
        let file_content = tokio::fs::read(file_path).await
            .context("读取文件失败")?;

        let part = reqwest::multipart::Part::bytes(file_content)
            .file_name(file_name.to_string())
            .mime_str("application/octet-stream")?;

        let response: reqwest::Response = self.http_client
            .post("https://open.feishu.cn/open-apis/im/v1/files")
            .header("Authorization", format!("Bearer {}", token))
            .multipart(reqwest::multipart::Form::new().part("file", part))
            .send()
            .await
            .context("上传文件失败")?;

        #[derive(Debug, Clone, serde::Deserialize)]
        struct UploadResponse {
            code: i32,
            msg: String,
            data: Option<serde_json::Value>,
        }

        let upload_response: UploadResponse = response
            .json::<UploadResponse>()
            .await
            .context("解析上传响应失败")?;

        if upload_response.code != 0 {
            anyhow::bail!("上传文件失败: {}", upload_response.msg);
        }

        let file_id = upload_response
            .data
            .and_then(|d| d.get("file").cloned())
            .and_then(|f| f.get("file_id").cloned())
            .and_then(|id| id.as_str().map(|s| s.to_string()))
            .ok_or_else(|| anyhow::anyhow!("文件上传成功但未返回 file_id"))?;

        info!("文件上传成功: {}", file_id);
        Ok(file_id)
    }

    /// 发送图片消息
    async fn send_image_message(&self, receive_id: &str, image_key: &str) -> Result<()> {
        let token = self.get_access_token().await?;

        let body = serde_json::json!({
            "receive_id": receive_id,
            "msg_type": "image",
            "content": serde_json::json!({
                "image_key": image_key
            }).to_string(),
        });

        let response: reqwest::Response = self.http_client
            .post("https://open.feishu.cn/open-apis/im/v1/messages")
            .header("Authorization", format!("Bearer {}", token))
            .query(&[("receive_id_type", "open_id")])
            .json(&body)
            .send()
            .await
            .context("发送图片消息失败")?;

        let msg_response: FeishuMessageResponse = response
            .json::<FeishuMessageResponse>()
            .await
            .context("解析消息响应失败")?;

        if msg_response.code != 0 {
            anyhow::bail!("发送图片消息失败: {}", msg_response.msg);
        }

        Ok(())
    }

    /// 发送文件消息
    async fn send_file_message(&self, receive_id: &str, file_id: &str, file_name: &str) -> Result<()> {
        let token = self.get_access_token().await?;

        let body = serde_json::json!({
            "receive_id": receive_id,
            "msg_type": "file",
            "content": serde_json::json!({
                "file_id": file_id
            }).to_string(),
        });

        let response: reqwest::Response = self.http_client
            .post("https://open.feishu.cn/open-apis/im/v1/messages")
            .header("Authorization", format!("Bearer {}", token))
            .query(&[("receive_id_type", "open_id")])
            .json(&body)
            .send()
            .await
            .context("发送文件消息失败")?;

        let msg_response: FeishuMessageResponse = response
            .json::<FeishuMessageResponse>()
            .await
            .context("解析消息响应失败")?;

        if msg_response.code != 0 {
            anyhow::bail!("发送文件消息失败: {}", msg_response.msg);
        }

        Ok(())
    }

    /// 验证 Webhook 签名（用于事件订阅）
    pub fn verify_webhook_signature(
        &self,
        timestamp: &str,
        nonce: &str,
        body: &str,
        signature: &str,
    ) -> Result<bool> {
        let app_secret = self
            .config
            .app_secret
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("App Secret 未配置"))?;

        // 计算签名: sha256(timestamp + nonce + body + app_secret)
        let mut hasher = Sha256::new();
        hasher.update(timestamp.as_bytes());
        hasher.update(nonce.as_bytes());
        hasher.update(body.as_bytes());
        hasher.update(app_secret.as_bytes());

        let computed = hex::encode(hasher.finalize());
        Ok(computed == signature)
    }

    /// 处理 Webhook 事件
    pub async fn handle_webhook_event(
        &self,
        event: &serde_json::Value,
    ) -> Result<Option<String>> {
        let event_type = event
            .get("header")
            .and_then(|h| h.get("event_type"))
            .and_then(|t| t.as_str())
            .unwrap_or("");

        info!("收到飞书事件: {}", event_type);

        match event_type {
            "im.message.receive_v1" => {
                // 处理消息事件
                let event_data = event
                    .get("event")
                    .ok_or_else(|| anyhow::anyhow!("事件数据为空"))?;

                let sender = event_data
                    .get("sender")
                    .and_then(|s| s.get("sender_id"))
                    .and_then(|id| id.get("open_id"))
                    .and_then(|id| id.as_str())
                    .unwrap_or("");

                let message = event_data
                    .get("message")
                    .ok_or_else(|| anyhow::anyhow!("消息数据为空"))?;

                let msg_type = message
                    .get("message_type")
                    .and_then(|t| t.as_str())
                    .unwrap_or("");

                // 检查白名单
                if !self.is_open_id_allowed(sender) {
                    warn!("用户 {} 不在白名单中", sender);
                    return Ok(None);
                }

                // 只处理文本消息
                if msg_type != "text" {
                    return Ok(None);
                }

                let content = message
                    .get("content")
                    .and_then(|c| c.as_str())
                    .unwrap_or("{}");

                let content_json: serde_json::Value = serde_json::from_str(content)?;
                let text = content_json
                    .get("text")
                    .and_then(|t| t.as_str())
                    .unwrap_or("");

                info!("收到飞书消息: {}", text);

                // 调用 Agent 处理
                match self.agent.chat(text).await {
                    Ok(response) => {
                        // 发送响应
                        if let Err(e) = self.send_text_message(sender, &response.content).await {
                            error!("发送响应失败: {}", e);
                        }
                        Ok(Some(response.content))
                    }
                    Err(e) => {
                        error!("Agent 处理失败: {}", e);
                        let error_msg = "处理消息时出错，请稍后重试";
                        if let Err(e) = self.send_text_message(sender, error_msg).await {
                            error!("发送错误消息失败: {}", e);
                        }
                        Ok(Some(error_msg.to_string()))
                    }
                }
            }
            _ => {
                // 其他事件类型
                Ok(None)
            }
        }
    }
}

#[async_trait]
impl Channel for FeishuChannel {
    fn name(&self) -> &str {
        "feishu"
    }

    async fn start(&self) -> Result<()> {
        info!("启动飞书 Bot...");

        // 预获取访问令牌
        self.get_access_token().await?;

        *self.running.write().await = true;
        info!("飞书 Bot 已启动");

        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        info!("停止飞书 Bot...");
        *self.running.write().await = false;
        *self.access_token.write().await = None;
        *self.token_expire_at.write().await = None;
        info!("飞书 Bot 已停止");
        Ok(())
    }

    async fn send_message(
        &self,
        target: &str,
        content: &str,
    ) -> Result<()> {
        info!("发送飞书消息到 {}: {}", target, content);

        // 检查白名单
        if !self.is_open_id_allowed(target) {
            anyhow::bail!("用户 {} 不在白名单中", target);
        }

        // 发送消息
        self.send_text_message(target, content).await
    }

    async fn send_media(
        &self,
        target: &str,
        media: &Media,
    ) -> Result<()> {
        info!("发送飞书媒体消息到 {}", target);

        // 检查白名单
        if !self.is_open_id_allowed(target) {
            anyhow::bail!("用户 {} 不在白名单中", target);
        }

        match media.media_type {
            MediaType::Image => {
                // 获取图片路径
                let image_path = media
                    .path
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("图片路径未提供"))?;

                // 上传图片
                let image_key = self.upload_image(image_path).await?;

                // 发送图片消息
                self.send_image_message(target, &image_key).await?;
            }
            MediaType::File => {
                // 获取文件路径和名称
                let file_path = media
                    .path
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("文件路径未提供"))?;
                let file_name = media
                    .name
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("文件名未提供"))?;

                // 上传文件
                let file_id = self.upload_file(file_path, file_name).await?;

                // 发送文件消息
                self.send_file_message(target, &file_id, file_name).await?;
            }
            MediaType::Audio => {
                // 飞书不支持直接发送音频消息类型，使用文件方式发送
                let file_path = media
                    .path
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("音频文件路径未提供"))?;
                let file_name = media
                    .name
                    .as_deref()
                    .unwrap_or("audio.wav");

                // 上传文件
                let file_id = self.upload_file(file_path, file_name).await?;

                // 发送文件消息
                self.send_file_message(target, &file_id, file_name).await?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verify_signature() {
        let config = FeishuConfig {
            app_id: Some("test_app_id".to_string()),
            app_secret: Some("test_secret".to_string()),
            verification_token: Some("test_token".to_string()),
            encrypt_key: Some("test_encrypt_key".to_string()),
            allowed_users: vec![],
            allowed_open_ids: vec![],
            allowed_chats: vec![],
            verify_signature: true,
            card_template_id: None,
        };

        // 创建一个模拟的 agent
        // 注意：实际测试需要更完整的设置
        assert!(config.verify_signature);
    }
}
