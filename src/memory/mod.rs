//! Memory 系统 - 持久化存储对话历史和记忆
//!
//! 使用 Markdown 文件格式，与 Python 版本保持一致
//! - 日常笔记: memory/YYYY-MM-DD.md
//! - 长期记忆: memory/MEMORY.md
//! - 对话历史: memory/conversations/{session_id}.md

use anyhow::{Context, Result};
use chrono::{DateTime, Local, Utc};
use std::path::{Path, PathBuf};
use tokio::fs;
use tracing::{debug, info};

/// Memory 存储
pub struct MemoryStore {
    /// 工作目录
    workspace: PathBuf,
    /// Memory 目录
    memory_dir: PathBuf,
    /// 长期记忆文件
    memory_file: PathBuf,
    /// 对话历史目录
    conversations_dir: PathBuf,
}

impl MemoryStore {
    /// 创建新的 MemoryStore
    pub async fn new(workspace: &Path) -> Result<Self> {
        let memory_dir = workspace.join("memory");
        let memory_file = memory_dir.join("MEMORY.md");
        let conversations_dir = memory_dir.join("conversations");

        // 确保目录存在
        fs::create_dir_all(&memory_dir).await
            .with_context(|| format!("创建 memory 目录失败: {}", memory_dir.display()))?;
        fs::create_dir_all(&conversations_dir).await
            .with_context(|| format!("创建 conversations 目录失败: {}", conversations_dir.display()))?;

        info!("MemoryStore 初始化完成: {}", memory_dir.display());

        Ok(Self {
            workspace: workspace.to_path_buf(),
            memory_dir,
            memory_file,
            conversations_dir,
        })
    }

    /// 获取今天的 memory 文件路径
    pub fn get_today_file(&self) -> PathBuf {
        let today = Local::now().format("%Y-%m-%d").to_string();
        self.memory_dir.join(format!("{}.md", today))
    }

    /// 读取今天的 memory
    pub async fn read_today(&self) -> Result<String> {
        let today_file = self.get_today_file();
        
        if today_file.exists() {
            fs::read_to_string(&today_file).await
                .with_context(|| format!("读取今天的 memory 失败: {}", today_file.display()))
        } else {
            Ok(String::new())
        }
    }

    /// 追加内容到今天的 memory
    pub async fn append_today(
        &self,
        content: impl AsRef<str>,
    ) -> Result<()> {
        let today_file = self.get_today_file();
        let content = content.as_ref();

        let existing = if today_file.exists() {
            fs::read_to_string(&today_file).await.unwrap_or_default()
        } else {
            // 新文件，添加标题
            let today = Local::now().format("%Y-%m-%d").to_string();
            format!("# {}\n\n", today)
        };

        let new_content = format!("{}\n{}", existing, content);
        
        fs::write(&today_file, new_content).await
            .with_context(|| format!("写入今天的 memory 失败: {}", today_file.display()))?;

        debug!("已追加内容到今天的 memory: {}", today_file.display());
        Ok(())
    }

    /// 读取长期记忆 (MEMORY.md)
    pub async fn read_long_term(&self) -> Result<String> {
        if self.memory_file.exists() {
            fs::read_to_string(&self.memory_file).await
                .with_context(|| format!("读取长期记忆失败: {}", self.memory_file.display()))
        } else {
            Ok(String::new())
        }
    }

    /// 写入长期记忆 (MEMORY.md)
    pub async fn write_long_term(
        &self,
        content: impl AsRef<str>,
    ) -> Result<()> {
        let content = content.as_ref();
        
        fs::write(&self.memory_file, content).await
            .with_context(|| format!("写入长期记忆失败: {}", self.memory_file.display()))?;

        info!("已更新长期记忆: {}", self.memory_file.display());
        Ok(())
    }

    /// 获取对话历史文件路径
    fn get_conversation_file(&self, session_id: &str) -> PathBuf {
        self.conversations_dir.join(format!("{}.md", session_id))
    }

    /// 添加对话消息
    pub async fn add_message(
        &self,
        session_id: &str,
        role: &str,
        content: &str,
        _tool_calls: Option<&str>,
    ) -> Result<()> {
        let conv_file = self.get_conversation_file(session_id);
        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

        let entry = format!(
            "## {}\n**{}**: {}\n\n",
            timestamp, role, content
        );

        let existing = if conv_file.exists() {
            fs::read_to_string(&conv_file).await.unwrap_or_default()
        } else {
            // 新对话，添加标题
            format!("# Conversation: {}\n\n", session_id)
        };

        let new_content = format!("{}{}", existing, entry);
        
        fs::write(&conv_file, new_content).await
            .with_context(|| format!("写入对话历史失败: {}", conv_file.display()))?;

        debug!("已添加消息到对话历史: {} - {}", session_id, role);
        Ok(())
    }

    /// 获取对话历史
    pub async fn get_conversation(
        &self,
        session_id: &str,
        _limit: i64,
    ) -> Result<Vec<ConversationMessage>> {
        let conv_file = self.get_conversation_file(session_id);

        if !conv_file.exists() {
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(&conv_file).await
            .with_context(|| format!("读取对话历史失败: {}", conv_file.display()))?;

        // 解析 Markdown 格式的对话历史
        let messages = parse_conversation_markdown(&content, session_id);
        
        Ok(messages)
    }

    /// 保存记忆（简化实现）
    pub async fn save_memory(
        &self,
        key: &str,
        value: &str,
        category: Option<&str>,
        _importance: i32,
    ) -> Result<()> {
        let mut content = self.read_long_term().await?;
        
        // 如果文件为空，初始化基本结构
        if content.is_empty() {
            content = "# Long-term Memory\n\n".to_string();
        }

        let category_display = category.unwrap_or("General");
        let section_header = format!("## {}", category_display);
        let entry = format!("- **{}**: {}", key, value);

        // 简单实现：直接追加到文件末尾
        if !content.contains(&section_header) {
            content.push_str(&format!("\n{}\n\n{}\n", section_header, entry));
        } else {
            // 在现有分类下追加
            content.push_str(&format!("{}\n", entry));
        }

        self.write_long_term(&content).await?;
        info!("已保存记忆: {} = {}", key, value);
        Ok(())
    }

    /// 获取记忆
    pub async fn get_memory(
        &self,
        key: &str,
    ) -> Result<Option<Memory>> {
        let content = self.read_long_term().await?;
        
        // 简单实现：在 Markdown 中搜索键
        for line in content.lines() {
            if line.contains(&format!("**{}**:", key)) {
                // 解析值
                if let Some(value) = line.split(':').nth(1) {
                    return Ok(Some(Memory {
                        key: key.to_string(),
                        value: value.trim().to_string(),
                        category: None,
                        importance: 0,
                        created_at: Utc::now(),
                        updated_at: Utc::now(),
                    }));
                }
            }
        }
        
        Ok(None)
    }

    /// 搜索记忆
    pub async fn search_memories(
        &self,
        query: &str,
        _limit: i64,
    ) -> Result<Vec<Memory>> {
        let content = self.read_long_term().await?;
        let mut results = Vec::new();
        
        for line in content.lines() {
            if line.to_lowercase().contains(&query.to_lowercase()) {
                // 尝试解析为记忆条目
                if line.starts_with("- **") {
                    if let Some(key_end) = line.find("**:") {
                        let key = line[4..key_end].to_string();
                        let value = line[key_end + 3..].trim().to_string();
                        results.push(Memory {
                            key,
                            value,
                            category: None,
                            importance: 0,
                            created_at: Utc::now(),
                            updated_at: Utc::now(),
                        });
                    }
                }
            }
        }
        
        Ok(results)
    }

    /// 删除记忆
    pub async fn delete_memory(
        &self,
        key: &str,
    ) -> Result<()> {
        let content = self.read_long_term().await?;
        let mut new_content = String::new();
        
        for line in content.lines() {
            if !line.contains(&format!("**{}**:", key)) {
                new_content.push_str(line);
                new_content.push('\n');
            }
        }
        
        self.write_long_term(new_content).await?;
        info!("已删除记忆: {}", key);
        
        Ok(())
    }

    /// 获取所有会话 ID
    pub async fn list_sessions(&self,
    ) -> Result<Vec<String>> {
        let mut sessions = Vec::new();
        
        let mut entries = fs::read_dir(&self.conversations_dir).await
            .with_context(|| "读取对话目录失败")?;
        
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().map(|e| e == "md").unwrap_or(false) {
                if let Some(stem) = path.file_stem() {
                    sessions.push(stem.to_string_lossy().to_string());
                }
            }
        }
        
        Ok(sessions)
    }

    /// 获取 memory 目录路径
    pub fn memory_dir(&self) -> &Path {
        &self.memory_dir
    }

    /// 获取工作区路径
    pub fn workspace(&self) -> &Path {
        &self.workspace
    }
}

/// 解析对话历史 Markdown
fn parse_conversation_markdown(content: &str, session_id: &str) -> Vec<ConversationMessage> {
    let mut messages = Vec::new();
    let mut current_timestamp = Utc::now();
    
    for line in content.lines() {
        // 解析时间戳行: ## 2026-02-07 12:30:00
        if line.starts_with("## ") {
            let timestamp_str = &line[3..];
            if let Ok(dt) = DateTime::parse_from_str(timestamp_str, "%Y-%m-%d %H:%M:%S") {
                current_timestamp = dt.with_timezone(&Utc);
            }
        }
        // 解析消息行: **User**: Hello
        else if line.starts_with("**") {
            if let Some(end_idx) = line.find("**:") {
                let role = line[2..end_idx].to_string();
                let content = line[end_idx + 3..].to_string();
                
                messages.push(ConversationMessage {
                    id: messages.len() as i64,
                    session_id: session_id.to_string(),
                    role: role.to_lowercase(),
                    content,
                    tool_calls: None,
                    created_at: current_timestamp,
                });
            }
        }
    }
    
    messages
}

/// 对话消息
#[derive(Debug, Clone)]
pub struct ConversationMessage {
    pub id: i64,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub tool_calls: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// 记忆条目
#[derive(Debug, Clone)]
pub struct Memory {
    pub key: String,
    pub value: String,
    pub category: Option<String>,
    pub importance: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_memory_store() {
        let temp_dir = TempDir::new().unwrap();
        let store = MemoryStore::new(temp_dir.path()).await.unwrap();

        // 测试今天的文件路径
        let today_file = store.get_today_file();
        assert!(today_file.to_string_lossy().ends_with(".md"));

        // 测试追加和读取
        store.append_today("Test content").await.unwrap();
        let content = store.read_today().await.unwrap();
        assert!(content.contains("Test content"));

        // 测试长期记忆
        store.write_long_term("# Test Memory\n").await.unwrap();
        let long_term = store.read_long_term().await.unwrap();
        assert_eq!(long_term, "# Test Memory\n");
    }

    #[tokio::test]
    async fn test_conversation() {
        let temp_dir = TempDir::new().unwrap();
        let store = MemoryStore::new(temp_dir.path()).await.unwrap();

        // 添加消息
        store.add_message("test-session", "user", "Hello", None).await.unwrap();
        store.add_message("test-session", "assistant", "Hi!", None).await.unwrap();

        // 读取对话
        let messages = store.get_conversation("test-session", 10).await.unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[0].content.trim(), "Hello");
    }
}
