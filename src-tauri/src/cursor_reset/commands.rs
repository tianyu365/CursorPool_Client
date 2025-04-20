use crate::api::client::ApiClient;
use crate::database::Database;
use crate::utils::hook::Hook;
use crate::utils::id_generator::generate_new_ids;
use crate::utils::paths::AppPaths;
use crate::utils::ErrorReporter;
use crate::utils::ProcessManager;
use rusqlite::Connection;
use serde_json::{json, Value};
use std::fs;
use std::time::Duration;
use tauri::State;
use tauri::Manager;
use tracing::error;
use tokio;

const MAX_CURSOR_ATTEMPTS: usize = 3;
const CURSOR_SLEEP_DURATION: Duration = Duration::from_millis(500);

/// 终止 Cursor 进程
#[tauri::command]
pub async fn close_cursor() -> Result<bool, String> {
    let process_manager = ProcessManager::new();

    // 检查Cursor是否在运行
    if !process_manager.is_cursor_running() {
        return Ok(false); // Cursor未运行，无需关闭
    }

    // 关闭Cursor进程
    if let Err(e) = process_manager.kill_cursor_processes() {
        error!(target: "cursor", "关闭Cursor进程失败: {}", e);
        return Err(e);
    }

    // 等待进程完全关闭
    let mut attempts = 0;
    while process_manager.is_cursor_running() && attempts < MAX_CURSOR_ATTEMPTS {
        tokio::time::sleep(CURSOR_SLEEP_DURATION).await;
        attempts += 1;
    }

    // 检查是否成功关闭
    if process_manager.is_cursor_running() {
        let err_msg = "无法完全关闭Cursor进程".to_string();
        error!(target: "cursor", "{}", err_msg);
        return Err(err_msg);
    }

    Ok(true)
}

/// 启动 Cursor 应用
#[tauri::command]
pub async fn launch_cursor(db: State<'_, Database>) -> Result<bool, String> {
    let paths = match AppPaths::new_with_db(Some(&db)) {
        Ok(p) => p,
        Err(e) => {
            error!(target: "cursor", "获取应用路径失败: {}", e);
            return Err(e);
        }
    };

    // 启动Cursor 
    if let Err(e) = paths.launch_cursor() {
        error!(target: "cursor", "启动Cursor失败: {}", e);
        return Err(e);
    }

    Ok(true)
}

/// 重置设备标识符
#[tauri::command]
pub async fn reset_machine_id(
    client: State<'_, ApiClient>,
    db: State<'_, Database>,
    force_kill: bool,
    machine_id: Option<String>,
) -> Result<bool, String> {
    let process_manager = ProcessManager::new();

    // 检查Cursor进程
    if !force_kill && process_manager.is_cursor_running() {
        error!(target: "reset", "重置失败: Cursor进程正在运行且没有强制关闭选项");
        return Err("Cursor进程正在运行, 请先关闭Cursor".to_string());
    }

    // 如果force_kill为true, 则强制终止Cursor进程
    if force_kill {
        match process_manager.kill_cursor_processes() {
            Ok(_) => {}
            Err(e) => {
                // 上报错误
                error!(target: "reset", "强制终止Cursor进程失败: {}", e);
                ErrorReporter::report_error(
                    client.clone(),
                    "reset_machine_id",
                    &e,
                    None,
                    Some("low".to_string()),
                )
                .await;
                return Err(e);
            }
        }
    }

    let paths = match AppPaths::new_with_db(Some(&db)) {
        Ok(p) => p,
        Err(e) => {
            // 上报错误
            error!(target: "reset", "获取应用路径失败: {}", e);
            ErrorReporter::report_error(
                client.clone(),
                "reset_machine_id",
                &e,
                None,
                Some("low".to_string()),
            )
            .await;
            return Err(e);
        }
    };

    let new_ids = if let Some(id) = machine_id {
        // 生成随机 ID
        let mut ids = generate_new_ids();
        // 替换 devDeviceId
        ids.insert("telemetry.devDeviceId".to_string(), id);
        ids
    } else {
        // 否则随机生成
        generate_new_ids()
    };

    // 更新 storage.json
    let mut storage_content = if paths.storage.exists() {
        let content = match fs::read_to_string(&paths.storage) {
            Ok(c) => c,
            Err(e) => {
                let err = format!("读取 storage.json 失败: {}", e);
                error!(target: "reset", "{}", err);
                ErrorReporter::report_error(
                    client.clone(),
                    "reset_machine_id",
                    &err,
                    None,
                    Some("low".to_string()),
                )
                .await;
                return Err(err);
            }
        };

        match serde_json::from_str(&content) {
            Ok(c) => c,
            Err(e) => {
                let err = format!("解析 storage.json 失败: {}", e);
                error!(target: "reset", "{}", err);
                ErrorReporter::report_error(
                    client.clone(),
                    "reset_machine_id",
                    &err,
                    None,
                    Some("low".to_string()),
                )
                .await;
                return Err(err);
            }
        }
    } else {
        error!(target: "reset", "storage.json不存在，将创建新文件");
        json!({})
    };

    if let Value::Object(ref mut map) = storage_content {
        map.insert(
            "telemetry.devDeviceId".to_string(),
            Value::String(new_ids.get("telemetry.devDeviceId").unwrap().clone()),
        );
        map.insert(
            "telemetry.macMachineId".to_string(),
            Value::String(new_ids.get("telemetry.macMachineId").unwrap().clone()),
        );
        map.insert(
            "telemetry.machineId".to_string(),
            Value::String(new_ids.get("telemetry.machineId").unwrap().clone()),
        );
        map.insert(
            "telemetry.sqmId".to_string(),
            Value::String(new_ids.get("telemetry.sqmId").unwrap().clone()),
        );
    } else {
        let err = "storage.json格式错误，不是有效的JSON对象".to_string();
        error!(target: "reset", "{}", err);
        ErrorReporter::report_error(
            client.clone(),
            "reset_machine_id",
            &err,
            None,
            Some("low".to_string()),
        )
        .await;
        return Err(err);
    }

    let storage_content_str = match serde_json::to_string_pretty(&storage_content) {
        Ok(s) => s,
        Err(e) => {
            let err = format!("序列化 storage.json 失败: {}", e);
            error!(target: "reset", "{}", err);
            ErrorReporter::report_error(
                client.clone(),
                "reset_machine_id",
                &err,
                None,
                Some("low".to_string()),
            )
            .await;
            return Err(err);
        }
    };

    // 检查并移除文件的只读属性
    if paths.storage.exists() {
        match std::fs::metadata(&paths.storage) {
            Ok(metadata) => {
                let mut permissions = metadata.permissions();
                if permissions.readonly() {
                    error!(target: "reset", "storage.json是只读文件，尝试移除只读属性");
                    permissions.set_readonly(false);
                    if let Err(e) = std::fs::set_permissions(&paths.storage, permissions) {
                        let err = format!("无法移除storage.json的只读属性: {}", e);
                        error!(target: "reset", "{}", err);
                    } else {
                        error!(target: "reset", "成功移除storage.json的只读属性");
                    }
                }
            }
            Err(e) => {
                let err = format!("获取storage.json权限失败: {}", e);
                error!(target: "reset", "{}", err);
            }
        }
    }

    if let Err(e) = fs::write(&paths.storage, &storage_content_str) {
        let err = format!("写入 storage.json 失败: {}", e);
        error!(target: "reset", "{}", err);
        ErrorReporter::report_error(
            client.clone(),
            "reset_machine_id",
            &err,
            None,
            Some("low".to_string()),
        )
        .await;
        return Err(err);
    }

    // 更新数据库
    if paths.db.exists() {
        let updates = vec![
            ("device_id", new_ids.get("telemetry.devDeviceId").unwrap()),
            ("mac_id", new_ids.get("telemetry.macMachineId").unwrap()),
            ("machineId", new_ids.get("telemetry.machineId").unwrap()),
            ("sqm_id", new_ids.get("telemetry.sqmId").unwrap()),
        ];

        if let Err(e) = update_database(&paths.db, &updates) {
            error!(target: "reset", "更新数据库失败: {}", e);
            ErrorReporter::report_error(
                client.clone(),
                "reset_machine_id",
                &e,
                None,
                Some("low".to_string()),
            )
            .await;
            return Err(e);
        }
        error!(target: "reset", "成功更新数据库中的设备ID信息");
    } else {
        error!(target: "reset", "数据库文件不存在，跳过数据库更新");
    }

    error!(target: "reset", "设备标识符重置完成");
    Ok(true)
}

/// 切换用户账号
#[tauri::command]
pub async fn switch_account(
    email: String,
    token: String,
    force_kill: bool,
    db: tauri::State<'_, crate::database::Database>,
) -> Result<bool, String> {
    error!(target: "account", "开始切换账号到: {}", email);
    let process_manager = ProcessManager::new();

    // 检查Cursor进程
    if !force_kill && process_manager.is_cursor_running() {
        error!(target: "account", "切换账号失败: Cursor进程正在运行且没有强制关闭选项");
        return Err("Cursor进程正在运行, 请先关闭Cursor".to_string());
    }

    // 如果force_kill为true, 则强制终止Cursor进程
    if force_kill {
        if let Err(e) = process_manager.kill_cursor_processes() {
            error!(target: "account", "强制终止Cursor进程失败: {}", e);
            return Err(e);
        }
        error!(target: "account", "已强制终止Cursor进程");
    }

    let paths = match AppPaths::new_with_db(Some(&db)) {
        Ok(p) => p,
        Err(e) => {
            error!(target: "account", "获取应用路径失败: {}", e);
            return Err(e);
        }
    };

    // 获取当前账户信息
    let mut current_email = String::new();
    let mut current_token = String::new();
    let mut machine_id = String::new();

    if paths.db.exists() {
        if let Ok(conn) = Connection::open(&paths.db) {
            // 读取当前机器码
            if let Ok(mut stmt) =
                conn.prepare("SELECT value FROM ItemTable WHERE key = 'telemetry.devDeviceId'")
            {
                if let Ok(mut rows) = stmt.query([]) {
                    if let Ok(Some(row)) = rows.next() {
                        if let Ok(device_id) = row.get::<_, String>(0) {
                            machine_id = device_id;
                        }
                    }
                }
            }

            // 读取当前邮箱
            if let Ok(mut stmt) =
                conn.prepare("SELECT value FROM ItemTable WHERE key = 'cursorAuth/cachedEmail'")
            {
                if let Ok(mut rows) = stmt.query([]) {
                    if let Ok(Some(row)) = rows.next() {
                        if let Ok(email) = row.get::<_, String>(0) {
                            current_email = email;
                        }
                    }
                }
            }

            // 读取当前token
            if let Ok(mut stmt) =
                conn.prepare("SELECT value FROM ItemTable WHERE key = 'cursorAuth/refreshToken'")
            {
                if let Ok(mut rows) = stmt.query([]) {
                    if let Ok(Some(row)) = rows.next() {
                        if let Ok(token) = row.get::<_, String>(0) {
                            current_token = token;
                        }
                    }
                }
            }
        } else {
            error!(target: "account", "打开数据库失败");
        }
    } else {
        error!(target: "account", "数据库文件不存在");
    }

    // 检查当前账户是否已在历史记录中
    let account_exists_in_history = if !current_email.is_empty() {
        match db.get_item("user.history.accounts") {
            Ok(Some(data)) => {
                match serde_json::from_str::<Vec<crate::api::types::HistoryAccountRecord>>(&data) {
                    Ok(accounts) => accounts.iter().any(|a| a.email == current_email),
                    Err(e) => {
                        error!(target: "account", "解析历史账户记录失败: {}", e);
                        false
                    }
                }
            }
            Ok(None) => {
                error!(target: "account", "历史账户记录为空");
                false
            }
            Err(e) => {
                error!(target: "account", "获取历史账户记录失败: {}", e);
                false
            }
        }
    } else {
        false
    };

    // 如果当前有账户信息且不在历史记录中，才保存
    if !current_email.is_empty()
        && !current_token.is_empty()
        && !machine_id.is_empty()
        && !account_exists_in_history
    {
        if let Err(e) = crate::api::interceptor::save_cursor_token_to_history(
            &db,
            &current_email,
            &current_token,
            &machine_id,
        )
        .await
        {
            error!(target: "account", "保存当前Cursor账户到历史记录失败: {}", e);
        } else {
            error!(target: "account", "成功保存当前账户 {} 到历史记录", current_email);
        }
    }

    // 处理token，分割并只取第二部分
    let processed_token = if token.contains("%3A%3A") {
        token.split("%3A%3A").nth(1).unwrap_or(&token).to_string()
    } else {
        token.clone()
    };

    // 更新数据库为-新账户
    let account_updates = vec![
        ("cursor.email", email.clone()),
        ("cursor.accessToken", processed_token.clone()),
        ("cursorAuth/refreshToken", processed_token.clone()),
        ("cursorAuth/accessToken", processed_token.clone()),
        ("cursorAuth/cachedEmail", email.clone()),
    ];

    if let Err(e) = update_database(&paths.db, &account_updates) {
        error!(target: "account", "更新数据库失败: {}", e);
        return Err(e);
    }
    error!(target: "account", "成功更新数据库中的账户信息");

    // 获取机器码（为了新账户使用）
    let result = get_machine_ids(db.clone()).await?;
    
    let machine_id = result["machineId"].as_str().unwrap_or_default().to_string();

    // ### 检查新账户是否需要保存 ###
    let new_account_exists_in_history = match db.get_item("user.history.accounts") {
        Ok(Some(data)) => {
            match serde_json::from_str::<Vec<crate::api::types::HistoryAccountRecord>>(&data) {
                Ok(accounts) => accounts.iter().any(|a| a.email == email),
                Err(e) => {
                    error!(target: "account", "解析历史账户记录失败: {}", e);
                    false
                }
            }
        }
        Ok(None) => {
            error!(target: "account", "历史账户记录为空");
            false
        }
        Err(e) => {
            error!(target: "account", "获取历史账户记录失败: {}", e);
            false
        }
    };

    // 如果新账户不在历史记录中，才添加
    if !new_account_exists_in_history {
        if let Err(e) = crate::api::interceptor::save_cursor_token_to_history(
            &db,
            &email,
            &processed_token,
            &machine_id,
        )
        .await
        {
            error!(target: "account", "保存新Cursor账户到历史记录失败: {}", e);
        } else {
            error!(target: "account", "成功保存新账户 {} 到历史记录", email);
        }
    } else {
        // 如果账户已存在但token可能更新了，更新历史记录
        if let Ok(Some(data)) = db.get_item("user.history.accounts") {
            if let Ok(mut accounts) =
                serde_json::from_str::<Vec<crate::api::types::HistoryAccountRecord>>(&data)
            {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as i64;

                // 查找并更新已有账户
                for account in &mut accounts {
                    if account.email == email {
                        // 更新token和最后使用时间
                        account.token = processed_token.clone(); // 使用处理后的token
                        account.last_used = now;
                        break;
                    }
                }

                // 保存更新后的记录
                if let Ok(json_data) = serde_json::to_string(&accounts) {
                    if let Err(e) = db.set_item("user.history.accounts", &json_data) {
                        error!(target: "account", "更新历史账户记录失败: {}", e);
                    } else {
                        error!(target: "account", "成功更新账户 {} 的历史记录", email);
                    }
                } else {
                    error!(target: "account", "序列化历史账户记录失败");
                }
            } else {
                error!(target: "account", "解析历史账户记录失败");
            }
        }
    }

    // 等待一段时间确保数据库更新完成
    tokio::time::sleep(CURSOR_SLEEP_DURATION).await;

    error!(target: "account", "成功切换到账号: {}", email);
    Ok(true)
}

/// 获取设备标识符和当前账号信息
#[tauri::command]
pub async fn get_machine_ids(db: State<'_, Database>) -> Result<Value, String> {
    let paths = match AppPaths::new_with_db(Some(&db)) {
        Ok(p) => p,
        Err(e) => {
            error!(target: "machine_id", "获取应用路径失败: {}", e);
            return Err(e);
        }
    };
    let mut result = json!({
        "machineId": "",
        "currentAccount": ""
    });

    // 从数据库读取机器码和 Cursor 邮箱
    if paths.db.exists() {
        if let Ok(conn) = Connection::open(&paths.db) {
            // 读取机器码
            if let Ok(mut stmt) =
                conn.prepare("SELECT value FROM ItemTable WHERE key = 'telemetry.devDeviceId'")
            {
                if let Ok(mut rows) = stmt.query([]) {
                    if let Ok(Some(row)) = rows.next() {
                        if let Ok(device_id) = row.get::<_, String>(0) {
                            result["machineId"] = json!(device_id);
                        }
                    }
                }
            }

            // 读取 Cursor 邮箱
            if let Ok(mut stmt) =
                conn.prepare("SELECT value FROM ItemTable WHERE key = 'cursorAuth/cachedEmail'")
            {
                if let Ok(mut rows) = stmt.query([]) {
                    if let Ok(Some(row)) = rows.next() {
                        if let Ok(email) = row.get::<_, String>(0) {
                            result["currentAccount"] = json!(email);
                        }
                    }
                }
            }

            // 读取cursor token
            if let Ok(mut stmt) =
                conn.prepare("SELECT value FROM ItemTable WHERE key = 'cursorAuth/refreshToken'")
            {
                if let Ok(mut rows) = stmt.query([]) {
                    if let Ok(Some(row)) = rows.next() {
                        if let Ok(token) = row.get::<_, String>(0) {
                            result["cursorToken"] = json!(token);
                        }
                    }
                }
            }
        }
    }

    Ok(result)
}

/// 检查 Cursor 进程状态
#[tauri::command]
pub fn check_cursor_running() -> Result<bool, String> {
    let process_manager = ProcessManager::new();
    Ok(process_manager.is_cursor_running())
}

/// 检查管理员权限
#[tauri::command]
pub fn check_admin_privileges() -> Result<bool, String> {
    crate::utils::check_admin_privileges()
}

/// 请求管理员权限
#[tauri::command]
pub fn request_admin_privileges(exe_path: String) -> Result<bool, String> {
    crate::utils::privileges::request_admin_privileges(&exe_path)
}

/// 更新数据库键值对
fn update_database(
    db_path: &std::path::Path,
    updates: &[(impl AsRef<str>, impl AsRef<str>)],
) -> Result<(), String> {
    error!(target: "database", "开始更新数据库: {}", db_path.display());

    let conn = match Connection::open(db_path) {
        Ok(c) => c,
        Err(e) => {
            let err_msg = format!("打开数据库失败: {}", e);
            error!(target: "database", "{}", err_msg);
            return Err(err_msg);
        }
    };

    for (key, value) in updates {
        let key = match key.as_ref() {
            "device_id" => "telemetry.devDeviceId",
            "mac_id" => "telemetry.macMachineId",
            "machineId" => "telemetry.machineId",
            "sqm_id" => "telemetry.sqmId",
            _ => key.as_ref(),
        };

        error!(target: "database", "更新数据库键值对: {} => {}", key, value.as_ref());

        // 先尝试更新已存在的记录
        let result = conn.execute(
            "UPDATE ItemTable SET value = ?1 WHERE key = ?2",
            [value.as_ref(), key],
        );

        // 如果记录不存在（没有更新任何行）, 则插入新记录
        if let Ok(0) = result {
            error!(target: "database", "键 {} 不存在，将插入新记录", key);
            if let Err(e) = conn.execute(
                "INSERT INTO ItemTable (key, value) VALUES (?1, ?2)",
                [key, value.as_ref()],
            ) {
                let err_msg = format!("插入数据失败: {}", e);
                error!(target: "database", "{}", err_msg);
                return Err(err_msg);
            }
        } else if let Err(e) = result {
            let err_msg = format!("更新数据失败: {}", e);
            error!(target: "database", "{}", err_msg);
            return Err(err_msg);
        }
    }

    error!(target: "database", "数据库更新完成");
    Ok(())
}

/// 检查 main.js 注入状态
#[tauri::command]
pub async fn is_hook(db: State<'_, Database>) -> Result<bool, String> {
    let paths = match AppPaths::new_with_db(Some(&db)) {
        Ok(p) => p,
        Err(e) => {
            error!(target: "hook", "获取应用路径失败: {}", e);
            return Err(e);
        }
    };

    let content = match &paths.main_js {
        Some(path) => match fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                let err_msg = format!("读取 main.js 失败: {}", e);
                error!(target: "hook_status", "{}", err_msg);
                return Err(err_msg);
            }
        },
        None => {
            let err_msg = "MAIN_JS_NOT_FOUND".to_string();
            error!(target: "hook_status", "{}", err_msg);
            return Err(err_msg);
        }
    };

    // 检查正则匹配
    let machine_id_matches = Hook::machine_id_regex().find_iter(&content).count();
    let mac_machine_id_matches = Hook::mac_machine_id_regex().find_iter(&content).count();

    // 如果找不到匹配，说明已经被 hook 了
    if machine_id_matches == 0 || mac_machine_id_matches == 0 {
        return Ok(true);
    }

    Ok(false)
}

/// 注入 main.js 文件
#[tauri::command]
pub async fn hook_main_js(
    client: State<'_, ApiClient>,
    db: State<'_, Database>,
    force_kill: bool,
) -> Result<(), String> {
    let process_manager = ProcessManager::new();

    // 检查 Cursor 进程
    if !force_kill && process_manager.is_cursor_running() {
        error!(target: "hook", "注入失败: Cursor进程正在运行且没有强制关闭选项");
        return Err("Cursor进程正在运行, 请先关闭Cursor".to_string());
    }

    // 如果 force_kill 为 true, 则强制终止 Cursor 进程
    if force_kill {
        match process_manager.kill_cursor_processes() {
            Ok(_) => {
                error!(target: "hook", "已强制终止Cursor进程");
            }
            Err(e) => {
                // 上报错误
                error!(target: "hook", "强制终止Cursor进程失败: {}", e);
                ErrorReporter::report_error(
                    client.clone(),
                    "hook_main_js",
                    &e,
                    None,
                    Some("medium".to_string()),
                )
                .await;
                return Err(e);
            }
        }
    }

    // 执行 hook 操作，传递 client 和 db 用于错误上报和路径保存
    error!(target: "hook", "开始注入main.js文件");
    let result = Hook::update_main_js_content(Some(client), Some(db)).await;
    if result.is_ok() {
        error!(target: "hook", "成功注入main.js文件");
    }
    result
}

/// 恢复 main.js 原始内容
#[tauri::command]
pub async fn restore_hook(
    client: State<'_, ApiClient>,
    db: State<'_, Database>,
    force_kill: bool,
) -> Result<(), String> {
    let process_manager = ProcessManager::new();

    // 检查 Cursor 进程
    if !force_kill && process_manager.is_cursor_running() {
        error!(target: "hook", "恢复失败: Cursor进程正在运行且没有强制关闭选项");
        return Err("Cursor进程正在运行, 请先关闭Cursor".to_string());
    }

    // 如果 force_kill 为 true, 则强制终止 Cursor 进程
    if force_kill {
        match process_manager.kill_cursor_processes() {
            Ok(_) => {
                error!(target: "hook", "已强制终止Cursor进程");
            }
            Err(e) => {
                // 上报错误
                error!(target: "hook", "强制终止Cursor进程失败: {}", e);
                ErrorReporter::report_error(
                    client.clone(),
                    "restore_hook",
                    &e,
                    None,
                    Some("medium".to_string()),
                )
                .await;
                return Err(e);
            }
        }
    }

    // 执行恢复操作，传递 client 和 db 用于错误上报和路径获取
    error!(target: "hook", "开始恢复main.js文件");
    let result = Hook::restore_from_backup(Some(client), Some(db)).await;
    if result.is_ok() {
        error!(target: "hook", "成功恢复main.js文件");
    }
    result
}

/// 查找并验证用户选择的 Cursor 路径
#[tauri::command]
pub async fn find_cursor_path(
    client: State<'_, ApiClient>,
    db: State<'_, Database>,
    selected_path: String,
) -> Result<bool, String> {
    error!(target: "path", "正在查找Cursor路径: {}", selected_path);

    // 尝试从选择的路径找到main.js
    let main_js_path = match AppPaths::find_main_js_from_selected_path(&selected_path) {
        Ok(path) => {
            error!(target: "path", "找到main.js路径: {}", path.display());
            path
        }
        Err(e) => {
            error!(target: "path", "从选择的路径查找main.js失败: {}", e);
            ErrorReporter::report_error(
                client.clone(),
                "find_cursor_path",
                &e,
                None,
                Some("low".to_string()),
            )
            .await;
            return Err(e);
        }
    };

    // 验证找到的文件确实是main.js
    if !main_js_path.exists()
        || main_js_path
            .file_name()
            .map_or(false, |name| name != "main.js")
    {
        let err_msg = "选择的路径不包含有效的main.js文件".to_string();
        error!(target: "path", "{}", err_msg);
        return Err(err_msg);
    }

    // 保存路径到数据库
    if let Err(e) = AppPaths::save_path_to_db(&db, &main_js_path) {
        error!(target: "path", "保存路径到数据库失败: {}", e);
        ErrorReporter::report_error(
            client.clone(),
            "find_cursor_path",
            &e,
            None,
            Some("low".to_string()),
        )
        .await;
        return Err(e);
    }

    error!(target: "path", "成功验证并保存Cursor路径");
    Ok(true)
}

/// 记录错误日志
#[tauri::command]
pub fn log_error(message: String, file: Option<String>, line: Option<u32>) {
    if let (Some(file), Some(line)) = (file, line) {
        error!(target: "frontend", "{}. Location: {} line {}", message, file, line);
    } else {
        error!(target: "frontend", "{}", message);
    }
}

/// 记录警告日志
#[tauri::command]
pub fn log_warn(message: String, file: Option<String>, line: Option<u32>) {
    if let (Some(file), Some(line)) = (file, line) {
        tracing::warn!(target: "frontend", "{}. Location: {} line {}", message, file, line);
    } else {
        tracing::warn!(target: "frontend", "{}", message);
    }
}

/// 记录信息日志
#[tauri::command]
pub fn log_info(message: String) {
    tracing::info!(target: "frontend", "{}", message);
}

#[tauri::command]
pub fn open_devtools(app_handle: tauri::AppHandle) {
    if let Some(window) = app_handle.get_webview_window("main") {
        if !window.is_devtools_open() {
            window.open_devtools();
        } else {
            window.close_devtools();
        }
    }
}

/// 获取正在运行的Cursor进程路径
#[tauri::command]
pub fn get_running_cursor_path() -> Result<String, String> {
    let process_manager = ProcessManager::new();
    
    if !process_manager.is_cursor_running() {
        error!(target: "cursor", "Cursor进程未运行，无法获取路径");
        return Err("Cursor进程未运行".to_string());
    }
    
    // 在Windows系统上使用PowerShell命令获取进程路径
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        let output = Command::new("powershell")
            .args([
                "-WindowStyle", "Hidden", 
                "-Command",
                "(Get-Process cursor | Where-Object {$_.Path -ne $null} | Select-Object -First 1).Path"
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
            
        match output {
            Ok(output) => {
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let clean_path = stdout.trim().to_string();
                    
                    if !clean_path.is_empty() {
                        error!(target: "cursor", "成功获取Cursor进程路径: {}", clean_path);
                        return Ok(clean_path);
                    }
                }
            }
            Err(e) => {
                error!(target: "cursor", "执行PowerShell命令失败: {}", e);
            }
        }
        
        // 使用wmic作为后备方案
        let wmic_output = Command::new("wmic")
            .args(["process", "where", "name='cursor.exe'", "get", "ExecutablePath", "/value"])
            .creation_flags(CREATE_NO_WINDOW) // CREATE_NO_WINDOW 标志
            .output();
            
        if let Ok(output) = wmic_output {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if let Some(path) = stdout.lines()
                    .find(|line| line.starts_with("ExecutablePath="))
                    .map(|line| line.trim_start_matches("ExecutablePath=").trim()) {
                    
                    if !path.is_empty() {
                        error!(target: "cursor", "通过wmic获取Cursor进程路径: {}", path);
                        return Ok(path.to_string());
                    }
                }
            }
        }
        
        let err_msg = "无法获取Cursor进程路径".to_string();
        error!(target: "cursor", "{}", err_msg);
        return Err(err_msg);
    }
    
    // 其他系统暂不支持
    #[cfg(not(target_os = "windows"))]
    {
        let err_msg = "当前系统不支持获取进程路径".to_string();
        error!(target: "cursor", "{}", err_msg);
        return Err(err_msg);
    }
}