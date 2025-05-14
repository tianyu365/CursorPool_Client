use crate::config;
use crate::database::Database;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};
use tracing::{error, info, warn};

// 线路配置数据结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboundConfig {
    pub inbound: Vec<InboundItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboundItem {
    pub name: String,
    pub url: String,
}

// 带延迟信息的线路条目
#[derive(Debug, Clone)]
struct InboundItemWithLatency {
    index: usize,
    item: InboundItem,
    latency: Option<Duration>,
}

/// 从远程获取线路配置
pub async fn fetch_inbound_config() -> Result<InboundConfig, String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| {
            error!(target: "inbound", "创建HTTP客户端失败: {}", e);
            format!("创建HTTP客户端失败: {}", e)
        })?;

    let config_url = config::get_config_file_url();
    let response = client.get(&config_url).send().await.map_err(|e| {
        error!(target: "inbound", "请求线路配置失败 - URL: {}, 错误: {}", config_url, e);
        format!("请求线路配置失败: {}", e)
    })?;

    let config = response.json::<InboundConfig>().await.map_err(|e| {
        error!(target: "inbound", "解析线路配置失败: {}", e);
        format!("解析线路配置失败: {}", e)
    })?;

    if config.inbound.is_empty() {
        error!(target: "inbound", "线路配置为空");
        return Err("线路配置为空".to_string());
    }

    Ok(config)
}

/// 测试单个线路的延迟
async fn test_inbound_latency(url: &str) -> Option<Duration> {
    let ping_timeout = config::get_ping_timeout();
    let client = match Client::builder().timeout(ping_timeout).build() {
        Ok(client) => client,
        Err(e) => {
            error!(target: "inbound", "创建延迟测试HTTP客户端失败 - URL: {}, 错误: {}", url, e);
            return None;
        }
    };

    // 测试版本信息接口（通常是轻量级的）
    let test_url = format!("{}/version", url);
    let start = Instant::now();

    match client.get(&test_url).send().await {
        Ok(response) => {
            if response.status().is_success() {
                Some(start.elapsed())
            } else {
                error!(target: "inbound", "延迟测试服务器返回错误 - URL: {}, 状态码: {}", test_url, response.status());
                None // 服务器返回错误
            }
        }
        Err(e) => {
            error!(target: "inbound", "延迟测试请求失败 - URL: {}, 错误: {}", test_url, e);
            None // 请求失败（超时或网络错误）
        }
    }
}

/// 测试所有线路并返回延迟最低的有效线路索引
async fn find_fastest_inbound(config: &InboundConfig) -> usize {
    // 如果只有一个线路，直接返回
    if config.inbound.len() == 1 {
        return 0;
    }

    let mut items_with_latency = Vec::new();

    // 测试所有线路
    for (index, item) in config.inbound.iter().enumerate() {
        let api_base_url = format!("{}/api", item.url);
        let latency = test_inbound_latency(&api_base_url).await;

        items_with_latency.push(InboundItemWithLatency {
            index,
            item: item.clone(),
            latency,
        });

        // 打印测试结果
        match latency {
            Some(duration) => info!(target: "inbound", "线路 [{}] {} 延迟: {:?}", index, item.name, duration),
            None => info!(target: "inbound", "线路 [{}] {} 不可用", index, item.name),
        }
    }

    // 筛选可用的线路
    let available_items: Vec<_> = items_with_latency
        .iter()
        .filter(|item| item.latency.is_some())
        .collect();

    if available_items.is_empty() {
        warn!(target: "inbound", "所有线路均不可用，使用第一个线路");
        return 0;
    }

    // 找到延迟最低的线路
    let fastest = available_items
        .iter()
        .min_by_key(|item| item.latency.unwrap())
        .unwrap();

    info!(
        target: "inbound",
        "选择延迟最低的线路 [{}] {}: {:?}",
        fastest.index,
        fastest.item.name,
        fastest.latency.unwrap()
    );

    fastest.index
}

/// 初始化线路配置
pub async fn init_inbound_config(app_handle: &AppHandle) -> Result<(), String> {
    let db = app_handle.state::<Database>();

    // 尝试从远程获取线路配置
    match fetch_inbound_config().await {
        Ok(config) => {
            // 序列化配置
            let config_json = serde_json::to_string(&config).map_err(|e| {
                error!(target: "inbound", "序列化线路配置失败: {}", e);
                format!("序列化线路配置失败: {}", e)
            })?;

            // 保存到数据库
            let inbound_config_key = config::get_db_key("inbound_config");
            db.set_item(&inbound_config_key, &config_json)
                .map_err(|e| {
                    error!(target: "inbound", "保存线路配置失败: {}", e);
                    format!("保存线路配置失败: {}", e)
                })?;

            // 每次启动都执行测速选择最佳线路
            let current_inbound_key = config::get_db_key("current_inbound");
            info!(target: "inbound", "开始测速选择最佳线路...");
            let best_index = find_fastest_inbound(&config).await;
            db.set_item(&current_inbound_key, &best_index.to_string())
                .map_err(|e| {
                    error!(target: "inbound", "设置当前线路失败: {}", e);
                    format!("设置当前线路失败: {}", e)
                })?;
            info!(target: "inbound", "已选择最佳线路，索引: {}", best_index);
        }
        Err(e) => {
            error!(target: "inbound", "获取远程线路配置失败: {}", e);
            warn!(target: "inbound", "获取远程线路配置失败: {}，将使用默认线路", e);

            // 检查数据库中是否已有配置
            let inbound_config_key = config::get_db_key("inbound_config");
            if let Ok(None) = db.get_item(&inbound_config_key) {
                // 创建默认配置
                let default_api_url = config::get_default_api_url();
                let default_config = InboundConfig {
                    inbound: vec![InboundItem {
                        name: "默认线路".to_string(),
                        url: default_api_url
                            .split("/api")
                            .next()
                            .unwrap_or("http://27.25.151.67:7771")
                            .to_string(),
                    }],
                };

                let config_json = serde_json::to_string(&default_config).map_err(|e| {
                    error!(target: "inbound", "序列化默认线路配置失败: {}", e);
                    format!("序列化默认线路配置失败: {}", e)
                })?;

                db.set_item(&inbound_config_key, &config_json)
                    .map_err(|e| {
                        error!(target: "inbound", "保存默认线路配置失败: {}", e);
                        format!("保存默认线路配置失败: {}", e)
                    })?;

                let current_inbound_key = config::get_db_key("current_inbound");
                db.set_item(&current_inbound_key, "0").map_err(|e| {
                    error!(target: "inbound", "设置默认当前线路失败: {}", e);
                    format!("设置当前线路失败: {}", e)
                })?;

                info!(target: "inbound", "已创建默认线路配置");
            }
        }
    }

    Ok(())
}

/// 获取当前线路URL
pub fn get_current_inbound_url(db: &Database) -> String {
    let default_api_url = config::get_default_api_url();

    // 获取当前选择的线路索引
    let current_inbound_key = config::get_db_key("current_inbound");
    let current_index = match db.get_item(&current_inbound_key) {
        Ok(Some(index)) => index.parse::<usize>().unwrap_or(0),
        Ok(None) => {
            error!(target: "inbound", "未找到当前线路索引配置");
            0
        }
        Err(e) => {
            error!(target: "inbound", "获取当前线路索引失败: {}", e);
            0
        }
    };

    // 获取线路配置
    let inbound_config_key = config::get_db_key("inbound_config");
    let config = match db.get_item(&inbound_config_key) {
        Ok(Some(json)) => match serde_json::from_str::<InboundConfig>(&json) {
            Ok(config) => config,
            Err(e) => {
                error!(target: "inbound", "解析线路配置失败: {}", e);
                return default_api_url;
            }
        },
        Ok(None) => {
            error!(target: "inbound", "未找到线路配置");
            return default_api_url;
        }
        Err(e) => {
            error!(target: "inbound", "获取线路配置失败: {}", e);
            return default_api_url;
        }
    };

    // 检查索引是否有效
    if current_index >= config.inbound.len() {
        error!(target: "inbound", "当前线路索引无效: {}, 线路总数: {}", current_index, config.inbound.len());
        return default_api_url;
    }

    // 返回API基础URL
    format!("{}/api", config.inbound[current_index].url)
}
