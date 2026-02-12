//! Google Gemini Provider
//!
//! 支持 Google Gemini 模型

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

use super::{ChatRequest, ChatResponse, LlmProvider, Message, Role};

/// Gemini API 响应
#[derive(Debug, Deserialize)]
struct GeminiResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    candidates: Option<Vec<GeminiCandidate>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    usageMetadata: Option<GeminiUsage>,
}

#[derive(Debug, Deserialize)]
struct GeminiCandidate {
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    finishReason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GeminiContent {
    parts: Vec<GeminiPart>,
    role: String,
}

#[derive(Debug, Deserialize)]
struct GeminiPart {
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    functionCall: Option<GeminiFunctionCall>,
}

#[derive(Debug, Deserialize)]
struct GeminiFunctionCall {
    name: String,
    args: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct GeminiUsage {
    promptTokenCount: u32,
    candidatesTokenCount: u32,
    totalTokenCount: u32,
}

/// Gemini Provider 实现
pub struct GeminiProvider {
    api_key: String,
    base_url: String,
    timeout_secs: u64,
}

impl GeminiProvider {
    pub fn new(api_key: String, base_url: Option<String>, timeout_secs: Option<u64>) -> Self {
        Self {
            api_key,
            base_url: base_url.unwrap_or_else(|| {
                "https://generativelanguage.googleapis.com/v1beta/models".to_string()
            }),
            timeout_secs: timeout_secs.unwrap_or(60),
        }
    }

    fn build_api_url(&self, model: &str) -> String {
        format!("{}/{}:generateContent", self.base_url.trim_end_matches("/"), model)
    }
}

#[async_trait]
impl LlmProvider for GeminiProvider {
    fn name(&self) -> &str {
        "gemini"
    }

    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(self.timeout_secs))
            .build()?;

        // 构建内容
        let contents: Vec<_> = request
            .messages
            .iter()
            .filter(|m| m.role != Role::System) // Gemini 处理系统提示的方式不同
            .map(|m| {
                let parts = if m.content.is_empty() && m.tool_calls.is_some() {
                    // 工具调用
                    vec![GeminiPart {
                        text: None,
                        functionCall: Some(GeminiFunctionCall {
                            name: m.tool_calls.as_ref().unwrap().first()
                                .map(|tc| tc.function.name.clone())
                                .unwrap_or_default(),
                            args: m.tool_calls.as_ref().unwrap().first()
                                .map(|tc| serde_json::from_str(&tc.function.arguments).unwrap_or_default())
                                .unwrap_or_default(),
                        }),
                    }]
                } else {
                    vec![GeminiPart {
                        text: Some(m.content.clone()),
                        functionCall: None,
                    }]
                };
                json!({
                    "role": match m.role {
                        Role::User => "user",
                        Role::Assistant => "model",
                        Role::Tool => "user",
                        Role::System => "user",
                    },
                    "parts": parts
                })
            })
            .collect();

        // 构建请求体
        let mut body = json!({
            "contents": contents,
        });

        // 添加 generationConfig
        let mut config = json!({});
        if let Some(temp) = request.temperature {
            config["temperature"] = temp;
        }
        if let Some(max_tokens) = request.max_tokens {
            config["maxOutputTokens"] = max_tokens;
        }
        body["generationConfig"] = config;

        let response = client
            .post(self.build_api_url(&request.model))
            .query(&[("key", &self.api_key)])
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow!("Gemini API 错误: {}", error_text));
        }

        let response_data: GeminiResponse = response.json().await?;

        let content = response_data
            .candidates
            .and_then(|c| c.into_iter().next())
            .and_then(|c| c.content)
            .and_then(|mut content| content.parts.pop())
            .and_then(|part| part.text)
            .unwrap_or_default();

        Ok(ChatResponse {
            message: Message::assistant(content),
            usage: response_data.usageMetadata.map(|u| super::Usage {
                prompt_tokens: u.promptTokenCount,
                completion_tokens: u.candidatesTokenCount,
                total_tokens: u.totalTokenCount,
            }),
            model: request.model,
        })
    }

    fn is_available(&self) -> bool {
        !self.api_key.is_empty()
    }
}
