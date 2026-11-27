//! 统一运行时存储：`~/.config/fq/store.json`（0600，原子写 tmp+rename）。
//!
//! 收纳四类运行时状态：cookie、本地书架 bookmarks、作者缓存、shelf 快照（TTL 600s，
//! 离线可看上次书架 + 秒开）。首启自动从旧格式迁移（cookie/books.txt/authors.json
//! 改 `.bak` 备份）；文件损坏时回退空存储不崩溃。
//!
//! 不进 store 的：config.ini（人手编辑配置）、cache/（性能层）、info.list（跟书目录走）。

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::util::{RED, RST};

/// shelf 快照有效期（秒）：600 内直接用快照免网络，过期重拉。
pub const SHELF_TTL: u64 = 600;

/// 统一存储根（serde 默认字段 = 各状态可独立缺失）。
#[derive(Default, Serialize, Deserialize)]
pub struct Store {
    #[serde(default)]
    pub v: u32,
    #[serde(default)]
    pub cookie: Option<Cookie>,
    #[serde(default)]
    pub bookmarks: Vec<Bookmark>,
    #[serde(default)]
    pub authors: HashMap<String, AuthorEntry>,
    #[serde(default)]
    pub shelf: Option<ShelfSnap>,
    /// read TUI 阅读进度：book_id → 位置（按 item_id 存，范围变化重入仍能定位）
    #[serde(default)]
    pub read: HashMap<String, ReadPos>,
}

#[derive(Serialize, Deserialize)]
pub struct Cookie {
    pub value: String,
    pub saved_at: u64,
}

#[derive(Serialize, Deserialize)]
pub struct Bookmark {
    pub id: String,
    pub title: String,
}

/// read 阅读进度：章内滚动行偏移 + 章 item_id（进度模型抄 rereader 的 chapter+offset）
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct ReadPos {
    #[serde(default)]
    pub item_id: String,
    #[serde(default)]
    pub scroll: usize,
}

#[derive(Serialize, Deserialize)]
pub struct AuthorEntry {
    pub a: String,
    pub t: u64,
}

/// shelf 快照：`fetch_shelf_web` 的原始输出 + 抓取时间（秒）。
#[derive(Serialize, Deserialize)]
pub struct ShelfSnap {
    pub data: Value,
    pub fetched_at: u64,
}

pub fn store_path() -> PathBuf {
    crate::config::config_dir().join("store.json")
}

pub fn now_sec() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 读取存储；缺失/损坏时迁移旧格式或回退空（损坏打 err 提示）。
pub fn load() -> Store {
    let p = store_path();
    if let Ok(s) = fs::read_to_string(&p) {
        match serde_json::from_str::<Store>(&s) {
            Ok(st) => return st,
            Err(e) => eprintln!("  {RED}err{RST} store.json 损坏（{e}），按空存储继续"),
        }
    }
    migrate_legacy()
}

/// 原子写（tmp + rename，unix 下 0600）。失败打 err（调用方多为尽力而为路径）。
pub fn save(st: &Store) -> Result<(), String> {
    let p = store_path();
    let dir = p.parent().ok_or("store 路径无父目录")?;
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let s = serde_json::to_string_pretty(st).map_err(|e| e.to_string())?;
    let tmp = p.with_extension("json.tmp");
    fs::write(&tmp, s).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600));
    }
    fs::rename(&tmp, &p).map_err(|e| e.to_string())
}

/// 旧格式迁移：cookie / books.txt / authors.json 导入后改 `.bak`（store.json 已存在则不会走到这里）。
fn migrate_legacy() -> Store {
    let mut st = Store::default();
    let dir = store_path()
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_default();
    if let Ok(s) = fs::read_to_string(dir.join("cookie")) {
        let v = s.trim().to_string();
        if !v.is_empty() {
            st.cookie = Some(Cookie {
                value: v,
                saved_at: now_sec(),
            });
        }
        let _ = fs::rename(dir.join("cookie"), dir.join("cookie.bak"));
    }
    if let Ok(text) = fs::read_to_string(dir.join("books.txt")) {
        for line in text.lines() {
            if let Some((id, title)) = line.trim().split_once(':') {
                st.bookmarks.push(Bookmark {
                    id: id.trim().into(),
                    title: title.trim().into(),
                });
            }
        }
        let _ = fs::rename(dir.join("books.txt"), dir.join("books.txt.bak"));
    }
    if let Ok(s) = fs::read_to_string(dir.join("authors.json")) {
        if let Ok(m) = serde_json::from_str::<HashMap<String, AuthorEntry>>(&s) {
            st.authors = m;
        }
        let _ = fs::rename(dir.join("authors.json"), dir.join("authors.json.bak"));
    }
    if st.cookie.is_some() || !st.bookmarks.is_empty() || !st.authors.is_empty() {
        let _ = save(&st);
    }
    st
}

// —— 各状态便捷读写（load→改→save，数据百级内整读整写）——

pub fn cookie_get() -> Option<String> {
    load().cookie.map(|c| c.value)
}

/// 存 cookie（调用方已完成校验与规范化）。
pub fn cookie_set(value: String) -> Result<(), String> {
    let mut st = load();
    st.cookie = Some(Cookie {
        value,
        saved_at: now_sec(),
    });
    save(&st)
}

pub fn cookie_clear() -> Result<(), String> {
    let mut st = load();
    if st.cookie.take().is_none() {
        return Err("未设置过 Cookie".into());
    }
    save(&st)
}

pub fn bookmarks() -> Vec<(String, String)> {
    load()
        .bookmarks
        .into_iter()
        .map(|b| (b.id, b.title))
        .collect()
}

pub fn bookmark_add(id: &str, title: &str) -> Result<(), String> {
    let mut st = load();
    st.bookmarks.retain(|b| b.id != id); // 防重复
    st.bookmarks.push(Bookmark {
        id: id.into(),
        title: title.into(),
    });
    save(&st)
}

/// 按下标移除，返回被移除项（不存在返回 None）。
pub fn bookmark_remove(idx: usize) -> Option<(String, String)> {
    let mut st = load();
    if idx >= st.bookmarks.len() {
        return None;
    }
    let b = st.bookmarks.remove(idx);
    let _ = save(&st);
    Some((b.id, b.title))
}

pub fn authors_load() -> HashMap<String, AuthorEntry> {
    load().authors
}

pub fn authors_save(m: HashMap<String, AuthorEntry>) -> Result<(), String> {
    let mut st = load();
    st.authors = m;
    save(&st)
}

/// shelf 快照：返回 (数据, 是否在 TTL 内)。无快照返回 None。
pub fn shelf_snap() -> Option<(Value, bool)> {
    let st = load();
    st.shelf.map(|s| {
        let fresh = now_sec().saturating_sub(s.fetched_at) < SHELF_TTL;
        (s.data, fresh)
    })
}

pub fn shelf_snap_set(data: Value) -> Result<(), String> {
    let mut st = load();
    st.shelf = Some(ShelfSnap {
        data,
        fetched_at: now_sec(),
    });
    save(&st)
}

/// 写操作（add/remove/move）成功后失效快照：新书立即可见，不等 TTL（98）。
pub fn shelf_snap_clear() {
    let mut st = load();
    st.shelf = None;
    let _ = save(&st);
}

/// read 进度读取。
pub fn read_get(book_id: &str) -> Option<ReadPos> {
    load().read.get(book_id).cloned()
}

/// read 进度保存（退出时调用）。
pub fn read_save(book_id: &str, pos: ReadPos) -> Result<(), String> {
    let mut st = load();
    st.read.insert(book_id.to_string(), pos);
    save(&st)
}
