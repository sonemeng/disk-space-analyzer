use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// 最近修改保护：2 小时内有活动的目录/文件视为“热”，跳过删除。
pub const HOT_PROTECT_AGE: Duration = Duration::from_secs(2 * 60 * 60);

/// 可重建目录：名称匹配 + 父目录存在标记文件（邻居验证）。
struct RebuildRule {
    names: &'static [&'static str],
    /// 父目录中任一文件存在即可确认
    markers: &'static [&'static str],
    /// 若为 true，父目录中存在任意 .py 也可确认（Python 缓存）
    allow_py_sibling: bool,
    /// 是否允许进入清理中心可勾选列表（false = 仅提示，不自动可清）
    cleanup_eligible: bool,
    label: &'static str,
    tip: &'static str,
}

const REBUILD_RULES: &[RebuildRule] = &[
    // —— 高置信：可进清理中心 ——
    RebuildRule {
        names: &["node_modules"],
        markers: &[
            "package.json",
            "package-lock.json",
            "pnpm-lock.yaml",
            "yarn.lock",
            "bun.lock",
            "bun.lockb",
        ],
        allow_py_sibling: false,
        cleanup_eligible: true,
        label: "Node.js 依赖",
        tip: "前端/Node 依赖目录；可用 npm/pnpm/yarn/bun install 重建。请先关闭相关开发服务。",
    },
    RebuildRule {
        names: &["target"],
        markers: &["Cargo.toml", "pom.xml", "build.gradle", "build.gradle.kts"],
        allow_py_sibling: false,
        cleanup_eligible: true,
        label: "构建产物 (target)",
        tip: "Rust/Java 等构建输出；可用 cargo clean 或重新编译生成。确认旁侧有 Cargo.toml 或 pom.xml。",
    },
    RebuildRule {
        names: &["__pycache__", ".pytest_cache", ".mypy_cache", ".ruff_cache"],
        markers: &["pyproject.toml", "setup.py", "setup.cfg", "requirements.txt", "Pipfile"],
        allow_py_sibling: true,
        cleanup_eligible: true,
        label: "Python 缓存",
        tip: "Python 字节码/工具缓存，删除后运行时会自动重建。",
    },
    RebuildRule {
        names: &[".gradle"],
        markers: &["build.gradle", "build.gradle.kts", "settings.gradle", "settings.gradle.kts"],
        allow_py_sibling: false,
        cleanup_eligible: true,
        label: "Gradle 缓存",
        tip: "Gradle 本地缓存/包装数据；可重新同步构建。",
    },
    RebuildRule {
        names: &[".next", ".nuxt", ".output", ".turbo", ".parcel-cache", ".svelte-kit"],
        markers: &["package.json", "next.config.js", "next.config.mjs", "next.config.ts", "nuxt.config.ts", "nuxt.config.js"],
        allow_py_sibling: false,
        cleanup_eligible: true,
        label: "前端框架缓存",
        tip: "Next/Nuxt 等框架构建缓存；重新 build/dev 会再生。",
    },
    RebuildRule {
        names: &["obj", "bin"],
        markers: &["*.csproj"],
        allow_py_sibling: false,
        cleanup_eligible: true,
        label: ".NET 构建产物",
        tip: "MSBuild 中间/输出目录；重新编译可生成。",
    },
    // —— 中置信：仅文件夹分析提示，不进清理中心勾选 ——
    RebuildRule {
        names: &["vendor"],
        markers: &["composer.json", "composer.lock", "go.mod"],
        allow_py_sibling: false,
        cleanup_eligible: false,
        label: "Vendor 依赖",
        tip: "可能含第三方依赖；请确认不是自维护源码后再手动清理。",
    },
    RebuildRule {
        names: &["build", "dist", "out"],
        markers: &[
            "package.json",
            "vite.config.ts",
            "vite.config.js",
            "webpack.config.js",
            "Cargo.toml",
            "CMakeLists.txt",
            "build.gradle",
            "build.gradle.kts",
            "pyproject.toml",
        ],
        allow_py_sibling: false,
        cleanup_eligible: false,
        label: "构建输出目录",
        tip: "名称常用于构建产物，也可能被用作源码目录；仅提示，不进入一键清理列表。",
    },
    RebuildRule {
        names: &[".cache", "Cache", "cache"],
        markers: &[],
        allow_py_sibling: false,
        cleanup_eligible: false,
        label: "缓存目录",
        tip: "通用缓存名；仅在工具缓存路径下提示，不进入一键清理列表。",
    },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathRisk {
    Protected,
    Rebuildable,
    Review,
}

impl PathRisk {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Protected => "protected",
            Self::Rebuildable => "rebuildable",
            Self::Review => "review",
        }
    }
}

pub struct Guidance {
    pub risk: PathRisk,
    pub recommendation: &'static str,
    pub label: Option<&'static str>,
    /// 是否允许出现在清理中心可勾选列表
    pub cleanup_eligible: bool,
}

fn name_eq(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

fn is_protected(path: &Path) -> bool {
    let value = path.to_string_lossy().to_ascii_lowercase();
    value.contains("\\windows\\")
        || value.ends_with("\\windows")
        || value.contains("\\program files")
        || value.contains("\\program files (x86)")
        || value.contains("\\programdata\\")
        || value.contains("\\system32")
        || value.contains("\\syswow64")
        || value.contains("\\winsxs")
}

fn parent_has_marker(parent: &Path, markers: &[&str], allow_py_sibling: bool) -> bool {
    let Ok(entries) = fs::read_dir(parent) else {
        return false;
    };
    let has_csproj = markers.iter().any(|m| *m == "*.csproj");
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        for marker in markers {
            if *marker == "*.csproj" {
                continue;
            }
            if name_eq(&name, marker) {
                return true;
            }
        }
        if has_csproj {
            if let Some(ext) = Path::new(name.as_ref()).extension() {
                if ext.eq_ignore_ascii_case("csproj") {
                    return true;
                }
            }
        }
        if allow_py_sibling {
            if let Some(ext) = Path::new(name.as_ref()).extension() {
                if ext.eq_ignore_ascii_case("py") {
                    return true;
                }
            }
        }
    }
    // 若 markers 为空，不在此函数确认
    false
}

/// 向上查找 monorepo / workspace 标记（最多 6 层）
fn ancestor_has_workspace_marker(start: &Path) -> bool {
    let markers = [
        "pnpm-workspace.yaml",
        "pnpm-workspace.yml",
        "lerna.json",
        "nx.json",
        "turbo.json",
    ];
    let mut current = Some(start);
    for _ in 0..6 {
        let Some(dir) = current else {
            break;
        };
        for marker in markers {
            if dir.join(marker).is_file() {
                return true;
            }
        }
        // package.json 内含 workspaces 字段
        let pkg = dir.join("package.json");
        if pkg.is_file() {
            if let Ok(text) = fs::read_to_string(&pkg) {
                if text.contains("\"workspaces\"") {
                    return true;
                }
            }
        }
        current = dir.parent();
    }
    false
}

/// 删除前浅层抽样：若目录内出现典型源码文件，拦截误删（依赖树内 .ts 很多，故只查顶层与一层子目录名）
pub fn looks_like_source_tree(path: &Path) -> bool {
    const SOURCE_EXTS: &[&str] = &[
        "rs", "go", "java", "kt", "cs", "cpp", "c", "h", "hpp", "py", "rb", "php", "swift",
    ];
    // 顶层文件
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten().take(60) {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            // 依赖目录顶层不应有 Cargo.toml / 整仓源码布局
            if name_eq(&name, "Cargo.toml")
                || name_eq(&name, "go.mod")
                || name_eq(&name, "pom.xml")
                || name_eq(&name, "CMakeLists.txt")
            {
                // node_modules 内几乎不会出现这些作为「项目根」；若出现则可疑
                if !name_eq(
                    &path
                        .file_name()
                        .map(|v| v.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    "node_modules",
                ) {
                    return true;
                }
            }
            let p = entry.path();
            if p.is_file() {
                if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                    let ext = ext.to_ascii_lowercase();
                    // node_modules 内 .ts/.js 极多，不把 ts/js 当拦截信号
                    if SOURCE_EXTS.iter().any(|s| *s == ext) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

fn is_user_tool_cache(path: &Path) -> bool {
    let value = path.to_string_lossy().to_ascii_lowercase();
    (value.contains("\\appdata\\local\\") || value.contains("\\appdata\\roaming\\") || value.contains("\\.cache\\"))
        && (value.contains("\\cache")
            || value.contains("\\caches")
            || value.contains("\\temp")
            || value.contains("\\tmp")
            || value.ends_with("\\.cache")
            || value.contains("\\npm-cache")
            || value.contains("\\pip\\cache")
            || value.contains("\\yarn\\cache")
            || value.contains("\\pnpm-store")
            || value.contains("\\cargo\\registry")
            || value.contains("\\cargo\\git"))
}

fn guidance_review(recommendation: &'static str, label: Option<&'static str>) -> Guidance {
    Guidance {
        risk: PathRisk::Review,
        recommendation,
        label,
        cleanup_eligible: false,
    }
}

/// 邻居验证：仅当目录名命中规则且父目录有工程标记时标为可重建。
pub fn classify_path(path: &Path) -> Guidance {
    if is_protected(path) {
        return Guidance {
            risk: PathRisk::Protected,
            recommendation: "系统或程序目录，不建议手动删除；应使用卸载程序或 Windows 存储设置",
            label: Some("系统保护"),
            cleanup_eligible: false,
        };
    }

    let folder_name = path
        .file_name()
        .map(|v| v.to_string_lossy().into_owned())
        .unwrap_or_default();
    if folder_name.is_empty() {
        return guidance_review("可能包含个人或项目数据，请先打开检查内容和最近修改时间", None);
    }

    // 源码目录名永不标可重建，杜绝把 src 等当缓存
    if matches!(
        folder_name.to_ascii_lowercase().as_str(),
        "src" | "source" | "sources" | "lib" | "libs" | "include" | "includes" | "app" | "apps" | "packages" | "components" | "pages" | "views" | "public" | "assets" | "static" | "resources" | "res" | "content" | "docs" | "doc" | "test" | "tests" | "spec" | "specs"
    ) {
        return guidance_review("常见源码/资源目录，不建议作为可重建项清理", Some("源码保护"));
    }

    let parent = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => {
            return guidance_review("可能包含个人或项目数据，请先打开检查内容和最近修改时间", None);
        }
    };

    for rule in REBUILD_RULES {
        if !rule.names.iter().any(|n| name_eq(&folder_name, n)) {
            continue;
        }
        // 通用 cache 名：仅工具缓存路径可标可重建，且默认不进清理列表
        if rule.markers.is_empty() {
            if is_user_tool_cache(path) {
                return Guidance {
                    risk: PathRisk::Rebuildable,
                    recommendation: rule.tip,
                    label: Some(rule.label),
                    cleanup_eligible: rule.cleanup_eligible,
                };
            }
            return guidance_review(
                "名称像缓存但未找到工程/工具标记，请打开确认后再处理",
                Some("疑似缓存"),
            );
        }
        if parent_has_marker(parent, rule.markers, rule.allow_py_sibling) {
            let recommendation = if name_eq(&folder_name, "node_modules")
                && ancestor_has_workspace_marker(parent)
            {
                "Node monorepo 依赖（检测到 pnpm-workspace / yarn workspaces / lerna / turbo）；可在仓库根 reinstall。请先关闭 dev server。"
            } else {
                rule.tip
            };
            return Guidance {
                risk: PathRisk::Rebuildable,
                recommendation,
                label: Some(rule.label),
                cleanup_eligible: rule.cleanup_eligible,
            };
        }
        return guidance_review(
            "目录名像构建产物，但旁侧未找到工程标记文件，请人工确认，勿直接删除",
            Some("未验证工程"),
        );
    }

    let lower = path.to_string_lossy().to_ascii_lowercase();
    if lower.contains("\\temp")
        || lower.contains("\\tmp")
        || lower.contains("\\logs")
        || lower.ends_with("\\log")
    {
        if is_protected(path) {
            return Guidance {
                risk: PathRisk::Protected,
                recommendation: "系统或程序目录，不建议手动删除；应使用卸载程序或 Windows 存储设置",
                label: Some("系统保护"),
                cleanup_eligible: false,
            };
        }
        return guidance_review(
            "临时或日志相关路径；关闭占用程序后可考虑清理，建议移入回收站",
            Some("临时/日志"),
        );
    }

    guidance_review("可能包含个人或项目数据，请先打开检查内容和最近修改时间", None)
}

pub fn is_confirmed_rebuildable(path: &Path) -> bool {
    classify_path(path).risk == PathRisk::Rebuildable && path.is_dir()
}

/// 高置信可重建：可进清理中心勾选（排除 build/dist/out 等仅提示项）
pub fn is_cleanup_eligible_rebuildable(path: &Path) -> bool {
    let g = classify_path(path);
    g.risk == PathRisk::Rebuildable && g.cleanup_eligible && path.is_dir()
}

/// 快速热判断：只看自身 mtime，供扫描列表使用（避免扫 node_modules 内上万文件）。
pub fn path_is_hot_shallow(path: &Path, window: Duration) -> bool {
    let Ok(meta) = fs::metadata(path) else {
        return false;
    };
    age_of(&meta) < window
}

/// 删除前热判断：自身 + 直接子项采样，防止正在构建的目录被清理。
pub fn path_is_hot(path: &Path, window: Duration) -> bool {
    if path_is_hot_shallow(path, window) {
        return true;
    }
    if !path.is_dir() {
        return false;
    }
    let Ok(entries) = fs::read_dir(path) else {
        return false;
    };
    for entry in entries.flatten().take(24) {
        if let Ok(meta) = entry.metadata() {
            if age_of(&meta) < window {
                return true;
            }
        }
    }
    false
}

fn age_of(metadata: &fs::Metadata) -> Duration {
    metadata
        .modified()
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .unwrap_or(Duration::from_secs(u64::MAX / 4))
}

pub fn file_age(metadata: &fs::Metadata) -> Duration {
    age_of(metadata)
}

/// 在范围内发现已邻居验证的可重建目录（不深入其内部）。
pub fn find_rebuildable_dirs(root: &Path, max_depth: usize, min_bytes: u64) -> Vec<FoundRebuildable> {
    let mut found = Vec::new();
    if !root.is_dir() {
        return found;
    }
    walk_find(root, 0, max_depth, min_bytes, &mut found);
    found.sort_by(|a, b| b.size.cmp(&a.size));
    found.truncate(100);
    found
}

#[derive(Clone)]
pub struct FoundRebuildable {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
    pub file_count: u64,
    pub label: String,
    pub tip: String,
}

fn walk_find(
    dir: &Path,
    depth: usize,
    max_depth: usize,
    min_bytes: u64,
    out: &mut Vec<FoundRebuildable>,
) {
    if depth > max_depth || out.len() >= 100 {
        return;
    }
    if is_protected(dir) {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if out.len() >= 100 {
            return;
        }
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        // 跳过明显无意义的深层系统/隐藏噪音（仍允许 .next 等规则名）
        let guidance = classify_path(&path);
        // 清理列表只收高置信可重建；低置信 rebuildable（如 build/dist）不收录
        if guidance.risk == PathRisk::Rebuildable && guidance.cleanup_eligible {
            let (size, files) = dir_size_quick(&path);
            if size >= min_bytes {
                out.push(FoundRebuildable {
                    path: path.clone(),
                    name,
                    size,
                    file_count: files,
                    label: guidance.label.unwrap_or("可重建").into(),
                    tip: guidance.recommendation.into(),
                });
            }
            // 不进入已确认的可重建目录
            continue;
        }
        if guidance.risk == PathRisk::Rebuildable {
            // 仅提示类可重建目录：不收录清理，但仍跳过深入（避免把 dist 内部再当独立项）
            continue;
        }
        // 跳过超大无关系统树
        let lower = name.to_ascii_lowercase();
        if matches!(
            lower.as_str(),
            "windows"
                | "program files"
                | "program files (x86)"
                | "programdata"
                | "$recycle.bin"
                | "system volume information"
                | "recovery"
        ) {
            continue;
        }
        walk_find(&path, depth + 1, max_depth, min_bytes, out);
    }
}

/// 有上限的体积统计，避免巨型 node_modules 拖死 UI 线程。
fn dir_size_quick(path: &Path) -> (u64, u64) {
    let mut size = 0_u64;
    let mut files = 0_u64;
    for entry in walkdir::WalkDir::new(path)
        .follow_links(false)
        .max_open(32)
        .into_iter()
        .flatten()
    {
        if entry.file_type().is_file() {
            if let Ok(meta) = entry.metadata() {
                size = size.saturating_add(meta.len());
                files += 1;
            }
        }
        // 超大目录截断计数，列表仍可用；删除前会再估一次
        if files >= 80_000 {
            break;
        }
    }
    (size, files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_case(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "disk-analyzer-dev-{}-{}",
            name,
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn node_modules_requires_package_json() {
        let root = temp_case("nm");
        let nm = root.join("node_modules");
        fs::create_dir_all(&nm).unwrap();
        assert_eq!(classify_path(&nm).risk, PathRisk::Review);
        fs::write(root.join("package.json"), "{}").unwrap();
        let g = classify_path(&nm);
        assert_eq!(g.risk, PathRisk::Rebuildable);
        assert!(g.cleanup_eligible);
        assert!(is_cleanup_eligible_rebuildable(&nm));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn target_requires_cargo_or_pom() {
        let root = temp_case("target");
        let target = root.join("target");
        fs::create_dir_all(&target).unwrap();
        assert_eq!(classify_path(&target).risk, PathRisk::Review);
        fs::write(root.join("Cargo.toml"), "[package]\nname=\"x\"\n").unwrap();
        assert_eq!(classify_path(&target).risk, PathRisk::Rebuildable);
        assert!(is_cleanup_eligible_rebuildable(&target));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn dist_is_hint_only_not_cleanup_eligible() {
        let root = temp_case("dist");
        let dist = root.join("dist");
        fs::create_dir_all(&dist).unwrap();
        fs::write(root.join("package.json"), "{}").unwrap();
        let g = classify_path(&dist);
        assert_eq!(g.risk, PathRisk::Rebuildable);
        assert!(!g.cleanup_eligible);
        assert!(!is_cleanup_eligible_rebuildable(&dist));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn src_directory_never_rebuildable() {
        let root = temp_case("src-protect");
        let src = root.join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(root.join("package.json"), "{}").unwrap();
        assert_eq!(classify_path(&src).risk, PathRisk::Review);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn monorepo_node_modules_tip_mentions_workspace() {
        let root = temp_case("mono");
        let pkg = root.join("packages").join("web");
        let nm = pkg.join("node_modules");
        fs::create_dir_all(&nm).unwrap();
        fs::write(root.join("pnpm-workspace.yaml"), "packages:\n  - 'packages/*'\n").unwrap();
        fs::write(pkg.join("package.json"), "{}").unwrap();
        let g = classify_path(&nm);
        assert_eq!(g.risk, PathRisk::Rebuildable);
        assert!(g.recommendation.contains("monorepo") || g.recommendation.contains("workspace"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn source_tree_sample_blocks_rs_at_top_level() {
        let root = temp_case("src-sample");
        let target = root.join("target");
        fs::create_dir_all(&target).unwrap();
        fs::write(root.join("Cargo.toml"), "[package]\nname=\"x\"\n").unwrap();
        fs::write(target.join("main.rs"), "fn main(){}").unwrap();
        assert!(looks_like_source_tree(&target));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn bare_notes_target_not_rebuildable() {
        let root = temp_case("notes");
        let target = root.join("target");
        fs::create_dir_all(&target).unwrap();
        fs::write(root.join("readme.txt"), "notes").unwrap();
        assert_eq!(classify_path(&target).risk, PathRisk::Review);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn pycache_with_py_sibling() {
        let root = temp_case("py");
        let cache = root.join("__pycache__");
        fs::create_dir_all(&cache).unwrap();
        fs::write(root.join("main.py"), "print(1)").unwrap();
        assert_eq!(classify_path(&cache).risk, PathRisk::Rebuildable);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn windows_path_protected() {
        assert_eq!(
            classify_path(Path::new("C:\\Windows\\System32")).risk,
            PathRisk::Protected
        );
    }
}

