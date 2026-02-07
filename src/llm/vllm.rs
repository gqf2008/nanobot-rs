//! vLLM 提供商实现
//! 
//! vLLM 是一个高吞吐量的 LLM 推理引擎，提供 OpenAI 兼容的 API
//! 支持本地部署和自定义端点
//! 文档: https://docs.vllm.ai/

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{ChatRequest, ChatResponse, LlmProvider, Message, Role, ToolCall, Usage};

pub struct VllmProvider {
    api_key: String,
    base_url: String,
    client: Client,
    default_model: String,
}

impl VllmProvider {
    pub fn new(
        api_key: String, 
        base_url: Option<String>, 
        timeout_secs: u64,
        default_model: Option<String>
    ) -> Self {
        let base_url = base_url.unwrap_or_else(|| "http://localhost:8000/v1".to_string());
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .build()
            .expect("创建 HTTP 客户端失败");

        Self {
            api_key,
            base_url,
            client,
            default_model: default_model.unwrap_or_else(|| "default".to_string()),
        }
    }

    /// 获取默认模型名称
    pub fn default_model(&self) -> &str {
        &self.default_model
    }

    /// 列出可用模型
    pub async fn list_models(&self) -> Result<Vec<String>> {
        let url = format!("{}/models", self.base_url);
        
        let response = self.client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(anyhow!("vLLM API 错误: {} - {}", status, text));
        }

        let models: VllmModelsResponse = response.json().await?;
        Ok(models.data.into_iter().map(|m| m.id).collect())
    }
}

#[async_trait]
impl LlmProvider for VllmProvider {
    fn name(&self) -> &str {
        "vllm"
    }

    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse> {
        let url = format!("{}/chat/completions", self.base_url);

        // 如果请求中没有指定模型，使用默认模型
        let mut body = VllmRequest::from(request);
        if body.model.is_empty() || body.model == "default" {
            body.model = self.default_model.clone();
        }

        let mut request_builder = self.client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body);

        // 如果提供了 API Key，添加到请求头
        if !self.api_key.is_empty() {
            request_builder = request_builder.header("Authorization", format!("Bearer {}", self.api_key));
        }

        let response = request_builder.send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(anyhow!("vLLM API 错误: {} - {}", status, text));
        }

        let completion: VllmResponse = response.json().await?;
        
        if completion.choices.is_empty() {
            return Err(anyhow!("vLLM 返回空响应"));
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
        // vLLM 通常不需要 API Key（本地部署）
        // 但我们需要 base_url 来连接
        !self.base_url.is_empty()
    }
}

// vLLM API 请求结构（OpenAI 兼容格式）
#[derive(Debug, Serialize)]
struct VllmRequest {
    model: String,
    messages: Vec<VllmMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<VllmTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    presence_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    frequency_penalty: Option<f32>,
}

#[derive(Debug, Serialize)]
struct VllmMessage {
    role: String,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct VllmTool {
    #[serde(rename = "type")]
    tool_type: String,
    function: VllmFunction,
}

#[derive(Debug, Serialize)]
struct VllmFunction {
    name: String,
    description: String,
    parameters: Value,
}

// vLLM API 响应结构
#[derive(Debug, Deserialize)]
struct VllmResponse {
    id: String,
    model: String,
    choices: Vec<VllmChoice>,
    usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
struct VllmChoice {
    index: u32,
    message: VllmResponseMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct VllmResponseMessage {
    role: String,
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<ToolCall>>,
}

// 模型列表响应
#[derive(Debug, Deserialize)]
struct VllmModelsResponse {
    object: String,
    data: Vec<VllmModel>,
}

#[derive(Debug, Deserialize)]
struct VllmModel {
    id: String,
    object: String,
}

impl From<ChatRequest> for VllmRequest {
    fn from(req: ChatRequest) -> Self {
        Self {
            model: req.model,
            messages: req.messages.into_iter().map(|m| VllmMessage {
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
            tools: req.tools.map(|tools| tools.into_iter().map(|t| VllmTool {
                tool_type: "function".to_string(),
                function: VllmFunction {
                    name: t.name,
                    description: t.description,
                    parameters: t.parameters,
                },
            }).collect()),
            temperature: req.temperature,
            max_tokens: req.max_tokens,
            top_p: None,
            presence_penalty: None,
            frequency_penalty: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vllm_provider_creation() {
        let provider = VllmProvider::new(
            "".to_string(),
            Some("http://localhost:8000/v1".to_string()),
            60,
            Some("llama-2-7b".to_string())
        );

        assert_eq!(provider.name(), "vllm");
        assert_eq!(provider.default_model(), "llama-2-7b");
        assert!(provider.is_available());
    }

    #[test]
    fn test_vllm_provider_default_model() {
        let provider = VllmProvider::new(
            "test-key".to_string(),
            None,
            60,
            None
        );

        assert_eq!(provider.default_model(), "default");
        assert_eq!(provider.name(), "vllm");
    }
}
