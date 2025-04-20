use chrono::Local;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::Manager;
use tracing::{debug, info, warn};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{
    fmt::{self, time::UtcTime},
    prelude::*,
    EnvFilter,
};

/// 日志系统配置选项
pub struct LogConfig {
    /// 日志文件夹根目录
    pub log_dir: PathBuf,
    /// 是否输出到控制台
    pub console_output: bool,
    /// 日志级别
    pub log_level: String,
    /// 是否使用JSON格式
    pub json_format: bool,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            log_dir: PathBuf::from("logs"),
            console_output: true,
            log_level: "info".to_string(),
            json_format: false,
        }
    }
}

/// 初始化日志系统
pub fn init_logger(config: LogConfig) -> Result<(), String> {
    // 获取当前日期
    let current_time = chrono::Local::now();
    let year = current_time.format("%Y").to_string();
    let month = current_time.format("%m").to_string();
    let day = current_time.format("%d").to_string();

    // 创建日志目录结构: logs/年/月/
    let log_dir = config.log_dir.join(&year).join(&month);
    fs::create_dir_all(&log_dir).map_err(|e| format!("创建日志目录失败: {}", e))?;

    // 日志文件名: 年-月-日.log
    let file_name = format!("{}-{}-{}.log", year, month, day);

    // 创建日志文件写入器
    let file_appender = RollingFileAppender::new(Rotation::NEVER, log_dir, file_name);

    // 设置日志过滤级别
    let log_level = config.log_level.clone();
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&log_level));

    // 修改日志格式
    if config.json_format {
        // JSON格式
        let json_layer = fmt::layer()
            .json()
            .with_timer(UtcTime::rfc_3339())
            .with_writer(file_appender);

        let subscriber = tracing_subscriber::registry()
            .with(env_filter)
            .with(json_layer);

        if config.console_output {
            // 控制台输出可以保留颜色
            let console_layer = fmt::layer().json().with_timer(UtcTime::rfc_3339());

            subscriber.with(console_layer).init();
        } else {
            subscriber.init();
        }
    } else {
        // 文本格式 - 移除颜色相关配置
        let fmt_layer = fmt::layer()
            .with_target(true)
            .with_thread_ids(true)
            .with_level(true)
            .with_timer(UtcTime::rfc_3339())
            .with_ansi(false) // 禁用ANSI颜色
            .with_writer(file_appender);

        let subscriber = tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer);

        if config.console_output {
            // 控制台输出可以保留颜色
            let console_layer = fmt::layer()
                .with_target(true)
                .with_thread_ids(true)
                .with_level(true)
                .with_timer(UtcTime::rfc_3339())
                .with_ansi(true);

            subscriber.with(console_layer).init();
        } else {
            subscriber.init();
        }
    }

    // 记录初始化日志
    info!("日志系统已初始化");
    debug!("日志级别: {}", log_level);
    debug!("日志目录: {}", config.log_dir.display());

    Ok(())
}

/// 获取应用数据目录下的日志目录
pub fn get_app_log_dir(app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("无法获取应用数据目录: {}", e))?;

    let log_dir = app_data_dir.join("logs");
    fs::create_dir_all(&log_dir).map_err(|e| format!("创建日志目录失败: {}", e))?;

    Ok(log_dir)
}

/// 用于测试的方法 - 清理指定日期之前的日志
pub fn clean_old_logs(log_dir: &Path, days: i64) -> Result<usize, String> {
    let cutoff_date = chrono::Local::now() - chrono::Duration::days(days);
    let mut removed_count = 0;

    // 遍历年份目录
    if let Ok(year_entries) = fs::read_dir(log_dir) {
        for year_entry in year_entries.flatten() {
            let year_path = year_entry.path();
            if !year_path.is_dir() {
                continue;
            }

            // 遍历月份目录
            if let Ok(month_entries) = fs::read_dir(&year_path) {
                for month_entry in month_entries.flatten() {
                    let month_path = month_entry.path();
                    if !month_path.is_dir() {
                        continue;
                    }

                    // 遍历日志文件
                    if let Ok(log_entries) = fs::read_dir(&month_path) {
                        for log_entry in log_entries.flatten() {
                            let log_path = log_entry.path();
                            if !log_path.is_file()
                                || !log_path.extension().map_or(false, |ext| ext == "log")
                            {
                                continue;
                            }

                            // 从文件名提取日期
                            if let Some(file_name) = log_path.file_stem() {
                                let file_name = file_name.to_string_lossy();
                                if let Ok(date) =
                                    chrono::NaiveDate::parse_from_str(&file_name, "%Y-%m-%d")
                                {
                                    let datetime =
                                        chrono::DateTime::<Local>::from_naive_utc_and_offset(
                                            date.and_hms_opt(0, 0, 0).unwrap(),
                                            *Local::now().offset(),
                                        );

                                    // 如果日期早于截止日期，删除文件
                                    if datetime < cutoff_date {
                                        if let Err(e) = fs::remove_file(&log_path) {
                                            warn!(
                                                "删除旧日志文件失败: {} - {}",
                                                log_path.display(),
                                                e
                                            );
                                        } else {
                                            removed_count += 1;
                                        }
                                    }
                                }
                            }
                        }

                        // 如果月份目录为空，删除目录
                        if fs::read_dir(&month_path).map_or(false, |rd| rd.count() == 0) {
                            let _ = fs::remove_dir(&month_path);
                        }
                    }
                }

                // 如果年份目录为空，删除目录
                if fs::read_dir(&year_path).map_or(false, |rd| rd.count() == 0) {
                    let _ = fs::remove_dir(&year_path);
                }
            }
        }
    }

    Ok(removed_count)
}
