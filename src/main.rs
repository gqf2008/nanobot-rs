//! Nanobot - 超轻量级个人 AI Agent
//! 
//! Rust 复刻版本，支持多 LLM 提供商、多通道、工具系统

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing::{info, warn};

mod agent;
mod bus;
mod channel;
mod cli;
mod config;
mod cron;
mod error;
mod llm;
mod memory;
mod module_tests;
mod session;
mod tools;

#[cfg(test)]
mod tests;

use crate::config::Config;

/// Nanobot CLI
#[derive(Parser)]
#[command(name = "nanobot")]
#[command(about = "超轻量级个人 AI Agent")]
#[command(version = "0.1.0")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    
    /// 配置文件路径
    #[arg(short, long, global = true)]
    config: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// 启动 AI Agent 对话模式
    Agent {
        /// 初始提示词
        #[arg(short, long)]
        prompt: Option<String>,
    },
    /// 启动网关服务（Telegram Bot 等）
    Gateway {
        /// 指定通道（如 telegram）
        #[arg(short, long)]
        channel: Option<String>,
    },
    /// 查看系统状态
    Status,
    /// 初始化配置文件
    Init {
        /// 强制覆盖已有配置
        #[arg(short, long)]
        force: bool,
    },
    /// 执行单个工具
    Tool {
        /// 工具名称
        name: String,
        /// 工具参数（JSON 格式）
        #[arg(short, long)]
        args: Option<String>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("nanobot=info".parse()?)
                .add_directive("teloxide=warn".parse()?),
        )
        .init();

    info!("🤖 Nanobot v0.1.0 启动中...");

    let cli = Cli::parse();

    // 加载配置
    let config_path = cli.config.as_deref();
    let config = match Config::load(config_path) {
        Ok(cfg) => cfg,
        Err(e) => {
            warn!("加载配置失败: {}，使用默认配置", e);
            Config::default()
        }
    };

    match cli.command {
        Commands::Agent { prompt } => {
            cli::agent::run(config, prompt).await?;
        }
        Commands::Gateway { channel } => {
            cli::gateway::run(config, channel).await?;
        }
        Commands::Status => {
            cli::status::run(config).await?;
        }
        Commands::Init { force } => {
            cli::init::run(config_path, force).await?;
        }
        Commands::Tool { name, args } => {
            cli::tool::run(config, &name, args).await?;
        }
    }

    Ok(())
}
