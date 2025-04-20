//! 任务调度模块，负责管理定时任务

use std::sync::Arc;
use tauri::AppHandle;
use tokio::sync::Mutex;
use tracing::info;

pub mod executor;

/// 任务调度器
#[derive(Clone)]
pub struct Scheduler(Arc<Mutex<executor::SchedulerState>>);

impl Scheduler {
    /// 创建新的调度器
    pub fn new(app_handle: AppHandle) -> Self {
        let state = executor::SchedulerState::new(app_handle);
        Self(Arc::new(Mutex::new(state)))
    }
    
    /// 初始化并启动调度器
    pub async fn start(&self) -> Result<(), String> {
        let mut state = self.0.lock().await;
        state.initialize().await?;
        info!("任务调度器已启动");
        Ok(())
    }
} 