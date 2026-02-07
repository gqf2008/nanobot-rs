//! 模块集成测试

#[cfg(test)]
mod tests {
    // 测试 Cron 模块
    #[tokio::test]
    async fn test_cron_module() {
        use crate::cron::{Job, Scheduler, JobType, JobStatus};
        use chrono::Utc;

        // 创建内存调度器
        let _scheduler = Scheduler::new().await.unwrap();

        // 创建 Cron 任务
        let job = Job::new_cron("test_cron", "0 * * * * *", "test_handler")
            .with_description("测试 Cron 任务");
        
        assert_eq!(job.name, "test_cron");
        assert_eq!(job.handler, "test_handler");
        assert!(matches!(job.job_type, JobType::Cron { .. }));
        assert_eq!(job.status, JobStatus::Pending);

        // 创建间隔任务
        let interval_job = Job::new_interval("test_interval", 60, "test_handler");
        assert!(matches!(interval_job.job_type, JobType::Interval { seconds: 60 }));

        // 创建一次性任务
        let once_job = Job::new_once("test_once", Utc::now(), "test_handler");
        assert!(matches!(once_job.job_type, JobType::Once { .. }));
    }

    // 测试 Event Bus 模块 - 使用结构体实现处理器
    #[tokio::test]
    async fn test_bus_module() {
        use crate::bus::{EventBus, EventHandler, SystemEvent};
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        struct TestHandler {
            counter: Arc<AtomicUsize>,
        }

        #[async_trait::async_trait]
        impl EventHandler<SystemEvent> for TestHandler {
            async fn handle(&self, _event: &SystemEvent) {
                self.counter.fetch_add(1, Ordering::SeqCst);
            }
        }

        let bus = EventBus::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let handler = TestHandler { counter: counter.clone() };

        // 订阅事件
        bus.subscribe(handler).await;

        // 发布事件
        bus.publish(SystemEvent {
            event_type: "test".to_string(),
            data: serde_json::json!({"test": true}),
            timestamp: chrono::Utc::now(),
        }).unwrap();

        // 给一点时间让事件处理
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    }

    // 测试 Session 模块
    #[tokio::test]
    async fn test_session_module() {
        use crate::session::{SessionManager, SessionState};

        // 创建内存会话管理器
        let manager = SessionManager::new();

        // 创建会话
        let session = manager.create_session("telegram", "123456").await.unwrap();

        {
            let s = session.read().await;
            assert_eq!(s.metadata.channel, "telegram");
            assert_eq!(s.metadata.channel_id, "123456");
            assert_eq!(s.state, SessionState::Active);
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

    // 测试 Channel 配置
    #[test]
    fn test_channel_configs() {
        use crate::config::{DiscordConfig, FeishuConfig};

        let discord = DiscordConfig {
            prefix: "!".to_string(),
            enable_slash_commands: true,
            ..Default::default()
        };
        assert_eq!(discord.prefix, "!");
        assert!(discord.enable_slash_commands);

        let feishu = FeishuConfig {
            verify_signature: true,
            ..Default::default()
        };
        assert!(feishu.verify_signature);
    }
}
