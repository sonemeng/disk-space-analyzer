#![cfg_attr(windows, windows_subsystem = "windows")]

mod media;

use media::{MediaScanOptions, MediaScanResult, RecycleResult};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};
use tauri::{AppHandle, Emitter, State};

const LARGE_FILE_BYTES: u64 = 100 * 1024 * 1024;
const CLEANUP_MIN_AGE: Duration = Duration::from_secs(24 * 60 * 60);

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
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CleanupReport {
    items: Vec<CleanupItem>,
    safe_bytes: u64,
    review_bytes: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CleanupResult {
    freed_bytes: u64,
    deleted_files: u64,
    failed_items: u64,
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
    id: &'static str,
    name: &'static str,
    description: &'static str,
    paths: Vec<PathBuf>,
    action: &'static str,
    risk: &'static str,
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

fn build_file_types(values: HashMap<&'static str, u64>) -> Vec<CategoryItem> {
    let mut items = values
        .into_iter()
        .map(|(name, size)| {
            let color = match name {
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

fn build_age_buckets(values: HashMap<&'static str, (u64, u64)>) -> Vec<AgeBucket> {
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
) -> Result<ScanResult, String> {
    let started = Instant::now();
    let large_file_bytes = options.large_file_bytes.clamp(1024 * 1024, 1024_u64.pow(4));
    let usage = disk_usage(&drive)?;
    let root = PathBuf::from(format!("{}\\", drive));
    emit_progress(&app, "正在读取根目录", 3, Some(&root));

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

    let mut directories = Vec::new();
    let mut large_files = Vec::new();
    let mut scanned_files = 0;
    let mut scanned_dirs = 0;
    let mut skipped_items = 0;
    let mut file_type_sizes = HashMap::new();
    let mut age_sizes = HashMap::new();
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
                let total = file_type_sizes.entry(group).or_insert(0_u64);
                *total = total.saturating_add(size);
                let bucket = age_sizes
                    .entry(age_bucket(modified_days))
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
            return Err("扫描已取消".into());
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
            let total = file_type_sizes.entry(*name).or_insert(0_u64);
            *total = total.saturating_add(*size);
        }
        for (id, (size, count)) in &aggregate.age_buckets {
            let bucket = age_sizes.entry(*id).or_insert((0_u64, 0_u64));
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

#[tauri::command]
async fn start_scan(
    app: AppHandle,
    state: State<'_, AppState>,
    drive: String,
    options: ScanOptions,
) -> Result<ScanResult, String> {
    let drive = normalize_drive(&drive)?;
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut active = state.active_scan.lock().map_err(|_| "扫描状态不可用")?;
        if let Some(previous) = active.replace(cancel.clone()) {
            previous.store(true, Ordering::Relaxed);
        }
    }
    let result =
        tauri::async_runtime::spawn_blocking(move || run_scan(app, drive, options, cancel))
            .await
            .map_err(|e| format!("扫描任务异常: {e}"))?;
    if let Ok(mut active) = state.active_scan.lock() {
        *active = None;
    }
    result
}

fn folder_guidance(path: &Path) -> (&'static str, &'static str) {
    let value = path.to_string_lossy().to_ascii_lowercase();
    if value.contains("\\windows")
        || value.contains("\\program files")
        || value.contains("\\programdata")
        || value.contains("system32")
    {
        (
            "protected",
            "系统或程序目录，不建议手动删除；应使用卸载程序或 Windows 存储设置",
        )
    } else if [
        "\\temp",
        "\\cache",
        "\\node_modules",
        "\\target",
        "\\.gradle",
        "\\.npm",
    ]
    .iter()
    .any(|part| value.contains(part))
    {
        (
            "rebuildable",
            "通常可以重新生成；关闭相关应用并确认项目不在使用后再处理",
        )
    } else {
        (
            "review",
            "可能包含个人或项目数据，请先打开检查内容和最近修改时间",
        )
    }
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
    tauri::async_runtime::spawn_blocking(move || media::recycle_media_files(paths))
        .await
        .map_err(|error| format!("回收站任务异常: {error}"))?
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

fn cleanup_definitions() -> Vec<CleanupDefinition> {
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
            id: "user-temp",
            name: "用户临时文件",
            description: "应用安装、解压和运行产生的过期临时文件",
            paths: temp.into_iter().collect(),
            action: "safe",
            risk: "low",
        },
        CleanupDefinition {
            id: "browser-cache",
            name: "浏览器缓存",
            description: "Chrome 与 Edge 可重新下载的网页缓存，清理前建议关闭浏览器",
            paths: [
                path(
                    &local,
                    &["Google", "Chrome", "User Data", "Default", "Cache"],
                ),
                path(
                    &local,
                    &["Microsoft", "Edge", "User Data", "Default", "Cache"],
                ),
            ]
            .into_iter()
            .flatten()
            .collect(),
            action: "safe",
            risk: "low",
        },
        CleanupDefinition {
            id: "crash-dumps",
            name: "程序崩溃转储",
            description: "用于故障诊断的旧转储文件，不影响程序正常运行",
            paths: path(&local, &["CrashDumps"]).into_iter().collect(),
            action: "safe",
            risk: "low",
        },
        CleanupDefinition {
            id: "windows-temp",
            name: "Windows 临时目录",
            description: "超过 24 小时且未被系统占用的临时文件，无权限项目会自动跳过",
            paths: path(&windows, &["Temp"]).into_iter().collect(),
            action: "safe",
            risk: "low",
        },
        CleanupDefinition {
            id: "large-downloads",
            name: "下载目录大文件",
            description: "下载目录中超过 100 MB 的内容，需要确认用途后手动处理",
            paths: path(&profile, &["Downloads"]).into_iter().collect(),
            action: "review",
            risk: "medium",
        },
        CleanupDefinition {
            id: "windows-storage",
            name: "Windows 系统清理",
            description: "更新缓存、旧系统文件和回收站应交给 Windows 存储设置处理",
            paths: Vec::new(),
            action: "system",
            risk: "medium",
        },
    ]
}

fn file_age(metadata: &fs::Metadata) -> Duration {
    metadata
        .modified()
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .unwrap_or_default()
}

fn should_include_cleanup_file(id: &str, metadata: &fs::Metadata) -> bool {
    match id {
        "large-downloads" => metadata.len() >= LARGE_FILE_BYTES,
        "user-temp" | "browser-cache" | "crash-dumps" | "windows-temp" => {
            file_age(metadata) >= CLEANUP_MIN_AGE
        }
        _ => false,
    }
}

fn measure_cleanup(definition: &CleanupDefinition) -> (u64, u64) {
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
                if should_include_cleanup_file(definition.id, &metadata) {
                    size = size.saturating_add(metadata.len());
                    files += 1;
                }
            }
        }
    }
    (size, files)
}

fn analyze_cleanup_sync() -> CleanupReport {
    let mut safe_bytes = 0_u64;
    let mut review_bytes = 0_u64;
    let mut items = Vec::new();
    for definition in cleanup_definitions() {
        let (size, file_count) = measure_cleanup(&definition);
        if definition.action == "safe" {
            safe_bytes = safe_bytes.saturating_add(size);
        } else if definition.action == "review" {
            review_bytes = review_bytes.saturating_add(size);
        }
        let path = definition
            .paths
            .first()
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Windows 设置 > 系统 > 存储 > 临时文件".into());
        items.push(CleanupItem {
            id: definition.id.into(),
            name: definition.name.into(),
            description: definition.description.into(),
            path,
            size,
            file_count,
            action: definition.action.into(),
            risk: definition.risk.into(),
        });
    }
    items.sort_by(|a, b| {
        let action_order = |action: &str| match action {
            "safe" => 0,
            "review" => 1,
            _ => 2,
        };
        action_order(&a.action)
            .cmp(&action_order(&b.action))
            .then_with(|| b.size.cmp(&a.size))
    });
    CleanupReport {
        items,
        safe_bytes,
        review_bytes,
    }
}

#[tauri::command]
async fn analyze_cleanup() -> Result<CleanupReport, String> {
    tauri::async_runtime::spawn_blocking(analyze_cleanup_sync)
        .await
        .map_err(|e| format!("清理分析任务异常: {e}"))
}

fn clean_definition(definition: &CleanupDefinition) -> CleanupResult {
    let mut result = CleanupResult {
        freed_bytes: 0,
        deleted_files: 0,
        failed_items: 0,
    };
    if definition.action != "safe" {
        result.failed_items = 1;
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
            if entry.path() == root {
                continue;
            }
            if entry.file_type().is_dir() {
                let _ = fs::remove_dir(entry.path());
                continue;
            }
            let Ok(metadata) = entry.metadata() else {
                result.failed_items += 1;
                continue;
            };
            if !should_include_cleanup_file(definition.id, &metadata) {
                continue;
            }
            match fs::remove_file(entry.path()) {
                Ok(()) => {
                    result.freed_bytes = result.freed_bytes.saturating_add(metadata.len());
                    result.deleted_files += 1;
                }
                Err(_) => result.failed_items += 1,
            }
        }
    }
    result
}

#[tauri::command]
async fn clean_items(ids: Vec<String>) -> Result<CleanupResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let requested: std::collections::HashSet<_> = ids.into_iter().collect();
        let definitions = cleanup_definitions();
        let known: std::collections::HashSet<_> = definitions
            .iter()
            .filter(|definition| definition.action == "safe")
            .map(|definition| definition.id.to_string())
            .collect();
        if requested.is_empty() || !requested.is_subset(&known) {
            return Err("清理请求包含无效或不可自动处理的项目".into());
        }
        let mut total = CleanupResult {
            freed_bytes: 0,
            deleted_files: 0,
            failed_items: 0,
        };
        for definition in definitions
            .iter()
            .filter(|definition| requested.contains(definition.id))
        {
            let result = clean_definition(definition);
            total.freed_bytes = total.freed_bytes.saturating_add(result.freed_bytes);
            total.deleted_files += result.deleted_files;
            total.failed_items += result.failed_items;
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
        .invoke_handler(tauri::generate_handler![
            start_scan,
            analyze_folder,
            find_duplicates,
            scan_media,
            recycle_media,
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
        let definitions = cleanup_definitions();
        let automatic: Vec<_> = definitions
            .iter()
            .filter(|definition| definition.action == "safe")
            .map(|definition| definition.id)
            .collect();
        assert!(automatic.contains(&"user-temp"));
        assert!(automatic.contains(&"browser-cache"));
        assert!(!automatic.contains(&"large-downloads"));
        assert!(!automatic.contains(&"windows-storage"));
    }

    #[test]
    fn folder_guidance_protects_system_paths() {
        assert_eq!(
            folder_guidance(Path::new("C:\\Windows\\System32")).0,
            "protected"
        );
        assert_eq!(
            folder_guidance(Path::new("D:\\project\\node_modules")).0,
            "rebuildable"
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
