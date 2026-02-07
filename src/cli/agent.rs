//! agent 命令 - 启动交互式对话模式

use anyhow::Result;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use std::sync::Arc;
use tracing::info;

use crate::agent::Agent;
use crate::config::Config;

pub async fn run(config: Config, initial_prompt: Option<String>) -> Result<()> {
    info!("启动 Nanobot Agent 模式...");

    // 创建 Agent
    let agent = Arc::new(Agent::new(config).await?);

    println!("🤖 Nanobot Agent 模式");
    println!("输入 'exit' 或 'quit' 退出，'clear' 清空上下文\n");

    // 如果有初始提示词，先执行
    if let Some(prompt) = initial_prompt {
        println!("用户: {}", prompt);
        match agent.chat(prompt).await {
            Ok(response) => {
                println!("\n🤖 {}\n", response.content);
            }
            Err(e) => {
                eprintln!("错误: {}", e);
            }
        }
    }

    // 启动交互式循环
    let mut rl = DefaultEditor::new()?;

    loop {
        match rl.readline("你: ") {
            Ok(line) => {
                let input = line.trim();
                
                if input.is_empty() {
                    continue;
                }

                // 添加到历史
                let _ = rl.add_history_entry(input);

                // 处理特殊命令
                match input.to_lowercase().as_str() {
                    "exit" | "quit" => {
                        println!("再见! 👋");
                        break;
                    }
                    "clear" => {
                        agent.clear_context().await;
                        println!("上下文已清空。\n");
                        continue;
                    }
                    "status" => {
                        let ctx_len = agent.context_length().await;
                        println!("会话 ID: {}", agent.session_id());
                        println!("上下文消息数: {}\n", ctx_len);
                        continue;
                    }
                    _ => {}
                }

                // 发送给 Agent
                match agent.chat(input).await {
                    Ok(response) => {
                        println!("\n🤖 {}\n", response.content);
                    }
                    Err(e) => {
                        eprintln!("错误: {}\n", e);
                    }
                }
            }
            Err(ReadlineError::Interrupted) => {
                println!("\n使用 'exit' 或 Ctrl+D 退出");
            }
            Err(ReadlineError::Eof) => {
                println!("\n再见! 👋");
                break;
            }
            Err(e) => {
                eprintln!("读取输入错误: {}", e);
                break;
            }
        }
    }

    Ok(())
}
