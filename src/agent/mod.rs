//! Agent 核心模块
//! 
//! 实现 LLM 对话循环、工具执行、上下文管理

use anyhow::{anyhow, Result};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::{
    config::Config,
    llm::{ChatRequest, LlmManager, Message, Role},
    memory::MemoryStore,
    tools::{ToolContext, ToolRegistry},
};

/// Agent 实例
pub struct Agent {
    config: Config,
    llm_manager: LlmManager,
    tool_registry: ToolRegistry,
    memory: Option<Arc<MemoryStore>>,
    session_id: Mutex<String>,
    context: Mutex<AgentContext>,
}

/// Agent 上下文
#[derive(Debug)]
struct AgentContext {
    messages: Vec<Message>,
    total_tokens: u32,
}

impl Agent {
    /// 创建新的 Agent 实例
    ///
    /// * `config` - 配置对象
    /// * `session_id` - 可选的会话 ID，如果为 None 则生成新的 UUID
    pub async fn new(config: Config, session_id: Option<String>) -> Result<Self> {
        let llm_manager = LlmManager::new(&config)?;
        let tool_registry = ToolRegistry::default_with_config(&config);
        
        // 初始化内存系统
        let memory = if !config.memory.workspace_path.as_os_str().is_empty() {
            match MemoryStore::new(&config.memory.workspace_path).await {
                Ok(m) => Some(Arc::new(m)),
                Err(e) => {
                    warn!("内存系统初始化失败: {}，继续运行", e);
                    None
                }
            }
        } else {
            None
        };

        // 如果提供了 session_id 则使用，否则生成新的 UUID
        let session_id = session_id.unwrap_or_else(|| Uuid::new_v4().to_string());

        // 初始化上下文
        let mut messages = vec![Message::system(&config.agent.system_prompt)];

        // 如果有内存系统，加载之前的对话
        if let Some(ref mem) = memory {
            let history = mem.get_conversation(&session_id, config.agent.max_context as i64).await?;
            for msg in history {
                // DeepSeek API 要求 tool 消息必须有 tool_call_id，跳过无效的 tool 消息
                if msg.role == "tool" && msg.tool_call_id.is_none() {
                    continue;
                }
                
                let role = match msg.role.as_str() {
                    "user" => Role::User,
                    "assistant" => Role::Assistant,
                    "tool" => Role::Tool,
                    _ => Role::System,
                };
                messages.push(Message {
                    role,
                    content: msg.content,
                    tool_calls: msg.tool_calls.and_then(|t| serde_json::from_str(&t).ok()),
                    tool_call_id: msg.tool_call_id,
                });
            }
        }

        Ok(Self {
            config,
            llm_manager,
            tool_registry,
            memory,
            session_id: Mutex::new(session_id),
            context: Mutex::new(AgentContext {
                messages,
                total_tokens: 0,
            }),
        })
    }

    /// 发送消息给 Agent
    pub async fn chat(&self,
        content: impl Into<String>,
    ) -> Result<AgentResponse> {
        let content = content.into();
        info!("用户: {}", content);

        // 添加用户消息到上下文
        {
            let mut ctx = self.context.lock().await;
            ctx.messages.push(Message::user(content.clone()));
            
            // 保存到内存
            if let Some(ref memory) = self.memory {
                let session_id = self.session_id.lock().await.clone();
                let _ = memory.add_message(&session_id, "user", &content, None).await;
            }
        }

        // 执行对话循环
        let response = self.run_loop().await?;

        Ok(response)
    }

    /// 核心对话循环
    async fn run_loop(&self,
    ) -> Result<AgentResponse> {
        let provider = self.llm_manager.default_provider()?;
        let max_iterations = 10;
        let mut iterations = 0;
        let session_id = self.session_id.lock().await.clone();

        loop {
            iterations += 1;
            if iterations > max_iterations {
                return Err(anyhow!("超过最大迭代次数"));
            }

            // 准备请求
            let tools = self.tool_registry.to_llm_tools();
            let request = {
                let ctx = self.context.lock().await;
                let mut req = ChatRequest::new(
                    self.config.agent.default_model.clone(),
                    ctx.messages.clone(),
                );
                if !tools.is_empty() {
                    req = req.with_tools(tools);
                }
                req
            };

            debug!("发送 LLM 请求，使用模型: {}", request.model);

            // 调用 LLM
            let llm_response = provider.chat(request).await?;
            
            let message = llm_response.message;
            debug!("LLM 响应: {:?}", message);

            // 检查是否有工具调用
            if let Some(tool_calls) = &message.tool_calls {
                if !tool_calls.is_empty() {
                    // 添加助手消息（带工具调用）到上下文
                    {
                        let mut ctx = self.context.lock().await;
                        ctx.messages.push(message.clone());
                    }

                    // 保存到内存
                    if let Some(ref memory) = self.memory {
                        // 获取第一个 tool_call 的 id
                        let tool_call_id = tool_calls.first()
                            .map(|c| c.id.as_str());
                        let _ = memory.add_message(
                            &session_id,
                            "assistant",
                            &message.content,
                            tool_call_id.as_deref(),
                        ).await;
                    }

                    // 执行工具
                    let tool_ctx = ToolContext::new(self.config.tools.clone());
                    
                    for tool_call in tool_calls {
                        let tool_name = &tool_call.function.name;
                        let tool_args: Value = serde_json::from_str(&tool_call.function.arguments)?;

                        info!("执行工具: {} 参数: {}", tool_name, tool_call.function.arguments);

                        let result = self.tool_registry.execute(
                            tool_name,
                            tool_args,
                            &tool_ctx,
                        ).await;

                        let result_str = match result {
                            Ok(r) => r.to_string(),
                            Err(e) => format!("工具执行错误: {}", e),
                        };

                        // 添加工具结果到上下文
                        {
                            let mut ctx = self.context.lock().await;
                            ctx.messages.push(Message::tool_result(
                                &tool_call.id,
                                result_str.clone(),
                            ));
                        }

                        // 保存到内存
                        if let Some(ref memory) = self.memory {
                            let _ = memory.add_message(
                                &session_id,
                                "tool",
                                &result_str,
                                Some(&tool_call.id),
                            ).await;
                        }
                    }

                    // 继续循环，让 LLM 处理工具结果
                    continue;
                }
            }

            // 没有工具调用，返回最终结果
            {
                let mut ctx = self.context.lock().await;
                ctx.messages.push(message.clone());
                
                // 清理上下文，保留最近的 N 条
                let max_context = self.config.agent.max_context;
                if ctx.messages.len() > max_context + 1 {
                    // 保留系统提示词和最近的 N 条
                    let system_msg = ctx.messages.remove(0);
                    let to_remove = ctx.messages.len() - max_context;
                    for _ in 0..to_remove {
                        if ctx.messages.len() > 1 {
                            ctx.messages.remove(0);
                        }
                    }
                    ctx.messages.insert(0, system_msg);
                }
            }

            // 保存到内存
            if let Some(ref memory) = self.memory {
                let _ = memory.add_message(
                    &session_id,
                    "assistant",
                    &message.content,
                    None,
                ).await;
            }

            return Ok(AgentResponse {
                content: message.content,
                model: llm_response.model,
            });
        }
    }

    /// 获取会话 ID
    pub async fn session_id(&self) -> String {
        self.session_id.lock().await.clone()
    }
    pub async fn context_length(&self) -> usize {
        self.context.lock().await.messages.len()
    }

    /// 清空上下文
    pub async fn clear_context(&self) {
        let mut ctx = self.context.lock().await;
        ctx.messages.clear();
        ctx.messages.push(Message::system(&self.config.agent.system_prompt));
    }

    /// 设置会话 ID（用于切换对话上下文）
    ///
    /// 这会保存当前对话到旧会话，然后加载新会话的历史
    pub async fn set_session_id(&self, session_id: &str) {
        // 保存当前消息到旧会话
        if let Some(ref memory) = self.memory {
            let old_session_id = self.session_id.lock().await.clone();
            let ctx = self.context.lock().await;
            
            // 保存对话历史
            for msg in &ctx.messages {
                let role = match msg.role {
                    Role::User => "user",
                    Role::Assistant => "assistant",
                    Role::Tool => "tool",
                    Role::System => "system",
                };
                let tool_calls = msg.tool_calls.as_ref()
                    .map(|t| serde_json::to_string(t).ok())
                    .flatten();
                let _ = memory.add_message(&old_session_id, role, &msg.content, tool_calls.as_deref()).await;
            }
        }

        // 清除并重新加载上下文
        {
            let mut ctx = self.context.lock().await;
            ctx.messages.clear();
            ctx.messages.push(Message::system(&self.config.agent.system_prompt));

            // 加载新会话的历史
            if let Some(ref memory) = self.memory {
                let history = memory.get_conversation(session_id, self.config.agent.max_context as i64).await.unwrap_or_default();
                for msg in history {
                    let role = match msg.role.as_str() {
                        "user" => Role::User,
                        "assistant" => Role::Assistant,
                        "tool" => Role::Tool,
                        _ => Role::System,
                    };
                    ctx.messages.push(Message {
                        role,
                        content: msg.content,
                        tool_calls: msg.tool_calls.and_then(|t| serde_json::from_str(&t).ok()),
                        tool_call_id: None,
                    });
                }
            }
        }

        // 更新会话 ID
        *self.session_id.lock().await = session_id.to_string();
    }
}

/// Agent 响应
#[derive(Debug, Clone)]
pub struct AgentResponse {
    pub content: String,
    pub model: String,
}
