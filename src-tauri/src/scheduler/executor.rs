use crate::config;
use crate::database::Database;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tokio::task::JoinHandle;
use tokio::time::interval;
use tracing::{error, info};

/// 任务调度器状态
pub struct SchedulerState {
    /// 调度器任务句柄
    tasks: HashMap<String, JoinHandle<()>>,
    /// 应用句柄
    app_handle: Arc<AppHandle>,
    /// 是否已初始化
    initialized: bool,
}

impl SchedulerState {
    /// 创建新的调度器状态
    pub fn new(app_handle: AppHandle) -> Self {
        Self {
            tasks: HashMap::new(),
            app_handle: Arc::new(app_handle),
            initialized: false,
        }
    }

    /// 初始化调度器并注册默认任务
    pub async fn initialize(&mut self) -> Result<(), String> {
        if self.initialized {
            return Ok(());
        }

        // 从数据库加载任务配置
        self.load_task_configs().await?;
        
        // 注册任务
        self.register_tasks().await?;
        self.initialized = true;
        Ok(())
    }
    
    /// 加载任务配置
    async fn load_task_configs(&self) -> Result<(), String> {
        // 获取数据库实例
        let db = match self.app_handle.try_state::<Database>() {
            Some(db) => db,
            None => {
                let err = "无法获取数据库实例".to_string();
                error!("{}", err);
                return Err(err);
            }
        };
        
        // 尝试从数据库加载刷新间隔
        let dashboard_refresh_key = config::get_db_key("dashboard_refresh_interval");
        let dashboard_interval = db.inner().get_item(&dashboard_refresh_key).ok().flatten();
        
        // 如果数据库中没有设置值，则使用默认值并保存到数据库
        let scheduler_config = config::get_scheduler_config();
        
        if dashboard_interval.is_none() {
            if let Err(e) = db.inner().set_item(
                &dashboard_refresh_key,
                &scheduler_config.dashboard_refresh_interval.to_string(),
            ) {
                error!("保存仪表盘刷新间隔到数据库失败: {}", e);
            }
        }
        
        info!("已加载任务配置");
        Ok(())
    }

    /// 注册所有任务
    async fn register_tasks(&mut self) -> Result<(), String> {
        // 注册刷新任务
        self.register_refresh_task().await?;
        
        info!("所有任务已注册完成");
        Ok(())
    }

    /// 注册刷新任务 - 使用数据库中的间隔配置
    async fn register_refresh_task(&mut self) -> Result<(), String> {
        let app_handle = self.app_handle.clone();
        let task_id = "refresh_dashboard".to_string();
        
        // 获取数据库实例
        let db = match self.app_handle.try_state::<Database>() {
            Some(db) => db,
            None => {
                let err = "无法获取数据库实例".to_string();
                error!("{}", err);
                return Err(err);
            }
        };
        
        // 从数据库获取刷新间隔
        let dashboard_refresh_key = config::get_db_key("dashboard_refresh_interval");
        let refresh_interval_str = match db.inner().get_item(&dashboard_refresh_key) {
            Ok(Some(val)) => val,
            _ => {
                // 使用默认值
                let default_interval = config::get_scheduler_config().dashboard_refresh_interval;
                default_interval.to_string()
            }
        };
        
        // 解析刷新间隔
        let refresh_interval = match refresh_interval_str.parse::<u64>() {
            Ok(val) => val,
            Err(e) => {
                let err = format!("解析仪表盘刷新间隔失败: {}", e);
                error!("{}", err);
                // 使用默认值
                config::get_scheduler_config().dashboard_refresh_interval
            }
        };
        
        info!("仪表盘刷新间隔设置为 {} 秒", refresh_interval);
        
        let handle = tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(refresh_interval));
            
            loop {
                interval.tick().await;
                
                // 通知前端刷新仪表盘
                if let Some(window) = app_handle.as_ref().get_webview_window("main") {
                    if let Err(e) = window.emit("refresh-dashboard", ()) {
                        error!("发送刷新事件失败: {}", e);
                    }
                }
            }
        });
        
        self.tasks.insert(task_id, handle);
        info!("已注册仪表盘刷新任务");
        Ok(())
    }
} 