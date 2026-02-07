//! 错误类型定义

use thiserror::Error;

#[derive(Error, Debug)]
pub enum NanobotError {
    #[error("配置错误: {0}")]
    Config(String),
    
    #[error("LLM 提供商错误: {0}")]
    Llm(String),
    
    #[error("工具执行错误: {0}")]
    Tool(String),
    
    #[error("通道错误: {0}")]
    Channel(String),
    
    #[error("内存系统错误: {0}")]
    Memory(String),
    
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("HTTP 错误: {0}")]
    Http(#[from] reqwest::Error),
    
    #[error("JSON 解析错误: {0}")]
    Json(#[from] serde_json::Error),
    
    #[error("未知错误: {0}")]
    Unknown(String),
}

pub type Result<T> = std::result::Result<T, NanobotError>;
