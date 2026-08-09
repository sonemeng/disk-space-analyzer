//! 清理记录 + 系统回收站工具。
//! 删除文件仍走 Windows 系统回收站（不额外占空间、资源管理器可见、卸载无残留），
//! 应用内只记录"每次清理删了什么"（来源/标签/大小/路径清单），
//! 并提供打开/清空系统回收站的入口。
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

/// 应用数据根目录（app_data_dir/recycle-bin），由 main.rs setup 时初始化
static BIN_ROOT: OnceLock<PathBuf> = OnceLock::new();

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredItem {
    /// 原路径
    pub original: String,
    /// 文件大小（记录用）
    pub size: u64,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecycleEntry {
    pub id: String,
    pub created_at: String,
    pub source: String,
    pub label: String,
    pub total_bytes: u64,
    pub file_count: u64,
    pub items: Vec<StoredItem>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecycleSummary {
    pub entries: Vec<RecycleEntry>,
    pub total_bytes: u64,
}

pub fn init(root: PathBuf) -> Result<(), String> {
    if BIN_ROOT.set(root.clone()).is_err() {
        return Ok(()); // 已初始化
    }
    fs::create_dir_all(&root).map_err(|e| format!("无法创建回收站记录目录: {e}"))
}

fn bin_dir() -> Result<&'static PathBuf, String> {
    BIN_ROOT.get().ok_or_else(|| "回收站记录未初始化".into())
}

fn entries_path() -> Result<PathBuf, String> {
    Ok(bin_dir()?.join("entries.json"))
}

fn now_string() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}.{:03}", now.as_secs(), now.subsec_millis())
}

fn read_entries() -> Result<Vec<RecycleEntry>, String> {
    let path = entries_path()?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(&path).map_err(|e| format!("读取清理记录失败: {e}"))?;
    if content.trim().is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str(&content).map_err(|e| format!("解析清理记录失败: {e}"))
}

fn write_entries(entries: &[RecycleEntry]) -> Result<(), String> {
    let path = entries_path()?;
    let content =
        serde_json::to_string_pretty(entries).map_err(|e| format!("序列化清理记录失败: {e}"))?;
    fs::write(path, content).map_err(|e| format!("保存清理记录失败: {e}"))
}

fn size_of_path(path: &Path) -> u64 {
    let metadata = fs::metadata(path);
    match metadata {
        Ok(m) if m.is_file() => m.len(),
        Ok(_) => walkdir::WalkDir::new(path)
            .follow_links(false)
            .into_iter()
            .flatten()
            .filter_map(|e| e.metadata().ok())
            .filter(|m| m.is_file())
            .fold(0_u64, |acc, m| acc.saturating_add(m.len())),
        Err(_) => 0,
    }
}

/// 记录一次清理动作（文件已由调用方移入系统回收站）
pub fn record_cleanup(paths: Vec<PathBuf>, source: &str, label: &str) -> Result<(), String> {
    if paths.is_empty() {
        return Ok(());
    }
    let mut entries = read_entries()?;
    let items: Vec<StoredItem> = paths
        .into_iter()
        .map(|path| StoredItem {
            original: path.to_string_lossy().into_owned(),
            size: size_of_path(&path),
        })
        .collect();
    let total_bytes: u64 = items.iter().map(|item| item.size).sum();
    entries.push(RecycleEntry {
        id: now_string(),
        created_at: now_string(),
        source: source.to_string(),
        label: label.to_string(),
        total_bytes,
        file_count: items.len() as u64,
        items,
    });
    // 最多保留 200 条记录，超出丢弃最旧的
    if entries.len() > 200 {
        entries.drain(..entries.len() - 200);
    }
    write_entries(&entries)
}

pub fn list_entries() -> Result<RecycleSummary, String> {
    let entries = read_entries()?;
    let total_bytes: u64 = entries.iter().map(|e| e.total_bytes).sum();
    Ok(RecycleSummary {
        entries,
        total_bytes,
    })
}

pub fn clear_entries() -> Result<(), String> {
    write_entries(&[])
}

/// 统计系统回收站（$Recycle.Bin）占用大小
pub fn system_bin_bytes() -> u64 {
    let mut total = 0_u64;
    for drive_letter in (b'A'..=b'Z').map(|b| (b as char).to_string()) {
        let root = PathBuf::from(format!("{drive_letter}:\\"));
        if !root.exists() {
            continue;
        }
        let bin = root.join("$Recycle.Bin");
        total = total.saturating_add(size_of_path(&bin));
    }
    total
}

/// 打开系统回收站窗口
#[cfg(windows)]
pub fn open_system_bin() -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let verb: Vec<u16> = std::ffi::OsStr::new("open")
        .encode_wide()
        .chain(Some(0))
        .collect();
    let target: Vec<u16> =
        std::ffi::OsStr::new("shell:RecycleBinFolder")
            .encode_wide()
            .chain(Some(0))
            .collect();
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            target.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    if result as isize > 32 {
        Ok(())
    } else {
        Err("无法打开系统回收站".into())
    }
}

#[cfg(not(windows))]
pub fn open_system_bin() -> Result<(), String> {
    Err("此操作仅支持 Windows".into())
}

/// 清空系统回收站（不可恢复）
#[cfg(windows)]
pub fn empty_system_bin() -> Result<(), String> {
    use windows_sys::Win32::UI::Shell::{SHEmptyRecycleBinW, SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI};
    let result = unsafe { SHEmptyRecycleBinW(std::ptr::null_mut(), std::ptr::null(), SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI) };
    if result == 0 {
        Ok(())
    } else {
        Err(format!("清空系统回收站失败，错误码 {result}"))
    }
}

#[cfg(not(windows))]
pub fn empty_system_bin() -> Result<(), String> {
    Err("此操作仅支持 Windows".into())
}

/// 历史清理快照（清理前自动记录，供后续对比查看）
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupSnapshot {
    pub id: String,
    pub created_at: String,
    pub drive: String,
    pub entries: Vec<SnapshotEntry>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotEntry {
    pub source: String,
    pub label: String,
    pub paths: Vec<SnapshotPath>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotPath {
    pub path: String,
    pub size: u64,
    pub modified_days: Option<u64>,
}

pub fn save_cleanup_snapshot(
    drive: &str,
    from_entries: Vec<SnapshotEntry>,
) -> Result<String, String> {
    let dir = bin_dir()?.parent().unwrap().join("cleanup-snapshots");
    fs::create_dir_all(&dir).map_err(|e| format!("无法创建快照目录: {e}"))?;
    let id = now_string();
    let snapshot = CleanupSnapshot {
        id: id.clone(),
        created_at: now_string(),
        drive: drive.to_string(),
        entries: from_entries,
    };
    let path = dir.join(format!("{id}.json"));
    let content = serde_json::to_string_pretty(&snapshot)
        .map_err(|e| format!("序列化快照失败: {e}"))?;
    fs::write(path, content).map_err(|e| format!("保存快照失败: {e}"))?;
    Ok(id)
}

pub fn list_cleanup_snapshots() -> Result<Vec<CleanupSnapshot>, String> {
    let dir = bin_dir()?.parent().unwrap().join("cleanup-snapshots");
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut result = Vec::new();
    for entry in fs::read_dir(&dir).map_err(|e| format!("读取快照目录失败: {e}"))? {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.path().extension().map(|e| e == "json").unwrap_or(false) {
            if let Ok(content) = fs::read_to_string(entry.path()) {
                if let Ok(snap) = serde_json::from_str::<CleanupSnapshot>(&content) {
                    result.push(snap);
                }
            }
        }
    }
    result.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(result)
}

pub fn delete_cleanup_snapshot(id: &str) -> Result<(), String> {
    let dir = bin_dir()?.parent().unwrap().join("cleanup-snapshots");
    let path = dir.join(format!("{id}.json"));
    if path.exists() {
        fs::remove_file(path).map_err(|e| format!("删除快照失败: {e}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("disk-analyzer-recycle-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn records_cleanup_entries_and_caps_at_200() {
        let root = temp_root("record");
        init(root.join("bin")).unwrap();
        let src = root.join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("a.txt"), "hello".as_bytes()).unwrap();

        record_cleanup(vec![src.join("a.txt")], "测试", "单测文件").unwrap();
        let summary = list_entries().unwrap();
        assert_eq!(summary.entries.len(), 1);
        assert_eq!(summary.entries[0].file_count, 1);
        assert_eq!(summary.total_bytes, 5);

        for _ in 0..210 {
            record_cleanup(vec![src.join("a.txt")], "测试", "批量").unwrap();
        }
        assert_eq!(list_entries().unwrap().entries.len(), 200);

        clear_entries().unwrap();
        assert!(list_entries().unwrap().entries.is_empty());
        let _ = fs::remove_dir_all(&root);
    }
}