//! fqapi 运行时封装
//!
//! 将 fanqie-lite-api 的 `fqapi_core` 以进程内方式驱动：初始化设备池与签名
//! 管理器，之后所有业务请求通过 `dispatch` 直调，不监听端口。CLI 的向外交
//! 互层完全建立在此之上，取代旧的 URL 模板 + 第三方源轮换。
//!
//! 数据目录（`~/.local/share/fqdt/`，可用 `FQ_DATA_DIR` 覆盖）：
//! - `config/config.json`  运行时配置（缺失时写入默认值）
//! - `config/filter.json`  响应过滤（可缺省，缺省即不过滤）
//! - `config/device_pool.json` 设备池（加密）
//! - `pool.key`            设备池加密密钥（64 hex）
//! - `src/`                媒体缓存目录

use std::path::PathBuf;
use std::sync::OnceLock;

use fqapi_core::dispatch::{self, Request};
use fqapi_core::runtime;
use fqapi_core::server::ListenMode;

/// 进程内共用的请求超时（毫秒）。下载类长任务另行放宽。
pub const DEFAULT_TIMEOUT_MS: i64 = 30_000;

/// 端点索引项（单点定义，供 `fqdt api --list`、封装子命令与 serve 复用）。
pub struct Endpoint {
    /// HTTP 方法
    pub method: &'static str,
    /// 路径模板，`{xxx}` 为路径参数占位
    pub path: &'static str,
    /// 必填 query 参数（逗号分隔，空表示无）
    pub required: &'static str,
    /// 中文说明
    pub note: &'static str,
}

/// 常用端点索引。完整端点表见 crates/fqapi-core 的路由定义（dispatch 一律直通，
/// 未列入此表的端点同样可用）。
pub const ENDPOINTS: &[Endpoint] = &[
    Endpoint { method: "GET", path: "/api/v1/search", required: "query", note: "综合搜索 (tab_type: 综合1 小说3 听书2)" },
    Endpoint { method: "GET", path: "/api/v1/search/hot", required: "", note: "热搜榜" },
    Endpoint { method: "GET", path: "/api/v1/search/suggest", required: "", note: "搜索建议 (未实现, 404)" },
    Endpoint { method: "GET", path: "/api/v1/category", required: "", note: "分类首页 (category_tab: 1男频 2女频)" },
    Endpoint { method: "GET", path: "/api/v1/recommend/homepage", required: "", note: "首页推荐流 (tab_type/tab_index)" },
    Endpoint { method: "GET", path: "/api/v1/recommend/channels", required: "", note: "推荐频道表" },
    Endpoint { method: "GET", path: "/api/v1/rank/{rank_id}", required: "", note: "榜单数据" },
    Endpoint { method: "GET", path: "/api/v1/books/{book_id}", required: "", note: "书籍详情 (legacy)" },
    Endpoint { method: "GET", path: "/api/v1/books/{book_id}/detail", required: "", note: "书籍详情" },
    Endpoint { method: "GET", path: "/api/v1/books/{book_id}/directory", required: "", note: "目录 (data.item_data_list)" },
    Endpoint { method: "GET", path: "/api/v1/books/{book_id}/reviews", required: "", note: "书评" },
    Endpoint { method: "GET", path: "/api/v1/books/{book_id}/comments", required: "", note: "段评聚合 (legacy)" },
    Endpoint { method: "GET", path: "/api/v1/books/{book_id}/related", required: "", note: "相关推荐" },
    Endpoint { method: "GET", path: "/api/v1/books/{book_id}/tones", required: "", note: "音色列表" },
    Endpoint { method: "GET", path: "/api/v1/books/{book_id}/share", required: "", note: "分享信息" },
    Endpoint { method: "GET", path: "/api/v1/chapters/{item_id}", required: "", note: "单章正文 (含解密)" },
    Endpoint { method: "GET", path: "/api/v1/chapters/batch", required: "item_ids", note: "批量正文 (逗号分隔)" },
    Endpoint { method: "GET", path: "/api/v1/chapters/{item_id}/reviews", required: "", note: "章评" },
    Endpoint { method: "GET", path: "/api/v1/chapters/{item_id}/paragraphs/{n}/reviews", required: "", note: "第 n 段段评" },
    Endpoint { method: "GET", path: "/api/v1/chapters/{item_id}/timeline", required: "", note: "听书字幕时间轴" },
    Endpoint { method: "GET", path: "/api/v1/items/{item_ids}", required: "", note: "章节信息 (不含正文)" },
    Endpoint { method: "GET", path: "/api/v1/authors/{author_id}", required: "", note: "作者信息" },
    Endpoint { method: "GET", path: "/api/v1/authors/{author_id}/bookshelf", required: "", note: "作者作品" },
    Endpoint { method: "GET", path: "/api/v1/audio/books/{book_id}", required: "", note: "有声书详情" },
    Endpoint { method: "GET", path: "/api/v1/audio/books/{book_id}/chapters/{chapter_id}", required: "", note: "章节播放信息 (尊重 tone_id)" },
    Endpoint { method: "GET", path: "/api/v1/audio/play", required: "item_ids,book_id", note: "播放源" },
    Endpoint { method: "GET", path: "/api/v1/audio/wkcontent/{item_ids}", required: "", note: "文稿" },
    Endpoint { method: "GET", path: "/api/v1/videos/{video_id}", required: "", note: "视频播放源 (mode=stream)" },
    Endpoint { method: "GET", path: "/api/v1/series/{series_id}", required: "", note: "短剧详情" },
    Endpoint { method: "GET", path: "/api/v1/series/{series_id}/comments", required: "", note: "剧评" },
    Endpoint { method: "GET", path: "/api/v1/comments/{comment_id}/replies", required: "", note: "评论回复" },
    Endpoint { method: "GET", path: "/api/v1/manga/{item_ids}", required: "", note: "漫画章节" },
];


/// 安装 rustls 加密提供方（`rustls-no-provider` 下 reqwest 构建前必须）。幂等。
pub fn install_crypto() {
    static DONE: std::sync::Once = std::sync::Once::new();
    DONE.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

/// 数据目录根。首次调用时解析并缓存。
fn data_dir() -> PathBuf {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        if let Ok(d) = std::env::var("FQ_DATA_DIR") {
            if !d.trim().is_empty() {
                return PathBuf::from(d);
            }
        }
        // XDG 数据目录优先，回退到用户主目录下的 .local/share。
        if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
            if !xdg.trim().is_empty() {
                return PathBuf::from(xdg).join("fqdt");
            }
        }
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home).join(".local/share/fqdt")
    })
    .clone()
}

/// 生成 32 字节随机密钥的 hex 表示（64 字符）。
fn random_key_hex() -> Result<String, String> {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// 载入或创建设备池密钥。
///
/// 优先级：环境变量 `FQ_POOL_KEY` → `<data-dir>/pool.key` → 自动生成并写回。
/// 拒绝在已加密的设备池上更换密钥（会导致池不可读）。
fn load_or_create_pool_key(data: &std::path::Path, pool_path: &std::path::Path) -> Result<String, String> {
    if let Ok(k) = std::env::var("FQ_POOL_KEY") {
        let k = k.trim().to_string();
        if k.len() == 64 {
            return Ok(k);
        }
        return Err("FQ_POOL_KEY 长度必须为 64 个 hex 字符".into());
    }
    let key_path = data.join("pool.key");
    if key_path.exists() {
        let k = std::fs::read_to_string(&key_path)
            .map_err(|e| format!("读取 pool.key 失败: {e}"))?;
        let k = k.trim().to_string();
        if k.len() == 64 {
            return Ok(k);
        }
        return Err("pool.key 长度异常，请删除后重试".into());
    }
    // 设备池若已存在且是密文，缺失密钥时铸新钥会让池不可读，直接拒绝。
    if pool_path.exists() {
        let head = std::fs::read(pool_path).unwrap_or_default();
        if head.starts_with(b"FQPOOL1") {
            return Err("device_pool.json 已加密但缺少 pool.key，请恢复密钥".into());
        }
    }
    let key = random_key_hex()?;
    std::fs::write(&key_path, &key).map_err(|e| format!("写入 pool.key 失败: {e}"))?;
    Ok(key)
}

/// 准备数据目录、默认配置与设备池密钥（ensure_runtime / serve_at 共用）。
/// 返回 (config_path, pool_path, filter_path, data_dir) 字符串。
fn prep_runtime() -> Result<(String, String, String, String), String> {
    install_crypto();
    let data = data_dir();
    let config_dir = data.join("config");
    std::fs::create_dir_all(&config_dir).map_err(|e| format!("创建数据目录失败: {e}"))?;

    let config_path = config_dir.join("config.json");
    if !config_path.exists() {
        // 默认配置：算法类型空串即用内置默认，不启用反爬中转。
        let default = r#"{"algorithm_type":"","zwsm":"","port":18921,"anti_crawler":{"enabled":false,"redirect_url":""},"admin":{"token":"","update_server_url":""}}"#;
        std::fs::write(&config_path, default).map_err(|e| format!("写入 config.json 失败: {e}"))?;
    }
    let filter_path = config_dir.join("filter.json");
    let pool_path = config_dir.join("device_pool.json");

    let key_hex = load_or_create_pool_key(&data, &pool_path)?;
    let key = decode_hex(&key_hex)?;
    runtime::set_device_pool_key(key)?;

    let s = |p: &PathBuf| p.to_string_lossy().into_owned();
    Ok((
        s(&config_path),
        s(&pool_path),
        s(&filter_path),
        s(&data),
    ))
}

/// 确保运行时已初始化（不监听端口）。幂等；重复调用直接返回。
pub async fn ensure_runtime() -> Result<(), String> {
    let (config, pool, filter, data) = prep_runtime()?;
    runtime::init_core(config, pool, filter, data, 0, ListenMode::Disabled, None).await?;
    Ok(())
}

/// 以监听模式启动运行时（`fqdt serve` 用），返回对外 base_url。
/// `lan=true` 监听局域网地址，否则仅本机回环；进程内仅允许一份运行时。
pub async fn serve_at(port: u16, lan: bool) -> Result<String, String> {
    // init 幂等且成功/复用均返回 "running"，无法据此判重，直接取 base_url。
    let (config, pool, filter, data) = prep_runtime()?;
    runtime::init(config, pool, filter, data, port, lan, String::new()).await?;
    Ok(runtime::base_url().await)
}

/// hex 字符串解码为字节数组（音频 CENC 密钥等复用）。
pub fn decode_hex(s: &str) -> Result<Vec<u8>, String> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|_| "密钥含非法 hex 字符".into()))
        .collect()
}

/// 一次进程内 API 调用的返回（todo14 `fqdt api` 透传用）。
#[allow(dead_code)]
pub struct ApiReply {
    pub status: u16,
    pub body: Vec<u8>,
}

#[allow(dead_code)]
impl ApiReply {
    /// 将响应体解析为 JSON。
    pub fn json(&self) -> Result<serde_json::Value, String> {
        serde_json::from_slice(&self.body).map_err(|e| format!("响应非 JSON: {e}"))
    }

    /// 将响应体按 UTF-8 文本返回。
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    /// 状态码在 200..300 时返回 JSON，否则带状态码报错。
    pub fn ok_json(&self) -> Result<serde_json::Value, String> {
        if !(200..300).contains(&self.status) {
            return Err(format!("HTTP {}: {}", self.status, truncate(&self.text())));
        }
        self.json()
    }
}

/// 截断过长文本用于错误消息。
#[allow(dead_code)]
fn truncate(s: &str) -> String {
    let s = s.trim();
    if s.chars().count() > 200 {
        s.chars().take(200).collect::<String>() + "…"
    } else {
        s.to_string()
    }
}

/// 发起一次进程内请求（todo14 透传用）。`query` 为 `a=1&b=2` 形式（不含问号）。
#[allow(dead_code)]
pub async fn call(method: &str, path: &str, query: &str, body: Vec<u8>) -> Result<ApiReply, String> {
    ensure_runtime().await?;
    let server = runtime::server()
        .await
        .ok_or_else(|| "运行时未就绪".to_string())?;
    let req = Request {
        method: method.to_string(),
        path: path.to_string(),
        query: query.trim_start_matches('?').to_string(),
        body,
        headers: Vec::new(),
    };
    let resp = dispatch::dispatch(&server, &req).await;
    let status = resp.status;
    let bytes = resp.into_bytes().await?;
    Ok(ApiReply { status, body: bytes })
}

/// 发起 GET 请求并解析 JSON（todo14 透传用）。
#[allow(dead_code)]
pub async fn get_json(path: &str, query: &str) -> Result<serde_json::Value, String> {
    call("GET", path, query, Vec::new()).await?.ok_json()
}

/// 带超时的 GET 请求。
///
/// 走 `call()` 进程内 dispatch（已验证稳定），外层 `tokio::time::timeout`
/// 兜底；此前走 `runtime::request` bridge 路径会无响应挂死（v0.11.0 修复）。
pub async fn get_json_timeout(
    path: &str,
    query: &str,
    timeout_ms: i64,
) -> Result<serde_json::Value, String> {
    let limit = std::time::Duration::from_millis(timeout_ms.max(1) as u64);
    let resp = tokio::time::timeout(limit, call("GET", path, query, Vec::new()))
        .await
        .map_err(|_| "请求超时".to_string())??;
    resp.ok_json()
}

/// 退出前释放运行时（刷盘设备池等）（todo16 serve 退出路径用）。
#[allow(dead_code)]
pub async fn shutdown() {
    let _ = runtime::shutdown().await;
}
