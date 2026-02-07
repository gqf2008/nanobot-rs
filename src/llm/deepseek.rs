//! DeepSeek 提供商实现
//! 
//! DeepSeek 提供高性价比的中文 LLM 服务

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{ChatRequest, ChatResponse, LlmProvider, Message, Role, ToolCall, Usage};

pub struct DeepSeekProvider {
    api_key: String,
    base_url: String,
    client: Client,
}

impl DeepSeekProvider {
    pub fn new(api_key: String, base_url: Option<String>, timeout_secs: u64) -> Self {
        let base_url = base_url.unwrap_or_else(|| "https://api.deepseek.com".to_string());
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
}

#[async_trait]
impl LlmProvider for DeepSeekProvider {
    fn name(&self) -> &str {
        "deepseek"
    }

    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse> {
        let url = format!("{}/chat/completions", self.base_url);

        let body = DeepSeekRequest::from(request);

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
            return Err(anyhow!("DeepSeek API 错误: {} - {}", status, text));
        }

        let completion: DeepSeekResponse = response.json().await?;
        
        if completion.choices.is_empty() {
            return Err(anyhow!("DeepSeek 返回空响应"));
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

// DeepSeek API 请求结构
#[derive(Debug, Serialize)]
struct DeepSeekRequest {
    model: String,
    messages: Vec<DeepSeekMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<DeepSeekTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
}

#[derive(Debug, Serialize)]
struct DeepSeekMessage {
    role: String,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct DeepSeekTool {
    #[serde(rename = "type")]
    tool_type: String,
    function: DeepSeekFunction,
}

#[derive(Debug, Serialize)]
struct DeepSeekFunction {
    name: String,
    description: String,
    parameters: Value,
}

// DeepSeek API 响应结构
#[derive(Debug, Deserialize)]
struct DeepSeekResponse {
    id: String,
    model: String,
    choices: Vec<DeepSeekChoice>,
    usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
struct DeepSeekChoice {
    index: u32,
    message: DeepSeekResponseMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DeepSeekResponseMessage {
    role: String,
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<ToolCall>>,
}

impl From<ChatRequest> for DeepSeekRequest {
    fn from(req: ChatRequest) -> Self {
        Self {
            model: req.model,
            messages: req.messages.into_iter().map(|m| DeepSeekMessage {
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
            tools: req.tools.map(|tools| tools.into_iter().map(|t| DeepSeekTool {
                tool_type: "function".to_string(),
                function: DeepSeekFunction {
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
