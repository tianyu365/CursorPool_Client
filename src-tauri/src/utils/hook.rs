use crate::api::client::ApiClient;
use crate::database::Database;
use crate::utils::paths::AppPaths;
use crate::utils::ErrorReporter;
use lazy_static::lazy_static;
use regex::Regex;
use std::fmt;
use std::fs;
use std::path::PathBuf;
use tauri::State;
use tracing::error;

lazy_static! {
    /// machineId 函数匹配模式
    static ref MACHINE_ID_REGEX: Regex = Regex::new(
        r#"async\s+(\w+)\s*\(\)\s*\{\s*return\s+this\.[\w.]+(?:\?\?|\?)\s*this\.([\w.]+)\.machineId\s*\}"#
    ).unwrap();

    /// macMachineId 函数匹配模式
    static ref MAC_MACHINE_ID_REGEX: Regex = Regex::new(
        r#"async\s+(\w+)\s*\(\)\s*\{\s*return\s+this\.[\w.]+(?:\?\?|\?)\s*this\.([\w.]+)\.macMachineId\s*\}"#
    ).unwrap();
}

pub struct Hook;

#[derive(Debug)]
pub enum HookError {
    // 找不到main.js路径
    MainJsNotFound(String),
    // 其他错误
    Other(String),
}

impl fmt::Display for HookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HookError::MainJsNotFound(msg) => write!(f, "MAIN_JS_NOT_FOUND:{}", msg),
            HookError::Other(msg) => write!(f, "{}", msg),
        }
    }
}

impl Hook {
    /// 获取 machineId 正则表达式
    pub fn machine_id_regex() -> &'static Regex {
        &MACHINE_ID_REGEX
    }

    /// 获取 macMachineId 正则表达式
    pub fn mac_machine_id_regex() -> &'static Regex {
        &MAC_MACHINE_ID_REGEX
    }

    /// 读取 main.js 文件内容
    fn get_main_js_content(db: Option<&Database>) -> Result<(String, PathBuf), HookError> {
        // 使用带数据库参数的AppPaths创建方法
        let paths = AppPaths::new_with_db(db).map_err(|e| {
            if e.contains("找不到Cursor的main.js文件") {
                error!(target: "hook", "找不到Cursor的main.js文件: {}", e);
                HookError::MainJsNotFound(e)
            } else {
                error!(target: "hook", "创建应用路径失败: {}", e);
                HookError::Other(format!("创建应用路径失败: {}", e))
            }
        })?;

        // 检查main_js是否存在
        let file_path = match &paths.main_js {
            Some(path) => path,
            None => {
                let error_msg = "未找到main.js路径".to_string();
                error!(target: "hook", "{}", error_msg);
                return Err(HookError::MainJsNotFound(error_msg));
            }
        };

        // 检查文件是否存在
        if !file_path.exists() {
            let error_msg = format!("文件不存在: {}", file_path.display());
            error!(target: "hook", "{}", error_msg);
            return Err(HookError::MainJsNotFound(error_msg));
        }

        fs::read_to_string(file_path)
            .map(|content| (content, file_path.clone()))
            .map_err(|e| {
                let error_msg = format!("读取 main.js 失败: {}", e);
                error!(target: "hook", "{}", error_msg);
                HookError::Other(error_msg)
            })
    }

    /// 更新 main.js 文件内容
    pub async fn update_main_js_content(
        client: Option<State<'_, ApiClient>>,
        db: Option<State<'_, Database>>,
    ) -> Result<(), String> {
        // 获取文件内容和路径
        let (content, file_path) = match Self::get_main_js_content(db.as_ref().map(|d| d.inner())) {
            Ok(result) => result,
            Err(e) => match e {
                HookError::MainJsNotFound(msg) => {
                    error!(target: "hook", "找不到main.js: {}", msg);
                    return Err(format!("MAIN_JS_NOT_FOUND:{}", msg));
                }
                HookError::Other(msg) => {
                    error!(target: "hook", "读取main.js失败: {}", msg);
                    if let Some(ref client) = client {
                        ErrorReporter::report_error(
                            client.clone(),
                            "read_main_js",
                            &msg,
                            None,
                            Some("high".to_string()),
                        )
                        .await;
                    }
                    return Err(msg);
                }
            },
        };

        // 创建备份
        let backup_path = file_path.with_extension("js.backup");
        if !backup_path.exists() {
            if let Err(e) = fs::write(&backup_path, &content) {
                let err_msg = format!("创建备份失败: {}", e);
                error!(target: "hook", "{}", err_msg);
                if let Some(ref client) = client {
                    ErrorReporter::report_error(
                        client.clone(),
                        "create_backup",
                        &err_msg,
                        None,
                        Some("high".to_string()),
                    )
                    .await;
                }
                return Err(err_msg);
            }
        }

        // 使用正则表达式进行替换
        let machine_id_matches = MACHINE_ID_REGEX.find_iter(&content).count();
        let mac_machine_id_matches = MAC_MACHINE_ID_REGEX.find_iter(&content).count();

        if machine_id_matches == 0 || mac_machine_id_matches == 0 {
            let err_msg = "无法找到匹配的 machineId 或 macMachineId 函数".to_string();
            error!(target: "hook", "{}", err_msg);
            return Err(err_msg);
        }

        // 替换 machineId
        let modified_content = MACHINE_ID_REGEX
            .replace_all(&content, |caps: &regex::Captures| {
                format!(
                    "async {}() {{ return this.{}.machineId }}",
                    &caps[1], &caps[2]
                )
            })
            .to_string();

        // 替换 macMachineId
        let modified_content = MAC_MACHINE_ID_REGEX
            .replace_all(&modified_content, |caps: &regex::Captures| {
                format!(
                    "async {}() {{ return this.{}.macMachineId }}",
                    &caps[1], &caps[2]
                )
            })
            .to_string();

        // 写入修改后的内容
        if let Err(e) = fs::write(&file_path, &modified_content) {
            let err_msg = format!("写入修改后的内容失败: {}", e);
            error!(target: "hook", "{}", err_msg);
            if let Some(ref client) = client {
                ErrorReporter::report_error(
                    client.clone(),
                    "write_modified_file",
                    &err_msg,
                    None,
                    Some("high".to_string()),
                )
                .await;
            }
            return Err(err_msg);
        }

        // 保存成功找到的路径到数据库
        if let Some(ref db) = db {
            if let Err(e) = AppPaths::save_path_to_db(db, &file_path) {
                error!(target: "hook", "保存main.js路径到数据库失败: {}", e);
                // 这里不返回错误，因为hook已经成功了
            }
        }

        Ok(())
    }

    /// 从备份恢复 main.js
    pub async fn restore_from_backup(
        client: Option<State<'_, ApiClient>>,
        db: Option<State<'_, Database>>,
    ) -> Result<(), String> {
        // 获取文件路径
        let (_, file_path) = match Self::get_main_js_content(db.as_ref().map(|d| d.inner())) {
            Ok(result) => result,
            Err(e) => match e {
                HookError::MainJsNotFound(msg) => {
                    error!(target: "hook", "找不到main.js: {}", msg);
                    return Err(format!("MAIN_JS_NOT_FOUND:{}", msg));
                }
                HookError::Other(msg) => {
                    error!(target: "hook", "获取main.js路径失败: {}", msg);
                    if let Some(ref client) = client {
                        ErrorReporter::report_error(
                            client.clone(),
                            "read_main_js",
                            &msg,
                            None,
                            Some("high".to_string()),
                        )
                        .await;
                    }
                    return Err(msg);
                }
            },
        };

        let backup_path = file_path.with_extension("js.backup");

        if !backup_path.exists() {
            let err_msg = "备份文件不存在".to_string();
            error!(target: "hook", "{}", err_msg);
            if let Some(ref client) = client {
                ErrorReporter::report_error(
                    client.clone(),
                    "restore_from_backup",
                    &err_msg,
                    None,
                    Some("medium".to_string()),
                )
                .await;
            }
            return Err(err_msg);
        }

        let backup_content = match fs::read_to_string(&backup_path) {
            Ok(content) => content,
            Err(e) => {
                let err_msg = format!("读取备份文件失败: {}", e);
                error!(target: "hook", "{}", err_msg);
                if let Some(ref client) = client {
                    ErrorReporter::report_error(
                        client.clone(),
                        "restore_from_backup",
                        &err_msg,
                        None,
                        Some("medium".to_string()),
                    )
                    .await;
                }
                return Err(err_msg);
            }
        };

        if let Err(e) = fs::write(&file_path, &backup_content) {
            let err_msg = format!("恢复文件失败: {}", e);
            error!(target: "hook", "{}", err_msg);
            if let Some(ref client) = client {
                ErrorReporter::report_error(
                    client.clone(),
                    "restore_from_backup",
                    &err_msg,
                    None,
                    Some("medium".to_string()),
                )
                .await;
            }
            return Err(err_msg);
        }

        if let Err(e) = fs::remove_file(backup_path) {
            let err_msg = format!("删除备份文件失败: {}", e);
            error!(target: "hook", "{}", err_msg);
            if let Some(ref client) = client {
                ErrorReporter::report_error(
                    client.clone(),
                    "restore_from_backup",
                    &err_msg,
                    None,
                    Some("low".to_string()),
                )
                .await;
            }
        }

        Ok(())
    }
}
