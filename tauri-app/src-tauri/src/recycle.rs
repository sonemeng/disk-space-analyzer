//! 应用内回收桶：删除 = 移入本地回收桶目录并记录原路径，
//! 支持一键还原到原位置（冲突时生成 " (还原)" 后缀，绝不覆盖）。
use crate::media::RecycleResult;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

/// 回收桶根目录（app_data_dir/recycle-bin），由 main.rs setup 时初始化
static BIN_ROOT: OnceLock<PathBuf> = OnceLock::new();
static SEQ: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredItem {
    /// 原路径（还原目标）
    pub original: String,
    /// 桶内相对路径（相对回收桶根目录）
    pub bin: String,
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

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreResult {
    pub restored: Vec<String>,
    pub failed: Vec<String>,
}

pub fn init(root: PathBuf) -> Result<(), String> {
    if BIN_ROOT.set(root.clone()).is_err() {
        return Ok(()); // 已初始化
    }
    fs::create_dir_all(&root).map_err(|e| format!("无法创建回收桶目录: {e}"))
}

fn bin_dir() -> Result<&'static PathBuf, String> {
    BIN_ROOT.get().ok_or_else(|| "回收桶未初始化".into())
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
    let content = fs::read_to_string(&path).map_err(|e| format!("读取回收记录失败: {e}"))?;
    if content.trim().is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str(&content).map_err(|e| format!("解析回收记录失败: {e}"))
}

fn write_entries(entries: &[RecycleEntry]) -> Result<(), String> {
    let path = entries_path()?;
    let content =
        serde_json::to_string_pretty(entries).map_err(|e| format!("序列化回收记录失败: {e}"))?;
    fs::write(path, content).map_err(|e| format!("保存回收记录失败: {e}"))
}

fn path_safe(path: &Path) -> Result<(), String> {
    if !path.is_absolute() {
        return Err("路径必须是绝对路径".into());
    }
    if path.starts_with(bin_dir()?) {
        return Err("不允许操作回收桶内部路径".into());
    }
    Ok(())
}

/// 递归复制（跨卷移动的退路）；仅累加字节数，文件数由 move_entry 统一结算
fn copy_recursive(source: &Path, dest: &Path, result: &mut RecycleResult) -> Result<(), String> {
    let metadata = fs::metadata(source).map_err(|e| format!("读取元数据失败: {e}"))?;
    if metadata.is_file() {
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).ok();
        }
        fs::copy(source, dest).map_err(|e| format!("复制失败: {e}"))?;
        fs::remove_file(source).map_err(|e| format!("删除源文件失败: {e}"))?;
        result.recycled_bytes = result.recycled_bytes.saturating_add(metadata.len());
        Ok(())
    } else if metadata.is_dir() {
        fs::create_dir_all(dest).map_err(|e| format!("创建目录失败: {e}"))?;
        for entry in fs::read_dir(source).map_err(|e| format!("读取目录失败: {e}"))? {
            let entry = entry.map_err(|e| e.to_string())?;
            copy_recursive(&entry.path(), &dest.join(entry.file_name()), result)?;
        }
        fs::remove_dir(source).ok();
        Ok(())
    } else {
        Err("不支持的路径类型".into())
    }
}

/// 移动（同卷 rename，跨卷复制+删除）；成功后累计 files/bytes
fn move_entry(
    source: &Path,
    dest: &Path,
    result: &mut RecycleResult,
) -> Result<(), String> {
    fs::rename(source, dest).or_else(|_| copy_recursive(source, dest, result))?;
    if let Ok(metadata) = fs::metadata(dest) {
        result.recycled_files += 1;
        result.recycled_bytes = result
            .recycled_bytes
            .saturating_add(if metadata.is_file() { metadata.len() } else { size_of_path(dest) });
    }
    Ok(())
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

fn count_files(path: &Path) -> u64 {
    if path.is_file() {
        return 1;
    }
    walkdir::WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .flatten()
        .filter(|e| e.file_type().is_file())
        .count() as u64
}

pub fn recycle_to_bin(
    paths: Vec<PathBuf>,
    source: &str,
    label: &str,
) -> Result<RecycleResult, String> {
    if paths.is_empty() || paths.len() > 2_000 {
        return Err("请选择 1 到 2000 个路径".into());
    }
    for p in &paths {
        path_safe(p)?;
    }
    let mut entries = read_entries()?;
    let id = now_string();
    let group_dir = bin_dir()?.join(&id);
    fs::create_dir_all(&group_dir).map_err(|e| format!("创建回收组目录失败: {e}"))?;

    let mut items = Vec::new();
    let mut result = RecycleResult::default();
    for source_path in paths {
        if !source_path.exists() {
            result.failed_items += 1;
            continue;
        }
        let seq = SEQ.fetch_add(1, Ordering::Relaxed);
        let name = source_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "unknown".into());
        let dest = group_dir.join(format!("{seq}-{name}"));
        match move_entry(&source_path, &dest, &mut result) {
            Ok(()) => {
                let relative = dest.strip_prefix(bin_dir()?).unwrap_or(&dest).to_string_lossy().into_owned();
                items.push(StoredItem {
                    original: source_path.to_string_lossy().into_owned(),
                    bin: relative,
                });
            }
            Err(_) => result.failed_items += 1,
        }
    }
    let entry = RecycleEntry {
        id: id.clone(),
        created_at: now_string(),
        source: source.to_string(),
        label: label.to_string(),
        total_bytes: items
            .iter()
            .map(|i| size_of_path(&bin_dir().unwrap().join(&i.bin)))
            .sum(),
        file_count: items
            .iter()
            .map(|i| count_files(&bin_dir().unwrap().join(&i.bin)))
            .sum(),
        items,
    };
    entries.push(entry);
    write_entries(&entries)?;
    Ok(result)
}

pub fn list_entries() -> Result<RecycleSummary, String> {
    let entries = read_entries()?;
    let total_bytes: u64 = entries.iter().map(|e| e.total_bytes).sum();
    Ok(RecycleSummary {
        entries,
        total_bytes,
    })
}

pub fn restore_entry(id: &str) -> Result<RestoreResult, String> {
    let mut entries = read_entries()?;
    let entry = entries
        .iter()
        .find(|e| e.id == id)
        .cloned()
        .ok_or_else(|| String::from("回收条目不存在"))?;
    let group_dir = bin_dir()?.join(&id);
    let mut restored = Vec::new();
    let mut failed = Vec::new();
    for item in &entry.items {
        let source = bin_dir()?.join(&item.bin);
        if !source.exists() {
            failed.push(format!("桶内缺失: {}", item.original));
            continue;
        }
        let original = PathBuf::from(&item.original);
        // 目标已存在时使用 " (还原)" 后缀，绝不覆盖
        let mut target = original.clone();
        if target.exists() {
            let name = target
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "item".into());
            let alt = if let Some(dot) = name.rfind('.') {
                let (stem, ext) = name.split_at(dot);
                format!("{stem} (还原){ext}")
            } else {
                format!("{name} (还原)")
            };
            target = target.with_file_name(alt);
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).ok();
        }
        match move_entry(&source, &target, &mut RecycleResult::default()) {
            Ok(()) => restored.push(target.to_string_lossy().into_owned()),
            Err(e) => failed.push(format!("{}: {e}", item.original)),
        }
    }
    let _ = fs::remove_dir_all(&group_dir);
    entries.retain(|e| e.id != id);
    write_entries(&entries)?;
    Ok(RestoreResult { restored, failed })
}

pub fn purge_entry(id: &str) -> Result<(), String> {
    let mut entries = read_entries()?;
    let _ = fs::remove_dir_all(bin_dir()?.join(id));
    entries.retain(|e| e.id != id);
    write_entries(&entries)
}

pub fn empty_bin() -> Result<(), String> {
    let root = bin_dir()?.to_path_buf();
    fs::remove_file(root.join("entries.json")).ok();
    for entry in fs::read_dir(&root).map_err(|e| format!("读取回收桶失败: {e}"))? {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.path().is_dir() {
            let _ = fs::remove_dir_all(entry.path());
        } else {
            let _ = fs::remove_file(entry.path());
        }
    }
    Ok(())
}

/// 历史清理快照（清理前自动记录，供后续对比还原）
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
        let dir = std::env::temp_dir().join(format!("disk-analyzer-recycle-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn recycle_and_restore_roundtrip() {
        let root = temp_root("roundtrip");
        let bin = root.join("bin");
        let source_dir = root.join("src");
        fs::create_dir_all(&source_dir.join("sub")).unwrap();
        fs::write(source_dir.join("a.txt"), "hello".as_bytes()).unwrap();
        fs::write(source_dir.join("sub").join("b.log"), "world".as_bytes()).unwrap();

        init(bin.clone()).unwrap();

        // 移入回收桶
        let result = recycle_to_bin(
            vec![
                source_dir.join("a.txt"),
                source_dir.join("sub").join("b.log"),
            ],
            "测试",
            "单测文件",
        )
        .unwrap();
        assert_eq!(result.recycled_files, 2);
        assert!(result.recycled_bytes > 0);
        assert!(!source_dir.join("a.txt").exists());

        // 列表可见
        let summary = list_entries().unwrap();
        assert_eq!(summary.entries.len(), 1);
        assert_eq!(summary.total_bytes, 10);

        // 还原到原位置（分两次还原模拟冲突：第二次 a.txt 目标已存在）
        let entry = summary.entries[0].clone();
        let restore = restore_entry(&entry.id).unwrap();
        assert_eq!(restore.restored.len(), 2);
        assert!(source_dir.join("a.txt").exists());
        assert!(source_dir.join("sub").join("b.log").exists());
        assert!(list_entries().unwrap().entries.is_empty());

        // 冲突：目标存在时生成 " (还原)" 后缀，不覆盖
        fs::write(source_dir.join("c.txt"), "new".as_bytes()).unwrap();
        let conflict = recycle_to_bin(
            vec![source_dir.join("c.txt")],
            "测试",
            "冲突",
        )
        .unwrap();
        assert_eq!(conflict.recycled_files, 1);
        fs::write(source_dir.join("c.txt"), "other".as_bytes()).unwrap();
        let c_entry = list_entries().unwrap().entries[0].clone();
        let restore2 = restore_entry(&c_entry.id).unwrap();
        assert_eq!(restore2.restored.len(), 1);
        assert!(restore2.restored[0].ends_with("(还原).txt"));

        // 清空
        let _ = recycle_to_bin(vec![source_dir.join("d.txt")], "测试", "清空").unwrap();
        empty_bin().unwrap();
        assert!(list_entries().unwrap().entries.is_empty());
        assert!(!bin.join("entries.json").exists());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rejects_bin_internal_paths() {
        let root = temp_root("safety");
        let bin = root.join("bin");
        init(bin.clone()).unwrap();
        let inside = bin.join("entries.json");
        let result = recycle_to_bin(vec![inside], "测试", "非法");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("回收桶内部"));
        let _ = fs::remove_dir_all(&root);
    }
}