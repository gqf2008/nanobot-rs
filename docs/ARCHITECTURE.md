# Nanobot-RS 架构设计文档

## 1. 架构概述

### 1.1 核心原则
- **Actor 模型**: 基于消息通信，持有 ActorRef 地址
- **职责分离**: Gateway、Supervisor、Planner、Executor 各司其职
- **总线驱动**: 通过 Event Bus 进行消息路由和分发

### 1.2 核心组件

```rust
// 核心抽象 - Agent trait
pub trait Agent: Send + Sync {
    fn id(&self) -> AgentId;
    async fn handle(&self, msg: Message, ctx: Context) -> Result<Response>;
}

// 运行时 - 注入 Agent 能力
pub struct AgentRuntime {
    bus: AgentBus,
    registry: AgentRegistry,
    memory: MemoryManager,
    tools: ToolRegistry,
}

// 执行者 - 具体干活的
pub struct AgentRunner<A: Agent> {
    agent: A,
    runtime: Arc<AgentRuntime>,
    session: Session,
}
```

## 2. Event Bus（总线）设计

### 2.1 核心结构

```rust
pub struct AgentBus {
    /// 主题订阅表
    subscribers: RwLock<HashMap<Topic, Vec<ActorRef<dyn Handler>>>>,
    /// 消息路由器
    router: MessageRouter,
    /// 消息队列（用于持久化）
    queue: MessageQueue,
}

/// 消息类型
pub enum Message {
    Task(Task),           // 任务分发
    Event(Event),         // 事件通知
    Query(Query),         // 查询请求
    Command(Command),     // 控制命令
    Result(Result),       // 结果返回
}

/// 消息处理器 trait
#[async_trait]
pub trait Handler: Send + Sync {
    async fn handle(&self, msg: Message, ctx: Context) -> Result<Response>;
}
```

### 2.2 使用模式

```rust
// 发布消息
bus.publish(Topic::Tasks, Message::Task(task)).await?;

// 订阅主题
bus.subscribe(Topic::Tasks, planner_ref).await?;

// 请求-响应模式
let result = bus.ask(executor_ref, Message::Task(task)).await?;
```

## 3. Agent 层级设计

### 3.1 Gateway（统一入口）

```rust
pub struct Gateway {
    bus: AgentBusRef,
    channels: Vec<Box<dyn Channel>>,
}

impl Gateway {
    pub async fn on_user_message(&self, msg: UserMessage) {
        // 1. 解析用户意图
        let intent = self.parse_intent(&msg.text).await;
        
        // 2. 发布到总线
        self.bus.publish(Message::Task(Task {
            intent,
            source: msg.source,
            context: msg.context,
        })).await;
    }
}
```

### 3.2 Supervisor（监督者）

```rust
pub struct Supervisor {
    bus: AgentBusRef,
    agents: HashMap<AgentId, AgentHealth>,
}

impl Supervisor {
    pub async fn monitor(&self) {
        // 监控 Agent 健康状态
        // 自动重启失败的 Agent
        // 负载均衡调度
    }
}
```

### 3.3 Planner（规划者）- 基于行为树

```rust
pub struct Planner {
    bus: AgentBusRef,
    behavior_tree: BehaviorTree,
}

impl Handler for Planner {
    async fn handle(&self, msg: Message, ctx: Context) -> Result<Response> {
        if let Message::Task(task) = msg {
            // 使用行为树分解任务
            let plan = self.behavior_tree.execute(task).await;
            
            // 发布执行计划
            self.bus.publish(Message::Event(Event::PlanReady(plan))).await?;
        }
        Ok(Response::Ack)
    }
}

// 行为树节点
pub enum BehaviorNode {
    Sequence(Vec<BehaviorNode>),      // 顺序执行
    Selector(Vec<BehaviorNode>),      // 选择执行
    Parallel(Vec<BehaviorNode>),      // 并行执行
    Action(Box<dyn Action>),          // 具体动作
    Condition(Box<dyn Condition>),    // 条件判断
    Decorator(Box<dyn Decorator>),    // 装饰器
}
```

### 3.4 Executor（执行者）

```rust
pub struct Executor {
    bus: AgentBusRef,
    tools: ToolRegistry,
}

impl Handler for Executor {
    async fn handle(&self, msg: Message, ctx: Context) -> Result<Response> {
        if let Message::Command(cmd) = msg {
            // 执行具体工具
            let result = match cmd {
                Command::Shell(args) => self.tools.shell.execute(args).await,
                Command::ReadFile(path) => self.tools.file.read(path).await,
                Command::WriteFile(path, content) => self.tools.file.write(path, content).await,
                Command::Grep(pattern, path) => self.tools.grep.search(pattern, path).await,
            }?;
            
            return Ok(Response::Result(result));
        }
        Ok(Response::Ack)
    }
}
```

## 4. 记忆系统设计

### 4.1 分层架构

```
记忆系统
├── 核心记忆 (Markdown)
│   ├── AGENTS.md      - Agent 行为指南
│   ├── MEMORY.md      - 长期知识库
│   ├── PROJECTS.md    - 项目状态
│   └── sessions/      - 持久化会话
│       └── {session_id}.md
│
└── 临时会话 (JSON)
    ├── active_session_{id}.json  - 活跃会话数据
    └── 规则:
        - 会话中实时更新
        - 超过上下文限制时提取关键信息 → Markdown
        - 用户要求保存时 → Markdown
        - 保存后清空 JSON
```

### 4.2 记忆管理器

```rust
pub struct MemoryManager {
    workspace: PathBuf,
    sessions_dir: PathBuf,
}

impl MemoryManager {
    /// 加载长期记忆
    pub async fn load_long_term(&self) -> Result<String>;
    
    /// 保存关键信息到 Markdown
    pub async fn save_to_markdown(&self, session_id: &str, summary: &str) -> Result<()>;
    
    /// 加载临时会话
    pub async fn load_session(&self, session_id: &str) -> Result<Vec<Message>>;
    
    /// 保存临时会话
    pub async fn save_session(&self, session_id: &str, messages: &[Message]) -> Result<()>;
    
    /// 清空临时会话
    pub async fn clear_session(&self, session_id: &str) -> Result<()>;
}
```

### 4.3 数据流

```rust
// 会话进行中
user_message → Agent → 生成回复 → 保存到 JSON (临时)

// 上下文满了
JSON (临时) → 提取关键信息 → 追加到 Markdown (长期) → 清空 JSON

// 用户要求保存
JSON (临时) → 总结 → 保存为 {session_id}.md → 清空 JSON
```

## 5. 工具系统设计

### 5.1 内置工具

```rust
pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl ToolRegistry {
    pub fn default() -> Self {
        let mut tools = HashMap::new();
        tools.insert("shell".to_string(), Box::new(ShellTool::new()));
        tools.insert("file_read".to_string(), Box::new(FileReadTool::new()));
        tools.insert("file_write".to_string(), Box::new(FileWriteTool::new()));
        tools.insert("grep".to_string(), Box::new(GrepTool::new()));
        tools.insert("list_dir".to_string(), Box::new(ListDirTool::new()));
        Self { tools }
    }
}

/// Tool trait
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> Value;
    async fn execute(&self, args: Value) -> Result<Value>;
}
```

### 5.2 Shell 工具

```rust
pub struct ShellTool {
    whitelist: Vec<String>,
}

impl Tool for ShellTool {
    fn name(&self) -> &str { "shell" }
    
    async fn execute(&self, args: Value) -> Result<Value> {
        let cmd = args["command"].as_str().unwrap();
        
        // 检查白名单
        if !self.is_allowed(cmd) {
            return Err("命令不在白名单中".into());
        }
        
        // 执行命令
        let output = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .output()
            .await?;
            
        Ok(json!({
            "stdout": String::from_utf8_lossy(&output.stdout),
            "stderr": String::from_utf8_lossy(&output.stderr),
            "code": output.status.code()
        }))
    }
}
```

### 5.3 文件工具

```rust
pub struct FileTool {
    allowed_paths: Vec<PathBuf>,
}

impl FileTool {
    fn check_path(&self, path: &Path) -> Result<()> {
        if !self.allowed_paths.iter().any(|p| path.starts_with(p)) {
            return Err("路径不在允许范围内".into());
        }
        Ok(())
    }
}

#[async_trait]
pub trait FileToolExt: Tool {
    async fn read(&self, path: &Path) -> Result<String>;
    async fn write(&self, path: &Path, content: &str) -> Result<()>;
    async fn grep(&self, pattern: &str, path: &Path) -> Result<Vec<String>>;
}
```

## 6. 消息流示例

### 6.1 用户发送消息

```
User → Telegram Channel → Gateway
                              ↓
                       Gateway 解析意图
                              ↓
                       Bus.publish(Task { intent, context })
                              ↓
                       Planner 订阅 Task
                              ↓
                       BehaviorTree 分解任务
                              ↓
                       Bus.publish(Plan { steps })
                              ↓
                       Executor 订阅 Plan
                              ↓
                       执行工具调用
                              ↓
                       Bus.publish(Result)
                              ↓
                       Gateway 接收结果
                              ↓
                       Telegram Channel → User
```

### 6.2 多 Agent 协作

```
Planner 分解任务:
- 步骤1: 读取文件 → 发布到 Bus
- 步骤2: 分析内容 → 发布到 Bus
- 步骤3: 生成报告 → 发布到 Bus

多个 Executor 并行处理:
- Executor A 处理步骤1
- Executor B 处理步骤2
- Executor C 处理步骤3

Supervisor 监控进度，协调资源
```

## 7. 实现路线图

### Phase 1: 核心骨架
- [ ] AgentBus 实现
- [ ] Agent trait 定义
- [ ] AgentRuntime 基础

### Phase 2: 基础 Agent
- [ ] Gateway Agent
- [ ] Executor Agent（工具执行）
- [ ] 迁移现有功能（Telegram、DeepSeek）

### Phase 3: 高级功能
- [ ] Supervisor Agent
- [ ] Planner Agent（行为树）
- [ ] 记忆系统（Markdown + JSON）

### Phase 4: 多 Agent 协作
- [ ] Agent 间通信协议
- [ ] 分布式部署支持
- [ ] 监控和日志

## 8. 技术选型

| 组件 | 选型 | 理由 |
|------|------|------|
| Actor 框架 | Kameo | Rust 原生，Tokio 集成，轻量级 |
| 序列化 | serde + serde_json | 标准方案 |
| 配置 | toml | 可读性好，支持热加载 |
| 日志 | tracing | 结构化日志，OpenTelemetry 兼容 |
| 错误处理 | thiserror + anyhow | 标准错误处理 |

---

*设计文档 v1.0 - 2026-02-07*
