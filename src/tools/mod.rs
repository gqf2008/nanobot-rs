//! 工具系统模块
//! 
//! 提供各种工具供 LLM 调用：shell、文件读写、web 搜索等

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

pub mod file;
pub mod message;
pub mod shell;
pub mod web;

/// 工具执行上下文
#[derive(Debug, Clone)]
pub struct ToolContext {
    pub config: crate::config::ToolsConfig,
    pub working_dir: std::path::PathBuf,
}

impl ToolContext {
    pub fn new(config: crate::config::ToolsConfig) -> Self {
        Self {
            config,
            working_dir: std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/tmp")),
        }
    }
}

/// 工具定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

impl ToolDef {
    pub fn to_llm_tool(&self) -> crate::llm::Tool {
        crate::llm::Tool {
            name: self.name.clone(),
            description: self.description.clone(),
            parameters: self.parameters.clone(),
        }
    }
}

/// 工具执行结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub success: bool,
    pub output: String,
    pub error: Option<String>,
}

impl ToolResult {
    pub fn success(output: impl Into<String>) -> Self {
        Self {
            success: true,
            output: output.into(),
            error: None,
        }
    }

    pub fn error(error: impl Into<String>) -> Self {
        Self {
            success: false,
            output: String::new(),
            error: Some(error.into()),
        }
    }

    pub fn to_string(&self) -> String {
        if self.success {
            self.output.clone()
        } else {
            format!("错误: {}", self.error.as_ref().unwrap_or(&"未知错误".to_string()))
        }
    }
}

/// 工具 trait
#[async_trait]
pub trait Tool: Send + Sync {
    /// 获取工具定义
    fn definition(&self) -> &ToolDef;
    
    /// 执行工具
    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<ToolResult>;
    
    /// 获取工具名称
    fn name(&self) -> &str {
        &self.definition().name
    }
}

/// 工具注册表
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// 注册工具
    pub fn register<T: Tool + 'static>(&mut self, tool: T) {
        let name = tool.name().to_string();
        self.tools.insert(name, Arc::new(tool));
    }

    /// 获取工具
    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    /// 列出所有工具
    pub fn list_tools(&self) -> Vec<&ToolDef> {
        self.tools.values().map(|t| t.definition()).collect()
    }

    /// 获取 LLM 可用的工具列表
    pub fn to_llm_tools(&self) -> Vec<crate::llm::Tool> {
        self.list_tools().into_iter().map(|t| t.to_llm_tool()).collect()
    }

    /// 执行工具
    pub async fn execute(
        &self,
        name: &str,
        args: Value,
        ctx: &ToolContext,
    ) -> Result<ToolResult> {
        let tool = self.tools
            .get(name)
            .ok_or_else(|| anyhow!("未知工具: {}", name))?;
        
        tool.execute(args, ctx).await
    }

    /// 创建默认工具集
    pub fn default_with_config(config: &crate::config::Config) -> Self {
        let mut registry = Self::new();
        
        // 注册 Shell 工具
        registry.register(shell::ShellTool);
        
        // 注册文件工具
        registry.register(file::ReadFileTool);
        registry.register(file::WriteFileTool);
        registry.register(file::ListDirTool);
        
        // 注册 Web 搜索工具（如果配置了 API Key）
        if config.tools.search_api_key.is_some() {
            registry.register(web::WebSearchTool::new(
                config.tools.search_api_key.clone().unwrap()
            ));
        }
        
        registry
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        let config = crate::config::Config::default();
        Self::default_with_config(&config)
    }
}
