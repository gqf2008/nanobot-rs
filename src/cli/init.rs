//! init 命令 - 初始化配置文件

use anyhow::{Context, Result};
use std::path::Path;
use tracing::info;

use crate::config::Config;

pub async fn run(config_path: Option<&str>, force: bool) -> Result<()> {
    let path = if let Some(p) = config_path {
        Path::new(p).to_path_buf()
    } else {
        Config::default_config_path()?
    };

    // 检查文件是否已存在
    if path.exists() && !force {
        println!("配置文件已存在: {}", path.display());
        println!("使用 --force 强制覆盖");
        return Ok(());
    }

    // 确保目录存在
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建目录失败: {}", parent.display()))?;
    }

    // 创建示例配置
    let config = Config::example();
    let content = toml::to_string_pretty(&config)?;
    
    std::fs::write(&path, content)
        .with_context(|| format!("写入配置文件失败: {}", path.display()))?;

    info!("配置文件已创建: {}", path.display());
    println!("✅ 配置文件已创建: {}", path.display());
    println!("\n请编辑配置文件，添加你的 API Key：");
    println!("  - OPENROUTER_API_KEY");
    println!("  - DEEPSEEK_API_KEY");
    println!("  - TELEGRAM_BOT_TOKEN（如果需要 Telegram Bot）");

    Ok(())
}
