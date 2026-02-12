//! Anthropic Provider
//!
//! 支持 Anthropic Claude 模型

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

use super::{ChatRequest, ChatResponse, LlmProvider, Message, Role, Tool, ToolCall};

/// Anthropic API 响应
#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    id: String,
    #[serde(rename = "type")]
    response_type: String,
    role: String,
    content: Vec<AnthropicContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    usage: Option<AnthropicUsage>,
}

#[derive(Debug, Deserialize)]
struct AnthropicContent {
    #[serde(rename = "type")]
    content_type: String,
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnthropicUsage {
    #[serde(rename = "input_tokens")]
    input_tokens: u32,
    #[serde(rename = "output_tokens")]
    output_tokens: u32,
}

/// Anthropic Provider 实现
pub struct AnthropicProvider {
    api_key: String,
    base_url: String,
    timeout_secs: u64,
}

impl AnthropicProvider {
    pub fn new(api_key: String, base_url: Option<String>, timeout_secs: Option<u64>) -> Self {
        Self {
            api_key,
            base_url: base_url.unwrap_or_else(|| "https://api.anthropic.com/v1".to_string()),
            timeout_secs: timeout_secs.unwrap_or(60),
        }
    }

    fn build_api_url(&self, model: &str) -> String {
        // Anthropic 使用 /messages API
        format!("{}/messages", self.base_url.trim_end_matches("/"))
    }
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    fn name(&self) -> &str {
        "anthropic"
    }

    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(self.timeout_secs))
            .build()?;

        // 构建消息
        let messages: Vec<_> = request
            .messages
            .iter()
            .map(|m| {
                json!({
                    "role": match m.role {
                        Role::System => "assistant",
                        Role::User => "user",
                        Role::Assistant => "assistant",
                        Role::Tool => "user",
                    },
                    "content": m.content
                })
            })
            .collect();

        // 构建请求体
        let mut body = json!({
            "model": request.model,
            "messages": messages,
            "max_tokens": request.max_tokens.unwrap_or(4096),
        });

        // 添加 temperature
        if let Some(temp) = request.temperature {
            body["temperature"] = temp;
        }

        // 添加工具（如果需要）
        if let Some(tools) = &request.tools {
            body["tools"] = json!(tools);
        }

        let response = client
            .post(self.build_api_url(&request.model))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow!("Anthropic API 错误: {}", error_text));
        }

        let response_data: AnthropicResponse = response.json().await?;

        // 解析响应内容
        let content = response_data
            .content
            .first()
            .ok_or_else(|| anyhow!("Empty response from Anthropic"))?;

        let message = match &content.content_type {
            "text" => Message::assistant(content.text.as_ref().unwrap_or(&String::new())),
            "tool_use" => {
                // 处理工具调用
                let tool_calls = vec![ToolCall {
                    id: response_data.id.clone(),
                    call_type: "function".to_string(),
                    function: super::FunctionCall {
                        name: content.text.as_ref()
                            .and_then(|t| serde_json::from_str::<serde_json::Value>(t).ok())
                            .and_then(|v| v.get("name").map(|n| n.as_str().unwrap_or("").to_string()))
                            .unwrap_or_default(),
                        arguments: content.text.as_ref()
                            .and_then(|t| serde_json::from_str::<serde_json::Value>(t).ok())
                            .and_then(|v| v.get("input").map(|i| i.to_string()))
                            .unwrap_or_default(),
                    },
                }];
                Message::assistant("").with_tool_calls(tool_calls)
            }
            _ => Message::assistant(content.text.as_ref().unwrap_or(&String::new())),
        };

        Ok(ChatResponse {
            message,
            usage: response_data.usage.map(|u| super::Usage {
                prompt_tokens: u.input_tokens,
                completion_tokens: u.output_tokens,
                total_tokens: u.input_tokens + u.output_tokens,
            }),
            model: request.model,
        })
    }

    fn is_available(&self) -> bool {
        !self.api_key.is_empty()
    }
}
