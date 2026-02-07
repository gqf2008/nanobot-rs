//! 文件操作工具 - 读写文件、列出目录

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::path::Path;

use super::{Tool, ToolContext, ToolDef, ToolResult};

/// 验证路径是否在允许范围内
fn validate_path(path: &Path, allowed_paths: &[String]) -> Result<()> {
    if allowed_paths.is_empty() {
        return Ok(());
    }

    let canonical_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

    for allowed in allowed_paths {
        let allowed_path = Path::new(allowed).canonicalize().unwrap_or_else(|_| Path::new(allowed).to_path_buf());
        if canonical_path.starts_with(&allowed_path) {
            return Ok(());
        }
    }

    Err(anyhow::anyhow!(
        "路径 '{}' 不在允许范围内。允许的路径: {:?}",
        path.display(),
        allowed_paths
    ))
}

/// 读取文件工具
pub struct ReadFileTool;

#[async_trait]
impl Tool for ReadFileTool {
    fn definition(&self) -> &ToolDef {
        lazy_static::lazy_static! {
            static ref DEF: ToolDef = ToolDef {
                name: "read_file".to_string(),
                description: "读取文件内容".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "文件路径"
                        }
                    },
                    "required": ["path"]
                }),
            };
        }
        &DEF
    }

    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let path_str = args.get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("缺少 path 参数"))?;

        let path = Path::new(path_str);

        // 验证路径
        if let Err(e) = validate_path(path, &ctx.config.allowed_paths) {
            return Ok(ToolResult::error(e.to_string()));
        }

        // 检查文件大小限制（1MB）
        let metadata = match tokio::fs::metadata(path).await {
            Ok(m) => m,
            Err(e) => return Ok(ToolResult::error(format!("无法读取文件: {}", e))),
        };

        if metadata.len() > 1024 * 1024 {
            return Ok(ToolResult::error("文件超过 1MB 限制".to_string()));
        }

        // 读取文件
        match tokio::fs::read_to_string(path).await {
            Ok(content) => Ok(ToolResult::success(content)),
            Err(e) => Ok(ToolResult::error(format!("读取失败: {}", e))),
        }
    }
}

/// 写入文件工具
pub struct WriteFileTool;

#[async_trait]
impl Tool for WriteFileTool {
    fn definition(&self) -> &ToolDef {
        lazy_static::lazy_static! {
            static ref DEF: ToolDef = ToolDef {
                name: "write_file".to_string(),
                description: "写入文件内容（覆盖模式）".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "文件路径"
                        },
                        "content": {
                            "type": "string",
                            "description": "文件内容"
                        }
                    },
                    "required": ["path", "content"]
                }),
            };
        }
        &DEF
    }

    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let path_str = args.get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("缺少 path 参数"))?;

        let content = args.get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("缺少 content 参数"))?;

        let path = Path::new(path_str);

        // 验证路径
        if let Err(e) = validate_path(path, &ctx.config.allowed_paths) {
            return Ok(ToolResult::error(e.to_string()));
        }

        // 确保父目录存在
        if let Some(parent) = path.parent() {
            if let Err(e) = tokio::fs::create_dir_all(parent).await {
                return Ok(ToolResult::error(format!("创建目录失败: {}", e)));
            }
        }

        // 写入文件
        match tokio::fs::write(path, content).await {
            Ok(_) => Ok(ToolResult::success(format!("文件已写入: {}", path.display()))),
            Err(e) => Ok(ToolResult::error(format!("写入失败: {}", e))),
        }
    }
}

/// 列出目录工具
pub struct ListDirTool;

#[async_trait]
impl Tool for ListDirTool {
    fn definition(&self) -> &ToolDef {
        lazy_static::lazy_static! {
            static ref DEF: ToolDef = ToolDef {
                name: "list_dir".to_string(),
                description: "列出目录内容".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "目录路径"
                        }
                    },
                    "required": ["path"]
                }),
            };
        }
        &DEF
    }

    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let path_str = args.get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("缺少 path 参数"))?;

        let path = Path::new(path_str);

        // 验证路径
        if let Err(e) = validate_path(path, &ctx.config.allowed_paths) {
            return Ok(ToolResult::error(e.to_string()));
        }

        // 读取目录
        let mut entries = match tokio::fs::read_dir(path).await {
            Ok(e) => e,
            Err(e) => return Ok(ToolResult::error(format!("无法读取目录: {}", e))),
        };

        let mut result = Vec::new();

        while let Ok(Some(entry)) = entries.next_entry().await {
            let name = entry.file_name().to_string_lossy().to_string();
            let metadata = entry.metadata().await.ok();
            
            let file_type = if let Some(ref m) = metadata {
                if m.is_dir() {
                    "[DIR]"
                } else if m.is_file() {
                    "[FILE]"
                } else {
                    "[OTHER]"
                }
            } else {
                "[UNKNOWN]"
            };

            let size = if let Some(ref m) = metadata {
                if m.is_file() {
                    format!("{} bytes", m.len())
                } else {
                    "-".to_string()
                }
            } else {
                "-".to_string()
            };

            result.push(format!("{} {:<10} {}", file_type, size, name));
        }

        if result.is_empty() {
            Ok(ToolResult::success("目录为空".to_string()))
        } else {
            Ok(ToolResult::success(result.join("\n")))
        }
    }
}
