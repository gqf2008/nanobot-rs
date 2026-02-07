//! 测试模块

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::llm::{Message, Role};
    use crate::tools::{ToolContext, ToolRegistry};

    #[test]
    fn test_config_default() {
        let config = Config::default();
        assert_eq!(config.agent.max_context, 20);
        assert!(!config.agent.system_prompt.is_empty());
    }

    #[test]
    fn test_message_creation() {
        let user_msg = Message::user("Hello");
        assert_eq!(user_msg.role, Role::User);
        assert_eq!(user_msg.content, "Hello");

        let system_msg = Message::system("You are a helpful assistant");
        assert_eq!(system_msg.role, Role::System);

        let assistant_msg = Message::assistant("Hi there!");
        assert_eq!(assistant_msg.role, Role::Assistant);
    }

    #[test]
    fn test_tool_registry_creation() {
        let config = Config::default();
        let registry = ToolRegistry::default_with_config(&config);
        
        // 检查默认工具是否已注册
        assert!(registry.get("shell").is_some());
        assert!(registry.get("read_file").is_some());
        assert!(registry.get("write_file").is_some());
        assert!(registry.get("list_dir").is_some());
    }

    #[tokio::test]
    async fn test_shell_tool_whitelist() {
        use crate::tools::Tool;
        use serde_json::json;

        let config = Config::default();
        let ctx = ToolContext::new(config.tools.clone());
        
        // 创建 Shell 工具
        let shell_tool = crate::tools::shell::ShellTool;
        
        // 测试白名单检查（应该失败，因为 echo 是白名单的）
        // 注意：实际执行会失败，因为没有允许的路径
        let args = json!({
            "command": "echo hello",
            "timeout": 5
        });
        
        let result = shell_tool.execute(args, &ctx).await;
        // 白名单检查通过，命令应该执行成功
        assert!(result.is_ok());
        
        let tool_result = result.unwrap();
        assert!(tool_result.success);
        assert!(tool_result.output.contains("hello"));
    }

    #[tokio::test]
    async fn test_file_operations() {
        use crate::tools::Tool;
        use serde_json::json;
        use std::path::PathBuf;
        use tempfile::TempDir;

        // 创建临时目录
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path().to_string_lossy().to_string();

        let mut config = Config::default();
        config.tools.allowed_paths = vec![temp_path.clone()];

        let ctx = ToolContext::new(config.tools);

        // 测试写入文件
        let write_tool = crate::tools::file::WriteFileTool;
        let file_path = PathBuf::from(&temp_path).join("test.txt");
        let args = json!({
            "path": file_path.to_string_lossy().to_string(),
            "content": "Hello, World!"
        });

        let result = write_tool.execute(args, &ctx).await.unwrap();
        assert!(result.success);

        // 测试读取文件
        let read_tool = crate::tools::file::ReadFileTool;
        let args = json!({
            "path": file_path.to_string_lossy().to_string()
        });

        let result = read_tool.execute(args, &ctx).await.unwrap();
        assert!(result.success);
        assert_eq!(result.output, "Hello, World!");

        // 测试列出目录
        let list_tool = crate::tools::file::ListDirTool;
        let args = json!({
            "path": temp_path
        });

        let result = list_tool.execute(args, &ctx).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("test.txt"));
    }
}
