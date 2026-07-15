use base64::{engine::general_purpose::STANDARD, Engine as _};
use img_hash::image::{DynamicImage, GenericImageView, ImageOutputFormat};
use img_hash::{HasherConfig, ImageHash};
use lofty::file::AudioFile;
use lofty::probe::Probe;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufReader, Cursor, Read};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Instant, SystemTime};
use tauri::{AppHandle, Emitter};

const MAX_RETURNED_ITEMS: usize = 2_000;
const MAX_THUMBNAILS: usize = 240;
const MAX_SIMILAR_IMAGES: usize = 1_500;

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaScanOptions {
    pub exclusions: Vec<String>,
    pub large_file_bytes: u64,
    pub threads: usize,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaItem {
    pub path: String,
    pub name: String,
    pub kind: String,
    pub format: String,
    pub size: u64,
    pub modified_days: Option<u64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub duration_ms: Option<u64>,
    pub codec: Option<String>,
    pub bitrate: Option<u64>,
    pub sample_rate: Option<u32>,
    pub lossless: bool,
    pub screenshot: bool,
    pub blurry: bool,
    pub blur_score: Option<f64>,
    pub oversized: bool,
    pub exact_group: Option<u32>,
    pub similar_group: Option<u32>,
    pub thumbnail: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaScanResult {
    pub scope: String,
    pub items: Vec<MediaItem>,
    pub image_count: u64,
    pub video_count: u64,
    pub audio_count: u64,
    pub image_bytes: u64,
    pub video_bytes: u64,
    pub audio_bytes: u64,
    pub exact_groups: u64,
    pub similar_groups: u64,
    pub duplicate_bytes: u64,
    pub screenshot_count: u64,
    pub blurry_count: u64,
    pub oversized_count: u64,
    pub elapsed_ms: u128,
    pub skipped_items: u64,
    pub ffprobe_available: bool,
    pub truncated: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecycleResult {
    pub recycled_files: u64,
    pub recycled_bytes: u64,
    pub failed_items: u64,
}

#[derive(Clone)]
struct MediaDraft {
    path: PathBuf,
    name: String,
    kind: &'static str,
    format: String,
    size: u64,
    modified_days: Option<u64>,
    width: Option<u32>,
    height: Option<u32>,
    duration_ms: Option<u64>,
    codec: Option<String>,
    bitrate: Option<u64>,
    sample_rate: Option<u32>,
    lossless: bool,
    screenshot: bool,
    blurry: bool,
    blur_score: Option<f64>,
    oversized: bool,
    exact_group: Option<u32>,
    similar_group: Option<u32>,
    thumbnail: Option<String>,
    perceptual_hash: Option<ImageHash>,
}

fn emit_progress(app: &AppHandle, message: impl Into<String>, percentage: u8, path: Option<&Path>) {
    let _ = app.emit(
        "media-progress",
        serde_json::json!({
            "message": message.into(),
            "percentage": percentage,
            "currentPath": path.map(|value| value.to_string_lossy().into_owned()),
        }),
    );
}

fn normalized_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_ascii_lowercase()
}

fn is_excluded(path: &Path, exclusions: &[String]) -> bool {
    let candidate = normalized_path(path);
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

fn media_kind(path: &Path) -> Option<(&'static str, String)> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    let kind = match extension.as_str() {
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "tif" | "tiff" => "image",
        "mp4" | "mkv" | "avi" | "mov" | "wmv" | "webm" | "m4v" | "mts" | "m2ts" => "video",
        "mp3" | "wav" | "flac" | "aac" | "ogg" | "m4a" | "wma" | "opus" | "aiff" => "audio",
        _ => return None,
    };
    Some((kind, extension.to_ascii_uppercase()))
}

pub fn is_supported_media(path: &Path) -> bool {
    media_kind(path).is_some()
}

fn modified_days(metadata: &fs::Metadata) -> Option<u64> {
    metadata
        .modified()
        .ok()
        .and_then(|value| SystemTime::now().duration_since(value).ok())
        .map(|value| value.as_secs() / 86_400)
}

fn is_screenshot(path: &Path) -> bool {
    let value = normalized_path(path);
    [
        "screenshot",
        "screenshots",
        "screen shot",
        "snipping",
        "截图",
        "截屏",
    ]
    .iter()
    .any(|keyword| value.contains(keyword))
}

fn blur_variance(image: &DynamicImage) -> f64 {
    let grayscale = image.thumbnail(320, 320).to_luma8();
    let (width, height) = grayscale.dimensions();
    if width < 3 || height < 3 {
        return 0.0;
    }
    let mut count = 0_f64;
    let mut sum = 0_f64;
    let mut sum_sq = 0_f64;
    for y in 1..height - 1 {
        for x in 1..width - 1 {
            let center = grayscale.get_pixel(x, y)[0] as f64;
            let laplacian = grayscale.get_pixel(x - 1, y)[0] as f64
                + grayscale.get_pixel(x + 1, y)[0] as f64
                + grayscale.get_pixel(x, y - 1)[0] as f64
                + grayscale.get_pixel(x, y + 1)[0] as f64
                - 4.0 * center;
            count += 1.0;
            sum += laplacian;
            sum_sq += laplacian * laplacian;
        }
    }
    let mean = sum / count;
    (sum_sq / count - mean * mean).max(0.0)
}

fn thumbnail_data(image: &DynamicImage) -> Option<String> {
    let thumbnail = image.thumbnail(320, 220);
    let mut output = Cursor::new(Vec::new());
    thumbnail
        .write_to(&mut output, ImageOutputFormat::Jpeg(76))
        .ok()?;
    Some(format!(
        "data:image/jpeg;base64,{}",
        STANDARD.encode(output.into_inner())
    ))
}

fn analyze_image(
    path: &Path,
    include_thumbnail: bool,
) -> Option<(u32, u32, f64, ImageHash, Option<String>)> {
    let image = img_hash::image::open(path).ok()?;
    let (width, height) = image.dimensions();
    let blur_score = blur_variance(&image);
    let hasher = HasherConfig::new().hash_size(8, 8).to_hasher();
    let hash = hasher.hash_image(&image);
    let thumbnail = include_thumbnail.then(|| thumbnail_data(&image)).flatten();
    Some((width, height, blur_score, hash, thumbnail))
}

fn analyze_audio(path: &Path) -> (Option<u64>, Option<u64>, Option<u32>) {
    let Ok(tagged) = Probe::open(path).and_then(|probe| probe.read()) else {
        return (None, None, None);
    };
    let properties = tagged.properties();
    (
        Some(properties.duration().as_millis() as u64),
        properties.audio_bitrate().map(|value| value as u64 * 1_000),
        properties.sample_rate(),
    )
}

#[cfg(windows)]
fn hidden_command(program: &str) -> Command {
    use std::os::windows::process::CommandExt;
    let mut command = Command::new(program);
    command.creation_flags(0x08000000);
    command
}

#[cfg(not(windows))]
fn hidden_command(program: &str) -> Command {
    Command::new(program)
}

fn ffprobe_available() -> bool {
    hidden_command("ffprobe")
        .arg("-version")
        .output()
        .is_ok_and(|output| output.status.success())
}

fn analyze_video(
    path: &Path,
    available: bool,
) -> (
    Option<u64>,
    Option<u32>,
    Option<u32>,
    Option<String>,
    Option<u64>,
) {
    if !available {
        return (None, None, None, None, None);
    }
    let output = hidden_command("ffprobe")
        .args([
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_entries",
            "format=duration,bit_rate:stream=codec_type,codec_name,width,height",
        ])
        .arg(path)
        .output();
    let Ok(output) = output else {
        return (None, None, None, None, None);
    };
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&output.stdout) else {
        return (None, None, None, None, None);
    };
    let video = value["streams"].as_array().and_then(|streams| {
        streams
            .iter()
            .find(|stream| stream["codec_type"] == "video")
    });
    let duration_ms = value["format"]["duration"]
        .as_str()
        .and_then(|value| value.parse::<f64>().ok())
        .map(|value| (value * 1_000.0) as u64);
    let bitrate = value["format"]["bit_rate"]
        .as_str()
        .and_then(|value| value.parse::<u64>().ok());
    (
        duration_ms,
        video
            .and_then(|stream| stream["width"].as_u64())
            .map(|value| value as u32),
        video
            .and_then(|stream| stream["height"].as_u64())
            .map(|value| value as u32),
        video
            .and_then(|stream| stream["codec_name"].as_str())
            .map(str::to_ascii_uppercase),
        bitrate,
    )
}

fn hash_file(path: &Path, cancel: &AtomicBool) -> Option<String> {
    let file = fs::File::open(path).ok()?;
    let mut reader = BufReader::with_capacity(1024 * 1024, file);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        let count = reader.read(&mut buffer).ok()?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Some(format!("{:x}", hasher.finalize()))
}

struct UnionFind {
    parents: Vec<usize>,
}

impl UnionFind {
    fn new(size: usize) -> Self {
        Self {
            parents: (0..size).collect(),
        }
    }

    fn find(&mut self, value: usize) -> usize {
        if self.parents[value] != value {
            self.parents[value] = self.find(self.parents[value]);
        }
        self.parents[value]
    }

    fn union(&mut self, left: usize, right: usize) {
        let left = self.find(left);
        let right = self.find(right);
        if left != right {
            self.parents[right] = left;
        }
    }
}

pub fn run_media_scan(
    app: AppHandle,
    scope: String,
    options: MediaScanOptions,
    cancel: Arc<AtomicBool>,
) -> Result<MediaScanResult, String> {
    let started = Instant::now();
    let root = PathBuf::from(&scope);
    if !root.is_absolute() || !root.is_dir() {
        return Err("请选择存在的绝对文件夹".into());
    }
    let large_file_bytes = options.large_file_bytes.clamp(1024 * 1024, 1024_u64.pow(4));
    let threads = options.threads.clamp(1, 16);
    let mut skipped_items = 0_u64;
    let mut drafts = Vec::new();
    emit_progress(&app, "正在查找图片、视频和音频", 2, Some(&root));
    let walker = walkdir::WalkDir::new(&root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| !is_excluded(entry.path(), &options.exclusions));
    for entry in walker {
        if cancel.load(Ordering::Relaxed) {
            return Err("媒体扫描已取消".into());
        }
        let Ok(entry) = entry else {
            skipped_items += 1;
            continue;
        };
        if !entry.file_type().is_file() {
            continue;
        }
        let Some((kind, format)) = media_kind(entry.path()) else {
            continue;
        };
        let Ok(metadata) = entry.metadata() else {
            skipped_items += 1;
            continue;
        };
        let size = metadata.len();
        drafts.push(MediaDraft {
            path: entry.path().to_path_buf(),
            name: entry.file_name().to_string_lossy().into_owned(),
            kind,
            format,
            size,
            modified_days: modified_days(&metadata),
            width: None,
            height: None,
            duration_ms: None,
            codec: None,
            bitrate: None,
            sample_rate: None,
            lossless: matches!(
                entry
                    .path()
                    .extension()
                    .and_then(|value| value.to_str())
                    .unwrap_or_default()
                    .to_ascii_lowercase()
                    .as_str(),
                "flac" | "wav" | "aiff"
            ),
            screenshot: is_screenshot(entry.path()),
            blurry: false,
            blur_score: None,
            oversized: size >= large_file_bytes,
            exact_group: None,
            similar_group: None,
            thumbnail: None,
            perceptual_hash: None,
        });
    }
    drafts.sort_by(|a, b| b.size.cmp(&a.size));
    let ffprobe_available = ffprobe_available();
    let thumbnail_paths = drafts
        .iter()
        .filter(|item| item.kind == "image")
        .take(MAX_THUMBNAILS)
        .map(|item| item.path.clone())
        .collect::<HashSet<_>>();
    emit_progress(&app, "正在读取媒体属性", 28, None);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()
        .map_err(|error| format!("无法创建媒体分析线程池: {error}"))?;
    pool.install(|| {
        drafts.par_iter_mut().for_each(|item| {
            if cancel.load(Ordering::Relaxed) {
                return;
            }
            match item.kind {
                "image" => {
                    if let Some((width, height, score, hash, thumbnail)) =
                        analyze_image(&item.path, thumbnail_paths.contains(&item.path))
                    {
                        item.width = Some(width);
                        item.height = Some(height);
                        item.blur_score = Some(score);
                        item.blurry = score < 55.0;
                        item.perceptual_hash = Some(hash);
                        item.thumbnail = thumbnail;
                    }
                }
                "audio" => {
                    let (duration, bitrate, sample_rate) = analyze_audio(&item.path);
                    item.duration_ms = duration;
                    item.bitrate = bitrate;
                    item.sample_rate = sample_rate;
                    item.codec = Some(item.format.clone());
                }
                "video" => {
                    let (duration, width, height, codec, bitrate) =
                        analyze_video(&item.path, ffprobe_available);
                    item.duration_ms = duration;
                    item.width = width;
                    item.height = height;
                    item.codec = codec.or_else(|| Some(item.format.clone()));
                    item.bitrate = bitrate;
                }
                _ => {}
            }
        });
    });
    if cancel.load(Ordering::Relaxed) {
        return Err("媒体扫描已取消".into());
    }

    emit_progress(&app, "正在校验完全重复文件", 62, None);
    let mut by_size: HashMap<u64, Vec<usize>> = HashMap::new();
    for (index, item) in drafts.iter().enumerate() {
        by_size.entry(item.size).or_default().push(index);
    }
    let candidates = by_size
        .into_values()
        .filter(|indices| indices.len() > 1)
        .flatten()
        .collect::<Vec<_>>();
    let hashes = pool.install(|| {
        candidates
            .par_iter()
            .filter_map(|index| hash_file(&drafts[*index].path, &cancel).map(|hash| (*index, hash)))
            .collect::<Vec<_>>()
    });
    let mut by_hash: HashMap<String, Vec<usize>> = HashMap::new();
    for (index, hash) in hashes {
        by_hash.entry(hash).or_default().push(index);
    }
    let mut exact_groups = 0_u64;
    let mut duplicate_bytes = 0_u64;
    for indices in by_hash.into_values().filter(|indices| indices.len() > 1) {
        exact_groups += 1;
        duplicate_bytes = duplicate_bytes.saturating_add(
            drafts[indices[0]]
                .size
                .saturating_mul(indices.len().saturating_sub(1) as u64),
        );
        for index in indices {
            drafts[index].exact_group = Some(exact_groups as u32);
        }
    }

    emit_progress(&app, "正在查找相似图片", 82, None);
    let image_indices = drafts
        .iter()
        .enumerate()
        .filter(|(_, item)| item.perceptual_hash.is_some())
        .map(|(index, _)| index)
        .take(MAX_SIMILAR_IMAGES)
        .collect::<Vec<_>>();
    let mut union = UnionFind::new(image_indices.len());
    for left in 0..image_indices.len() {
        for right in left + 1..image_indices.len() {
            let left_item = &drafts[image_indices[left]];
            let right_item = &drafts[image_indices[right]];
            if left_item.exact_group.is_some() && left_item.exact_group == right_item.exact_group {
                continue;
            }
            let distance = left_item
                .perceptual_hash
                .as_ref()
                .zip(right_item.perceptual_hash.as_ref())
                .map(|(left_hash, right_hash)| left_hash.dist(right_hash))
                .unwrap_or(u32::MAX);
            if distance <= 10 {
                union.union(left, right);
            }
        }
    }
    let mut similar_sets: HashMap<usize, Vec<usize>> = HashMap::new();
    for position in 0..image_indices.len() {
        similar_sets
            .entry(union.find(position))
            .or_default()
            .push(image_indices[position]);
    }
    let mut similar_groups = 0_u64;
    for indices in similar_sets
        .into_values()
        .filter(|indices| indices.len() > 1)
    {
        similar_groups += 1;
        for index in indices {
            drafts[index].similar_group = Some(similar_groups as u32);
        }
    }

    let image_count = drafts.iter().filter(|item| item.kind == "image").count() as u64;
    let video_count = drafts.iter().filter(|item| item.kind == "video").count() as u64;
    let audio_count = drafts.iter().filter(|item| item.kind == "audio").count() as u64;
    let image_bytes = drafts
        .iter()
        .filter(|item| item.kind == "image")
        .map(|item| item.size)
        .sum();
    let video_bytes = drafts
        .iter()
        .filter(|item| item.kind == "video")
        .map(|item| item.size)
        .sum();
    let audio_bytes = drafts
        .iter()
        .filter(|item| item.kind == "audio")
        .map(|item| item.size)
        .sum();
    let screenshot_count = drafts.iter().filter(|item| item.screenshot).count() as u64;
    let blurry_count = drafts.iter().filter(|item| item.blurry).count() as u64;
    let oversized_count = drafts.iter().filter(|item| item.oversized).count() as u64;
    let truncated = drafts.len() > MAX_RETURNED_ITEMS;
    let items = drafts
        .into_iter()
        .take(MAX_RETURNED_ITEMS)
        .map(|item| MediaItem {
            path: item.path.to_string_lossy().into_owned(),
            name: item.name,
            kind: item.kind.into(),
            format: item.format,
            size: item.size,
            modified_days: item.modified_days,
            width: item.width,
            height: item.height,
            duration_ms: item.duration_ms,
            codec: item.codec,
            bitrate: item.bitrate,
            sample_rate: item.sample_rate,
            lossless: item.lossless,
            screenshot: item.screenshot,
            blurry: item.blurry,
            blur_score: item.blur_score,
            oversized: item.oversized,
            exact_group: item.exact_group,
            similar_group: item.similar_group,
            thumbnail: item.thumbnail,
        })
        .collect();
    emit_progress(&app, "媒体分析完成", 100, Some(&root));
    Ok(MediaScanResult {
        scope,
        items,
        image_count,
        video_count,
        audio_count,
        image_bytes,
        video_bytes,
        audio_bytes,
        exact_groups,
        similar_groups,
        duplicate_bytes,
        screenshot_count,
        blurry_count,
        oversized_count,
        elapsed_ms: started.elapsed().as_millis(),
        skipped_items,
        ffprobe_available,
        truncated,
    })
}

pub fn recycle_media_files(paths: Vec<String>) -> Result<RecycleResult, String> {
    if paths.is_empty() || paths.len() > 1_000 {
        return Err("请选择 1 到 1000 个媒体文件".into());
    }
    let mut recycled_files = 0_u64;
    let mut recycled_bytes = 0_u64;
    let mut failed_items = 0_u64;
    for value in paths {
        let path = PathBuf::from(value);
        if !path.is_absolute() || !path.is_file() || media_kind(&path).is_none() {
            failed_items += 1;
            continue;
        }
        let size = fs::metadata(&path)
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        match trash::delete(&path) {
            Ok(()) => {
                recycled_files += 1;
                recycled_bytes = recycled_bytes.saturating_add(size);
            }
            Err(_) => failed_items += 1,
        }
    }
    Ok(RecycleResult {
        recycled_files,
        recycled_bytes,
        failed_items,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_supported_media_extensions() {
        assert_eq!(media_kind(Path::new("photo.jpg")).unwrap().0, "image");
        assert_eq!(media_kind(Path::new("movie.mkv")).unwrap().0, "video");
        assert_eq!(media_kind(Path::new("track.flac")).unwrap().0, "audio");
        assert!(media_kind(Path::new("notes.txt")).is_none());
    }

    #[test]
    fn exclusions_match_only_the_directory_boundary() {
        let exclusions = vec!["C:\\Media\\Private".into()];
        assert!(is_excluded(
            Path::new("C:\\Media\\Private\\photo.jpg"),
            &exclusions
        ));
        assert!(!is_excluded(
            Path::new("C:\\Media\\PrivateCopy\\photo.jpg"),
            &exclusions
        ));
    }

    #[test]
    fn detects_common_screenshot_names() {
        assert!(is_screenshot(Path::new(
            "C:\\Pictures\\Screenshots\\capture.png"
        )));
        assert!(is_screenshot(Path::new("D:\\截图 2026-07-15.png")));
        assert!(!is_screenshot(Path::new("D:\\Photos\\holiday.jpg")));
    }

    #[test]
    fn flat_images_have_low_blur_variance() {
        let image = DynamicImage::new_luma8(32, 32);
        assert_eq!(blur_variance(&image), 0.0);
    }
}
