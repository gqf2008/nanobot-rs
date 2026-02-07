//! status 命令 - 显示系统状态

use anyhow::Result;

use crate::config::Config;

pub async fn run(config: Config) -> Result<()> {
    println!("🤖 Nanobot 状态\n");

    // 显示配置信息
    println!("📁 配置:");
    println!("  默认提供商: {}", config.agent.default_provider);
    println!("  默认模型: {}", config.agent.default_model);
    println!("  最大上下文: {}", config.agent.max_context);

    // 检查 LLM 提供商
    println!("\n🧠 LLM 提供商:");
    
    if config.llm.openrouter.api_key.is_some() {
        println!("  ✅ OpenRouter");
    } else {
        println!("  ❌ OpenRouter（未配置）");
    }

    if config.llm.deepseek.api_key.is_some() {
        println!("  ✅ DeepSeek");
    } else {
        println!("  ❌ DeepSeek（未配置）");
    }

    if config.llm.openai.api_key.is_some() {
        println!("  ✅ OpenAI");
    } else {
        println!("  ❌ OpenAI（未配置）");
    }

    if config.llm.anthropic.api_key.is_some() {
        println!("  ✅ Anthropic");
    } else {
        println!("  ❌ Anthropic（未配置）");
    }

    // 检查通道
    println!("\n📡 通道:");
    
    if config.channel.telegram.bot_token.is_some() {
        println!("  ✅ Telegram Bot");
    } else {
        println!("  ❌ Telegram Bot（未配置）");
    }

    // 检查工具
    println!("\n🔧 工具:");
    if config.tools.search_api_key.is_some() {
        println!("  ✅ Web 搜索");
    } else {
        println!("  ❌ Web 搜索（未配置）");
    }

    // 内存系统
    println!("\n💾 内存:");
    println!("  工作目录: {}", config.memory.workspace_path.display());
    println!("  最大记忆数: {}", config.memory.max_memories);

    println!("\n使用 `nanobot agent` 启动交互式对话");
    println!("使用 `nanobot gateway` 启动网关服务");

    Ok(())
}
