//! 智谱 AI (Zhipu) 提供商实现
//! 
//! 智谱 AI 提供 GLM 系列大语言模型服务
//! API 文档: https://open.bigmodel.cn/dev/api

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{ChatRequest, ChatResponse, LlmProvider, Message, Role, ToolCall, Usage};

pub struct ZhipuProvider {
    api_key: String,
    base_url: String,
    client: Client,
}

impl ZhipuProvider {
    pub fn new(api_key: String, base_url: Option<String>, timeout_secs: u64) -> Self {
        let base_url = base_url.unwrap_or_else(|| "https://open.bigmodel.cn/api/paas/v4".to_string());
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
        "glm-4"
    }
}

#[async_trait]
impl LlmProvider for ZhipuProvider {
    fn name(&self) -> &str {
        "zhipu"
    }

    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse> {
        let url = format!("{}/chat/completions", self.base_url);

        let body = ZhipuRequest::from(request);

        let response = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(anyhow!("智谱 AI API 错误: {} - {}", status, text));
        }

        let completion: ZhipuResponse = response.json().await?;
        
        if completion.choices.is_empty() {
            return Err(anyhow!("智谱 AI 返回空响应"));
        }

        let choice = &completion.choices[0];
        let message = Message {
            role: match choice.message.role.as_str() {
                "system" => Role::System,
                "assistant" => Role::Assistant,
                "tool" => Role::Tool,
                _ => Role::User,
            },
            content: choice.message.content.clone().unwrap_or_default(),
            tool_calls: choice.message.tool_calls.clone(),
            tool_call_id: None,
        };

        Ok(ChatResponse {
            message,
            usage: completion.usage,
            model: completion.model,
        })
    }

    fn is_available(&self) -> bool {
        !self.api_key.is_empty()
    }
}

// 智谱 AI API 请求结构
#[derive(Debug, Serialize)]
struct ZhipuRequest {
    model: String,
    messages: Vec<ZhipuMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<ZhipuTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
}

#[derive(Debug, Serialize)]
struct ZhipuMessage {
    role: String,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct ZhipuTool {
    #[serde(rename = "type")]
    tool_type: String,
    function: ZhipuFunction,
}

#[derive(Debug, Serialize)]
struct ZhipuFunction {
    name: String,
    description: String,
    parameters: Value,
}

// 智谱 AI API 响应结构
#[derive(Debug, Deserialize)]
struct ZhipuResponse {
    id: String,
    model: String,
    choices: Vec<ZhipuChoice>,
    usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
struct ZhipuChoice {
    index: u32,
    message: ZhipuResponseMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ZhipuResponseMessage {
    role: String,
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<ToolCall>>,
}

impl From<ChatRequest> for ZhipuRequest {
    fn from(req: ChatRequest) -> Self {
        Self {
            model: req.model,
            messages: req.messages.into_iter().map(|m| ZhipuMessage {
                role: match m.role {
                    Role::System => "system".to_string(),
                    Role::User => "user".to_string(),
                    Role::Assistant => "assistant".to_string(),
                    Role::Tool => "tool".to_string(),
                },
                content: m.content,
                tool_calls: m.tool_calls,
                tool_call_id: m.tool_call_id,
            }).collect(),
            tools: req.tools.map(|tools| tools.into_iter().map(|t| ZhipuTool {
                tool_type: "function".to_string(),
                function: ZhipuFunction {
                    name: t.name,
                    description: t.description,
                    parameters: t.parameters,
                },
            }).collect()),
            temperature: req.temperature,
            max_tokens: req.max_tokens,
        }
    }
}
