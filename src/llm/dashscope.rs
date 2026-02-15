//! 阿里云 DashScope (Qwen) 提供商实现
//! 
//! 阿里云提供 Qwen 系列大语言模型服务
//! API 文档: https://help.aliyun.com/zh/model-studio/developer-reference/compatibility-api-overview

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{ChatRequest, ChatResponse, LlmProvider, Message, Role, ToolCall, Usage};

pub struct DashScopeProvider {
    api_key: String,
    base_url: String,
    client: Client,
}

impl DashScopeProvider {
    pub fn new(api_key: String, base_url: Option<String>, timeout_secs: u64) -> Self {
        let base_url = base_url.unwrap_or_else(|| "https://dashscope.aliyuncs.com/api/v1".to_string());
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .build()
            .expect("创建 HTTP 客户端失败");

        Self {
            api_key,
            base_url,
            client,
        }
    }

    /// 获取默认模型
    pub fn default_model() -> &'static str {
        "qwen-turbo"
    }
}

#[async_trait]
impl LlmProvider for DashScopeProvider {
    fn name(&self) -> &str {
        "dashscope"
    }

    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse> {
        let url = format!("{}/services/aigc/text-generation/generation", self.base_url);

        let body = DashScopeRequest::from(request);

        let response = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .header("X-DashScope-Async", "disable")
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(anyhow!("DashScope API 错误: {} - {}", status, text));
        }

        let completion: DashScopeResponse = response.json().await?;
        
        if completion.output.choices.is_empty() {
            return Err(anyhow!("DashScope 返回空响应"));
        }

        let choice = &completion.output.choices[0];
        let message = Message {
            role: match choice.message.role.as_str() {
                "system" => Role::System,
                "assistant" => Role::Assistant,
                "tool" => Role::Tool,
                _ => Role::User,
            },
            content: choice.message.content.clone().unwrap_or_default(),
            tool_calls: None,
            tool_call_id: None,
        };

        Ok(ChatResponse {
            message,
            usage: completion.usage,
            model: completion.output.model,
        })
    }

    fn is_available(&self) -> bool {
        !self.api_key.is_empty()
    }
}

// DashScope API 请求结构
#[derive(Debug, Serialize)]
struct DashScopeRequest {
    model: String,
    input: DashScopeInput,
    parameters: DashScopeParameters,
}

#[derive(Debug, Serialize)]
struct DashScopeInput {
    messages: Vec<DashScopeMessage>,
}

#[derive(Debug, Serialize)]
struct DashScopeMessage {
    role: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct DashScopeParameters {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result_format: Option<String>,
}

// DashScope API 响应结构
#[derive(Debug, Deserialize)]
struct DashScopeResponse {
    output: DashScopeOutput,
    usage: Option<Usage>,
    request_id: String,
}

#[derive(Debug, Deserialize)]
struct DashScopeOutput {
    choices: Vec<DashScopeChoice>,
    model: String,
}

#[derive(Debug, Deserialize)]
struct DashScopeChoice {
    index: u32,
    message: DashScopeResponseMessage,
    finish_reason: String,
}

#[derive(Debug, Deserialize)]
struct DashScopeResponseMessage {
    role: String,
    content: Option<String>,
}

impl From<ChatRequest> for DashScopeRequest {
    fn from(req: ChatRequest) -> Self {
        Self {
            model: req.model,
            input: DashScopeInput {
                messages: req.messages.into_iter().map(|m| DashScopeMessage {
                    role: match m.role {
                        Role::System => "system".to_string(),
                        Role::User => "user".to_string(),
                        Role::Assistant => "assistant".to_string(),
                        Role::Tool => "tool".to_string(),
                    },
                    content: m.content,
                }).collect(),
            },
            parameters: DashScopeParameters {
                temperature: req.temperature,
                max_tokens: req.max_tokens,
                result_format: Some("message".to_string()),
            },
        }
    }
}
