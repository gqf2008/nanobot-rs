//! 事件总线模块 - 发布/订阅模式实现
//!
//! 提供类型安全的事件系统，支持异步事件处理
//! 用于解耦模块间通信

use anyhow::Result;
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, info, warn};

/// 事件 trait
pub trait Event: Send + Sync + Debug + 'static {
    /// 事件名称
    fn event_name(&self) -> &'static str;
}

/// 事件处理器 trait
#[async_trait::async_trait]
pub trait EventHandler<E: Event>: Send + Sync {
    /// 处理事件
    async fn handle(&self, event: &E);
}

/// 类型擦除的事件处理器
#[async_trait::async_trait]
trait ErasedEventHandler: Send + Sync {
    async fn handle_erased(&self, event: &(dyn Any + Send + Sync));
}

/// 类型擦除包装器
struct HandlerWrapper<E: Event, H: EventHandler<E>> {
    handler: H,
    _phantom: std::marker::PhantomData<E>,
}

#[async_trait::async_trait]
impl<E, H> ErasedEventHandler for HandlerWrapper<E, H>
where
    E: Event,
    H: EventHandler<E>,
{
    async fn handle_erased(&self, event: &(dyn Any + Send + Sync)) {
        if let Some(typed_event) = event.downcast_ref::<E>() {
            self.handler.handle(typed_event).await;
        } else {
            warn!("事件类型不匹配");
        }
    }
}

/// 订阅者信息
struct Subscriber {
    id: String,
    handler: Arc<dyn ErasedEventHandler>,
}

/// 事件总线
pub struct EventBus {
    /// 订阅者映射：事件类型 -> 订阅者列表
    subscribers: Arc<RwLock<HashMap<TypeId, Vec<Subscriber>>>>,
    /// 事件通道发送端
    sender: mpsc::UnboundedSender<Box<dyn Any + Send + Sync>>,
    /// 事件通道接收端（存储在 Option 中以便 take）
    receiver: Arc<RwLock<Option<mpsc::UnboundedReceiver<Box<dyn Any + Send + Sync>>>>>,
}

impl EventBus {
    /// 创建新的事件总线
    pub fn new() -> Arc<Self> {
        let (sender, receiver) = mpsc::unbounded_channel();

        Arc::new(Self {
            subscribers: Arc::new(RwLock::new(HashMap::new())),
            sender,
            receiver: Arc::new(RwLock::new(Some(receiver))),
        })
    }

    /// 订阅事件
    pub async fn subscribe<E, H>(&self, handler: H) -> String
    where
        E: Event,
        H: EventHandler<E> + 'static,
    {
        let subscriber_id = uuid::Uuid::new_v4().to_string();

        let wrapper = HandlerWrapper {
            handler,
            _phantom: std::marker::PhantomData,
        };

        let subscriber = Subscriber {
            id: subscriber_id.clone(),
            handler: Arc::new(wrapper),
        };

        let mut subs = self.subscribers.write().await;
        subs.entry(TypeId::of::<E>())
            .or_insert_with(Vec::new)
            .push(subscriber);

        info!("订阅事件 {}: {}", std::any::type_name::<E>(), subscriber_id);
        subscriber_id
    }

    /// 取消订阅
    pub async fn unsubscribe<E>(&self, subscriber_id: &str) -> Result<()>
    where
        E: Event,
    {
        let mut subs = self.subscribers.write().await;
        if let Some(handlers) = subs.get_mut(&TypeId::of::<E>()) {
            handlers.retain(|s| s.id != subscriber_id);
            info!("取消订阅事件 {}: {}", std::any::type_name::<E>(), subscriber_id);
        }
        Ok(())
    }

    /// 发布事件
    pub fn publish<E>(&self, event: E) -> Result<()>
    where
        E: Event,
    {
        debug!("发布事件: {}", event.event_name());
        self.sender
            .send(Box::new(event))
            .map_err(|_| anyhow::anyhow!("事件总线已关闭"))?;
        Ok(())
    }

    /// 启动事件分发循环
    pub async fn start(self: Arc<Self>) -> Result<()> {
        let mut receiver = self
            .receiver
            .write()
            .await
            .take()
            .ok_or_else(|| anyhow::anyhow!("事件总线已启动"))?;

        info!("启动事件总线...");

        while let Some(event) = receiver.recv().await {
            let subs = self.subscribers.clone();

            // 获取事件类型 ID
            let type_id = (*event).type_id();

            tokio::spawn(async move {
                let subscribers = subs.read().await;
                if let Some(handlers) = subscribers.get(&type_id) {
                    for subscriber in handlers {
                        let handler = subscriber.handler.clone();
                        let event_ref: &(dyn Any + Send + Sync) = &*event;
                        handler.handle_erased(event_ref).await;
                    }
                }
            });
        }

        info!("事件总线已停止");
        Ok(())
    }
}

impl Default for EventBus {
    fn default() -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();

        Self {
            subscribers: Arc::new(RwLock::new(HashMap::new())),
            sender,
            receiver: Arc::new(RwLock::new(Some(receiver))),
        }
    }
}

// ============== 预定义事件类型 ==============

/// Agent 消息事件
#[derive(Debug, Clone)]
pub struct AgentMessageEvent {
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl Event for AgentMessageEvent {
    fn event_name(&self) -> &'static str {
        "agent.message"
    }
}

/// 工具调用事件
#[derive(Debug, Clone)]
pub struct ToolCallEvent {
    pub session_id: String,
    pub tool_name: String,
    pub args: serde_json::Value,
    pub result: Option<String>,
    pub success: bool,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl Event for ToolCallEvent {
    fn event_name(&self) -> &'static str {
        "tool.call"
    }
}

/// 会话创建事件
#[derive(Debug, Clone)]
pub struct SessionCreatedEvent {
    pub session_id: String,
    pub channel: String,
    pub user_id: Option<String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl Event for SessionCreatedEvent {
    fn event_name(&self) -> &'static str {
        "session.created"
    }
}

/// 会话结束事件
#[derive(Debug, Clone)]
pub struct SessionEndedEvent {
    pub session_id: String,
    pub reason: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl Event for SessionEndedEvent {
    fn event_name(&self) -> &'static str {
        "session.ended"
    }
}

/// 系统事件
#[derive(Debug, Clone)]
pub struct SystemEvent {
    pub event_type: String,
    pub data: serde_json::Value,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl Event for SystemEvent {
    fn event_name(&self) -> &'static str {
        "system"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone)]
    struct TestEvent {
        message: String,
    }

    impl Event for TestEvent {
        fn event_name(&self) -> &'static str {
            "test"
        }
    }

    struct TestHandler {
        received: Arc<RwLock<Vec<String>>>,
    }

    #[async_trait::async_trait]
    impl EventHandler<TestEvent> for TestHandler {
        async fn handle(&self, event: &TestEvent) {
            self.received.write().await.push(event.message.clone());
        }
    }

    #[tokio::test]
    async fn test_event_bus() {
        let bus = EventBus::new();
        let received = Arc::new(RwLock::new(Vec::new()));

        let handler = TestHandler {
            received: received.clone(),
        };

        // 订阅事件
        let _sub_id = bus.subscribe(handler).await;

        // 启动事件总线
        let bus_clone = bus.clone();
        tokio::spawn(async move {
            bus_clone.start().await.unwrap();
        });

        // 发布事件
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        bus.publish(TestEvent {
            message: "Hello".to_string(),
        })
        .unwrap();

        // 等待处理
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // 验证
        let msgs = received.read().await;
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0], "Hello");
    }
}
