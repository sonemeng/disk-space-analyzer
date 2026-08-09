#![cfg_attr(windows, windows_subsystem = "windows")]

mod app_cache_rules;
mod dev_rules;
mod media;
mod recycle;
mod registry;
mod tool_ai_rules;

use media::{MediaScanOptions, MediaScanResult, RecycleResult};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};
use tauri::{AppHandle, Emitter, Manager, State};

const LARGE_FILE_BYTES: u64 = 100 * 1024 * 1024;
const DEV_SCAN_MAX_DEPTH: usize = 8;
const DEV_MIN_BYTES: u64 = 5 * 1024 * 1024;

#[derive(Default)]
struct AppState {
    active_scan: Mutex<Option<Arc<AtomicBool>>>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScanProgress {
    message: String,
    percentage: u8,
    current_path: Option<String>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScanOptions {
    exclusions: Vec<String>,
    large_file_bytes: u64,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DiskUsage {
    total: u64,
    used: u64,
    free: u64,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DirectoryItem {
    path: String,
    name: String,
    size: u64,
    file_count: u64,
    dir_count: u64,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LargeFile {
    path: String,
    name: String,
    size: u64,
    modified_days: Option<u64>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CategoryItem {
    name: String,
    size: u64,
    color: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AgeBucket {
    id: String,
    label: String,
    size: u64,
    file_count: u64,
    color: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScanResult {
    drive: String,
    usage: DiskUsage,
    directories: Vec<DirectoryItem>,
    large_files: Vec<LargeFile>,
    categories: Vec<CategoryItem>,
    file_types: Vec<CategoryItem>,
    age_buckets: Vec<AgeBucket>,
    scanned_files: u64,
    scanned_dirs: u64,
    elapsed_ms: u128,
    skipped_items: u64,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CleanupItem {
    id: String,
    name: String,
    description: String,
    path: String,
    size: u64,
    file_count: u64,
    action: String,
    risk: String,
    /// fixed | developer | toolai | app
    category: String,
    /// 规则 ID（工具/AI 规则包）；固定/开发项可为空
    #[serde(default, skip_serializing_if = "Option::is_none")]
    rule_id: Option<String>,
    /// 规则包版本
    #[serde(default, skip_serializing_if = "Option::is_none")]
    rulepack_version: Option<String>,
    /// true 表示只读展示，禁止 clean_items
    #[serde(default)]
    readonly: bool,
    /// 模型等：前端须强确认，后端须 strong_confirm
    #[serde(default)]
    requires_strong_confirm: bool,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CleanupReport {
    items: Vec<CleanupItem>,
    safe_bytes: u64,
    review_bytes: u64,
    developer_bytes: u64,
    /// 工具/AI 缓存合计
    #[serde(default)]
    tool_ai_bytes: u64,
    /// 应用/社交通讯缓存合计
    #[serde(default)]
    app_cache_bytes: u64,
    #[serde(default)]
    rulepack_version: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CleanupResult {
    freed_bytes: u64,
    deleted_files: u64,
    failed_items: u64,
    dry_run: bool,
    skipped_hot: u64,
}



#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FolderItem {
    path: String,
    name: String,
    size: u64,
    file_count: u64,
    dir_count: u64,
    kind: String,
    risk: String,
    recommendation: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FolderAnalysis {
    path: String,
    name: String,
    total_size: u64,
    file_count: u64,
    dir_count: u64,
    children: Vec<FolderItem>,
    large_files: Vec<LargeFile>,
    elapsed_ms: u128,
    skipped_items: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DuplicateGroup {
    hash: String,
    size: u64,
    files: Vec<String>,
    wasted_bytes: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DuplicateReport {
    scope: String,
    groups: Vec<DuplicateGroup>,
    scanned_files: u64,
    hashed_files: u64,
    duplicate_files: u64,
    wasted_bytes: u64,
    elapsed_ms: u128,
    skipped_items: u64,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScanSnapshot {
    id: String,
    created_at: String,
    drive: String,
    total: u64,
    used: u64,
    free: u64,
    scanned_files: u64,
    directories: Vec<DirectoryItem>,
    file_types: Vec<CategoryItem>,
    age_buckets: Vec<AgeBucket>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateStatus {
    current_version: String,
    latest_version: Option<String>,
    available: bool,
    release_url: Option<String>,
    message: String,
}

struct CleanupDefinition {
    id: String,
    name: String,
    description: String,
    paths: Vec<PathBuf>,
    action: &'static str,
    risk: &'static str,
    category: &'static str,
    /// 开发者目录：整目录进回收站；固定白名单：按文件筛选
    whole_dir: bool,
    /// 已预计算体积（开发者扫描时填入，避免二次 WalkDir）
    precomputed_size: Option<(u64, u64)>,
}

#[derive(Default)]
struct DirAggregate {
    size: u64,
    files: u64,
    dirs: u64,
    skipped: u64,
    large_files: Vec<LargeFile>,
    file_types: HashMap<&'static str, u64>,
    age_buckets: HashMap<&'static str, (u64, u64)>,
}

fn emit_progress(app: &AppHandle, message: impl Into<String>, percentage: u8, path: Option<&Path>) {
    emit_progress_event(app, "scan-progress", message, percentage, path);
}

fn emit_progress_event(
    app: &AppHandle,
    event: &str,
    message: impl Into<String>,
    percentage: u8,
    path: Option<&Path>,
) {
    let _ = app.emit(
        event,
        ScanProgress {
            message: message.into(),
            percentage,
            current_path: path.map(|p| p.to_string_lossy().into_owned()),
        },
    );
}

fn normalize_drive(drive: &str) -> Result<String, String> {
    let trimmed = drive.trim().trim_end_matches(['\\', '/']);
    if trimmed.len() == 2
        && trimmed.as_bytes()[0].is_ascii_alphabetic()
        && trimmed.as_bytes()[1] == b':'
    {
        Ok(trimmed.to_ascii_uppercase())
    } else {
        Err("无效的盘符".into())
    }
}

fn metadata_age_days(metadata: &fs::Metadata) -> Option<u64> {
    metadata
        .modified()
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .map(|duration| duration.as_secs() / 86_400)
}

fn age_bucket(days: Option<u64>) -> &'static str {
    match days {
        Some(value) if value < 30 => "recent",
        Some(value) if value < 90 => "quarter",
        Some(value) if value < 365 => "year",
        Some(_) => "old",
        None => "unknown",
    }
}

fn path_is_excluded(path: &Path, exclusions: &[String]) -> bool {
    let candidate = path
        .to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_ascii_lowercase();
    exclusions.iter().any(|excluded| {
        let excluded = excluded
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_ascii_lowercase();
        !excluded.is_empty()
            && (candidate == excluded
                || candidate
                    .strip_prefix(&excluded)
                    .is_some_and(|suffix| suffix.starts_with('\\')))
    })
}

fn scan_directory(
    app: &AppHandle,
    path: &Path,
    cancel: &AtomicBool,
    percentage: u8,
    event: &str,
    exclusions: &[String],
    large_file_bytes: u64,
) -> DirAggregate {
    let mut result = DirAggregate::default();
    let walker = walkdir::WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| !path_is_excluded(entry.path(), exclusions));

    for entry in walker {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        match entry {
            Ok(entry) => {
                if entry.depth() == 0 {
                    continue;
                }
                let file_type = entry.file_type();
                if file_type.is_dir() {
                    result.dirs += 1;
                    continue;
                }
                if !file_type.is_file() {
                    continue;
                }
                result.files += 1;
                if result.files % 5_000 == 0 {
                    emit_progress_event(
                        app,
                        event,
                        format!("已读取 {} 个文件", result.files),
                        percentage,
                        Some(entry.path()),
                    );
                }
                match entry.metadata() {
                    Ok(metadata) => {
                        let size = metadata.len();
                        let modified_days = metadata_age_days(&metadata);
                        result.size = result.size.saturating_add(size);
                        let (group, _) = file_type_group(entry.path());
                        let group_size = result.file_types.entry(group).or_insert(0);
                        *group_size = group_size.saturating_add(size);
                        let bucket = result
                            .age_buckets
                            .entry(age_bucket(modified_days))
                            .or_insert((0, 0));
                        bucket.0 = bucket.0.saturating_add(size);
                        bucket.1 += 1;
                        if size >= large_file_bytes {
                            result.large_files.push(LargeFile {
                                path: entry.path().to_string_lossy().into_owned(),
                                name: entry.file_name().to_string_lossy().into_owned(),
                                size,
                                modified_days,
                            });
                        }
                    }
                    Err(_) => result.skipped += 1,
                }
            }
            Err(_) => result.skipped += 1,
        }
    }
    result
}

fn category_for(path: &str) -> (&'static str, &'static str) {
    let p = path.to_ascii_lowercase();
    if p.contains("windows") {
        ("系统文件", "#ef4444")
    } else if p.contains("program files") || p.contains("programdata") {
        ("应用程序", "#22c55e")
    } else if p.contains("appdata") || p.contains("cache") || p.contains("temp") {
        ("缓存与临时文件", "#f59e0b")
    } else if [
        "\\users",
        "desktop",
        "documents",
        "downloads",
        "pictures",
        "videos",
    ]
    .iter()
    .any(|part| p.contains(part))
    {
        ("用户文件", "#3b82f6")
    } else if [".cargo", ".gradle", ".android", "node_modules", ".cache"]
        .iter()
        .any(|part| p.contains(part))
    {
        ("开发工具", "#06b6d4")
    } else {
        ("其他", "#64748b")
    }
}

fn file_type_group(path: &Path) -> (&'static str, &'static str) {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "svg" | "heic" => ("图片", "#22c55e"),
        "mp4" | "mkv" | "avi" | "mov" | "wmv" | "webm" | "m4v" => ("视频", "#8b5cf6"),
        "mp3" | "wav" | "flac" | "aac" | "ogg" | "m4a" => ("音频", "#f97316"),
        "doc" | "docx" | "pdf" | "txt" | "xls" | "xlsx" | "ppt" | "pptx" | "md" | "csv" => {
            ("文档", "#3b82f6")
        }
        "zip" | "rar" | "7z" | "tar" | "gz" | "iso" => ("压缩与镜像", "#eab308"),
        "exe" | "msi" | "dll" | "appx" | "msix" => ("程序文件", "#ef4444"),
        "js" | "ts" | "vue" | "py" | "java" | "rs" | "go" | "cpp" | "h" | "json" | "xml"
        | "yaml" | "yml" | "toml" => ("开发文件", "#06b6d4"),
        _ => ("其他文件", "#64748b"),
    }
}

fn build_categories(items: &[DirectoryItem]) -> Vec<CategoryItem> {
    let mut values: HashMap<&'static str, (u64, &'static str)> = HashMap::new();
    for item in items {
        let (name, color) = category_for(&item.path);
        let entry = values.entry(name).or_insert((0, color));
        entry.0 = entry.0.saturating_add(item.size);
    }
    let mut categories: Vec<_> = values
        .into_iter()
        .map(|(name, (size, color))| CategoryItem {
            name: name.into(),
            size,
            color: color.into(),
        })
        .collect();
    categories.sort_by(|a, b| b.size.cmp(&a.size));
    categories
}

fn build_file_types(values: HashMap<String, u64>) -> Vec<CategoryItem> {
    let mut items = values
        .into_iter()
        .map(|(name, size)| {
            let color = match name.as_str() {
                "图片" => "#22c55e",
                "视频" => "#8b5cf6",
                "音频" => "#f97316",
                "文档" => "#3b82f6",
                "压缩与镜像" => "#eab308",
                "程序文件" => "#ef4444",
                "开发文件" => "#06b6d4",
                _ => "#64748b",
            };
            CategoryItem {
                name: name.into(),
                size,
                color: color.into(),
            }
        })
        .collect::<Vec<_>>();
    items.sort_by(|a, b| b.size.cmp(&a.size));
    items
}

fn build_age_buckets(values: HashMap<String, (u64, u64)>) -> Vec<AgeBucket> {
    [
        ("recent", "最近 30 天", "#22c55e"),
        ("quarter", "30–90 天", "#3b82f6"),
        ("year", "90–365 天", "#eab308"),
        ("old", "超过 1 年", "#ef4444"),
        ("unknown", "时间未知", "#64748b"),
    ]
    .into_iter()
    .map(|(id, label, color)| {
        let (size, file_count) = values.get(id).copied().unwrap_or_default();
        AgeBucket {
            id: id.into(),
            label: label.into(),
            size,
            file_count,
            color: color.into(),
        }
    })
    .collect()
}

fn run_scan(
    app: AppHandle,
    drive: String,
    options: ScanOptions,
    cancel: Arc<AtomicBool>,
    resume: Option<ResumeState>,
) -> Result<ScanResult, String> {
    let started = Instant::now();
    let large_file_bytes = options.large_file_bytes.clamp(1024 * 1024, 1024_u64.pow(4));
    let usage = disk_usage(&drive)?;
    let root = PathBuf::from(format!("{}\\", drive));
    emit_progress(&app, "正在读取根目录", 3, Some(&root));

    if let Some(state) = &resume {
        emit_progress(
            &app,
            format!("断点续扫：已跳过 {} 个目录", state.completed_roots.len()),
            5,
            None,
        );
    }

    let mut roots = Vec::new();
    let mut root_files = Vec::new();
    for entry in fs::read_dir(&root).map_err(|e| format!("无法读取 {}: {e}", root.display()))? {
        if cancel.load(Ordering::Relaxed) {
            return Err("扫描已取消".into());
        }
        if let Ok(entry) = entry {
            if let Ok(kind) = entry.file_type() {
                if path_is_excluded(&entry.path(), &options.exclusions) {
                    continue;
                }
                if kind.is_dir() {
                    roots.push(entry.path());
                } else if kind.is_file() {
                    root_files.push(entry.path());
                }
            }
        }
    }
    roots.sort_by_key(|path| path.to_string_lossy().to_ascii_lowercase());

let mut directories = resume
        .as_ref()
        .map(|state| state.completed_roots.clone())
        .unwrap_or_default();
    let mut large_files = resume
        .as_ref()
        .map(|state| state.large_files.clone())
        .unwrap_or_default();
    let mut scanned_files = resume
        .as_ref()
        .map(|state| state.completed_files)
        .unwrap_or(0);
    let mut scanned_dirs = resume
        .as_ref()
        .map(|state| state.completed_dirs)
        .unwrap_or(0);
    let mut skipped_items = resume
        .as_ref()
        .map(|state| state.completed_skipped)
        .unwrap_or(0);
    let mut file_type_sizes = resume
        .as_ref()
        .map(|state| state.file_type_sizes.clone())
        .unwrap_or_default();
    let mut age_sizes = resume
        .as_ref()
        .map(|state| state.age_sizes.clone())
        .unwrap_or_default();
    let done_paths: std::collections::HashSet<String> = resume
        .as_ref()
        .map(|state| {
            state
                .completed_roots
                .iter()
                .map(|item| item.path.to_ascii_lowercase())
                .collect()
        })
        .unwrap_or_default();
    let total_roots = roots.len().max(1);

    let mut root_size = 0_u64;
    for path in root_files {
        match fs::metadata(&path) {
            Ok(metadata) => {
                let size = metadata.len();
                let modified_days = metadata_age_days(&metadata);
                root_size = root_size.saturating_add(size);
                scanned_files += 1;
let (group, _) = file_type_group(&path);
                let total = file_type_sizes.entry(group.to_string()).or_insert(0_u64);
                *total = total.saturating_add(size);
                let bucket = age_sizes
                    .entry(age_bucket(modified_days).to_string())
                    .or_insert((0_u64, 0_u64));
                bucket.0 = bucket.0.saturating_add(size);
                bucket.1 += 1;
                if size >= large_file_bytes {
                    large_files.push(LargeFile {
                        path: path.to_string_lossy().into_owned(),
                        name: path
                            .file_name()
                            .map(|value| value.to_string_lossy().into_owned())
                            .unwrap_or_else(|| path.display().to_string()),
                        size,
                        modified_days,
                    });
                }
            }
            Err(_) => skipped_items += 1,
        }
    }
    if scanned_files > 0 {
        directories.push(DirectoryItem {
            path: root.to_string_lossy().into_owned(),
            name: "根目录文件".into(),
            size: root_size,
            file_count: scanned_files,
            dir_count: 0,
        });
    }

for (index, path) in roots.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Err("扫描已取消 (进度已保存，可下次继续扫描)".into());
        }
        if done_paths.contains(&path.to_string_lossy().to_ascii_lowercase()) {
            continue;
        }
        let percentage = 5 + ((index * 83 / total_roots) as u8);
        let name = path
            .file_name()
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        emit_progress(&app, format!("正在分析 {name}"), percentage, Some(path));

        let aggregate = scan_directory(
            &app,
            path,
            &cancel,
            percentage,
            "scan-progress",
            &options.exclusions,
            large_file_bytes,
        );
        scanned_files += aggregate.files;
        scanned_dirs += aggregate.dirs;
        skipped_items += aggregate.skipped;
        for (name, size) in &aggregate.file_types {
            let total = file_type_sizes.entry(name.to_string()).or_insert(0_u64);
            *total = total.saturating_add(*size);
        }
        for (id, (size, count)) in &aggregate.age_buckets {
            let bucket = age_sizes.entry(id.to_string()).or_insert((0_u64, 0_u64));
            bucket.0 = bucket.0.saturating_add(*size);
            bucket.1 += *count;
        }
        large_files.extend(aggregate.large_files);
        directories.push(DirectoryItem {
            path: path.to_string_lossy().into_owned(),
            name,
            size: aggregate.size,
            file_count: aggregate.files,
            dir_count: aggregate.dirs,
        });
        // 断点：每完成一个根目录即持久化进度
        write_resume(&ResumeState {
            drive: drive.clone(),
            started_at: String::new(),
            completed_roots: directories.clone(),
            completed_files: scanned_files,
            completed_dirs: scanned_dirs,
            completed_skipped: skipped_items,
            all_roots: roots
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect(),
            file_type_sizes: file_type_sizes.clone(),
            age_sizes: age_sizes.clone(),
            large_files: large_files.clone(),
        });
    }

    emit_progress(&app, "正在整理分析结果", 92, None);
    directories.sort_by(|a, b| b.size.cmp(&a.size));
    directories.truncate(50);
    large_files.sort_by(|a, b| b.size.cmp(&a.size));
    large_files.dedup_by(|a, b| a.path.eq_ignore_ascii_case(&b.path));
    large_files.truncate(25);
    let categories = build_categories(&directories);
    let file_types = build_file_types(file_type_sizes);
    let age_buckets = build_age_buckets(age_sizes);
emit_progress(&app, "扫描完成", 100, None);
    clear_resume(&drive);

    Ok(ScanResult {
        drive,
        usage,
        directories,
        large_files,
        categories,
        file_types,
        age_buckets,
        scanned_files,
        scanned_dirs,
        elapsed_ms: started.elapsed().as_millis(),
        skipped_items,
    })
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResumeState {
    drive: String,
    started_at: String,
    completed_roots: Vec<DirectoryItem>,
    completed_files: u64,
    completed_dirs: u64,
    completed_skipped: u64,
    all_roots: Vec<String>,
    file_type_sizes: HashMap<String, u64>,
    age_sizes: HashMap<String, (u64, u64)>,
    large_files: Vec<LargeFile>,
}

/// 应用数据根目录（app_data_dir），setup 时初始化
static DATA_ROOT: OnceLock<PathBuf> = OnceLock::new();

fn resume_file_path(drive: &str) -> PathBuf {
    DATA_ROOT
        .get()
        .cloned()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("scan-resume")
        .join(format!(
            "{}.json",
            drive.trim_end_matches(':').to_ascii_lowercase()
        ))
}

fn read_resume(drive: &str) -> Option<ResumeState> {
    let path = resume_file_path(drive);
    fs::read_to_string(path).ok().and_then(|c| serde_json::from_str(&c).ok())
}

fn write_resume(state: &ResumeState) {
    let path = resume_file_path(&state.drive);
    let _ = fs::create_dir_all(path.parent().unwrap_or_else(|| Path::new(".")));
    if let Ok(content) = serde_json::to_string(state) {
        let _ = fs::write(path, content);
    }
}

fn clear_resume(drive: &str) {
    let _ = fs::remove_file(resume_file_path(drive));
}

#[tauri::command]
fn has_pending_scan() -> Result<Option<ResumeState>, String> {
    for drive_letter in (b'A'..=b'Z').map(|b| (b as char).to_string()) {
        if let Some(state) = read_resume(&drive_letter) {
            return Ok(Some(state));
        }
    }
    Ok(None)
}

#[tauri::command]
async fn start_scan(
    app: AppHandle,
    state: State<'_, AppState>,
    drive: String,
    options: ScanOptions,
    resume: Option<bool>,
) -> Result<ScanResult, String> {
    let drive = normalize_drive(&drive)?;
    let resume_state = match resume.unwrap_or(false) {
        true => read_resume(&drive),
        false => {
            clear_resume(&drive);
            None
        }
    };
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut active = state.active_scan.lock().map_err(|_| "扫描状态不可用")?;
        if let Some(previous) = active.replace(cancel.clone()) {
            previous.store(true, Ordering::Relaxed);
        }
    }
    let result = tauri::async_runtime::spawn_blocking(move || {
        run_scan(app, drive, options, cancel, resume_state)
    })
    .await
    .map_err(|e| format!("扫描任务异常: {e}"))?;
    if let Ok(mut active) = state.active_scan.lock() {
        *active = None;
    }
    result
}

fn folder_guidance(path: &Path) -> (&'static str, &'static str) {
    let g = dev_rules::classify_path(path);
    (g.risk.as_str(), g.recommendation)
}

fn run_folder_analysis(
    app: AppHandle,
    folder: String,
    options: ScanOptions,
    cancel: Arc<AtomicBool>,
) -> Result<FolderAnalysis, String> {
    let started = Instant::now();
    let large_file_bytes = options.large_file_bytes.clamp(1024 * 1024, 1024_u64.pow(4));
    let root = PathBuf::from(folder);
    if !root.is_absolute() || !root.is_dir() {
        return Err("请选择存在的绝对文件夹路径".into());
    }
    emit_progress_event(&app, "folder-progress", "正在读取文件夹", 2, Some(&root));
    let mut entries = fs::read_dir(&root)
        .map_err(|e| format!("无法读取 {}: {e}", root.display()))?
        .filter_map(Result::ok)
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.file_name().to_string_lossy().to_ascii_lowercase());

    let mut children = Vec::new();
    let mut large_files = Vec::new();
    let mut total_size = 0_u64;
    let mut file_count = 0_u64;
    let mut dir_count = 0_u64;
    let mut skipped_items = 0_u64;
    let total_entries = entries.len().max(1);

    for (index, entry) in entries.into_iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Err("文件夹分析已取消".into());
        }
        let path = entry.path();
        if path_is_excluded(&path, &options.exclusions) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let percentage = 4 + ((index * 91 / total_entries) as u8);
        emit_progress_event(
            &app,
            "folder-progress",
            format!("正在分析 {name}"),
            percentage,
            Some(&path),
        );
        let file_type = match entry.file_type() {
            Ok(value) => value,
            Err(_) => {
                skipped_items += 1;
                continue;
            }
        };
        let (size, files, dirs, kind) = if file_type.is_dir() {
            let aggregate = scan_directory(
                &app,
                &path,
                &cancel,
                percentage,
                "folder-progress",
                &options.exclusions,
                large_file_bytes,
            );
            skipped_items += aggregate.skipped;
            large_files.extend(aggregate.large_files);
            (aggregate.size, aggregate.files, aggregate.dirs, "directory")
        } else if file_type.is_file() {
            match entry.metadata() {
                Ok(metadata) => {
                    let size = metadata.len();
                    let modified_days = metadata_age_days(&metadata);
                    if size >= large_file_bytes {
                        large_files.push(LargeFile {
                            path: path.to_string_lossy().into_owned(),
                            name: name.clone(),
                            size,
                            modified_days,
                        });
                    }
                    (size, 1, 0, "file")
                }
                Err(_) => {
                    skipped_items += 1;
                    (0, 0, 0, "file")
                }
            }
        } else {
            skipped_items += 1;
            (0, 0, 0, "link")
        };
        let (risk, recommendation) = folder_guidance(&path);
        total_size = total_size.saturating_add(size);
        file_count += files;
        dir_count += dirs + u64::from(file_type.is_dir());
        children.push(FolderItem {
            path: path.to_string_lossy().into_owned(),
            name,
            size,
            file_count: files,
            dir_count: dirs,
            kind: kind.into(),
            risk: risk.into(),
            recommendation: recommendation.into(),
        });
    }
    children.sort_by(|a, b| b.size.cmp(&a.size));
    large_files.sort_by(|a, b| b.size.cmp(&a.size));
    large_files.truncate(25);
    emit_progress_event(&app, "folder-progress", "文件夹分析完成", 100, Some(&root));
    let name = root
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| root.display().to_string());
    Ok(FolderAnalysis {
        path: root.to_string_lossy().into_owned(),
        name,
        total_size,
        file_count,
        dir_count,
        children,
        large_files,
        elapsed_ms: started.elapsed().as_millis(),
        skipped_items,
    })
}

#[tauri::command]
async fn analyze_folder(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    options: ScanOptions,
) -> Result<FolderAnalysis, String> {
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut active = state.active_scan.lock().map_err(|_| "扫描状态不可用")?;
        if let Some(previous) = active.replace(cancel.clone()) {
            previous.store(true, Ordering::Relaxed);
        }
    }
    let result = tauri::async_runtime::spawn_blocking(move || {
        run_folder_analysis(app, path, options, cancel)
    })
    .await
    .map_err(|e| format!("文件夹分析任务异常: {e}"))?;
    if let Ok(mut active) = state.active_scan.lock() {
        *active = None;
    }
    result
}

fn hash_file(path: &Path, cancel: &AtomicBool) -> Result<String, String> {
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut reader = BufReader::with_capacity(1024 * 1024, file);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("重复文件检测已取消".into());
        }
        let count = reader.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn run_duplicate_scan(
    app: AppHandle,
    scope: String,
    min_size: u64,
    exclusions: Vec<String>,
    cancel: Arc<AtomicBool>,
) -> Result<DuplicateReport, String> {
    let started = Instant::now();
    let root = PathBuf::from(&scope);
    if !root.is_absolute() || !root.is_dir() {
        return Err("请选择存在的绝对文件夹或磁盘路径".into());
    }
    let min_size = min_size.clamp(1024 * 1024, 10 * 1024 * 1024 * 1024);
    let mut by_size: HashMap<u64, Vec<PathBuf>> = HashMap::new();
    let mut scanned_files = 0_u64;
    let mut skipped_items = 0_u64;
    emit_progress_event(
        &app,
        "duplicate-progress",
        "正在按大小预筛文件",
        2,
        Some(&root),
    );
    let walker = walkdir::WalkDir::new(&root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| !path_is_excluded(entry.path(), &exclusions));
    for entry in walker {
        if cancel.load(Ordering::Relaxed) {
            return Err("重复文件检测已取消".into());
        }
        let Ok(entry) = entry else {
            skipped_items += 1;
            continue;
        };
        if !entry.file_type().is_file() {
            continue;
        }
        scanned_files += 1;
        if scanned_files % 5_000 == 0 {
            emit_progress_event(
                &app,
                "duplicate-progress",
                format!("已预筛 {scanned_files} 个文件"),
                12,
                Some(entry.path()),
            );
        }
        match entry.metadata() {
            Ok(metadata) if metadata.len() >= min_size => {
                by_size
                    .entry(metadata.len())
                    .or_default()
                    .push(entry.path().to_path_buf());
            }
            Ok(_) => {}
            Err(_) => skipped_items += 1,
        }
    }
    by_size.retain(|_, paths| paths.len() > 1);
    let candidate_count = by_size.values().map(Vec::len).sum::<usize>().max(1);
    let mut hashed_files = 0_u64;
    let mut by_hash: HashMap<(u64, String), Vec<String>> = HashMap::new();
    for (size, paths) in by_size {
        for path in paths {
            if cancel.load(Ordering::Relaxed) {
                return Err("重复文件检测已取消".into());
            }
            let percentage = 18 + ((hashed_files as usize * 77 / candidate_count) as u8);
            emit_progress_event(
                &app,
                "duplicate-progress",
                format!("正在计算哈希 {}/{}", hashed_files + 1, candidate_count),
                percentage,
                Some(&path),
            );
            match hash_file(&path, &cancel) {
                Ok(hash) => {
                    by_hash
                        .entry((size, hash))
                        .or_default()
                        .push(path.to_string_lossy().into_owned());
                    hashed_files += 1;
                }
                Err(error) if error == "重复文件检测已取消" => return Err(error),
                Err(_) => skipped_items += 1,
            }
        }
    }
    let mut groups = by_hash
        .into_iter()
        .filter(|(_, files)| files.len() > 1)
        .map(|((size, hash), files)| DuplicateGroup {
            wasted_bytes: size.saturating_mul((files.len() - 1) as u64),
            hash,
            size,
            files,
        })
        .collect::<Vec<_>>();
    groups.sort_by(|a, b| b.wasted_bytes.cmp(&a.wasted_bytes));
    groups.truncate(200);
    let duplicate_files = groups.iter().map(|group| group.files.len() as u64).sum();
    let wasted_bytes = groups.iter().map(|group| group.wasted_bytes).sum();
    emit_progress_event(
        &app,
        "duplicate-progress",
        "重复文件检测完成",
        100,
        Some(&root),
    );
    Ok(DuplicateReport {
        scope: root.to_string_lossy().into_owned(),
        groups,
        scanned_files,
        hashed_files,
        duplicate_files,
        wasted_bytes,
        elapsed_ms: started.elapsed().as_millis(),
        skipped_items,
    })
}

#[tauri::command]
async fn find_duplicates(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    min_size: u64,
    exclusions: Vec<String>,
) -> Result<DuplicateReport, String> {
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut active = state.active_scan.lock().map_err(|_| "扫描状态不可用")?;
        if let Some(previous) = active.replace(cancel.clone()) {
            previous.store(true, Ordering::Relaxed);
        }
    }
    let result = tauri::async_runtime::spawn_blocking(move || {
        run_duplicate_scan(app, path, min_size, exclusions, cancel)
    })
    .await
    .map_err(|e| format!("重复文件任务异常: {e}"))?;
    if let Ok(mut active) = state.active_scan.lock() {
        *active = None;
    }
    result
}

#[tauri::command]
async fn scan_media(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    options: MediaScanOptions,
) -> Result<MediaScanResult, String> {
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut active = state.active_scan.lock().map_err(|_| "扫描状态不可用")?;
        if let Some(previous) = active.replace(cancel.clone()) {
            previous.store(true, Ordering::Relaxed);
        }
    }
    let result = tauri::async_runtime::spawn_blocking(move || {
        media::run_media_scan(app, path, options, cancel)
    })
    .await
    .map_err(|error| format!("媒体扫描任务异常: {error}"))?;
    if let Ok(mut active) = state.active_scan.lock() {
        *active = None;
    }
    result
}

#[tauri::command]
async fn recycle_media(paths: Vec<String>) -> Result<RecycleResult, String> {
    let pathbufs: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
    tauri::async_runtime::spawn_blocking(move || {
        if pathbufs.is_empty() || pathbufs.len() > 1_000 {
            return Err("请选择 1 到 1000 个媒体文件".into());
        }
        recycle::recycle_to_bin(pathbufs, "媒体中心", "媒体文件")
    })
    .await
    .map_err(|error| format!("回收站任务异常: {error}"))?
}

/// 通用文件移入应用回收桶（文件审查 / 重复文件等）
#[tauri::command]
async fn recycle_paths(
    paths: Vec<String>,
    source: Option<String>,
    label: Option<String>,
) -> Result<RecycleResult, String> {
    let pathbufs: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
    let source = source.unwrap_or_else(|| "文件审查".into());
    let label = label.unwrap_or_else(|| "文件回收".into());
    tauri::async_runtime::spawn_blocking(move || recycle::recycle_to_bin(pathbufs, &source, &label))
        .await
        .map_err(|e| format!("回收站任务异常: {e}"))?
}

#[tauri::command]
async fn list_recycle_items() -> Result<recycle::RecycleSummary, String> {
    tauri::async_runtime::spawn_blocking(recycle::list_entries)
        .await
        .map_err(|e| format!("读取回收桶异常: {e}"))?
}

#[tauri::command]
async fn restore_recycle_item(id: String) -> Result<recycle::RestoreResult, String> {
    let id = id.clone();
    tauri::async_runtime::spawn_blocking(move || recycle::restore_entry(&id))
        .await
        .map_err(|e| format!("还原任务异常: {e}"))?
}

#[tauri::command]
async fn purge_recycle_item(id: String) -> Result<(), String> {
    let id = id.clone();
    tauri::async_runtime::spawn_blocking(move || recycle::purge_entry(&id))
        .await
        .map_err(|e| format!("删除回收条目异常: {e}"))?
}

#[tauri::command]
async fn empty_recycle_bin() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(recycle::empty_bin)
        .await
        .map_err(|e| format!("清空回收桶异常: {e}"))?
}

#[tauri::command]
async fn list_cleanup_snapshots() -> Result<Vec<recycle::CleanupSnapshot>, String> {
    tauri::async_runtime::spawn_blocking(recycle::list_cleanup_snapshots)
        .await
        .map_err(|e| format!("读取清理快照异常: {e}"))?
}

#[tauri::command]
async fn delete_cleanup_snapshot(id: String) -> Result<(), String> {
    let id = id.clone();
    tauri::async_runtime::spawn_blocking(move || recycle::delete_cleanup_snapshot(&id))
        .await
        .map_err(|e| format!("删除清理快照异常: {e}"))?
}

#[tauri::command]
async fn analyze_registry(
    app: AppHandle,
    options: Option<registry::RegistryScanOptions>,
) -> Result<registry::RegistryReport, String> {
    let options = options.unwrap_or_default();
    tauri::async_runtime::spawn_blocking(move || registry::scan_registry(options, Some(app)))
        .await
        .map_err(|error| format!("注册表检查任务异常: {error}"))?
}

#[tauri::command]
async fn repair_registry(ids: Vec<String>) -> Result<registry::RegistryRepairResult, String> {
    tauri::async_runtime::spawn_blocking(move || registry::repair_registry(ids))
        .await
        .map_err(|error| format!("注册表修复任务异常: {error}"))?
}

#[tauri::command]
async fn list_registry_backups() -> Result<Vec<registry::RegistryBackupInfo>, String> {
    tauri::async_runtime::spawn_blocking(registry::list_registry_backups)
        .await
        .map_err(|error| format!("读取注册表备份异常: {error}"))?
}

#[tauri::command]
async fn create_registry_backup(
    label: Option<String>,
    destination_dir: Option<String>,
) -> Result<registry::RegistryBackupInfo, String> {
    tauri::async_runtime::spawn_blocking(move || {
        registry::create_full_registry_backup(label, destination_dir)
    })
    .await
    .map_err(|error| format!("创建注册表备份异常: {error}"))?
}

#[tauri::command]
async fn restore_registry_backup(path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || registry::restore_registry_backup(path))
        .await
        .map_err(|error| format!("恢复注册表备份异常: {error}"))?
}

fn snapshot_file() -> Result<PathBuf, String> {
    let base = std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .ok_or("无法定位应用数据目录")?;
    let directory = base.join("DiskAnalyzer");
    fs::create_dir_all(&directory).map_err(|e| format!("无法创建快照目录: {e}"))?;
    Ok(directory.join("snapshots.json"))
}

fn read_snapshots() -> Result<Vec<ScanSnapshot>, String> {
    let path = snapshot_file()?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(&path).map_err(|e| format!("无法读取快照: {e}"))?;
    serde_json::from_str(&content).map_err(|e| format!("快照数据损坏: {e}"))
}

fn write_snapshots(snapshots: &[ScanSnapshot]) -> Result<(), String> {
    let content = serde_json::to_string_pretty(snapshots).map_err(|e| e.to_string())?;
    fs::write(snapshot_file()?, content).map_err(|e| format!("无法保存快照: {e}"))
}

#[tauri::command]
fn save_snapshot(result: ScanResult, limit: usize) -> Result<ScanSnapshot, String> {
    let now = chrono::Utc::now();
    let snapshot = ScanSnapshot {
        id: format!(
            "{}-{}",
            result.drive.replace(':', ""),
            now.timestamp_millis()
        ),
        created_at: now.to_rfc3339(),
        drive: result.drive.clone(),
        total: result.usage.total,
        used: result.usage.used,
        free: result.usage.free,
        scanned_files: result.scanned_files,
        directories: result.directories.into_iter().take(20).collect(),
        file_types: result.file_types,
        age_buckets: result.age_buckets,
    };
    let mut snapshots = read_snapshots()?;
    snapshots.push(snapshot.clone());
    snapshots.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    let limit = limit.clamp(2, 100);
    while snapshots
        .iter()
        .filter(|item| item.drive == snapshot.drive)
        .count()
        > limit
    {
        if let Some(index) = snapshots
            .iter()
            .position(|item| item.drive == snapshot.drive)
        {
            snapshots.remove(index);
        }
    }
    write_snapshots(&snapshots)?;
    Ok(snapshot)
}

#[tauri::command]
fn get_snapshots(drive: String) -> Result<Vec<ScanSnapshot>, String> {
    let drive = normalize_drive(&drive)?;
    let mut snapshots = read_snapshots()?
        .into_iter()
        .filter(|item| item.drive.eq_ignore_ascii_case(&drive))
        .collect::<Vec<_>>();
    snapshots.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    Ok(snapshots)
}

#[tauri::command]
fn clear_snapshots(drive: Option<String>) -> Result<u64, String> {
    let mut snapshots = read_snapshots()?;
    let before = snapshots.len();
    if let Some(drive) = drive {
        let drive = normalize_drive(&drive)?;
        snapshots.retain(|item| !item.drive.eq_ignore_ascii_case(&drive));
    } else {
        snapshots.clear();
    }
    let removed = before.saturating_sub(snapshots.len()) as u64;
    write_snapshots(&snapshots)?;
    Ok(removed)
}

#[tauri::command]
fn cancel_scan(state: State<'_, AppState>) -> Result<(), String> {
    if let Some(cancel) = state
        .active_scan
        .lock()
        .map_err(|_| "扫描状态不可用")?
        .as_ref()
    {
        cancel.store(true, Ordering::Relaxed);
    }
    Ok(())
}

#[tauri::command]
fn get_drives() -> Vec<String> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Storage::FileSystem::GetLogicalDrives;
        let mask = unsafe { GetLogicalDrives() };
        return (0..26)
            .filter(|index| mask & (1 << index) != 0)
            .map(|index| format!("{}:", (b'A' + index as u8) as char))
            .collect();
    }
    #[cfg(not(windows))]
    Vec::new()
}

#[tauri::command]
fn get_disk_usage(drive: String) -> Result<DiskUsage, String> {
    disk_usage(&normalize_drive(&drive)?)
}

#[cfg(windows)]
fn disk_usage(drive: &str) -> Result<DiskUsage, String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let path: Vec<u16> = std::ffi::OsStr::new(&format!("{drive}\\"))
        .encode_wide()
        .chain(Some(0))
        .collect();
    let mut free_available = 0_u64;
    let mut total = 0_u64;
    let mut free = 0_u64;
    let success =
        unsafe { GetDiskFreeSpaceExW(path.as_ptr(), &mut free_available, &mut total, &mut free) };
    if success == 0 {
        Err(format!("无法读取 {drive} 的容量信息"))
    } else {
        Ok(DiskUsage {
            total,
            used: total.saturating_sub(free),
            free,
        })
    }
}

#[cfg(not(windows))]
fn disk_usage(_drive: &str) -> Result<DiskUsage, String> {
    Err("磁盘容量查询仅支持 Windows".into())
}

/// Chrome/Edge 多配置 + 现代缓存目录（不只 Default\\Cache）。
fn browser_cache_paths(local: &Option<PathBuf>) -> Vec<PathBuf> {
    let Some(local) = local else {
        return Vec::new();
    };
    let mut paths = Vec::new();
    for browser in [
        local.join("Google").join("Chrome").join("User Data"),
        local.join("Microsoft").join("Edge").join("User Data"),
    ] {
        if !browser.is_dir() {
            continue;
        }
        // Default + Profile * + Guest Profile
        let Ok(entries) = fs::read_dir(&browser) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let is_profile = name == "Default"
                || name == "Guest Profile"
                || name == "System Profile"
                || name.starts_with("Profile ");
            if !is_profile || !entry.path().is_dir() {
                continue;
            }
            for sub in [
                "Cache",
                "Code Cache",
                "GPUCache",
                "Service Worker\\CacheStorage",
                "GrShaderCache",
                "ShaderCache",
            ] {
                let p = entry.path().join(sub);
                if p.is_dir() {
                    paths.push(p);
                }
            }
        }
    }
    paths
}

fn fixed_cleanup_definitions() -> Vec<CleanupDefinition> {
    let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    let temp = std::env::var_os("TEMP").map(PathBuf::from);
    let profile = std::env::var_os("USERPROFILE").map(PathBuf::from);
    let windows = std::env::var_os("WINDIR").map(PathBuf::from);
    let path = |base: &Option<PathBuf>, parts: &[&str]| {
        base.as_ref().map(|root| {
            parts
                .iter()
                .fold(root.clone(), |current, part| current.join(part))
        })
    };

    vec![
        CleanupDefinition {
            id: "user-temp".into(),
            name: "用户临时文件".into(),
            description: "应用安装、解压和运行产生的过期临时文件（移入回收站，跳过 2 小时内热文件）"
                .into(),
            paths: temp.into_iter().collect(),
            action: "safe",
            risk: "low",
            category: "fixed",
            whole_dir: false,
            precomputed_size: None,
        },
        CleanupDefinition {
            id: "browser-cache".into(),
            name: "浏览器缓存".into(),
            description: "Chrome/Edge 可重建网页缓存（含 Code Cache/GPUCache）；清理前请关闭浏览器".into(),
            paths: browser_cache_paths(&local),
            action: "safe",
            risk: "low",
            category: "fixed",
            whole_dir: false,
            precomputed_size: None,
        },
        CleanupDefinition {
            id: "crash-dumps".into(),
            name: "程序崩溃转储".into(),
            description: "用于故障诊断的旧转储文件，不影响程序正常运行".into(),
            paths: path(&local, &["CrashDumps"]).into_iter().collect(),
            action: "safe",
            risk: "low",
            category: "fixed",
            whole_dir: false,
            precomputed_size: None,
        },
        CleanupDefinition {
            id: "windows-temp".into(),
            name: "Windows 临时目录".into(),
            description: "超过 6 小时且未被占用的临时文件；无权限项自动跳过".into(),
            paths: path(&windows, &["Temp"]).into_iter().collect(),
            action: "safe",
            risk: "low",
            category: "fixed",
            whole_dir: false,
            precomputed_size: None,
        },
        CleanupDefinition {
            id: "large-downloads".into(),
            name: "下载目录大文件".into(),
            description: "下载目录中超过 100 MB 的内容，需要确认用途后手动处理".into(),
            paths: path(&profile, &["Downloads"]).into_iter().collect(),
            action: "review",
            risk: "medium",
            category: "fixed",
            whole_dir: false,
            precomputed_size: None,
        },
        CleanupDefinition {
            id: "windows-storage".into(),
            name: "Windows 系统清理".into(),
            description: "更新缓存、旧系统文件和回收站应交给 Windows 存储设置处理".into(),
            paths: Vec::new(),
            action: "system",
            risk: "medium",
            category: "fixed",
            whole_dir: false,
            precomputed_size: None,
        },
    ]
}

/// 明确是代码仓库根的目录名（不扫 Documents，避免拖死启动）
const PROJECT_DIR_NAMES: &[&str] = &[
    "Projects",
    "project",
    "projects",
    "code",
    "Code",
    "dev",
    "Dev",
    "workspace",
    "workspaces",
    "repos",
    "github",
    "GitHub",
    "source",
    "www",
    "work",
    "Work",
];

fn drive_letter(drive: &str) -> Option<char> {
    let trimmed = drive.trim().trim_end_matches(['\\', '/']);
    let bytes = trimmed.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let c = bytes[0] as char;
    if c.is_ascii_alphabetic() {
        Some(c.to_ascii_uppercase())
    } else {
        None
    }
}

fn path_on_drive(path: &Path, drive: &str) -> bool {
    let Some(want) = drive_letter(drive) else {
        return false;
    };
    let value = path.to_string_lossy();
    let bytes = value.as_bytes();
    bytes.len() >= 2
        && bytes[1] == b':'
        && (bytes[0] as char).eq_ignore_ascii_case(&want)
}

/// 仅扫描指定盘上的开发相关根路径。
/// 关键：绝不能把 `C:\Users` 整树当根深扫（极慢且易丢结果）；优先 Desktop/项目目录。
fn developer_scan_roots(drive: &str) -> Vec<(PathBuf, usize)> {
    let mut roots: Vec<(PathBuf, usize)> = Vec::new();
    let drive_letter_str = drive.trim().trim_end_matches(['\\', '/']);
    let drive_root = PathBuf::from(format!("{}\\", drive_letter_str));
    if !drive_root.is_dir() {
        return roots;
    }

    let skip_top = [
        "windows",
        "program files",
        "program files (x86)",
        "programdata",
        "$recycle.bin",
        "system volume information",
        "recovery",
        "perflogs",
        "boot",
        "efi",
        // 禁止整树扫 Users；下面单独加 Desktop/Documents/Projects
        "users",
    ];

    // 数据盘：盘根一级目录深扫（如 E:\Projects、E:\code）
    // 系统盘：跳过 Users，只扫其它非系统一级目录（通常很少）
    if let Ok(entries) = fs::read_dir(&drive_root) {
        for entry in entries.flatten().take(120) {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if skip_top.iter().any(|s| *s == name) {
                continue;
            }
            roots.push((path, DEV_SCAN_MAX_DEPTH));
        }
    }

    for name in PROJECT_DIR_NAMES {
        let path = drive_root.join(name);
        if path.is_dir() {
            roots.push((path, DEV_SCAN_MAX_DEPTH + 1));
        }
    }

    // 当前用户目录（C: 上开发项目几乎都在这里）
    if let Some(profile) = std::env::var_os("USERPROFILE").map(PathBuf::from) {
        if path_on_drive(&profile, drive) {
            for name in [
                "Desktop",
                "Documents",
                "Downloads",
                "Projects",
                "project",
                "projects",
                "code",
                "Code",
                "dev",
                "Dev",
                "source",
                "Source",
                "workspace",
                "workspaces",
                "repos",
                "github",
                "GitHub",
                "www",
                "work",
                "Work",
            ] {
                let path = profile.join(name);
                if path.is_dir() {
                    // Desktop 下常见 仓库/子目录/node_modules 或 src-tauri/target，深度要够
                    let depth = if name.eq_ignore_ascii_case("Desktop")
                        || name.eq_ignore_ascii_case("Documents")
                        || name.eq_ignore_ascii_case("Downloads")
                    {
                        DEV_SCAN_MAX_DEPTH + 2
                    } else {
                        DEV_SCAN_MAX_DEPTH + 1
                    };
                    roots.push((path, depth));
                }
            }
            // 用户主目录浅扫一层（覆盖直接放在用户根下的项目）
            roots.push((profile, 3));
        }
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
        if path_on_drive(&local, drive) {
            for extra in [
                local.join("npm-cache"),
                local.join("Yarn"),
                local.join("pnpm-cache"),
                local.join("pip"),
                local.join("Cargo"),
            ] {
                if extra.is_dir() {
                    roots.push((extra, 2));
                }
            }
        }
    }

    roots.sort_by(|a, b| a.0.cmp(&b.0));
    roots.dedup_by(|a, b| a.0 == b.0);
    roots
}

fn path_is_blacklisted(path: &Path, blacklist: &[String]) -> bool {
    if blacklist.is_empty() {
        return false;
    }
    let value = path.to_string_lossy().to_ascii_lowercase();
    blacklist.iter().any(|entry| {
        let needle = entry.trim().trim_end_matches(['\\', '/']).to_ascii_lowercase();
        !needle.is_empty() && (value == needle || value.starts_with(&format!("{needle}\\")))
    })
}

fn developer_cleanup_definitions(drive: &str, blacklist: &[String]) -> Vec<CleanupDefinition> {
    let mut seen = std::collections::HashSet::new();
    let mut items = Vec::new();
    for (root, depth) in developer_scan_roots(drive) {
        if path_is_blacklisted(&root, blacklist) {
            continue;
        }
        let found = dev_rules::find_rebuildable_dirs(&root, depth, DEV_MIN_BYTES);
        for entry in found {
            if !path_on_drive(&entry.path, drive) {
                continue;
            }
            if path_is_blacklisted(&entry.path, blacklist) {
                continue;
            }
            // 列表阶段不做热过滤：正在用的项目也应能看见；删除时再跳过热目录
            let key = entry.path.to_string_lossy().to_ascii_lowercase();
            if !seen.insert(key) {
                continue;
            }
            let hot = dev_rules::path_is_hot_shallow(&entry.path, dev_rules::HOT_PROTECT_AGE);
            let id = format!("dev:{}", entry.path.to_string_lossy());
            let hot_note = if hot {
                "；目录最近有活动，执行清理时可能被热保护跳过"
            } else {
                ""
            };
            items.push(CleanupDefinition {
                id,
                name: format!(
                    "{} · {}（{} 个文件）",
                    entry.label, entry.name, entry.file_count
                ),
                description: format!(
                    "{}（邻居验证通过；移入回收站可还原{}）",
                    entry.tip, hot_note
                ),
                paths: vec![entry.path],
                action: "safe",
                risk: "low",
                category: "developer",
                whole_dir: true,
                precomputed_size: Some((entry.size, entry.file_count)),
            });
        }
    }
    items.sort_by(|a, b| {
        let sa = a.precomputed_size.map(|(s, _)| s).unwrap_or(0);
        let sb = b.precomputed_size.map(|(s, _)| s).unwrap_or(0);
        sb.cmp(&sa)
    });
    items.truncate(50);
    items
}

fn cleanup_definitions(drive: &str, blacklist: &[String]) -> Vec<CleanupDefinition> {
    let mut all: Vec<CleanupDefinition> = fixed_cleanup_definitions()
        .into_iter()
        .filter(|definition| {
            if definition.paths.is_empty() {
                // Windows 存储设置：仅系统盘显示
                return drive.eq_ignore_ascii_case("C:");
            }
            definition
                .paths
                .iter()
                .any(|path| path_on_drive(path, drive) && !path_is_blacklisted(path, blacklist))
        })
        .map(|mut definition| {
            definition
                .paths
                .retain(|path| path_on_drive(path, drive) && !path_is_blacklisted(path, blacklist));
            definition
        })
        .collect();
    all.extend(developer_cleanup_definitions(drive, blacklist));
    all
}

fn should_include_cleanup_file(id: &str, metadata: &fs::Metadata) -> bool {
    if id.starts_with("dev:") {
        return false;
    }
    let age = dev_rules::file_age(metadata);
    match id {
        // 下载大文件：只按体积，人工复核
        "large-downloads" => metadata.len() >= LARGE_FILE_BYTES,
        // 浏览器缓存会被频繁访问，mtime 一直很新；只跳过 15 分钟内极热文件
        "browser-cache" => age >= Duration::from_secs(15 * 60),
        // 崩溃转储：1 小时后可清
        "crash-dumps" => age >= Duration::from_secs(60 * 60),
        // 系统/用户临时：6 小时（原 24h 过严 + 2h 热保护叠乘后常显示为空）
        "user-temp" | "windows-temp" => age >= Duration::from_secs(6 * 60 * 60),
        _ => false,
    }
}

fn measure_cleanup(definition: &CleanupDefinition) -> (u64, u64) {
    // 开发者项在发现阶段已计量；切勿在此处因“热目录”再清零，否则列表会只剩系统白名单
    if let Some(pre) = definition.precomputed_size {
        return pre;
    }
    if definition.whole_dir {
        let mut size = 0_u64;
        let mut files = 0_u64;
        for root in &definition.paths {
            if !root.is_dir() {
                continue;
            }
            for entry in walkdir::WalkDir::new(root)
                .follow_links(false)
                .max_open(32)
                .into_iter()
                .flatten()
            {
                if entry.file_type().is_file() {
                    if let Ok(metadata) = entry.metadata() {
                        size = size.saturating_add(metadata.len());
                        files += 1;
                    }
                }
                if files >= 80_000 {
                    break;
                }
            }
        }
        return (size, files);
    }
    let mut size = 0_u64;
    let mut files = 0_u64;
    for root in &definition.paths {
        if !root.exists() {
            continue;
        }
        for entry in walkdir::WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .flatten()
        {
            if !entry.file_type().is_file() {
                continue;
            }
            if let Ok(metadata) = entry.metadata() {
                if should_include_cleanup_file(&definition.id, &metadata) {
                    size = size.saturating_add(metadata.len());
                    files += 1;
                }
            }
        }
    }
    (size, files)
}

#[derive(Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct CleanupOptions {
    #[serde(default)]
    blacklist: Vec<String>,
    /// 活跃工作区路径前缀 → S0，不出现在工具/AI 列表
    #[serde(default)]
    protect_prefixes: Vec<String>,
    /// P3: 清理含模型项时必须为 true
    #[serde(default)]
    strong_confirm: bool,
}

fn analyze_cleanup_sync(
    drive: String,
    blacklist: Vec<String>,
    protect_prefixes: Vec<String>,
) -> Result<CleanupReport, String> {
    let drive = normalize_drive(&drive)?;
    let mut safe_bytes = 0_u64;
    let mut review_bytes = 0_u64;
    let mut developer_bytes = 0_u64;
    let mut tool_ai_bytes = 0_u64;
    let mut app_cache_bytes = 0_u64;
    let mut items = Vec::new();
    for definition in cleanup_definitions(&drive, &blacklist) {
        let (size, file_count) = measure_cleanup(&definition);
        if definition.action == "safe" {
            safe_bytes = safe_bytes.saturating_add(size);
            if definition.category == "developer" {
                developer_bytes = developer_bytes.saturating_add(size);
            }
        } else if definition.action == "review" {
            review_bytes = review_bytes.saturating_add(size);
        }
        let path = definition
            .paths
            .first()
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Windows 设置 > 系统 > 存储 > 临时文件".into());
        items.push(CleanupItem {
            id: definition.id.clone(),
            name: definition.name.clone(),
            description: definition.description.clone(),
            path,
            size,
            file_count,
            action: definition.action.into(),
            risk: definition.risk.into(),
            category: definition.category.into(),
            rule_id: None,
            rulepack_version: None,
            readonly: false,
            requires_strong_confirm: false,
        });
    }
    // P3: 工具/AI — B/C 默认可清；D 模型可清但 requires_strong_confirm
    for hit in tool_ai_rules::discover_tool_ai(&drive, &protect_prefixes, &blacklist) {
        tool_ai_bytes = tool_ai_bytes.saturating_add(hit.size);
        let strong = hit.requires_strong_confirm;
        let (action, risk) = if strong {
            // 计入 review 展示「高成本」，但仍 action=safe 以便勾选进 clean 列表
            review_bytes = review_bytes.saturating_add(hit.size);
            ("safe", "medium")
        } else {
            safe_bytes = safe_bytes.saturating_add(hit.size);
            ("safe", "low")
        };
        items.push(CleanupItem {
            id: hit.id,
            name: hit.name,
            description: hit.description,
            path: hit.path.to_string_lossy().into_owned(),
            size: hit.size,
            file_count: hit.file_count,
            action: action.into(),
            risk: risk.into(),
            category: "toolai".into(),
            rule_id: Some(hit.rule_id),
            rulepack_version: Some(tool_ai_rules::RULEPACK_VERSION.into()),
            readonly: false,
            requires_strong_confirm: strong,
        });
    }
    // 应用/社交通讯缓存
    for hit in app_cache_rules::discover_app_caches(&drive, &blacklist) {
        app_cache_bytes = app_cache_bytes.saturating_add(hit.size);
        let strong = hit.requires_strong_confirm;
        if strong {
            review_bytes = review_bytes.saturating_add(hit.size);
        } else {
            safe_bytes = safe_bytes.saturating_add(hit.size);
        }
        items.push(CleanupItem {
            id: hit.id,
            name: hit.name,
            description: hit.description,
            path: hit.path.to_string_lossy().into_owned(),
            size: hit.size,
            file_count: hit.file_count,
            action: "safe".into(),
            risk: if strong { "medium" } else { "low" }.into(),
            category: "app".into(),
            rule_id: Some(hit.rule_id),
            rulepack_version: Some(app_cache_rules::RULEPACK_VERSION.into()),
            readonly: false,
            requires_strong_confirm: strong,
        });
    }
    items.sort_by(|a, b| {
        let action_order = |action: &str| match action {
            "safe" => 0,
            "review" => 1,
            _ => 2,
        };
        let cat_order = |category: &str| match category {
            "fixed" => 0,
            "developer" => 1,
            "toolai" => 2,
            "app" => 3,
            _ => 4,
        };
        action_order(&a.action)
            .cmp(&action_order(&b.action))
            .then_with(|| cat_order(&a.category).cmp(&cat_order(&b.category)))
            .then_with(|| b.size.cmp(&a.size))
    });
    Ok(CleanupReport {
        items,
        safe_bytes,
        review_bytes,
        developer_bytes,
        tool_ai_bytes,
        app_cache_bytes,
        rulepack_version: format!(
            "tool:{}|app:{}",
            tool_ai_rules::RULEPACK_VERSION,
            app_cache_rules::RULEPACK_VERSION
        ),
    })
}

#[tauri::command]
async fn analyze_cleanup(
    drive: String,
    options: Option<CleanupOptions>,
) -> Result<CleanupReport, String> {
    let opts = options.unwrap_or_default();
    let blacklist = opts.blacklist;
    let protect_prefixes = opts.protect_prefixes;
    tauri::async_runtime::spawn_blocking(move || {
        let report = analyze_cleanup_sync(drive, blacklist, protect_prefixes)?;

        // 调试日志：便于确认开发项是否进入 IPC 返回
        let dev_n = report
            .items
            .iter()
            .filter(|i| i.category == "developer")
            .count();
        eprintln!(
            "[cleanup] items={} developer={} developer_bytes={}",
            report.items.len(),
            dev_n,
            report.developer_bytes
        );
        Ok(report)
    })
    .await
    .map_err(|e| format!("清理分析任务异常: {e}"))?
}

fn trash_path(path: &Path, queued: &mut Vec<PathBuf>) {
    queued.push(path.to_path_buf());
}

fn size_of(path: &Path) -> u64 {
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

fn flush_recycle(queued: &mut Vec<PathBuf>, source: &str, label: &str) -> Result<(), String> {
    if queued.is_empty() {
        return Ok(());
    }
    let paths = std::mem::take(queued);
    recycle::recycle_to_bin(paths, source, label)?;
    Ok(())
}

fn clean_definition(
    definition: &CleanupDefinition,
    dry_run: bool,
    queued: &mut Vec<PathBuf>,
) -> CleanupResult {
    let mut result = CleanupResult {
        freed_bytes: 0,
        deleted_files: 0,
        failed_items: 0,
        dry_run,
        skipped_hot: 0,
    };
    if definition.action != "safe" {
        result.failed_items = 1;
        return result;
    }

    if definition.whole_dir {
        for root in &definition.paths {
            if !root.is_dir() {
                result.failed_items += 1;
                continue;
            }
            // 再次邻居验证 + 仅高置信可清理 + 源码抽样拦截 + 热保护
            if !dev_rules::is_cleanup_eligible_rebuildable(root) {
                result.failed_items += 1;
                continue;
            }
            if dev_rules::looks_like_source_tree(root) {
                result.failed_items += 1;
                continue;
            }
            if dev_rules::path_is_hot(root, dev_rules::HOT_PROTECT_AGE) {
                result.skipped_hot += 1;
                continue;
            }
            let mut size = 0_u64;
            let mut files = 0_u64;
            for entry in walkdir::WalkDir::new(root)
                .follow_links(false)
                .into_iter()
                .flatten()
            {
                if entry.file_type().is_file() {
                    if let Ok(metadata) = entry.metadata() {
                        size = size.saturating_add(metadata.len());
                        files += 1;
                    }
                }
            }
            if dry_run {
                result.freed_bytes = result.freed_bytes.saturating_add(size);
                result.deleted_files = result.deleted_files.saturating_add(files.max(1));
                continue;
            }
trash_path(root, queued);
            result.freed_bytes = result.freed_bytes.saturating_add(size);
            result.deleted_files = result.deleted_files.saturating_add(files.max(1));
        }
        let _ = flush_recycle(queued, "清理中心", &definition.name);
        return result;
    }

    for root in &definition.paths {
        if !root.exists() {
            continue;
        }
        let entries = walkdir::WalkDir::new(root)
            .follow_links(false)
            .contents_first(true)
            .into_iter();
        for entry in entries {
            let Ok(entry) = entry else {
                result.failed_items += 1;
                continue;
            };
            if entry.path() == root.as_path() {
                continue;
            }
            if entry.file_type().is_dir() {
                // 不主动删空目录；回收站按文件移入后系统可整理
                continue;
            }
            let Ok(metadata) = entry.metadata() else {
                result.failed_items += 1;
                continue;
            };
            if !should_include_cleanup_file(&definition.id, &metadata) {
                if dev_rules::file_age(&metadata) < dev_rules::HOT_PROTECT_AGE {
                    result.skipped_hot += 1;
                }
                continue;
            }
            if dry_run {
                result.freed_bytes = result.freed_bytes.saturating_add(metadata.len());
                result.deleted_files += 1;
                continue;
            }
trash_path(entry.path(), queued);
                result.freed_bytes = result.freed_bytes.saturating_add(metadata.len());
                result.deleted_files += 1;
        }
    }
    let _ = flush_recycle(queued, "清理中心", &definition.name);
    result
}

fn clean_app_path(
    rule_id: &str,
    path: &Path,
    drive: &str,
    blacklist: &[String],
    dry_run: bool,
    queued: &mut Vec<PathBuf>,
) -> CleanupResult {
    let mut result = CleanupResult {
        freed_bytes: 0,
        deleted_files: 0,
        failed_items: 0,
        dry_run,
        skipped_hot: 0,
    };
    if !app_cache_rules::revalidate_app(rule_id, path, drive, blacklist) {
        result.failed_items = 1;
        return result;
    }
    if dev_rules::path_is_hot(path, dev_rules::HOT_PROTECT_AGE) {
        result.skipped_hot = 1;
        return result;
    }
    let mut size = 0_u64;
    let mut files = 0_u64;
    for entry in walkdir::WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .flatten()
    {
        if entry.file_type().is_file() {
            if let Ok(metadata) = entry.metadata() {
                size = size.saturating_add(metadata.len());
                files += 1;
            }
        }
    }
    if dry_run {
        result.freed_bytes = size;
        result.deleted_files = files.max(1);
        return result;
    }
    trash_path(path, queued);
    result.freed_bytes = size;
    result.deleted_files = files.max(1);
    let _ = flush_recycle(queued, "清理中心", rule_id);
    result
}

fn clean_toolai_path(
    rule_id: &str,
    path: &Path,
    drive: &str,
    protect_prefixes: &[String],
    blacklist: &[String],
    dry_run: bool,
    queued: &mut Vec<PathBuf>,
) -> CleanupResult {
    let mut result = CleanupResult {
        freed_bytes: 0,
        deleted_files: 0,
        failed_items: 0,
        dry_run,
        skipped_hot: 0,
    };
    if !tool_ai_rules::revalidate_cleanable(rule_id, path, drive, protect_prefixes, blacklist) {
        result.failed_items = 1;
        return result;
    }
    if dev_rules::path_is_hot(path, dev_rules::HOT_PROTECT_AGE) {
        result.skipped_hot = 1;
        return result;
    }
    // 拒绝误伤：目录名像源码树（极少见于包缓存，双保险）
    if dev_rules::looks_like_source_tree(path) {
        result.failed_items = 1;
        return result;
    }
    let mut size = 0_u64;
    let mut files = 0_u64;
    for entry in walkdir::WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .flatten()
    {
        if entry.file_type().is_file() {
            if let Ok(metadata) = entry.metadata() {
                size = size.saturating_add(metadata.len());
                files += 1;
            }
        }
    }
    if dry_run {
        result.freed_bytes = size;
        result.deleted_files = files.max(1);
        return result;
    }
    trash_path(path, queued);
    result.freed_bytes = size;
    result.deleted_files = files.max(1);
    let _ = flush_recycle(queued, "清理中心", rule_id);
    result
}

#[tauri::command]
async fn clean_items(
    drive: String,
    ids: Vec<String>,
    dry_run: Option<bool>,
    options: Option<CleanupOptions>,
) -> Result<CleanupResult, String> {
    let dry_run = dry_run.unwrap_or(false);
    let opts = options.unwrap_or_default();
    let blacklist = opts.blacklist;
    let protect_prefixes = opts.protect_prefixes;
    let strong_confirm = opts.strong_confirm;
    tauri::async_runtime::spawn_blocking(move || {
        let drive = normalize_drive(&drive)?;
        let requested: std::collections::HashSet<_> = ids.into_iter().collect();
        if requested.is_empty() {
            return Err("清理请求为空".into());
        }
        let tool_hits = tool_ai_rules::discover_tool_ai(&drive, &protect_prefixes, &blacklist);
        let app_hits = app_cache_rules::discover_app_caches(&drive, &blacklist);
        let needs_strong = requested.iter().any(|id| {
            tool_hits
                .iter()
                .any(|h| h.id == *id && h.requires_strong_confirm)
                || app_hits
                    .iter()
                    .any(|h| h.id == *id && h.requires_strong_confirm)
                || tool_ai_rules::parse_toolai_id(id)
                    .map(|(rid, _)| {
                        tool_hits
                            .iter()
                            .any(|h| h.rule_id == rid && h.requires_strong_confirm)
                    })
                    .unwrap_or(false)
                || app_cache_rules::parse_app_id(id)
                    .map(|(rid, _)| {
                        app_hits
                            .iter()
                            .any(|h| h.rule_id == rid && h.requires_strong_confirm)
                    })
                    .unwrap_or(false)
        });
        if needs_strong && !strong_confirm && !dry_run {
            return Err(
                "所选含模型/应用等高成本项，请勾选风险确认并完成确认词后再执行".into(),
            );
        }
        let definitions = cleanup_definitions(&drive, &blacklist);
        let mut known: std::collections::HashSet<_> = definitions
            .iter()
            .filter(|definition| definition.action == "safe")
            .map(|definition| definition.id.clone())
            .collect();
        for id in &requested {
            if id.starts_with("toolai:") {
                if let Some((rule_id, path)) = tool_ai_rules::parse_toolai_id(id) {
                    if tool_ai_rules::revalidate_cleanable(
                        &rule_id,
                        &path,
                        &drive,
                        &protect_prefixes,
                        &blacklist,
                    ) {
                        known.insert(id.clone());
                    }
                }
            } else if id.starts_with("app:") {
                if let Some((rule_id, path)) = app_cache_rules::parse_app_id(id) {
                    if app_cache_rules::revalidate_app(&rule_id, &path, &drive, &blacklist) {
                        known.insert(id.clone());
                    }
                }
            }
        }
        if !requested.is_subset(&known) {
            return Err("清理请求包含无效或不可自动处理的项目".into());
        }
        let mut total = CleanupResult {
            freed_bytes: 0,
            deleted_files: 0,
            failed_items: 0,
            dry_run,
            skipped_hot: 0,
        };
        // 清理前：收集本次将删除的文件信息，生成可对照的快照（复习还原）
        let mut snapshot_entries: Vec<recycle::SnapshotEntry> = Vec::new();
        for id in &requested {
            let mut paths: Vec<(PathBuf, u64)> = Vec::new();
            let mut label = id.clone();
            if id.starts_with("toolai:") {
                if let Some((rule_id, path)) = tool_ai_rules::parse_toolai_id(id) {
                    label = rule_id.clone();
                    let p = path.clone();
                    paths.push((path, size_of(&p)));
                }
            } else if id.starts_with("app:") {
                if let Some((rule_id, path)) = app_cache_rules::parse_app_id(id) {
                    label = rule_id.clone();
                    let p = path.clone();
                    paths.push((path, size_of(&p)));
                }
            } else if let Some(definition) = definitions.iter().find(|d| d.id == *id) {
                label = definition.name.clone();
                paths = definition
                    .paths
                    .iter()
                    .map(|p| (p.clone(), size_of(p)))
                    .collect();
            }
            if !dry_run && !paths.is_empty() {
                snapshot_entries.push(recycle::SnapshotEntry {
                    source: "清理中心".into(),
                    label,
                    paths: paths
                        .into_iter()
                        .map(|(path, size)| recycle::SnapshotPath {
                            path: path.to_string_lossy().into_owned(),
                            size,
                            modified_days: fs::metadata(&path)
                                .ok()
                                .and_then(|m| metadata_age_days(&m)),
                        })
                        .collect(),
                });
            }
        }
        let mut queued: Vec<PathBuf> = Vec::new();
        for id in &requested {
            if id.starts_with("toolai:") {
                if let Some((rule_id, path)) = tool_ai_rules::parse_toolai_id(id) {
                    let result = clean_toolai_path(
                        &rule_id,
                        &path,
                        &drive,
                        &protect_prefixes,
                        &blacklist,
                        dry_run,
                        &mut queued,
                    );
                    total.freed_bytes = total.freed_bytes.saturating_add(result.freed_bytes);
                    total.deleted_files += result.deleted_files;
                    total.failed_items += result.failed_items;
                    total.skipped_hot += result.skipped_hot;
                } else {
                    total.failed_items += 1;
                }
                continue;
            }
            if id.starts_with("app:") {
                if let Some((rule_id, path)) = app_cache_rules::parse_app_id(id) {
                    let result = clean_app_path(
                        &rule_id,
                        &path,
                        &drive,
                        &blacklist,
                        dry_run,
                        &mut queued,
                    );
                    total.freed_bytes = total.freed_bytes.saturating_add(result.freed_bytes);
                    total.deleted_files += result.deleted_files;
                    total.failed_items += result.failed_items;
                    total.skipped_hot += result.skipped_hot;
                } else {
                    total.failed_items += 1;
                }
                continue;
            }
            if let Some(definition) = definitions.iter().find(|d| d.id == *id) {
                let result = clean_definition(definition, dry_run, &mut queued);
                total.freed_bytes = total.freed_bytes.saturating_add(result.freed_bytes);
                total.deleted_files += result.deleted_files;
                total.failed_items += result.failed_items;
                total.skipped_hot += result.skipped_hot;
            }
        }
        if !dry_run && !snapshot_entries.is_empty() {
            if let Ok(_snap_id) =
                recycle::save_cleanup_snapshot(&drive, snapshot_entries.clone())
            {
                eprintln!("[cleanup] 已保存清理快照 {_snap_id}");
            }
        }
        Ok(total)
    })
    .await
    .map_err(|e| format!("清理任务异常: {e}"))?
}


#[cfg(windows)]
fn shell_open(target: &str) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let open: Vec<u16> = std::ffi::OsStr::new("open")
        .encode_wide()
        .chain(Some(0))
        .collect();
    let target: Vec<u16> = std::ffi::OsStr::new(target)
        .encode_wide()
        .chain(Some(0))
        .collect();
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            open.as_ptr(),
            target.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    if result as isize > 32 {
        Ok(())
    } else {
        Err("Windows 无法打开目标".into())
    }
}

#[cfg(not(windows))]
fn shell_open(_target: &str) -> Result<(), String> {
    Err("此操作仅支持 Windows".into())
}

#[tauri::command]
fn open_storage_settings() -> Result<(), String> {
    shell_open("ms-settings:storagesense")
}

#[tauri::command]
fn open_in_explorer(path: String, select_file: bool) -> Result<(), String> {
    let path = PathBuf::from(path);
    if !path.exists() {
        return Err("路径不存在".into());
    }
    let mut command = Command::new("explorer.exe");
    if select_file && path.is_file() {
        command.arg(format!("/select,{}", path.display()));
    } else {
        command.arg(path);
    }
    command
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("无法打开资源管理器: {e}"))
}

#[tauri::command]
fn open_media_file(path: String) -> Result<(), String> {
    let path = PathBuf::from(path);
    if !path.is_file() || !media::is_supported_media(&path) {
        return Err("媒体文件不存在或格式不受支持".into());
    }
    shell_open(&path.to_string_lossy())
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn format_size(bytes: u64) -> String {
    let mut value = bytes as f64;
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut unit = 0;
    while value >= 1024.0 && unit < units.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", units[unit])
}

fn resolve_output_directory(value: Option<String>) -> Result<PathBuf, String> {
    if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
        let path = PathBuf::from(value);
        if path.is_absolute() && path.is_dir() {
            return Ok(path);
        }
        return Err("报告保存位置不存在或不是绝对目录".into());
    }
    std::env::var("USERPROFILE")
        .map(PathBuf::from)
        .map(|path| path.join("Desktop"))
        .map_err(|_| "无法定位桌面目录".into())
}

#[tauri::command]
fn export_report(result: ScanResult, output_directory: Option<String>) -> Result<String, String> {
    let directory = resolve_output_directory(output_directory)?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let output = directory.join(format!(
        "磁盘分析报告-{}-{stamp}.html",
        result.drive.replace(':', "")
    ));
    let used_pct = if result.usage.total == 0 {
        0.0
    } else {
        result.usage.used as f64 / result.usage.total as f64 * 100.0
    };
    let directory_rows = result
        .directories
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let pct = if result.usage.used == 0 {
                0.0
            } else {
                item.size as f64 / result.usage.used as f64 * 100.0
            };
            format!(
                "<tr><td>{}</td><td title=\"{}\">{}</td><td>{}</td><td>{:.1}%</td><td>{}</td></tr>",
                index + 1,
                escape_html(&item.path),
                escape_html(&item.name),
                format_size(item.size),
                pct,
                item.file_count
            )
        })
        .collect::<Vec<_>>()
        .join("");
    let file_rows = result
        .large_files
        .iter()
        .enumerate()
        .map(|(index, item)| {
            format!(
                "<tr><td>{}</td><td title=\"{}\">{}</td><td>{}</td></tr>",
                index + 1,
                escape_html(&item.path),
                escape_html(&item.name),
                format_size(item.size)
            )
        })
        .collect::<Vec<_>>()
        .join("");
    let generated = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
    let html = format!(
        r#"<!doctype html><html lang="zh-CN"><meta charset="utf-8"><meta name="viewport" content="width=device-width"><title>磁盘分析报告 - {drive}</title><style>body{{margin:0;background:#f4f6f8;color:#17202a;font:14px "Microsoft YaHei",sans-serif}}main{{max-width:1100px;margin:auto;padding:32px}}h1{{font-size:24px}}.meta{{color:#667085}}.metrics{{display:grid;grid-template-columns:repeat(3,1fr);gap:12px;margin:24px 0}}.metric,section{{background:white;border:1px solid #e4e7ec;border-radius:6px;padding:18px}}.metric b{{display:block;font-size:22px;margin-top:5px}}section{{margin:12px 0;overflow:auto}}table{{width:100%;border-collapse:collapse}}th,td{{padding:10px;border-bottom:1px solid #eaecf0;text-align:left}}th{{font-size:12px;color:#667085}}@media(max-width:650px){{.metrics{{grid-template-columns:1fr}}main{{padding:16px}}}}</style><main><h1>{drive} 磁盘分析报告</h1><div class="meta">生成于 {generated} · 只读分析 · 用时 {elapsed:.1} 秒</div><div class="metrics"><div class="metric">总容量<b>{total}</b></div><div class="metric">已使用 {used_pct:.0}%<b>{used}</b></div><div class="metric">可用空间<b>{free}</b></div></div><section><h2>目录占用 TOP 50</h2><table><thead><tr><th>#</th><th>目录</th><th>大小</th><th>已用占比</th><th>文件数</th></tr></thead><tbody>{directory_rows}</tbody></table></section><section><h2>大文件 TOP 25</h2><table><thead><tr><th>#</th><th>文件</th><th>大小</th></tr></thead><tbody>{file_rows}</tbody></table></section></main></html>"#,
        drive = escape_html(&result.drive),
        generated = generated,
        elapsed = result.elapsed_ms as f64 / 1000.0,
        total = format_size(result.usage.total),
        used = format_size(result.usage.used),
        free = format_size(result.usage.free),
        used_pct = used_pct,
        directory_rows = directory_rows,
        file_rows = file_rows
    );
    fs::write(&output, html).map_err(|e| format!("报告写入失败: {e}"))?;
    let output_string = output.to_string_lossy().into_owned();
    shell_open(&output_string).map_err(|e| format!("报告已生成，但无法打开浏览器: {e}"))?;
    Ok(output_string)
}

#[tauri::command]
fn export_diagnostics(
    output_directory: Option<String>,
    settings: serde_json::Value,
) -> Result<String, String> {
    let directory = resolve_output_directory(output_directory)?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let output = directory.join(format!("磁盘空间分析器-诊断-{stamp}.json"));
    let snapshot_path = snapshot_file()?;
    let snapshots = read_snapshots().unwrap_or_default();
    let payload = serde_json::json!({
        "generatedAt": chrono::Local::now().to_rfc3339(),
        "application": "磁盘空间分析器",
        "version": env!("CARGO_PKG_VERSION"),
        "architecture": std::env::consts::ARCH,
        "operatingSystem": std::env::consts::OS,
        "snapshotFile": snapshot_path,
        "snapshotCount": snapshots.len(),
        "settings": settings,
    });
    fs::write(
        &output,
        serde_json::to_string_pretty(&payload).map_err(|error| error.to_string())?,
    )
    .map_err(|error| format!("诊断信息写入失败: {error}"))?;
    Ok(output.to_string_lossy().into_owned())
}

#[tauri::command]
async fn check_for_updates(repository: String) -> Result<UpdateStatus, String> {
    let valid = repository.split('/').count() == 2
        && repository
            .chars()
            .all(|value| value.is_ascii_alphanumeric() || matches!(value, '-' | '_' | '.' | '/'));
    if !valid {
        return Err("更新仓库格式应为 owner/repository".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let current = semver::Version::parse(env!("CARGO_PKG_VERSION"))
            .map_err(|error| format!("当前版本格式错误: {error}"))?;
        let url = format!("https://api.github.com/repos/{repository}/releases/latest");
        let response = ureq::get(&url)
            .set("Accept", "application/vnd.github+json")
            .set("User-Agent", "disk-space-analyzer")
            .timeout(Duration::from_secs(8))
            .call();
        let response = match response {
            Ok(value) => value,
            Err(ureq::Error::Status(404, _)) => {
                return Ok(UpdateStatus {
                    current_version: current.to_string(),
                    latest_version: None,
                    available: false,
                    release_url: Some(format!("https://github.com/{repository}/releases")),
                    message: "仓库尚未发布 GitHub Release".into(),
                })
            }
            Err(error) => return Err(format!("无法连接 GitHub 更新服务: {error}")),
        };
        let payload: serde_json::Value = response
            .into_json()
            .map_err(|error| format!("更新响应解析失败: {error}"))?;
        let tag = payload["tag_name"]
            .as_str()
            .ok_or("更新响应缺少版本号")?
            .trim_start_matches(['v', 'V']);
        let latest =
            semver::Version::parse(tag).map_err(|error| format!("远程版本格式错误: {error}"))?;
        let available = latest > current;
        Ok(UpdateStatus {
            current_version: current.to_string(),
            latest_version: Some(latest.to_string()),
            available,
            release_url: payload["html_url"].as_str().map(str::to_owned),
            message: if available {
                format!("发现新版本 {latest}")
            } else {
                "当前已是最新版本".into()
            },
        })
    })
    .await
    .map_err(|error| format!("更新检查任务异常: {error}"))?
}

fn main() {
tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .setup(|app| {
            let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
            let _ = DATA_ROOT.set(data_dir.clone());
            recycle::init(data_dir.join("recycle-bin"))?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
start_scan,
            has_pending_scan,
            analyze_folder,
            find_duplicates,
            scan_media,
            recycle_media,
            recycle_paths,
            list_recycle_items,
            restore_recycle_item,
            purge_recycle_item,
            empty_recycle_bin,
            list_cleanup_snapshots,
            delete_cleanup_snapshot,
            analyze_registry,
            repair_registry,
            list_registry_backups,
            create_registry_backup,
            restore_registry_backup,
            cancel_scan,
            get_drives,
            get_disk_usage,
            save_snapshot,
            get_snapshots,
            clear_snapshots,
            analyze_cleanup,
            clean_items,
            open_in_explorer,
            open_media_file,
            open_storage_settings,
            export_report,
            export_diagnostics,
            check_for_updates
        ])
        .run(tauri::generate_context!())
        .expect("启动磁盘空间分析器失败");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_and_normalizes_windows_drive_letters() {
        assert_eq!(normalize_drive("c:\\").unwrap(), "C:");
        assert_eq!(normalize_drive(" d: ").unwrap(), "D:");
        assert!(normalize_drive("C:\\Windows").is_err());
    }

    #[test]
    fn escapes_report_content() {
        assert_eq!(
            escape_html("A&B <\"file\">"),
            "A&amp;B &lt;&quot;file&quot;&gt;"
        );
    }

    #[test]
    fn groups_known_directory_categories() {
        assert_eq!(category_for("C:\\Windows\\WinSxS").0, "系统文件");
        assert_eq!(category_for("C:\\Users\\me\\Downloads").0, "用户文件");
        assert_eq!(category_for("D:\\Program Files\\Tool").0, "应用程序");
    }

    #[test]
    fn only_low_risk_cleanup_items_are_automatic() {
        let definitions = fixed_cleanup_definitions();
        let automatic: Vec<_> = definitions
            .iter()
            .filter(|definition| definition.action == "safe")
            .map(|definition| definition.id.as_str())
            .collect();
        assert!(automatic.contains(&"user-temp"));
        assert!(automatic.contains(&"browser-cache"));
        assert!(!automatic.iter().any(|id| *id == "large-downloads"));
        assert!(!automatic.iter().any(|id| *id == "windows-storage"));
    }

    #[test]
    fn cleanup_items_are_scoped_to_selected_drive() {
        assert!(path_on_drive(Path::new("C:\\Users\\a\\AppData\\Local\\Temp"), "C:"));
        assert!(!path_on_drive(Path::new("C:\\Users\\a\\AppData\\Local\\Temp"), "E:"));
        assert!(path_on_drive(Path::new("E:\\Projects\\app\\node_modules"), "E:"));
        assert!(!path_on_drive(Path::new("E:\\Projects\\app\\node_modules"), "C:"));
        // E: 上不应出现固定白名单（路径都在 C:）
        let e_defs = cleanup_definitions("E:", &[]);
        assert!(e_defs.iter().all(|d| d.category == "developer" || d.paths.iter().all(|p| path_on_drive(p, "E:"))));
        assert!(!e_defs.iter().any(|d| d.id == "user-temp" || d.id == "browser-cache"));
    }

    #[test]
    fn developer_roots_prefer_user_project_dirs_not_all_users() {
        let roots = developer_scan_roots("C:");
        // 不得把 C:\Users 整树当作深扫根
        assert!(
            !roots.iter().any(|(p, depth)| {
                let s = p.to_string_lossy().to_ascii_lowercase();
                (s.ends_with("\\users") || s.ends_with("\\users\\")) && *depth >= 4
            }),
            "must not deep-scan entire C:\\Users"
        );
        let has_desktop = roots.iter().any(|(p, _)| {
            p.file_name()
                .map(|n| n.to_string_lossy().eq_ignore_ascii_case("Desktop"))
                .unwrap_or(false)
        });
        assert!(has_desktop, "C: roots should include user Desktop when present");
    }

    #[test]
    fn developer_cleanup_finds_desktop_node_modules_on_this_machine() {
        let defs = developer_cleanup_definitions("C:", &[]);
        let dev: Vec<_> = defs.iter().filter(|d| d.category == "developer").collect();
        eprintln!("developer items on C: = {}", dev.len());
        for d in dev.iter().take(12) {
            eprintln!("  {} -> {}", d.name, d.paths[0].display());
        }
        // 本机 Desktop 上确有带 package.json 的 node_modules
        assert!(
            !dev.is_empty(),
            "expected developer cleanup items under Desktop/projects on this machine"
        );
        assert!(dev.iter().any(|d| {
            d.paths.iter().any(|p| {
                p.file_name()
                    .map(|n| n.to_string_lossy().eq_ignore_ascii_case("node_modules")
                        || n.to_string_lossy().eq_ignore_ascii_case("target")
                        || n.to_string_lossy().eq_ignore_ascii_case(".next"))
                    .unwrap_or(false)
            })
        }));
    }

    #[test]
    fn analyze_cleanup_report_includes_developer_items() {
        let report = analyze_cleanup_sync("C:".into(), vec![], vec![]).unwrap();
        let dev: Vec<_> = report
            .items
            .iter()
            .filter(|i| i.category == "developer")
            .collect();
        eprintln!(
            "report items={} developer={} developer_bytes={}",
            report.items.len(),
            dev.len(),
            report.developer_bytes
        );
        for i in dev.iter().take(8) {
            eprintln!("  {} size={} path={}", i.name, i.size, i.path);
        }
        assert!(
            !dev.is_empty() && report.developer_bytes > 0,
            "analyze_cleanup_sync must surface developer items with non-zero size"
        );
    }

    #[test]
    fn folder_guidance_protects_system_paths() {
        assert_eq!(
            folder_guidance(Path::new("C:\\Windows\\System32")).0,
            "protected"
        );
        // 无邻居标记时不得标可重建
        assert_eq!(
            folder_guidance(Path::new("D:\\project\\node_modules")).0,
            "review"
        );
        assert_eq!(
            folder_guidance(Path::new("C:\\Users\\me\\Documents")).0,
            "review"
        );
    }

    #[test]
    fn groups_common_file_types() {
        assert_eq!(file_type_group(Path::new("movie.mkv")).0, "视频");
        assert_eq!(file_type_group(Path::new("backup.iso")).0, "压缩与镜像");
        assert_eq!(file_type_group(Path::new("photo.webp")).0, "图片");
        assert_eq!(file_type_group(Path::new("main.rs")).0, "开发文件");
    }

    #[test]
    fn groups_file_age_boundaries() {
        assert_eq!(age_bucket(Some(0)), "recent");
        assert_eq!(age_bucket(Some(30)), "quarter");
        assert_eq!(age_bucket(Some(90)), "year");
        assert_eq!(age_bucket(Some(365)), "old");
        assert_eq!(age_bucket(None), "unknown");
    }

    #[test]
    fn hashes_identical_content_consistently() {
        let directory =
            std::env::temp_dir().join(format!("disk-analyzer-hash-test-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let first = directory.join("first.bin");
        let second = directory.join("second.bin");
        let different = directory.join("different.bin");
        fs::write(&first, b"same-content").unwrap();
        fs::write(&second, b"same-content").unwrap();
        fs::write(&different, b"different-content").unwrap();
        let cancel = AtomicBool::new(false);
        assert_eq!(
            hash_file(&first, &cancel).unwrap(),
            hash_file(&second, &cancel).unwrap()
        );
        assert_ne!(
            hash_file(&first, &cancel).unwrap(),
            hash_file(&different, &cancel).unwrap()
        );
        fs::remove_file(first).unwrap();
        fs::remove_file(second).unwrap();
        fs::remove_file(different).unwrap();
        fs::remove_dir(directory).unwrap();
    }
}

