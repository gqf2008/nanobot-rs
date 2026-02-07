//! Shell 工具 - 执行系统命令

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use super::{Tool, ToolContext, ToolDef, ToolResult};

/// Shell 命令执行工具
pub struct ShellTool;

impl ShellTool {
    fn validate_command(&self, command: &str, config: &crate::config::ToolsConfig) -> Result<()> {
        // 检查命令是否在白名单中
        if config.shell_whitelist.is_empty() {
            return Ok(());
        }

        let cmd = command.split_whitespace().next()
            .ok_or_else(|| anyhow::anyhow!("空命令"))?;

        // 提取基础命令（去除路径）
        let base_cmd = std::path::Path::new(cmd)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(cmd);

        if !config.shell_whitelist.iter().any(|w| w == base_cmd) {
            return Err(anyhow::anyhow!(
                "命令 '{}' 不在白名单中。允许的命令: {:?}",
                base_cmd, config.shell_whitelist
            ));
        }

        Ok(())
    }
}

#[async_trait]
impl Tool for ShellTool {
    fn definition(&self) -> &ToolDef {
        lazy_static::lazy_static! {
            static ref DEF: ToolDef = ToolDef {
                name: "shell".to_string(),
                description: "执行系统 shell 命令。使用前请确认命令在白名单中。".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "要执行的 shell 命令"
                        },
                        "timeout": {
                            "type": "integer",
                            "description": "超时时间（秒），默认 30",
                            "default": 30
                        }
                    },
                    "required": ["command"]
                }),
            };
        }
        &DEF
    }

    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let command = args.get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("缺少 command 参数"))?;

        let timeout = args.get("timeout")
            .and_then(|v| v.as_u64())
            .unwrap_or(30);

        // 验证命令
        if let Err(e) = self.validate_command(command, &ctx.config) {
            return Ok(ToolResult::error(e.to_string()));
        }

        // 执行命令
        let output = tokio::time::timeout(
            std::time::Duration::from_secs(timeout),
            tokio::process::Command::new("sh")
                .arg("-c")
                .arg(command)
                .current_dir(&ctx.working_dir)
                .output()
        ).await;

        match output {
            Ok(Ok(result)) => {
                let stdout = String::from_utf8_lossy(&result.stdout);
                let stderr = String::from_utf8_lossy(&result.stderr);

                if result.status.success() {
                    let output = if stdout.is_empty() {
                        if stderr.is_empty() {
                            "命令执行成功（无输出）".to_string()
                        } else {
                            stderr.to_string()
                        }
                    } else {
                        stdout.to_string()
                    };
                    Ok(ToolResult::success(output))
                } else {
                    let error = format!(
                        "退出码: {}\n标准输出: {}\n标准错误: {}",
                        result.status.code().unwrap_or(-1),
                        stdout,
                        stderr
                    );
                    Ok(ToolResult::error(error))
                }
            }
            Ok(Err(e)) => {
                Ok(ToolResult::error(format!("执行失败: {}", e)))
            }
            Err(_) => {
                Ok(ToolResult::error(format!("命令执行超时（{}秒）", timeout)))
            }
        }
    }
}
