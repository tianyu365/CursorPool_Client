use tracing::error;

use crate::config;
use crate::database::Database;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

// 用于存储Cursor路径的数据库键
const CURSOR_MAIN_JS_PATH_KEY: &str = "system.cursor.path.mainJs";

pub struct AppPaths {
    pub storage: PathBuf,
    pub auth: PathBuf,
    pub db: PathBuf,
    pub cursor_exe: PathBuf,
    pub main_js: Option<PathBuf>,
}

impl AppPaths {
    pub fn new() -> Result<Self, String> {
        Self::new_with_db(None)
    }

    // 新增：使用数据库查找保存的路径或默认路径
    pub fn new_with_db(db: Option<&Database>) -> Result<Self, String> {
        let base_dir = if cfg!(target_os = "windows") {
            // Windows: %APPDATA%\Cursor\User\globalStorage
            let app_data =
                std::env::var("APPDATA").map_err(|e| format!("获取 APPDATA 路径失败: {}", e))?;
            PathBuf::from(app_data).join("Cursor")
        } else if cfg!(target_os = "macos") {
            // macOS: ~/Library/Application Support/Cursor/User/globalStorage
            let home = std::env::var("HOME").map_err(|e| format!("获取 HOME 路径失败: {}", e))?;
            PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("Cursor")
        } else if cfg!(target_os = "linux") {
            // Linux: ~/.config/Cursor/User/globalStorage
            let home = std::env::var("HOME").map_err(|e| format!("获取 HOME 路径失败: {}", e))?;
            PathBuf::from(home).join(".config").join("Cursor")
        } else {
            return Err(format!("不支持的操作系统: {}", std::env::consts::OS));
        };

        let global_storage = base_dir.join("User").join("globalStorage");

        // 获取 Cursor 可执行文件路径 - 添加环境变量查找逻辑
        let cursor_exe = if cfg!(target_os = "windows") {
            let default_path = {
                let local_app_data = std::env::var("LOCALAPPDATA")
                    .map_err(|e| format!("获取 LOCALAPPDATA 路径失败: {}", e))?;
                let default_path_str = config::CONFIG
                    .read()
                    .unwrap()
                    .paths
                    .windows
                    .cursor_exe
                    .clone();
                PathBuf::from(default_path_str.replace("%LOCALAPPDATA%", &local_app_data))
            };

            // 检查默认路径是否存在
            if default_path.exists() {
                default_path
            } else {
                // 尝试从环境变量PATH中查找
                match Self::find_cursor_exe_from_env_path() {
                    Ok(path) => path,
                    Err(_) => default_path, // 如果找不到，依然返回默认路径
                }
            }
        } else if cfg!(target_os = "macos") {
            let default_path_str = config::CONFIG
                .read()
                .unwrap()
                .paths
                .macos
                .cursor_app
                .clone();
            PathBuf::from(default_path_str)
        } else {
            let default_path_str = config::CONFIG
                .read()
                .unwrap()
                .paths
                .linux
                .cursor_exe
                .clone();
            PathBuf::from(default_path_str)
        };

        // 获取 main.js 路径 - 现在优先从数据库查找，但如果找不到则设置为None而不是返回错误
        let main_js = if let Some(db) = db {
            if let Ok(Some(saved_path)) = Self::get_saved_path_from_db(db) {
                // 检查保存的路径是否有效
                if saved_path.exists() {
                    Some(saved_path)
                } else {
                    // 尝试查找但不抛出错误
                    match Self::find_main_js_path() {
                        Ok(path) => Some(path),
                        Err(e) => {
                            // 记录错误但不阻止初始化
                            error!(target: "paths", "查找main.js失败: {}", e);
                            None
                        }
                    }
                }
            } else {
                // 尝试查找但不抛出错误
                match Self::find_main_js_path() {
                    Ok(path) => Some(path),
                    Err(e) => {
                        // 记录错误但不阻止初始化
                        error!(target: "paths", "查找main.js失败: {}", e);
                        None
                    }
                }
            }
        } else {
            // 尝试查找但不抛出错误
            match Self::find_main_js_path() {
                Ok(path) => Some(path),
                Err(e) => {
                    // 记录错误但不阻止初始化
                    error!(target: "paths", "查找main.js失败: {}", e);
                    None
                }
            }
        };
        
        // 如果cursor_exe路径不存在但main_js存在，尝试从main_js推导cursor_exe路径
        let cursor_exe = if !cursor_exe.exists() && cfg!(target_os = "windows") {
            if let Some(ref main_js_path) = main_js {
                // 预期路径结构: X:\path\to\cursor\resources\app\out\main.js
                if let Some(out_dir) = main_js_path.parent() {
                    if let Some(app_dir) = out_dir.parent() {
                        if let Some(resources_dir) = app_dir.parent() {
                            if let Some(cursor_dir) = resources_dir.parent() {
                                let inferred_path = cursor_dir.join("Cursor.exe");
                                if inferred_path.exists() {
                                    println!("从main.js路径推导到Cursor.exe: {}", inferred_path.display());
                                    inferred_path
                                } else {
                                    cursor_exe
                                }
                            } else {
                                cursor_exe
                            }
                        } else {
                            cursor_exe
                        }
                    } else {
                        cursor_exe
                    }
                } else {
                    cursor_exe
                }
            } else {
                cursor_exe
            }
        } else {
            cursor_exe
        };

        let paths = Self {
            storage: global_storage.join("storage.json"),
            auth: global_storage.join("cursor.auth.json"),
            db: global_storage.join("state.vscdb"),
            cursor_exe,
            main_js,
        };

        // 确保目录存在
        if let Some(parent) = paths.storage.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {}", e))?;
        }

        Ok(paths)
    }

    // 查找cursor.exe
    fn find_cursor_exe_from_env_path() -> Result<PathBuf, String> {
        let path = std::env::var("PATH").map_err(|e| format!("获取PATH环境变量失败: {}", e))?;

        // 分割PATH变量并查找cursor.exe
        for dir in path.split(';') {
            let path_buf = PathBuf::from(dir);

            // 如果PATH中包含cursor相关路径
            if dir.to_lowercase().contains("cursor") {
                // 情况1: 如果是"resources/app/bin"这样的路径，向上查找cursor.exe
                if dir.to_lowercase().contains("resources")
                    && dir.to_lowercase().contains("app")
                    && (dir.to_lowercase().ends_with("bin")
                        || dir.to_lowercase().contains("\\bin")
                        || dir.to_lowercase().contains("/bin"))
                {
                    // 向上两级查找，cursor.exe通常在bin的上两级目录
                    if let Some(app_dir) = path_buf.parent() {
                        if let Some(resources_dir) = app_dir.parent() {
                            if let Some(cursor_dir) = resources_dir.parent() {
                                let cursor_exe_path = cursor_dir.join("Cursor.exe");
                                if cursor_exe_path.exists() {
                                    return Ok(cursor_exe_path);
                                }
                            }
                        }
                    }
                }

                // 情况2: 直接在当前目录查找
                let cursor_exe_path = path_buf.join("Cursor.exe");
                if cursor_exe_path.exists() {
                    println!("找到Cursor.exe: {}", cursor_exe_path.display());
                    return Ok(cursor_exe_path);
                }

                // 情况3: 在父目录查找
                if let Some(parent) = path_buf.parent() {
                    let cursor_exe_path = parent.join("Cursor.exe");
                    if cursor_exe_path.exists() {
                        println!("找到Cursor.exe: {}", cursor_exe_path.display());
                        return Ok(cursor_exe_path);
                    }
                }
            }
        }

        Err("在环境变量PATH中未找到Cursor.exe".to_string())
    }

    // 新增：寻找main.js路径的方法
    fn find_main_js_path() -> Result<PathBuf, String> {
        if cfg!(target_os = "windows") {
            // Windows系统下，直接从环境变量PATH中查找
            match Self::find_cursor_from_env_path() {
                Ok(path) => Ok(path),
                Err(e) => Err(format!("从环境变量中查找Cursor路径失败: {}", e))
            }
        } else if cfg!(target_os = "macos") {
            let resources_path_str = config::CONFIG
                .read()
                .unwrap()
                .paths
                .macos
                .cursor_resources
                .clone();
            let path = PathBuf::from(resources_path_str);
            if path.exists() {
                Ok(path)
            } else {
                Err("macOS系统下未找到main.js文件".to_string())
            }
        } else {
            // Linux 路径
            let resources_path_str = config::CONFIG
                .read()
                .unwrap()
                .paths
                .linux
                .cursor_resources
                .clone();
            let path = PathBuf::from(resources_path_str);
            if path.exists() {
                Ok(path)
            } else {
                Err("Linux系统下未找到main.js文件".to_string())
            }
        }
    }

    // 新增：从环境变量PATH查找Cursor路径
    fn find_cursor_from_env_path() -> Result<PathBuf, String> {
        let path = std::env::var("PATH").map_err(|e| format!("获取PATH环境变量失败: {}", e))?;

        // 分割PATH变量并查找包含"cursor"的目录
        for dir in path.split(';') {
            let path_buf = PathBuf::from(dir);

            // 处理已经包含"resources/app/bin"这样路径的情况
            if dir.to_lowercase().contains("cursor") {
                // 如果路径包含bin目录，尝试查找同级的out目录
                if dir.to_lowercase().contains("resources")
                    && dir.to_lowercase().contains("app")
                    && (dir.to_lowercase().ends_with("bin")
                        || dir.to_lowercase().contains("\\bin")
                        || dir.to_lowercase().contains("/bin"))
                {
                    let potential_base = if let Some(parent) = path_buf.parent() {
                        parent
                    } else {
                        continue;
                    };

                    // 尝试构建main.js路径（同级out目录）
                    let main_js_path = potential_base.join("out").join("main.js");
                    if main_js_path.exists() {
                        return Ok(main_js_path);
                    }
                }

                // 原有检查：构建完整路径
                let main_js_path = PathBuf::from(dir)
                    .join("resources")
                    .join("app")
                    .join("out")
                    .join("main.js");

                if main_js_path.exists() {
                    return Ok(main_js_path);
                }
            }

            // 向上一级查找cursor安装目录（原有逻辑）
            if let Some(parent) = path_buf.parent() {
                if dir.to_lowercase().contains("cursor") {
                    // 如果找到cursor相关目录，构建main.js路径
                    let main_js_path = PathBuf::from(parent)
                        .join("resources")
                        .join("app")
                        .join("out")
                        .join("main.js");

                    if main_js_path.exists() {
                        return Ok(main_js_path);
                    }
                }
            }
        }

        Err("在环境变量PATH中未找到Cursor路径".to_string())
    }

    // 从用户选择的路径中查找main.js
    pub fn find_main_js_from_selected_path(selected_path: &str) -> Result<PathBuf, String> {
        let clean_path = selected_path.trim();
        let selected_path_buf = PathBuf::from(clean_path);

        // 检查是否选择的是cursor.exe或Cursor.exe
        if selected_path_buf.file_name().map_or(false, |name| {
            let name_str = name.to_string_lossy().to_lowercase();
            name_str == "cursor.exe" || name_str == "cursor"
        }) {
            println!("检测到用户选择了cursor.exe文件");

            // 尝试从cursor.exe所在目录推断main.js位置
            if let Some(parent_dir) = selected_path_buf.parent() {
                // 常见情况: cursor.exe与resources在同一级目录
                let main_js_path = parent_dir
                    .join("resources")
                    .join("app")
                    .join("out")
                    .join("main.js");
                if main_js_path.exists() {
                    return Ok(main_js_path);
                }

                // 作为兜底，使用递归搜索
                if let Ok(found_path) = Self::find_main_js_in_directory(parent_dir) {
                    return Ok(found_path);
                }
            }
        }

        // 检查直接选择的是否为main.js
        if selected_path_buf
            .file_name()
            .map_or(false, |name| name == "main.js")
            && selected_path_buf.exists()
        {
            return Ok(selected_path_buf);
        }

        // 检查是否为目录路径
        if selected_path_buf.is_dir() {
            // 尝试常见路径
            let main_js_path = selected_path_buf
                .join("resources")
                .join("app")
                .join("out")
                .join("main.js");
            if main_js_path.exists() {
                return Ok(main_js_path);
            }

            // 兜底机制：递归搜索
            if let Ok(found_path) = Self::find_main_js_in_directory(&selected_path_buf) {
                return Ok(found_path);
            }
        }

        Err(format!(
            "在选择的路径 '{}' 中未找到main.js文件",
            selected_path
        ))
    }

    // 新增：在目录中递归查找main.js文件
    fn find_main_js_in_directory(dir: &Path) -> Result<PathBuf, String> {
        if !dir.is_dir() {
            return Err("提供的路径不是目录".to_string());
        }

        // 最大递归深度，防止无限递归
        const MAX_DEPTH: usize = 5;

        fn search_recursively(
            dir: &Path,
            current_depth: usize,
            max_depth: usize,
        ) -> Option<PathBuf> {
            if current_depth > max_depth {
                return None;
            }

            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();

                    // 检查是否为main.js文件
                    if path.is_file()
                        && path.file_name().map_or(false, |name| name == "main.js")
                        && path.parent().map_or(false, |parent| {
                            parent.file_name().map_or(false, |name| name == "out")
                                && parent.parent().map_or(false, |p| {
                                    p.file_name().map_or(false, |name| name == "app")
                                        && p.parent().map_or(false, |pp| {
                                            pp.file_name().map_or(false, |name| name == "resources")
                                        })
                                })
                        })
                    {
                        return Some(path);
                    }

                    // 如果是目录，递归搜索
                    if path.is_dir() {
                        if let Some(found_path) =
                            search_recursively(&path, current_depth + 1, max_depth)
                        {
                            return Some(found_path);
                        }
                    }
                }
            }

            None
        }

        if let Some(path) = search_recursively(dir, 0, MAX_DEPTH) {
            Ok(path)
        } else {
            Err(format!(
                "在目录 '{}' 及其子目录中未找到 main.js 文件",
                dir.display()
            ))
        }
    }

    // 新增：从数据库获取保存的main.js路径
    pub fn get_saved_path_from_db(db: &Database) -> Result<Option<PathBuf>, String> {
        let cursor_main_js_path_key = config::get_db_key("cursor_main_js_path");
        match db.get_item(&cursor_main_js_path_key) {
            Ok(Some(path)) => Ok(Some(PathBuf::from(path))),
            Ok(None) => Ok(None),
            Err(e) => Err(format!("获取保存的main.js路径失败: {}", e)),
        }
    }

    // 新增：保存main.js路径到数据库
    pub fn save_path_to_db(db: &Database, path: &Path) -> Result<(), String> {
        let path_str = path.to_string_lossy().to_string();
        db.set_item(CURSOR_MAIN_JS_PATH_KEY, &path_str)
            .map_err(|e| format!("保存路径到数据库失败: {}", e))
    }

    // 确保父目录存在
    pub fn ensure_parent_exists(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {}", e))?;
        }
        Ok(())
    }

    // 启动 Cursor
    pub fn launch_cursor(&self) -> Result<(), String> {
        if !self.cursor_exe.exists() {
            // 当默认cursor.exe路径不存在时，尝试从main.js路径推导
            if cfg!(target_os = "windows") {
                if let Some(ref main_js_path) = self.main_js {
                    if main_js_path.exists() {
                        // 预期路径结构: X:\path\to\cursor\resources\app\out\main.js
                        if let Some(out_dir) = main_js_path.parent() {
                            if let Some(app_dir) = out_dir.parent() {
                                if let Some(resources_dir) = app_dir.parent() {
                                    if let Some(cursor_dir) = resources_dir.parent() {
                                        // 推导的cursor.exe路径
                                        let cursor_exe_path = cursor_dir.join("Cursor.exe");
                                        
                                        // 检查文件是否存在
                                        if cursor_exe_path.exists() {
                                            println!("根据main.js路径推导到Cursor.exe: {}", cursor_exe_path.display());
                                            
                                            // 直接使用推导的路径启动
                                            std::process::Command::new(&cursor_exe_path)
                                                .spawn()
                                                .map_err(|e| format!("启动 Cursor 失败: {}", e))?;
                                                
                                            return Ok(());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            
            return Err("Cursor 可执行文件不存在".to_string());
        }

        std::process::Command::new(&self.cursor_exe)
            .spawn()
            .map_err(|e| format!("启动 Cursor 失败: {}", e))?;

        Ok(())
    }
}
