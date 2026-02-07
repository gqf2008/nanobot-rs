//! tool 命令 - 直接执行工具

use anyhow::{anyhow, Result};
use serde_json::Value;

use crate::config::Config;
use crate::tools::{ToolContext, ToolRegistry};

pub async fn run(
    config: Config,
    name: &str,
    args: Option<String>,
) -> Result<()> {
    println!("🔧 执行工具: {}\n", name);

    // 解析参数
    let args: Value = if let Some(args_str) = args {
        serde_json::from_str(&args_str)?
    } else {
        Value::Object(serde_json::Map::new())
    };

    // 创建工具注册表
    let registry = ToolRegistry::default_with_config(&config);

    // 创建工具上下文
    let ctx = ToolContext::new(config.tools);

    // 执行工具
    match registry.execute(name, args, &ctx).await {
        Ok(result) => {
            if result.success {
                println!("✅ 执行成功:\n{}", result.output);
            } else {
                println!("❌ 执行失败:\n{}", result.error.unwrap_or_default());
            }
        }
        Err(e) => {
            return Err(anyhow!("工具执行错误: {}", e));
        }
    }

    Ok(())
}
