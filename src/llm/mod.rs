//! LLM 提供商模块
//!
//! 支持多个 LLM 提供商：OpenRouter、DeepSeek、Moonshot/Kimi、MiniMax、vLLM、OpenAI、Anthropic

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

pub mod deepseek;
pub mod minimax;
pub mod moonshot;
pub mod openrouter;
pub mod vllm;

/// 消息角色
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

/// 聊天消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: content.into(),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    pub fn with_tool_calls(mut self, calls: Vec<ToolCall>) -> Self {
        self.tool_calls = Some(calls);
        self
    }

    pub fn tool_result(id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: content.into(),
            tool_calls: None,
            tool_call_id: Some(id.into()),
        }
    }
}

/// 工具定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

/// 工具调用
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: FunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

/// LLM 请求
#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub tools: Option<Vec<Tool>>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
}

impl ChatRequest {
    pub fn new(model: impl Into<String>, messages: Vec<Message>) -> Self {
        Self {
            model: model.into(),
            messages,
            tools: None,
            temperature: Some(0.7),
            max_tokens: None,
        }
    }

    pub fn with_tools(mut self, tools: Vec<Tool>) -> Self {
        self.tools = Some(tools);
        self
    }

    pub fn with_temperature(mut self, temp: f32) -> Self {
        self.temperature = Some(temp);
        self
    }
}

/// LLM 响应
#[derive(Debug, Clone)]
pub struct ChatResponse {
    pub message: Message,
    pub usage: Option<Usage>,
    pub model: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// LLM 提供商 trait
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// 获取提供商名称
    fn name(&self) -> &str;
    
    /// 发送聊天请求
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse>;
    
    /// 检查是否可用
    fn is_available(&self) -> bool;
}

/// LLM 提供商工厂
pub struct LlmProviderFactory;

impl LlmProviderFactory {
    /// 创建提供商实例
    pub fn create(
        name: &str, config: &crate::config::ProviderConfig) -> Result<Arc<dyn LlmProvider>> {
        match name {
            "openrouter" => {
                let api_key = config.api_key.as_ref()
                    .ok_or_else(|| anyhow!("OpenRouter 需要 API Key"))?;
                let provider = openrouter::OpenRouterProvider::new(
                    api_key.clone(),
                    config.base_url.clone(),
                    config.timeout_secs,
                );
                Ok(Arc::new(provider))
            }
            "deepseek" => {
                let api_key = config.api_key.as_ref()
                    .ok_or_else(|| anyhow!("DeepSeek 需要 API Key"))?;
                let provider = deepseek::DeepSeekProvider::new(
                    api_key.clone(),
                    config.base_url.clone(),
                    config.timeout_secs,
                );
                Ok(Arc::new(provider))
            }
            "moonshot" => {
                let api_key = config.api_key.as_ref()
                    .ok_or_else(|| anyhow!("Moonshot 需要 API Key"))?;
                let provider = moonshot::MoonshotProvider::new(
                    api_key.clone(),
                    config.base_url.clone(),
                    config.timeout_secs,
                );
                Ok(Arc::new(provider))
            }
            "minimax" => {
                let api_key = config.api_key.as_ref()
                    .ok_or_else(|| anyhow!("MiniMax 需要 API Key"))?;
                let provider = minimax::MiniMaxProvider::new(
                    api_key.clone(),
                    config.base_url.clone(),
                    Some(config.timeout_secs),
                );
                Ok(Arc::new(provider))
            }
            "vllm" => {
                let api_key = config.api_key.clone().unwrap_or_default();
                let provider = vllm::VllmProvider::new(
                    api_key,
                    config.base_url.clone(),
                    config.timeout_secs,
                    config.default_model.clone(),
                );
                Ok(Arc::new(provider))
            }
            _ => Err(anyhow!("未知的 LLM 提供商: {}", name)),
        }
    }
}

/// LLM 管理器
pub struct LlmManager {
    providers: std::collections::HashMap<String, Arc<dyn LlmProvider>>,
    default_provider: String,
}

impl LlmManager {
    pub fn new(config: &crate::config::Config) -> Result<Self> {
        let mut providers = std::collections::HashMap::new();

        // 注册 OpenRouter
        if config.llm.openrouter.api_key.is_some() {
            match LlmProviderFactory::create("openrouter", &config.llm.openrouter) {
                Ok(provider) => {
                    providers.insert("openrouter".to_string(), provider);
                }
                Err(e) => tracing::warn!("无法创建 OpenRouter 提供商: {}", e),
            }
        }

        // 注册 DeepSeek
        if config.llm.deepseek.api_key.is_some() {
            match LlmProviderFactory::create("deepseek", &config.llm.deepseek) {
                Ok(provider) => {
                    providers.insert("deepseek".to_string(), provider);
                }
                Err(e) => tracing::warn!("无法创建 DeepSeek 提供商: {}", e),
            }
        }

        // 注册 MiniMax
        if config.llm.minimax.api_key.is_some() {
            match LlmProviderFactory::create("minimax", &config.llm.minimax) {
                Ok(provider) => {
                    providers.insert("minimax".to_string(), provider);
                }
                Err(e) => tracing::warn!("无法创建 MiniMax 提供商: {}", e),
            }
        }

        // 注册 Moonshot
        if config.llm.moonshot.api_key.is_some() {
            match LlmProviderFactory::create("moonshot", &config.llm.moonshot) {
                Ok(provider) => {
                    providers.insert("moonshot".to_string(), provider);
                }
                Err(e) => tracing::warn!("无法创建 Moonshot 提供商: {}", e),
            }
        }

        // 注册 vLLM
        if config.llm.vllm.base_url.is_some() {
            match LlmProviderFactory::create("vllm", &config.llm.vllm) {
                Ok(provider) => {
                    providers.insert("vllm".to_string(), provider);
                }
                Err(e) => tracing::warn!("无法创建 vLLM 提供商: {}", e),
            }
        }

        if providers.is_empty() {
            anyhow::bail!("没有可用的 LLM 提供商，请配置 API Key");
        }

        Ok(Self {
            providers,
            default_provider: config.agent.default_provider.clone(),
        })
    }

    /// 获取提供商
    pub fn get_provider(&self, name: Option<&str>) -> Result<Arc<dyn LlmProvider>> {
        let name = name.unwrap_or(&self.default_provider);
        self.providers
            .get(name)
            .cloned()
            .ok_or_else(|| anyhow!("提供商 '{}' 不可用", name))
    }

    /// 获取默认提供商
    pub fn default_provider(&self) -> Result<Arc<dyn LlmProvider>> {
        self.get_provider(None)
    }

    /// 列出可用提供商
    pub fn list_providers(&self) -> Vec<&str> {
        self.providers.keys().map(|s| s.as_str()).collect()
    }
}
