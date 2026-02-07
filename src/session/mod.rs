//! 会话管理模块
//!
//! 独立会话管理，支持多会话并发
//! 会话状态持久化，与会话 ID 关联的上下文

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqlitePoolOptions, Pool, Sqlite};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;
use uuid::Uuid;

/// 会话状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    /// 活跃状态
    Active,
    /// 空闲状态（一段时间无活动）
    Idle,
    /// 已暂停
    Paused,
    /// 已结束
    Ended,
}

/// 会话元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    /// 用户 ID（如果已知）
    pub user_id: Option<String>,
    /// 通道类型
    pub channel: String,
    /// 通道特定 ID（如 Telegram chat_id）
    pub channel_id: String,
    /// 额外属性
    pub properties: HashMap<String, String>,
}

impl Default for SessionMetadata {
    fn default() -> Self {
        Self {
            user_id: None,
            channel: "unknown".to_string(),
            channel_id: Uuid::new_v4().to_string(),
            properties: HashMap::new(),
        }
    }
}

/// 会话统计
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionStats {
    /// 消息数量
    pub message_count: u64,
    /// 用户消息数量
    pub user_message_count: u64,
    /// 助手消息数量
    pub assistant_message_count: u64,
    /// 工具调用次数
    pub tool_call_count: u64,
    /// 总令牌数（估算）
    pub total_tokens: u64,
}

/// 会话上下文
#[derive(Debug, Clone)]
pub struct SessionContext {
    /// 会话数据存储
    data: Arc<RwLock<HashMap<String, serde_json::Value>>>,
}

impl SessionContext {
    pub fn new() -> Self {
        Self {
            data: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 设置值
    pub async fn set<T: Serialize>(
        &self, key: &str, value: T) -> Result<()> {
        let json_value = serde_json::to_value(value)?;
        self.data.write().await.insert(key.to_string(), json_value);
        Ok(())
    }

    /// 获取值
    pub async fn get<T: for<'de> Deserialize<'de>>(&self, key: &str) -> Option<T> {
        let data = self.data.read().await;
        data.get(key).and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    /// 删除值
    pub async fn remove(&self, key: &str) -> Option<serde_json::Value> {
        self.data.write().await.remove(key)
    }

    /// 清空所有数据
    pub async fn clear(&self) {
        self.data.write().await.clear();
    }
}

impl Default for SessionContext {
    fn default() -> Self {
        Self::new()
    }
}

/// 会话
#[derive(Debug, Clone)]
pub struct Session {
    /// 会话唯一 ID
    pub id: String,
    /// 会话状态
    pub state: SessionState,
    /// 会话元数据
    pub metadata: SessionMetadata,
    /// 会话上下文
    pub context: SessionContext,
    /// 会话统计
    pub stats: SessionStats,
    /// 创建时间
    pub created_at: DateTime<Utc>,
    /// 最后活动时间
    pub last_activity: DateTime<Utc>,
    /// 结束时间
    pub ended_at: Option<DateTime<Utc>>,
}

impl Session {
    /// 创建新会话
    pub fn new(channel: impl Into<String>, channel_id: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            state: SessionState::Active,
            metadata: SessionMetadata {
                channel: channel.into(),
                channel_id: channel_id.into(),
                ..Default::default()
            },
            context: SessionContext::new(),
            stats: SessionStats::default(),
            created_at: now,
            last_activity: now,
            ended_at: None,
        }
    }

    /// 设置用户 ID
    pub fn with_user_id(mut self, user_id: impl Into<String>) -> Self {
        self.metadata.user_id = Some(user_id.into());
        self
    }

    /// 添加属性
    pub fn with_property(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.properties.insert(key.into(), value.into());
        self
    }

    /// 更新活动时间
    pub fn touch(&mut self) {
        self.last_activity = Utc::now();
    }

    /// 记录消息
    pub fn record_message(&mut self, is_user: bool) {
        self.stats.message_count += 1;
        if is_user {
            self.stats.user_message_count += 1;
        } else {
            self.stats.assistant_message_count += 1;
        }
        self.touch();
    }

    /// 记录工具调用
    pub fn record_tool_call(&mut self) {
        self.stats.tool_call_count += 1;
        self.touch();
    }

    /// 记录令牌使用
    pub fn record_tokens(&mut self, tokens: u64) {
        self.stats.total_tokens += tokens;
    }

    /// 暂停会话
    pub fn pause(&mut self) {
        self.state = SessionState::Paused;
        info!("会话 {} 已暂停", self.id);
    }

    /// 恢复会话
    pub fn resume(&mut self) {
        self.state = SessionState::Active;
        self.touch();
        info!("会话 {} 已恢复", self.id);
    }

    /// 结束会话
    pub fn end(&mut self, reason: impl Into<String>) {
        self.state = SessionState::Ended;
        self.ended_at = Some(Utc::now());
        info!("会话 {} 已结束: {}", self.id, reason.into());
    }

    /// 检查是否空闲
    pub fn is_idle(&self, timeout_secs: u64) -> bool {
        let elapsed = Utc::now().signed_duration_since(self.last_activity);
        elapsed.num_seconds() > timeout_secs as i64
    }

    /// 获取持续时间（秒）
    pub fn duration_secs(&self) -> i64 {
        let end = self.ended_at.unwrap_or_else(Utc::now);
        end.signed_duration_since(self.created_at).num_seconds()
    }
}

/// 会话管理器
pub struct SessionManager {
    /// 活跃会话
    sessions: Arc<RwLock<HashMap<String, Arc<RwLock<Session>>>>>,
    /// 数据库连接池
    pool: Option<Pool<Sqlite>>,
    /// 空闲超时（秒）
    idle_timeout: u64,
}

impl SessionManager {
    /// 创建内存模式的会话管理器
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            pool: None,
            idle_timeout: 3600, // 默认 1 小时
        })
    }

    /// 创建带持久化的会话管理器
    pub async fn with_db(db_path: &str) -> Result<Arc<Self>> {
        // 确保目录存在
        if let Some(parent) = std::path::Path::new(db_path).parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&format!("sqlite:{}", db_path))
            .await
            .context("连接数据库失败")?;

        let manager = Arc::new(Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            pool: Some(pool),
            idle_timeout: 3600,
        });

        // 初始化数据库
        manager.init_db().await?;

        Ok(manager)
    }

    /// 初始化数据库表
    async fn init_db(&self) -> Result<()> {
        if let Some(ref pool) = self.pool {
            sqlx::query(
                r#"
                CREATE TABLE IF NOT EXISTS sessions (
                    id TEXT PRIMARY KEY,
                    state TEXT NOT NULL,
                    user_id TEXT,
                    channel TEXT NOT NULL,
                    channel_id TEXT NOT NULL,
                    properties TEXT,
                    stats TEXT,
                    created_at TIMESTAMP NOT NULL,
                    last_activity TIMESTAMP NOT NULL,
                    ended_at TIMESTAMP
                )
                "#
            )
            .execute(pool)
            .await?;

            sqlx::query(
                "CREATE INDEX IF NOT EXISTS idx_sessions_user ON sessions(user_id)"
            )
            .execute(pool)
            .await?;

            sqlx::query(
                "CREATE INDEX IF NOT EXISTS idx_sessions_channel ON sessions(channel, channel_id)"
            )
            .execute(pool)
            .await?;

            // 会话上下文表
            sqlx::query(
                r#"
                CREATE TABLE IF NOT EXISTS session_context (
                    session_id TEXT NOT NULL,
                    key TEXT NOT NULL,
                    value TEXT NOT NULL,
                    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                    PRIMARY KEY (session_id, key)
                )
                "#
            )
            .execute(pool)
            .await?;
        }
        Ok(())
    }

    /// 创建新会话
    pub async fn create_session(
        &self,
        channel: impl Into<String>,
        channel_id: impl Into<String>,
    ) -> Result<Arc<RwLock<Session>>> {
        let session = Session::new(channel, channel_id);
        let session_id = session.id.clone();
        let session_arc = Arc::new(RwLock::new(session));

        // 保存到内存
        self.sessions
            .write()
            .await
            .insert(session_id.clone(), session_arc.clone());

        // 持久化
        if let Some(ref pool) = self.pool {
            let session_guard = session_arc.read().await;
            self.save_session_to_db(&*session_guard, pool).await?;
        }

        info!("创建会话: {}", session_id);
        Ok(session_arc)
    }

    /// 获取会话
    pub async fn get_session(&self, session_id: &str) -> Option<Arc<RwLock<Session>>> {
        self.sessions.read().await.get(session_id).cloned()
    }

    /// 通过通道 ID 查找会话
    pub async fn find_by_channel(
        &self,
        channel: &str,
        channel_id: &str,
    ) -> Vec<Arc<RwLock<Session>>> {
        let sessions = self.sessions.read().await;
        sessions
            .values()
            .filter(|s| {
                let s = s.blocking_read();
                s.metadata.channel == channel && s.metadata.channel_id == channel_id
            })
            .cloned()
            .collect()
    }

    /// 列出所有活跃会话
    pub async fn list_active_sessions(&self) -> Vec<Arc<RwLock<Session>>> {
        let sessions = self.sessions.read().await;
        sessions
            .values()
            .filter(|s| s.blocking_read().state == SessionState::Active)
            .cloned()
            .collect()
    }

    /// 结束会话
    pub async fn end_session(&self, session_id: &str, reason: impl Into<String>) -> Result<()> {
        let reason = reason.into();

        if let Some(session) = self.sessions.read().await.get(session_id) {
            let mut s = session.write().await;
            s.end(reason.clone());

            // 持久化
            if let Some(ref pool) = self.pool {
                self.save_session_to_db(&s, pool).await?;
            }
        }

        Ok(())
    }

    /// 清理空闲会话
    pub async fn cleanup_idle_sessions(&self) -> Result<usize> {
        let mut count = 0;
        let sessions = self.sessions.read().await;

        for (id, session) in sessions.iter() {
            let s = session.read().await;
            if s.state == SessionState::Active && s.is_idle(self.idle_timeout) {
                drop(s);
                self.end_session(id, "空闲超时").await?;
                count += 1;
            }
        }

        if count > 0 {
            info!("清理了 {} 个空闲会话", count);
        }

        Ok(count)
    }

    /// 获取会话统计
    pub async fn get_global_stats(&self) -> (usize, SessionStats) {
        let sessions = self.sessions.read().await;
        let total = sessions.len();
        let mut global_stats = SessionStats::default();

        for session in sessions.values() {
            let s = session.read().await;
            global_stats.message_count += s.stats.message_count;
            global_stats.user_message_count += s.stats.user_message_count;
            global_stats.assistant_message_count += s.stats.assistant_message_count;
            global_stats.tool_call_count += s.stats.tool_call_count;
            global_stats.total_tokens += s.stats.total_tokens;
        }

        (total, global_stats)
    }

    /// 保存会话到数据库
    async fn save_session_to_db(
        &self,
        session: &Session,
        pool: &Pool<Sqlite>,
    ) -> Result<()> {
        let properties = serde_json::to_string(&session.metadata.properties)?;
        let stats = serde_json::to_string(&session.stats)?;

        sqlx::query(
            r#"
            INSERT OR REPLACE INTO sessions 
            (id, state, user_id, channel, channel_id, properties, stats,
             created_at, last_activity, ended_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            "#
        )
        .bind(&session.id)
        .bind(match session.state {
            SessionState::Active => "active",
            SessionState::Idle => "idle",
            SessionState::Paused => "paused",
            SessionState::Ended => "ended",
        })
        .bind(&session.metadata.user_id)
        .bind(&session.metadata.channel)
        .bind(&session.metadata.channel_id)
        .bind(properties)
        .bind(stats)
        .bind(session.created_at)
        .bind(session.last_activity)
        .bind(session.ended_at)
        .execute(pool)
        .await?;

        Ok(())
    }

    /// 设置空闲超时
    pub fn with_idle_timeout(mut self, seconds: u64) -> Self {
        self.idle_timeout = seconds;
        self
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            pool: None,
            idle_timeout: 3600,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_session_creation() {
        let session = Session::new("telegram", "123456");
        assert_eq!(session.metadata.channel, "telegram");
        assert_eq!(session.metadata.channel_id, "123456");
        assert_eq!(session.state, SessionState::Active);
    }

    #[tokio::test]
    async fn test_session_stats() {
        let mut session = Session::new("test", "1");

        session.record_message(true);
        session.record_message(false);
        session.record_tool_call();
        session.record_tokens(100);

        assert_eq!(session.stats.message_count, 2);
        assert_eq!(session.stats.user_message_count, 1);
        assert_eq!(session.stats.assistant_message_count, 1);
        assert_eq!(session.stats.tool_call_count, 1);
        assert_eq!(session.stats.total_tokens, 100);
    }

    #[tokio::test]
    async fn test_session_manager() {
        let manager = SessionManager::new();

        // 创建会话
        let session = manager.create_session("telegram", "123").await.unwrap();

        // 记录活动
        {
            let mut s = session.write().await;
            s.record_message(true);
        }

        // 获取会话
        let session_id = session.read().await.id.clone();
        let retrieved = manager.get_session(&session_id).await;
        assert!(retrieved.is_some());

        // 结束会话
        manager.end_session(&session_id, "测试结束").await.unwrap();

        let s = session.read().await;
        assert_eq!(s.state, SessionState::Ended);
    }
}
