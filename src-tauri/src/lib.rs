// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use api::ApiClient;
use database::Database;
use std::env;
use std::error::Error as StdError;
use std::path::PathBuf;
use tauri::{generate_context, generate_handler, Manager};
use tracing::{debug, error, info};
use utils::{get_app_log_dir, init_logger, LogConfig};

pub mod api;
pub mod auth;
pub mod config;
pub mod cursor_reset;
pub mod database;
pub mod scheduler;
pub mod tray;
pub mod utils;

pub fn run() {
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_process::init());

    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }));
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        builder = builder.plugin(tauri_plugin_updater::Builder::new().build());
    }

    builder
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_positioner::init())
        .plugin(tauri_plugin_macos_permissions::init())
        .setup(|app| {
            // 初始化配置
            if let Err(e) = config::init_config() {
                eprintln!("初始化配置失败: {}", e);
            }

            // 初始化日志系统
            let log_dir = match get_app_log_dir(app.handle()) {
                Ok(dir) => dir,
                Err(e) => {
                    eprintln!("初始化日志目录失败: {}", e);
                    PathBuf::from("logs") // 回退到当前目录下的logs文件夹
                }
            };

            // 配置日志系统
            let log_config = LogConfig {
                log_dir,
                console_output: true,
                log_level: if cfg!(debug_assertions) {
                    "debug,hyper=off,reqwest=off,tauri_plugin_updater=off".to_string()
                } else {
                    "info,hyper=off,reqwest=off,tauri_plugin_updater=off".to_string()
                },
                json_format: false,
            };

            // 初始化日志系统
            if let Err(e) = init_logger(log_config) {
                eprintln!("初始化日志系统失败: {}", e);
            }

            // 记录应用启动信息
            info!("应用启动");
            debug!("调试模式: {}", cfg!(debug_assertions));

            // 初始化数据库
            let db = match Database::new(app.handle()) {
                Ok(db) => {
                    info!("数据库初始化成功");
                    db
                }
                Err(e) => {
                    error!("数据库初始化失败: {}", e);
                    return Err(Box::<dyn StdError>::from(e.to_string()));
                }
            };
            app.manage(db);

            // 异步初始化线路配置
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = api::inbound::init_inbound_config(&app_handle).await {
                    error!("初始化线路配置失败: {}", e);
                } else {
                    info!("线路配置初始化成功");
                }
            });

            // 延迟一小段时间，以便线路配置初始化完成
            std::thread::sleep(std::time::Duration::from_millis(100));

            let api_client = ApiClient::new(Some(app.handle().clone()));
            app.manage(api_client);

            // 初始化系统托盘
            if let Err(e) = tray::setup_system_tray(app) {
                error!("初始化系统托盘失败: {}", e);
                return Err(Box::<dyn StdError>::from(e.to_string()));
            }
            info!("系统托盘初始化成功");
            
            // 初始化任务调度器
            let scheduler = scheduler::Scheduler::new(app.handle().clone());
            app.manage(scheduler.clone());
            
            // 在单独的任务中启动调度器，避免阻塞主线程
            tauri::async_runtime::spawn(async move {
                if let Err(e) = scheduler.start().await {
                    error!("启动任务调度器失败: {}", e);
                } else {
                    info!("任务调度器启动成功");
                }
            });

            Ok(())
        })
        .invoke_handler(generate_handler![
            // 登录
            api::login,
            api::logout,
            api::check_user,
            api::send_code,
            api::reset_password,
            api::register,
            // 主页
            api::get_user_info,
            api::get_account,
            api::get_usage,
            api::get_public_info,
            api::get_article_list,
            api::mark_article_read,
            // 设置
            api::activate,
            api::change_password,
            // 数据库
            api::set_user_data,
            api::get_user_data,
            api::del_user_data,
            // 换号
            cursor_reset::commands::reset_machine_id,
            cursor_reset::commands::switch_account,
            cursor_reset::commands::get_machine_ids,
            // 权限
            cursor_reset::commands::check_cursor_running,
            cursor_reset::commands::check_admin_privileges,
            // hook
            cursor_reset::commands::is_hook,
            cursor_reset::commands::hook_main_js,
            cursor_reset::commands::restore_hook,
            cursor_reset::commands::get_running_cursor_path,
            // cursor
            cursor_reset::commands::close_cursor,
            cursor_reset::commands::launch_cursor,
            cursor_reset::commands::find_cursor_path,
            // 日志
            cursor_reset::commands::log_error,
            cursor_reset::commands::log_warn,
            cursor_reset::commands::log_info,
            // devtools
            cursor_reset::commands::open_devtools,
        ])
        .run(generate_context!())
        .expect("error while running tauri application")
}
