//! Moonshot/Kimi 提供商实现
//! 
//! Moonshot AI 提供 Kimi 系列大语言模型服务
//! API 文档: https://platform.moonshot.cn/docs

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{ChatRequest, ChatResponse, LlmProvider, Message, Role, ToolCall, Usage};

pub struct MoonshotProvider {
    api_key: String,
    base_url: String,
    client: Client,
}

impl MoonshotProvider {
    pub fn new(api_key: String, base_url: Option<String>, timeout_secs: u64) -> Self {
        let base_url = base_url.unwrap_or_else(|| "https://api.moonshot.cn/v1".to_string());
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
        "moonshot-v1-8k"
    }

    /// 检查是否需要固定 temperature（如 kimi-k2.5 只支持 temperature=1.0）
    fn adjust_temperature(&self, model: &str, temperature: Option<f32>) -> Option<f32> {
        if model.to_lowercase().contains("kimi-k2.5") {
            Some(1.0)
        } else {
            temperature
        }
    }
}

#[async_trait]
impl LlmProvider for MoonshotProvider {
    fn name(&self) -> &str {
        "moonshot"
    }

    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse> {
        let url = format!("{}/chat/completions", self.base_url);

        let mut body = MoonshotRequest::from(request.clone());
        
        // 调整 temperature（某些模型有特殊要求）
        body.temperature = self.adjust_temperature(&body.model, request.temperature);

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
            return Err(anyhow!("Moonshot API 错误: {} - {}", status, text));
        }

        let completion: MoonshotResponse = response.json().await?;
        
        if completion.choices.is_empty() {
            return Err(anyhow!("Moonshot 返回空响应"));
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

// Moonshot API 请求结构
#[derive(Debug, Serialize)]
struct MoonshotRequest {
    model: String,
    messages: Vec<MoonshotMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<MoonshotTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
}

#[derive(Debug, Serialize)]
struct MoonshotMessage {
    role: String,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct MoonshotTool {
    #[serde(rename = "type")]
    tool_type: String,
    function: MoonshotFunction,
}

#[derive(Debug, Serialize)]
struct MoonshotFunction {
    name: String,
    description: String,
    parameters: Value,
}

// Moonshot API 响应结构
#[derive(Debug, Deserialize)]
struct MoonshotResponse {
    id: String,
    model: String,
    choices: Vec<MoonshotChoice>,
    usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
struct MoonshotChoice {
    index: u32,
    message: MoonshotResponseMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MoonshotResponseMessage {
    role: String,
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<ToolCall>>,
}

impl From<ChatRequest> for MoonshotRequest {
    fn from(req: ChatRequest) -> Self {
        Self {
            model: req.model,
            messages: req.messages.into_iter().map(|m| MoonshotMessage {
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
            tools: req.tools.map(|tools| tools.into_iter().map(|t| MoonshotTool {
                tool_type: "function".to_string(),
                function: MoonshotFunction {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adjust_temperature() {
        let provider = MoonshotProvider::new(
            "test-key".to_string(),
            None,
            60
        );

        // kimi-k2.5 应该返回 1.0
        assert_eq!(provider.adjust_temperature("kimi-k2.5", Some(0.7)), Some(1.0));
        assert_eq!(provider.adjust_temperature("moonshot-kimi-k2.5", Some(0.7)), Some(1.0));
        
        // 其他模型保持原值
        assert_eq!(provider.adjust_temperature("moonshot-v1-8k", Some(0.7)), Some(0.7));
        assert_eq!(provider.adjust_temperature("moonshot-v1-32k", None), None);
    }
}
