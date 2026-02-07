//! Cron 定时任务模块
//! 
//! 使用 tokio-cron-scheduler 实现定时任务调度
//! 支持 cron 表达式和时间间隔
//! 任务持久化到 SQLite

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqlitePoolOptions, Pool, Sqlite};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_cron_scheduler::{Job as CronJob, JobScheduler};
use tracing::{error, info, warn};
use uuid::Uuid;

/// 任务类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum JobType {
    /// Cron 表达式任务
    Cron { expression: String },
    /// 固定间隔任务（秒）
    Interval { seconds: u64 },
    /// 一次性任务
    Once { run_at: DateTime<Utc> },
}

/// 任务状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    /// 等待执行
    Pending,
    /// 运行中
    Running,
    /// 已完成（一次性任务）
    Completed,
    /// 已暂停
    Paused,
    /// 失败
    Failed,
}

/// 任务定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    /// 任务唯一 ID
    pub id: String,
    /// 任务名称
    pub name: String,
    /// 任务描述
    pub description: Option<String>,
    /// 任务类型
    pub job_type: JobType,
    /// 任务状态
    pub status: JobStatus,
    /// 任务处理器（存储为 JSON 字符串，实际执行时需要注册）
    pub handler: String,
    /// 处理器参数
    pub handler_args: Option<serde_json::Value>,
    /// 创建时间
    pub created_at: DateTime<Utc>,
    /// 最后执行时间
    pub last_run: Option<DateTime<Utc>>,
    /// 下次执行时间
    pub next_run: Option<DateTime<Utc>>,
    /// 执行次数
    pub run_count: i64,
    /// 最大执行次数（null 表示无限制）
    pub max_runs: Option<i64>,
    /// 是否持久化
    pub persistent: bool,
}

impl Job {
    /// 创建新的 Cron 任务
    pub fn new_cron(
        name: impl Into<String>,
        expression: impl Into<String>,
        handler: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            description: None,
            job_type: JobType::Cron {
                expression: expression.into(),
            },
            status: JobStatus::Pending,
            handler: handler.into(),
            handler_args: None,
            created_at: Utc::now(),
            last_run: None,
            next_run: None,
            run_count: 0,
            max_runs: None,
            persistent: true,
        }
    }

    /// 创建新的间隔任务
    pub fn new_interval(
        name: impl Into<String>,
        seconds: u64,
        handler: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            description: None,
            job_type: JobType::Interval { seconds },
            status: JobStatus::Pending,
            handler: handler.into(),
            handler_args: None,
            created_at: Utc::now(),
            last_run: None,
            next_run: None,
            run_count: 0,
            max_runs: None,
            persistent: true,
        }
    }

    /// 创建一次性任务
    pub fn new_once(
        name: impl Into<String>,
        run_at: DateTime<Utc>,
        handler: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            description: None,
            job_type: JobType::Once { run_at },
            status: JobStatus::Pending,
            handler: handler.into(),
            handler_args: None,
            created_at: Utc::now(),
            last_run: None,
            next_run: Some(run_at),
            run_count: 0,
            max_runs: Some(1),
            persistent: true,
        }
    }

    /// 设置任务描述
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// 设置处理器参数
    pub fn with_args(mut self, args: serde_json::Value) -> Self {
        self.handler_args = Some(args);
        self
    }

    /// 设置最大执行次数
    pub fn with_max_runs(mut self, max: i64) -> Self {
        self.max_runs = Some(max);
        self
    }

    /// 设置为非持久化
    pub fn non_persistent(mut self) -> Self {
        self.persistent = false;
        self
    }
}

/// 任务处理器 trait
#[async_trait::async_trait]
pub trait JobHandler: Send + Sync {
    /// 处理器名称
    fn name(&self) -> &str;
    
    /// 执行任务
    async fn execute(&self, job: &Job, args: Option<serde_json::Value>) -> Result<()>;
}

/// 任务处理器注册表
type HandlerRegistry = Arc<RwLock<std::collections::HashMap<String, Arc<dyn JobHandler>>>>;

/// 任务调度器
pub struct Scheduler {
    /// 内部调度器
    scheduler: Arc<RwLock<JobScheduler>>,
    /// 数据库连接池
    pool: Option<Pool<Sqlite>>,
    /// 处理器注册表
    handlers: HandlerRegistry,
    /// 已注册任务
    jobs: Arc<RwLock<std::collections::HashMap<String, Job>>>,
    /// 运行状态
    running: Arc<RwLock<bool>>,
}

impl Scheduler {
    /// 创建新的调度器（内存模式）
    pub async fn new() -> Result<Arc<Self>> {
        let scheduler = JobScheduler::new()
            .await
            .context("创建任务调度器失败")?;

        Ok(Arc::new(Self {
            scheduler: Arc::new(RwLock::new(scheduler)),
            pool: None,
            handlers: Arc::new(RwLock::new(std::collections::HashMap::new())),
            jobs: Arc::new(RwLock::new(std::collections::HashMap::new())),
            running: Arc::new(RwLock::new(false)),
        }))
    }

    /// 创建带持久化的调度器
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

        let scheduler = JobScheduler::new()
            .await
            .context("创建任务调度器失败")?;

        let instance = Arc::new(Self {
            scheduler: Arc::new(RwLock::new(scheduler)),
            pool: Some(pool),
            handlers: Arc::new(RwLock::new(std::collections::HashMap::new())),
            jobs: Arc::new(RwLock::new(std::collections::HashMap::new())),
            running: Arc::new(RwLock::new(false)),
        });

        // 初始化数据库表
        instance.init_db().await?;

        // 加载持久化任务
        instance.load_persistent_jobs().await?;

        Ok(instance)
    }

    /// 初始化数据库表
    async fn init_db(&self) -> Result<()> {
        if let Some(ref pool) = self.pool {
            sqlx::query(
                r#"
                CREATE TABLE IF NOT EXISTS cron_jobs (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    description TEXT,
                    job_type TEXT NOT NULL,
                    job_type_data TEXT NOT NULL,
                    status TEXT NOT NULL,
                    handler TEXT NOT NULL,
                    handler_args TEXT,
                    created_at TIMESTAMP NOT NULL,
                    last_run TIMESTAMP,
                    next_run TIMESTAMP,
                    run_count INTEGER DEFAULT 0,
                    max_runs INTEGER,
                    persistent BOOLEAN DEFAULT 1
                )
                "#
            )
            .execute(pool)
            .await?;

            sqlx::query(
                "CREATE INDEX IF NOT EXISTS idx_jobs_status ON cron_jobs(status)"
            )
            .execute(pool)
            .await?;
        }
        Ok(())
    }

    /// 加载持久化任务
    async fn load_persistent_jobs(&self) -> Result<()> {
        if let Some(ref pool) = self.pool {
            let rows: Vec<JobRow> = sqlx::query_as(
                "SELECT * FROM cron_jobs WHERE status != 'completed' AND persistent = 1"
            )
            .fetch_all(pool)
            .await?;

            for row in rows {
                if let Ok(job) = row.to_job() {
                    info!("加载持久化任务: {} ({})", job.name, job.id);
                    self.jobs.write().await.insert(job.id.clone(), job);
                }
            }
        }
        Ok(())
    }

    /// 保存任务到数据库
    async fn save_job(&self, job: &Job) -> Result<()> {
        if let Some(ref pool) = self.pool {
            if !job.persistent {
                return Ok(());
            }

            let job_type_data = serde_json::to_string(&job.job_type)?;

            sqlx::query(
                r#"
                INSERT OR REPLACE INTO cron_jobs 
                (id, name, description, job_type, job_type_data, status, handler, handler_args,
                 created_at, last_run, next_run, run_count, max_runs, persistent)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
                "#
            )
            .bind(&job.id)
            .bind(&job.name)
            .bind(&job.description)
            .bind(match &job.job_type {
                JobType::Cron { .. } => "cron",
                JobType::Interval { .. } => "interval",
                JobType::Once { .. } => "once",
            })
            .bind(job_type_data)
            .bind(match job.status {
                JobStatus::Pending => "pending",
                JobStatus::Running => "running",
                JobStatus::Completed => "completed",
                JobStatus::Paused => "paused",
                JobStatus::Failed => "failed",
            })
            .bind(&job.handler)
            .bind(job.handler_args.as_ref().map(|v| v.to_string()))
            .bind(job.created_at)
            .bind(job.last_run)
            .bind(job.next_run)
            .bind(job.run_count)
            .bind(job.max_runs)
            .bind(job.persistent)
            .execute(pool)
            .await?;
        }
        Ok(())
    }

    /// 注册任务处理器
    pub async fn register_handler(&self, handler: Arc<dyn JobHandler>) {
        let name = handler.name().to_string();
        info!("注册任务处理器: {}", name);
        self.handlers.write().await.insert(name, handler);
    }

    /// 添加任务
    pub async fn add_job(&self, job: Job) -> Result<String> {
        let job_id = job.id.clone();
        
        // 保存到内存
        self.jobs.write().await.insert(job_id.clone(), job.clone());
        
        // 持久化
        self.save_job(&job).await?;

        // 如果调度器已启动，立即调度任务
        if *self.running.read().await {
            self.schedule_job(&job).await?;
        }

        info!("添加任务: {} ({})", job.name, job_id);
        Ok(job_id)
    }

    /// 调度任务到内部调度器
    async fn schedule_job(&self, job: &Job) -> Result<()> {
        let handlers = self.handlers.clone();
        let jobs = self.jobs.clone();
        let pool = self.pool.clone();
        let job_id = job.id.clone();

        let cron_job = match &job.job_type {
            JobType::Cron { expression } => {
                let expression = expression.as_str();
                CronJob::new_async(expression, move |_uuid, _l| {
                    let handlers = handlers.clone();
                    let jobs = jobs.clone();
                    let pool = pool.clone();
                    let job_id = job_id.clone();
                    
                    Box::pin(async move {
                        if let Err(e) = Self::execute_job(&job_id, handlers, jobs, pool).await {
                            error!("任务执行失败 {}: {}", job_id, e);
                        }
                    })
                })?
            }
            JobType::Interval { seconds } => {
                let seconds = *seconds;
                CronJob::new_repeated_async(
                    std::time::Duration::from_secs(seconds),
                    move |_uuid, _l| {
                        let handlers = handlers.clone();
                        let jobs = jobs.clone();
                        let pool = pool.clone();
                        let job_id = job_id.clone();
                        
                        Box::pin(async move {
                            if let Err(e) = Self::execute_job(&job_id, handlers, jobs, pool).await {
                                error!("任务执行失败 {}: {}", job_id, e);
                            }
                        })
                    },
                )?
            }
            JobType::Once { run_at } => {
                let now = Utc::now();
                let duration = if run_at > &now {
                    run_at.signed_duration_since(now).to_std().unwrap_or_default()
                } else {
                    std::time::Duration::from_secs(0)
                };
                
                CronJob::new_one_shot_async(duration, move |_uuid, _l| {
                    let handlers = handlers.clone();
                    let jobs = jobs.clone();
                    let pool = pool.clone();
                    let job_id = job_id.clone();
                    
                    Box::pin(async move {
                        if let Err(e) = Self::execute_job(&job_id, handlers, jobs, pool).await {
                            error!("任务执行失败 {}: {}", job_id, e);
                        }
                    })
                })?
            }
        };

        self.scheduler.write().await.add(cron_job).await?;
        Ok(())
    }

    /// 执行任务
    async fn execute_job(
        job_id: &str,
        handlers: HandlerRegistry,
        jobs: Arc<RwLock<std::collections::HashMap<String, Job>>>,
        pool: Option<Pool<Sqlite>>,
    ) -> Result<()> {
        // 获取任务
        let job = {
            let jobs_guard = jobs.read().await;
            jobs_guard.get(job_id).cloned()
        };

        if let Some(mut job) = job {
            // 检查执行次数
            if let Some(max) = job.max_runs {
                if job.run_count >= max {
                    info!("任务 {} 已达到最大执行次数", job_id);
                    return Ok(());
                }
            }

            // 更新状态
            job.status = JobStatus::Running;
            job.last_run = Some(Utc::now());
            job.run_count += 1;

            // 查找处理器
            let handler = {
                let handlers_guard = handlers.read().await;
                handlers_guard.get(&job.handler).cloned()
            };

            if let Some(handler) = handler {
                info!("执行任务: {} ({})", job.name, job_id);
                
                match handler.execute(&job, job.handler_args.clone()).await {
                    Ok(_) => {
                        info!("任务执行成功: {} ({})", job.name, job_id);
                        
                        // 更新任务状态
                        if matches!(job.job_type, JobType::Once { .. }) {
                            job.status = JobStatus::Completed;
                        } else {
                            job.status = JobStatus::Pending;
                        }
                    }
                    Err(e) => {
                        error!("任务执行失败: {} ({}): {}", job.name, job_id, e);
                        job.status = JobStatus::Failed;
                    }
                }
            } else {
                warn!("未找到处理器: {} for job {}", job.handler, job_id);
                job.status = JobStatus::Failed;
            }

            // 更新内存中的任务
            jobs.write().await.insert(job_id.to_string(), job.clone());

            // 持久化
            if let Some(ref pool) = pool {
                let _ = sqlx::query(
                    "UPDATE cron_jobs SET status = ?1, last_run = ?2, run_count = ?3 WHERE id = ?4"
                )
                .bind(match job.status {
                    JobStatus::Pending => "pending",
                    JobStatus::Running => "running",
                    JobStatus::Completed => "completed",
                    JobStatus::Paused => "paused",
                    JobStatus::Failed => "failed",
                })
                .bind(job.last_run)
                .bind(job.run_count)
                .bind(&job.id)
                .execute(pool)
                .await;
            }
        }

        Ok(())
    }

    /// 启动调度器
    pub async fn start(&self) -> Result<()> {
        info!("启动任务调度器...");

        // 调度所有待执行的任务
        let jobs_to_schedule: Vec<Job> = {
            let jobs_guard = self.jobs.read().await;
            jobs_guard
                .values()
                .filter(|j| j.status == JobStatus::Pending)
                .cloned()
                .collect()
        };

        for job in jobs_to_schedule {
            if let Err(e) = self.schedule_job(&job).await {
                warn!("调度任务失败 {}: {}", job.id, e);
            }
        }

        // 启动调度器
        self.scheduler.write().await.start().await?;
        *self.running.write().await = true;

        info!("任务调度器已启动");
        Ok(())
    }

    /// 停止调度器
    pub async fn stop(&self) -> Result<()> {
        info!("停止任务调度器...");
        self.scheduler.write().await.shutdown().await?;
        *self.running.write().await = false;
        info!("任务调度器已停止");
        Ok(())
    }

    /// 获取任务
    pub async fn get_job(&self, job_id: &str) -> Option<Job> {
        self.jobs.read().await.get(job_id).cloned()
    }

    /// 列出所有任务
    pub async fn list_jobs(&self) -> Vec<Job> {
        self.jobs.read().await.values().cloned().collect()
    }

    /// 删除任务
    pub async fn remove_job(&self, job_id: &str) -> Result<()> {
        self.jobs.write().await.remove(job_id);
        
        if let Some(ref pool) = self.pool {
            sqlx::query("DELETE FROM cron_jobs WHERE id = ?1")
                .bind(job_id)
                .execute(pool)
                .await?;
        }

        info!("删除任务: {}", job_id);
        Ok(())
    }

    /// 暂停任务
    pub async fn pause_job(&self, job_id: &str) -> Result<()> {
        if let Some(job) = self.jobs.write().await.get_mut(job_id) {
            job.status = JobStatus::Paused;
            self.save_job(job).await?;
        }
        Ok(())
    }

    /// 恢复任务
    pub async fn resume_job(&self, job_id: &str) -> Result<()> {
        if let Some(job) = self.jobs.write().await.get_mut(job_id) {
            job.status = JobStatus::Pending;
            self.save_job(job).await?;
        }
        Ok(())
    }
}

/// 数据库行结构
#[derive(sqlx::FromRow)]
struct JobRow {
    id: String,
    name: String,
    description: Option<String>,
    job_type: String,
    job_type_data: String,
    status: String,
    handler: String,
    handler_args: Option<String>,
    created_at: DateTime<Utc>,
    last_run: Option<DateTime<Utc>>,
    next_run: Option<DateTime<Utc>>,
    run_count: i64,
    max_runs: Option<i64>,
    persistent: bool,
}

impl JobRow {
    fn to_job(&self) -> Result<Job> {
        let job_type: JobType = serde_json::from_str(&self.job_type_data)?;
        let status = match self.status.as_str() {
            "pending" => JobStatus::Pending,
            "running" => JobStatus::Running,
            "completed" => JobStatus::Completed,
            "paused" => JobStatus::Paused,
            "failed" => JobStatus::Failed,
            _ => JobStatus::Pending,
        };
        let handler_args = self.handler_args.as_ref()
            .and_then(|s| serde_json::from_str(s).ok());

        Ok(Job {
            id: self.id.clone(),
            name: self.name.clone(),
            description: self.description.clone(),
            job_type,
            status,
            handler: self.handler.clone(),
            handler_args,
            created_at: self.created_at,
            last_run: self.last_run,
            next_run: self.next_run,
            run_count: self.run_count,
            max_runs: self.max_runs,
            persistent: self.persistent,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestHandler;

    #[async_trait::async_trait]
    impl JobHandler for TestHandler {
        fn name(&self) -> &str {
            "test_handler"
        }

        async fn execute(&self, _job: &Job, _args: Option<serde_json::Value>) -> Result<()> {
            info!("测试处理器执行");
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_job_creation() {
        let job = Job::new_cron("test", "0 * * * * *", "test_handler")
            .with_description("测试任务")
            .with_max_runs(10);

        assert_eq!(job.name, "test");
        assert_eq!(job.handler, "test_handler");
        assert_eq!(job.max_runs, Some(10));
        assert!(job.description.is_some());
    }
}
