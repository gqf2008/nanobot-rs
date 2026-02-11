//! MiniMax LLM 提供商实现
//!
//! 使用 OpenAI 兼容 API: https://api.minimax.io/v1

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;

use super::{ChatRequest, ChatResponse, LlmProvider, Message, Role};

/// MiniMax 提供商配置
#[derive(Debug, Clone)]
pub struct MiniMaxConfig {
    pub api_key: String,
    pub base_url: Option<String>,
    pub timeout_secs: Option<u64>,
}

impl Default for MiniMaxConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            base_url: Some("https://api.minimax.io/v1".to_string()),
            timeout_secs: Some(60),
        }
    }
}

/// MiniMax LLM 提供商
#[derive(Debug)]
pub struct MiniMaxProvider {
    client: Client,
    base_url: String,
    model: String,
    api_key: String,
}

impl MiniMaxProvider {
    /// 创建新的 MiniMax 提供商
    pub fn new(api_key: impl Into<String>, base_url: Option<String>, timeout_secs: Option<u64>) -> Self {
        let api_key = api_key.into();
        let base_url = base_url.unwrap_or_else(|| "https://api.minimax.io/v1".to_string());
        let timeout = std::time::Duration::from_secs(timeout_secs.unwrap_or(60));

        let client = Client::builder()
            .timeout(timeout)
            .default_headers({
                let mut headers = reqwest::header::HeaderMap::new();
                let auth_value = reqwest::header::HeaderValue::from_str(&format!("Bearer {}", api_key))
                    .expect("Invalid API key");
                headers.insert(reqwest::header::AUTHORIZATION, auth_value);
                headers
            })
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            base_url,
            model: "MiniMax-M2.1".to_string(), // 默认模型
            api_key,
        }
    }

    /// 设置模型
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }
}

#[async_trait]
impl LlmProvider for MiniMaxProvider {
    fn name(&self) -> &str {
        "minimax"
    }

    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse> {
        let url = format!("{}/chat/completions", self.base_url);

        // 转换消息格式
        let messages: Vec<_> = request.messages.iter().map(|m| json!({
            "role": match m.role {
                Role::System => "system",
                Role::User => "user",
                Role::Assistant => "assistant",
                Role::Tool => "tool",
            },
            "content": m.content
        })).collect();

        // 构建请求体
        let mut body = json!({
            "model": if request.model.starts_with("minimax/") {
                request.model.strip_prefix("minimax/").unwrap_or(&request.model).to_string()
            } else {
                request.model
            },
            "messages": messages,
        });

        // 添加可选参数
        if let Some(temp) = request.temperature {
            body["temperature"] = json!(temp);
        }
        if let Some(max_tokens) = request.max_tokens {
            body["max_tokens"] = json!(max_tokens);
        }
        if let Some(tools) = &request.tools {
            body["tools"] = json!(tools);
        }

        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| anyhow!("MiniMax API 请求失败: {}", e))?;

        // 处理错误响应
        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            tracing::error!("MiniMax API 错误: {}", error_text);
            return Err(anyhow!("MiniMax API 错误: {}", error_text));
        }

        let response_json: MiniMaxResponse = response
            .json()
            .await
            .map_err(|e| anyhow!("解析 MiniMax 响应失败: {}", e))?;

        // 提取消息内容
        let content = response_json.choices
            .first()
            .and_then(|c| c.message.content.clone())
            .unwrap_or_default();

        // 转换工具调用
        let tool_calls = response_json.choices
            .first()
            .and_then(|c| {
                if !c.message.tool_calls.is_empty() {
                    Some(c.message.tool_calls.clone())
                } else {
                    None
                }
            })
            .map(|calls| calls.into_iter().map(|tc| super::ToolCall {
                id: tc.id,
                call_type: "function".to_string(),
                function: super::FunctionCall {
                    name: tc.function.name,
                    arguments: tc.function.arguments,
                }
            }).collect());

        let message = Message {
            role: Role::Assistant,
            content,
            tool_calls,
            tool_call_id: None,
        };

        let usage = response_json.usage.map(|u| super::Usage {
            prompt_tokens: u.prompt_tokens as u32,
            completion_tokens: u.completion_tokens as u32,
            total_tokens: u.total_tokens as u32,
        });

        Ok(ChatResponse {
            message,
            usage,
            model: response_json.model,
        })
    }

    fn is_available(&self) -> bool {
        !self.api_key.is_empty()
    }
}

/// MiniMax API 响应
#[derive(Debug, Deserialize)]
struct MiniMaxResponse {
    id: String,
    object: String,
    created: u64,
    model: String,
    choices: Vec<MiniMaxChoice>,
    usage: Option<MiniMaxUsage>,
}

#[derive(Debug, Deserialize)]
struct MiniMaxChoice {
    index: u32,
    message: MiniMaxMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
struct MiniMaxMessage {
    role: String,
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<MiniMaxToolCall>,
}

#[derive(Debug, Deserialize, Clone)]
struct MiniMaxToolCall {
    id: String,
    #[serde(rename = "type")]
    call_type: String,
    function: MiniMaxFunctionCall,
}

#[derive(Debug, Deserialize, Clone)]
struct MiniMaxFunctionCall {
    name: String,
    arguments: String,
}

#[derive(Debug, Deserialize)]
struct MiniMaxUsage {
    prompt_tokens: u64,
    completion_tokens: u64,
    total_tokens: u64,
}
