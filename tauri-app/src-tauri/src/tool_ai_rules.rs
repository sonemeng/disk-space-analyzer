//! 工具/AI 缓存规则包。
//! P1 发现；P2 B/C 默认可清；P3 D 层模型强确认后可清。
//! 主干构建产物仍在 dev_rules。

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

/// 规则包版本（展示用，随规则表递增）
pub const RULEPACK_VERSION: &str = "0.5.0-p4";

const MIN_BYTES: u64 = 5 * 1024 * 1024;
const MAX_ITEMS: usize = 120;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiscoveryTier {
    /// S2 模型/高成本：可清，但必须强确认
    Review,
    /// S3 包缓存/编辑器 Cache：可清，默认不勾选
    RebuildIntent,
}

impl DiscoveryTier {
    /// 是否允许进入 clean_items（模型另需 strong_confirm）
    pub fn is_cleanable(self) -> bool {
        matches!(self, Self::RebuildIntent | Self::Review)
    }
    pub fn requires_strong_confirm(self) -> bool {
        matches!(self, Self::Review)
    }
}

#[derive(Clone)]
pub struct ToolAiHit {
    pub id: String,
    pub rule_id: String,
    pub name: String,
    pub description: String,
    pub path: PathBuf,
    pub size: u64,
    pub file_count: u64,
    pub tier: DiscoveryTier,
    pub layer: &'static str,
    /// 是否允许移入回收站
    pub cleanable: bool,
    /// 模型等：前端必须二次强确认
    pub requires_strong_confirm: bool,
}

struct RuleDef {
    rule_id: &'static str,
    title: &'static str,
    note: &'static str,
    layer: &'static str,
    tier: DiscoveryTier,
    /// 相对用户主目录 / LocalAppData / Roaming 的候选
    kind: RuleKind,
}

enum RuleKind {
    /// %USERPROFILE%\<rel>
    ProfileRel(&'static [&'static str]),
    /// %LOCALAPPDATA%\<rel>
    LocalRel(&'static [&'static str]),
    /// %APPDATA%\<rel>
    RoamingRel(&'static [&'static str]),
    /// 环境变量根（若存在）
    EnvRoot(&'static [&'static str]),
    /// Electron 应用下仅 Cache 类子树
    ElectronCaches(&'static [&'static str]),
}

const RULES: &[RuleDef] = &[
    // —— B 包管理器 ——
    RuleDef {
        rule_id: "tool-npm-cache",
        title: "npm 缓存",
        note: "包管理器缓存，可重建；默认不勾选，移入回收站可还原。",
        layer: "B",
        tier: DiscoveryTier::RebuildIntent,
        kind: RuleKind::LocalRel(&["npm-cache"]),
    },
    RuleDef {
        rule_id: "tool-yarn-cache",
        title: "Yarn 缓存",
        note: "Yarn 全局缓存，可重建；默认不勾选。",
        layer: "B",
        tier: DiscoveryTier::RebuildIntent,
        kind: RuleKind::LocalRel(&["Yarn\\Cache", "Yarn\\Berry\\Cache"]),
    },
    RuleDef {
        rule_id: "tool-pnpm-store",
        title: "pnpm store",
        note: "全局 store，清理后多项目需重下；默认不勾选，请确认无进行中的安装。",
        layer: "B",
        tier: DiscoveryTier::RebuildIntent,
        kind: RuleKind::LocalRel(&["pnpm-cache", "pnpm-store", "pnpm\\store"]),
    },
    RuleDef {
        rule_id: "tool-pip-cache",
        title: "pip 缓存",
        note: "Python pip 下载缓存，可重建；默认不勾选。",
        layer: "B",
        tier: DiscoveryTier::RebuildIntent,
        kind: RuleKind::LocalRel(&["pip\\Cache"]),
    },
    RuleDef {
        rule_id: "tool-cargo-registry",
        title: "Cargo registry",
        note: "Rust crates 缓存，体积大、编译变慢；默认不勾选。",
        layer: "B",
        tier: DiscoveryTier::RebuildIntent,
        kind: RuleKind::ProfileRel(&[".cargo\\registry"]),
    },
    RuleDef {
        rule_id: "tool-cargo-git",
        title: "Cargo git 依赖缓存",
        note: "Cargo git 检出缓存；默认不勾选。",
        layer: "B",
        tier: DiscoveryTier::RebuildIntent,
        kind: RuleKind::ProfileRel(&[".cargo\\git"]),
    },
    RuleDef {
        rule_id: "tool-gradle-caches",
        title: "Gradle caches",
        note: "Gradle 依赖缓存，可重建；默认不勾选。",
        layer: "B",
        tier: DiscoveryTier::RebuildIntent,
        kind: RuleKind::ProfileRel(&[".gradle\\caches"]),
    },
    RuleDef {
        rule_id: "tool-nuget-cache",
        title: "NuGet 缓存",
        note: "NuGet 包缓存，可重建；默认不勾选。",
        layer: "B",
        tier: DiscoveryTier::RebuildIntent,
        kind: RuleKind::LocalRel(&["NuGet\\v3-cache", "NuGet\\Cache", "NuGet\\plugins-cache"]),
    },
    // —— C Electron 应用 Cache（仅 Cache 子树，默认可清）——
    RuleDef {
        rule_id: "editor-electron-caches",
        title: "编辑器/Agent 缓存",
        note: "仅 Cache/Code Cache/GPUCache/logs 等子树；可清，默认不勾选。不删扩展配置与对话。",
        layer: "C",
        tier: DiscoveryTier::RebuildIntent,
        kind: RuleKind::ElectronCaches(&[
            "Cursor", "Code", "Code - Insiders", "VSCodium", "Windsurf", "Trae", "Qoder",
            "Void", "PearAI", "Continue", "Zed", "Antigravity", "CodeBuddy", "MarsCode",
            "Claude", "ChatGPT", "OpenAI", "OpenCode", "opencode", "Hermes", "hermes-desktop",
            "Hermes Desktop", "autoclaw", "AutoClaw", "Codex Show Studio", "ZCODE", "ZCode",
            "zcode", "LM Studio", "ai.opencode.desktop", "com.nousresearch.hermes.setup",
            "com.xiaofei.liveagent", "TabNine", "Codeium", "GitHubCopilot", "Amazon Q",
            "JetBrains", "CherryStudio", "cherry-studio", "Kiro", "Workbuddy", "Fitten",
            "BaiduComate", "Comate", "CodeGeeX", "Lingma", "Tongyi", "iFlytek", "SparkDesk",
            "Kimi", "Moonshot", "AnythingLLM", "Jan", "GPT4All", "LocalAI", "SillyTavern",
            "KoboldCPP", "text-generation-webui", "OpenHands", "Goose",
        ]),
    },
    // —— C2 用户点目录 / Agent 配置（强确认：可能含密钥与索引）——
    RuleDef {
        rule_id: "agent-dot-homes",
        title: "AI/Agent 用户目录",
        note: "用户主目录下 AI 工具配置/缓存/索引；可能含密钥，需强确认。",
        layer: "C",
        tier: DiscoveryTier::Review,
        kind: RuleKind::ProfileRel(&[
            ".cursor", ".ccursor", ".claude", ".claudiatron", ".codex", ".continue",
            ".opencode", ".config\\opencode", ".hermes", ".codeium", ".copilot",
            ".gemini", ".grok", ".kiro", ".openclaw", ".openclaw-autoclaw",
            ".cherrystudio", ".codebuddy", ".qoder", ".windsurf", ".cwindsurf",
            ".trae-cn", ".trae-aicc", ".devin", ".fitten", ".workbuddy", ".agents",
            ".aider", ".aider.chat", ".tabnine", ".cody", ".sourcegraph",
            ".augment", ".antigravity_cockpit", ".liveagent", ".paseo",
            ".local\\share\\aider", ".cache\\aider",
        ]),
    },
    RuleDef {
        rule_id: "agent-appdata-homes",
        title: "AI/Agent 应用数据",
        note: "AppData 下 AI 工具整目录；可能含登录态，需强确认。",
        layer: "C",
        tier: DiscoveryTier::Review,
        kind: RuleKind::RoamingRel(&[
            "Hermes", "hermes-desktop", "autoclaw", "AutoClaw", "CodeBuddy",
            "Codex Show Studio", "Qoder", "Windsurf", "Cursor", "Claude",
            "TabNine", "Codeium", "Sourcegraph", "CherryStudio", "Kimi",
            "Moonshot", "iFlytekSpark", "SparkDesk", "TongyiLingma", "Comate",
            "CodeGeeX", "Amazon Q", "GitHub Copilot",
        ]),
    },
    RuleDef {
        rule_id: "agent-local-homes",
        title: "AI/Agent 本地数据",
        note: "LocalAppData 下 AI 工具目录；需强确认。",
        layer: "C",
        tier: DiscoveryTier::Review,
        kind: RuleKind::LocalRel(&[
            "autoclaw", "AutoClaw", "autoclaw-updater", "hermes",
            "claude-cli-nodejs", "CodeBuddyExtension", "cursor-updater",
            "GitHubCopilot", "github-copilot", "TabNine", "Codeium",
            "AmazonQ", "com.nousresearch.hermes.setup",
        ]),
    },
    // —— D 本地模型 / 推理 / 生图（强确认）——
    RuleDef {
        rule_id: "ai-ollama-models",
        title: "Ollama 模型",
        note: "关联 Ollama · 本地模型权重；需强确认。",
        layer: "D",
        tier: DiscoveryTier::Review,
        kind: RuleKind::EnvRoot(&["OLLAMA_MODELS"]),
    },
    RuleDef {
        rule_id: "ai-ollama-home",
        title: "Ollama 数据目录",
        note: "关联 Ollama · .ollama（models/blobs）；需强确认。",
        layer: "D",
        tier: DiscoveryTier::Review,
        kind: RuleKind::ProfileRel(&[".ollama"]),
    },
    RuleDef {
        rule_id: "ai-hf-hub",
        title: "Hugging Face Hub",
        note: "关联 Hugging Face · 模型/数据集缓存；需强确认。",
        layer: "D",
        tier: DiscoveryTier::Review,
        kind: RuleKind::EnvRoot(&["HF_HOME", "HUGGINGFACE_HUB_CACHE", "TRANSFORMERS_CACHE", "DIFFUSERS_CACHE"]),
    },
    RuleDef {
        rule_id: "ai-hf-default",
        title: "Hugging Face 默认缓存",
        note: "关联 Hugging Face · .cache\\huggingface；需强确认。",
        layer: "D",
        tier: DiscoveryTier::Review,
        kind: RuleKind::ProfileRel(&[".cache\\huggingface"]),
    },
    RuleDef {
        rule_id: "ai-torch-hub",
        title: "Torch hub 缓存",
        note: "关联 PyTorch · hub 缓存；需强确认。",
        layer: "D",
        tier: DiscoveryTier::Review,
        kind: RuleKind::ProfileRel(&[".cache\\torch"]),
    },
    RuleDef {
        rule_id: "ai-lmstudio",
        title: "LM Studio 模型/数据",
        note: "关联 LM Studio · 模型与本地数据；需强确认。",
        layer: "D",
        tier: DiscoveryTier::Review,
        kind: RuleKind::ProfileRel(&[".cache\\lm-studio", ".lmstudio"]),
    },
    RuleDef {
        rule_id: "ai-lmstudio-appdata",
        title: "LM Studio 应用数据",
        note: "关联 LM Studio · AppData；需强确认。",
        layer: "D",
        tier: DiscoveryTier::Review,
        kind: RuleKind::RoamingRel(&["LM Studio", "lm-studio"]),
    },
    RuleDef {
        rule_id: "ai-jan",
        title: "Jan 本地数据",
        note: "关联 Jan · 本地模型/应用数据；需强确认。",
        layer: "D",
        tier: DiscoveryTier::Review,
        kind: RuleKind::ProfileRel(&["jan", ".jan"]),
    },
    RuleDef {
        rule_id: "ai-gpt4all",
        title: "GPT4All 模型",
        note: "关联 GPT4All · 本地模型；需强确认。",
        layer: "D",
        tier: DiscoveryTier::Review,
        kind: RuleKind::ProfileRel(&[".nomic.ai", "nomic.ai"]),
    },
    RuleDef {
        rule_id: "ai-anythingllm",
        title: "AnythingLLM 数据",
        note: "关联 AnythingLLM · 向量/模型数据；需强确认。",
        layer: "D",
        tier: DiscoveryTier::Review,
        kind: RuleKind::RoamingRel(&["anythingllm-desktop", "AnythingLLM"]),
    },
    RuleDef {
        rule_id: "ai-localai",
        title: "LocalAI 数据",
        note: "关联 LocalAI · 本地模型/配置；需强确认。",
        layer: "D",
        tier: DiscoveryTier::Review,
        kind: RuleKind::ProfileRel(&[".localai", "localai", ".cache\\localai"]),
    },
    RuleDef {
        rule_id: "ai-sillytavern",
        title: "SillyTavern 数据",
        note: "关联 SillyTavern · 角色/缓存数据；需强确认。",
        layer: "D",
        tier: DiscoveryTier::Review,
        kind: RuleKind::ProfileRel(&["SillyTavern", ".sillytavern"]),
    },
    RuleDef {
        rule_id: "ai-kobold",
        title: "KoboldCPP 数据",
        note: "关联 Kobold · 本地模型相关目录；需强确认。",
        layer: "D",
        tier: DiscoveryTier::Review,
        kind: RuleKind::ProfileRel(&["KoboldCPP", "koboldcpp", ".koboldcpp"]),
    },
    RuleDef {
        rule_id: "ai-textgen-webui",
        title: "text-generation-webui",
        note: "关联 oobabooga · 模型/输出目录；需强确认。",
        layer: "D",
        tier: DiscoveryTier::Review,
        kind: RuleKind::ProfileRel(&[
            "text-generation-webui", "text-generation-webui\\models",
            "oobabooga", "oobabooga_windows",
        ]),
    },
    RuleDef {
        rule_id: "ai-comfyui",
        title: "ComfyUI 模型/输出",
        note: "关联 ComfyUI · models/output 常见位置；需强确认。",
        layer: "D",
        tier: DiscoveryTier::Review,
        kind: RuleKind::ProfileRel(&[
            "ComfyUI", "ComfyUI\\models", "ComfyUI\\output",
            "Documents\\ComfyUI", "Documents\\ComfyUI\\models",
            "AppData\\Local\\Programs\\ComfyUI",
        ]),
    },
    RuleDef {
        rule_id: "ai-sd-webui",
        title: "Stable Diffusion WebUI",
        note: "关联 A1111 WebUI · models/outputs；需强确认。",
        layer: "D",
        tier: DiscoveryTier::Review,
        kind: RuleKind::ProfileRel(&[
            "stable-diffusion-webui", "stable-diffusion-webui\\models",
            "stable-diffusion-webui\\outputs", "webui", "webui\\models",
            "Documents\\stable-diffusion-webui",
        ]),
    },
];

const ELECTRON_CACHE_NAMES: &[&str] = &[
    "Cache",
    "Code Cache",
    "GPUCache",
    "ShaderCache",
    "GrShaderCache",
    "logs",
    "Crashpad",
];

fn env_path(key: &str) -> Option<PathBuf> {
    env::var_os(key).map(PathBuf::from)
}

fn profile_dir() -> Option<PathBuf> {
    env_path("USERPROFILE")
}

fn local_app_data() -> Option<PathBuf> {
    env_path("LOCALAPPDATA")
}

fn roaming_app_data() -> Option<PathBuf> {
    env_path("APPDATA")
}

fn join_rel(base: &Path, rel: &str) -> PathBuf {
    let mut p = base.to_path_buf();
    for part in rel.split(['\\', '/']).filter(|s| !s.is_empty()) {
        p.push(part);
    }
    p
}

fn path_on_drive(path: &Path, drive: &str) -> bool {
    let d = drive.trim_end_matches(['\\', '/']).to_ascii_uppercase();
    path.to_string_lossy()
        .to_ascii_uppercase()
        .starts_with(&d)
}

/// 过宽前缀（用户主目录 / Users / 盘符根）不能当 S0，否则工具缓存全被滤成 0B
fn is_broad_protect_prefix(needle: &str) -> bool {
    let n = needle.trim().trim_end_matches(['\\', '/']).to_ascii_lowercase();
    if n.len() < 4 {
        return true;
    }
    let parts: Vec<&str> = n
        .split(['\\', '/'])
        .filter(|s| !s.is_empty())
        .collect();
    // C: 或 C:\Users 或 C:\Users\name
    if parts.len() <= 2 {
        return true;
    }
    if parts.len() == 3 && parts[1].eq_ignore_ascii_case("users") {
        return true;
    }
    // AppData 浅层根
    if n.contains("\\appdata\\local") && parts.len() <= 5 {
        return true;
    }
    if n.contains("\\appdata\\roaming") && parts.len() <= 5 {
        return true;
    }
    false
}

/// 活跃工作区 / 保护前缀：命中则 S0，不进入工具/AI 列表
pub fn is_protected_workspace(path: &Path, protect_prefixes: &[String]) -> bool {
    if protect_prefixes.is_empty() {
        return false;
    }
    let value = path.to_string_lossy().to_ascii_lowercase();
    protect_prefixes.iter().any(|entry| {
        let needle = entry.trim().trim_end_matches(['\\', '/']).to_ascii_lowercase();
        if needle.is_empty() || is_broad_protect_prefix(&needle) {
            return false;
        }
        value == needle || value.starts_with(&format!("{needle}\\"))
    })
}

fn dir_size_quick(path: &Path) -> (u64, u64) {
    let mut size = 0_u64;
    let mut files = 0_u64;
    for entry in WalkDir::new(path)
        .follow_links(false)
        .max_open(24)
        .into_iter()
        .flatten()
    {
        if entry.file_type().is_file() {
            if let Ok(meta) = entry.metadata() {
                size = size.saturating_add(meta.len());
                files += 1;
            }
        }
        if files >= 80_000 {
            break;
        }
    }
    (size, files)
}

fn push_candidate(
    out: &mut Vec<ToolAiHit>,
    seen: &mut std::collections::HashSet<String>,
    rule: &RuleDef,
    path: PathBuf,
    drive: &str,
    protect_prefixes: &[String],
) {
    if out.len() >= MAX_ITEMS {
        return;
    }
    if !path.is_dir() {
        return;
    }
    if !path_on_drive(&path, drive) {
        return;
    }
    if is_protected_workspace(&path, protect_prefixes) {
        return;
    }
    let key = path.to_string_lossy().to_ascii_lowercase();
    if !seen.insert(key) {
        return;
    }
    let (size, file_count) = dir_size_quick(&path);
    if size < MIN_BYTES {
        return;
    }
    let cleanable = rule.tier.is_cleanable();
    let requires_strong_confirm = rule.tier.requires_strong_confirm();
    let tier_label = if requires_strong_confirm {
        "模型/高成本 · 需强确认"
    } else {
        "可清理 · 默认不勾选"
    };
    let product = product_label_for_path(rule, &path);
    let leaf = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let name = if product.is_empty() {
        format!("{} · {}", rule.title, leaf)
    } else if leaf.eq_ignore_ascii_case(&product) || rule.title.contains(&product) {
        format!("{} · {}", rule.title, leaf)
    } else {
        format!("{} · {} · {}", rule.title, product, leaf)
    };
    let description = if product.is_empty() {
        format!("{} · {}", tier_label, rule.note)
    } else {
        format!("{} · 关联 {} · {}", tier_label, product, rule.note)
    };
    out.push(ToolAiHit {
        id: format!("toolai:{}:{}", rule.rule_id, path.to_string_lossy()),
        rule_id: rule.rule_id.into(),
        name,
        description,
        path,
        size,
        file_count,
        tier: rule.tier,
        layer: rule.layer,
        cleanable,
        requires_strong_confirm,
    });
}

/// 从路径/规则推断展示用产品名（用户可见「和什么相关」）
fn product_label_for_path(rule: &RuleDef, path: &Path) -> String {
    let lower = path.to_string_lossy().to_ascii_lowercase();
    let table: &[(&str, &str)] = &[
        ("\\cursor", "Cursor"), (".cursor", "Cursor"), ("\\ccursor", "Cursor"),
        ("\\windsurf", "Windsurf"), (".windsurf", "Windsurf"), ("\\cwindsurf", "Windsurf"),
        ("\\qoder", "Qoder"), (".qoder", "Qoder"),
        ("\\trae", "Trae"), (".trae", "Trae"),
        ("\\void", "Void"), ("\\pearai", "PearAI"),
        ("\\continue", "Continue"), (".continue", "Continue"),
        ("\\claude", "Claude"), (".claude", "Claude"), ("\\claudiatron", "Claude"),
        ("\\chatgpt", "ChatGPT"), ("\\openai", "OpenAI"),
        ("\\opencode", "OpenCode"), (".opencode", "OpenCode"),
        ("\\hermes", "Hermes"), (".hermes", "Hermes"),
        ("\\autoclaw", "AutoClaw"), ("\\openclaw", "OpenClaw/AutoClaw"),
        ("\\codex", "Codex"), (".codex", "Codex"),
        ("\\zcode", "ZCODE"), ("\\codebuddy", "CodeBuddy"), (".codebuddy", "CodeBuddy"),
        ("\\copilot", "GitHub Copilot"), (".copilot", "GitHub Copilot"),
        ("\\codeium", "Codeium/Windsurf"), (".codeium", "Codeium"),
        ("\\tabnine", "Tabnine"), ("\\cody", "Cody"), ("\\sourcegraph", "Cody/Sourcegraph"),
        ("\\amazon", "Amazon Q"), ("\\supermaven", "Supermaven"),
        ("\\aider", "Aider"), ("\\cline", "Cline"), ("\\roo", "Roo Code"),
        ("\\openhands", "OpenHands"), ("\\goose", "Goose"),
        ("\\gemini", "Gemini CLI"), (".gemini", "Gemini"),
        ("\\grok", "Grok"), (".grok", "Grok"),
        ("\\kiro", "Kiro"), (".kiro", "Kiro"),
        ("\\devin", "Devin"), (".devin", "Devin"),
        ("\\fitten", "Fitten"), ("\\workbuddy", "Workbuddy"),
        ("\\cherrystudio", "Cherry Studio"),
        ("\\marscode", "MarsCode"), ("\\lingma", "通义灵码"), ("\\tongyi", "通义"),
        ("\\codegeex", "CodeGeeX"), ("\\comate", "Baidu Comate"),
        ("\\kimi", "Kimi"), ("\\moonshot", "Kimi/Moonshot"),
        ("\\iflytek", "讯飞星火"), ("\\spark", "讯飞星火"),
        ("\\lm studio", "LM Studio"), ("\\lm-studio", "LM Studio"), (".lmstudio", "LM Studio"),
        (".ollama", "Ollama"), ("\\ollama", "Ollama"),
        ("\\huggingface", "Hugging Face"), ("\\.cache\\torch", "PyTorch"),
        ("\\anythingllm", "AnythingLLM"), ("\\gpt4all", "GPT4All"), ("\\nomic.ai", "GPT4All"),
        (".jan", "Jan"), ("\\jan\\", "Jan"),
        ("\\localai", "LocalAI"), ("\\sillytavern", "SillyTavern"),
        ("\\kobold", "Kobold"), ("\\text-generation-webui", "text-generation-webui"),
        ("\\oobabooga", "text-generation-webui"),
        ("\\comfyui", "ComfyUI"), ("\\stable-diffusion", "Stable Diffusion WebUI"),
        ("\\code - insiders", "VS Code Insiders"), ("\\code\\", "VS Code"), ("\\vscodium", "VSCodium"),
        ("\\npm-cache", "npm"), ("\\yarn\\", "Yarn"), ("\\pnpm", "pnpm"), ("\\pip\\", "pip"),
        (".cargo", "Cargo/Rust"), (".gradle", "Gradle"), ("\\nuget", "NuGet"),
        ("\\jetbrains", "JetBrains AI"),
    ];
    for (needle, label) in table {
        if lower.contains(needle) {
            return (*label).into();
        }
    }
    // Electron 规则：用 AppData 下应用文件夹名
    if rule.rule_id == "editor-electron-caches" {
        for part in path.iter().rev().skip(1).take(6) {
            let s = part.to_string_lossy();
            if ELECTRON_CACHE_NAMES
                .iter()
                .any(|n| n.eq_ignore_ascii_case(&s))
            {
                continue;
            }
            let low = s.to_ascii_lowercase();
            if low == "appdata" || low == "roaming" || low == "local" || low == "users" {
                continue;
            }
            if s.len() >= 2 {
                return s.into_owned();
            }
        }
    }
    String::new()
}

fn resolve_paths(rule: &RuleDef) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    match rule.kind {
        RuleKind::ProfileRel(rels) => {
            if let Some(base) = profile_dir() {
                for rel in rels {
                    paths.push(join_rel(&base, rel));
                }
            }
        }
        RuleKind::LocalRel(rels) => {
            if let Some(base) = local_app_data() {
                for rel in rels {
                    paths.push(join_rel(&base, rel));
                }
            }
        }
        RuleKind::RoamingRel(rels) => {
            if let Some(base) = roaming_app_data() {
                for rel in rels {
                    paths.push(join_rel(&base, rel));
                }
            }
        }
        RuleKind::EnvRoot(keys) => {
            for key in keys {
                if let Some(p) = env_path(key) {
                    if p.as_os_str().len() > 0 {
                        paths.push(p);
                    }
                }
            }
        }
        RuleKind::ElectronCaches(apps) => {
            for base_opt in [roaming_app_data(), local_app_data()] {
                let Some(base) = base_opt else { continue };
                for app in apps {
                    let app_root = base.join(app);
                    if !app_root.is_dir() {
                        continue;
                    }
                    // 应用根下直接 Cache 子树 + 再下一层（User Data / 配置目录）
                    collect_electron_caches(&app_root, &mut paths);
                    if let Ok(entries) = fs::read_dir(&app_root) {
                        for entry in entries.flatten().take(40) {
                            let p = entry.path();
                            if p.is_dir() {
                                collect_electron_caches(&p, &mut paths);
                                // 再下一层（部分 Electron 把 Cache 放在更深层）
                                if let Ok(sub) = fs::read_dir(&p) {
                                    for child in sub.flatten().take(20) {
                                        let cp = child.path();
                                        if cp.is_dir() {
                                            collect_electron_caches(&cp, &mut paths);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    paths
}

fn collect_electron_caches(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if ELECTRON_CACHE_NAMES
            .iter()
            .any(|n| n.eq_ignore_ascii_case(&name))
        {
            let p = entry.path();
            if p.is_dir() {
                out.push(p);
            }
        }
    }
}

/// 发现工具/AI 缓存候选（只读，不产生可删除 ID）
pub fn discover_tool_ai(
    drive: &str,
    protect_prefixes: &[String],
    blacklist: &[String],
) -> Vec<ToolAiHit> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for rule in RULES {
        for path in resolve_paths(rule) {
            if blacklist.iter().any(|b| {
                let needle = b.trim().trim_end_matches(['\\', '/']).to_ascii_lowercase();
                let value = path.to_string_lossy().to_ascii_lowercase();
                !needle.is_empty() && (value == needle || value.starts_with(&format!("{needle}\\")))
            }) {
                continue;
            }
            push_candidate(&mut out, &mut seen, rule, path, drive, protect_prefixes);
        }
    }
    // 额外：LOCALAPPDATA 下常见 pnpm store 变体
    if let Some(local) = local_app_data() {
        for rel in ["pnpm", "pnpm-cache", "Yarn"] {
            let p = local.join(rel);
            if p.is_dir() {
                // 已由规则覆盖的不重复；这里只补 pnpm 深层 store
                if rel == "pnpm" {
                    let store = p.join("store");
                    if store.is_dir() {
                        if let Some(rule) = RULES.iter().find(|r| r.rule_id == "tool-pnpm-store") {
                            push_candidate(
                                &mut out,
                                &mut seen,
                                rule,
                                store,
                                drive,
                                protect_prefixes,
                            );
                        }
                    }
                }
            }
        }
    }
    out.sort_by(|a, b| b.size.cmp(&a.size));
    out.truncate(MAX_ITEMS);
    out
}


/// 解析 toolai:rule_id:path 形式 ID
pub fn parse_toolai_id(id: &str) -> Option<(String, PathBuf)> {
    let rest = id.strip_prefix("toolai:")?;
    let (rule_id, path_str) = rest.split_once(':')?;
    if rule_id.is_empty() || path_str.is_empty() {
        return None;
    }
    Some((rule_id.to_string(), PathBuf::from(path_str)))
}

/// 删除前再校验：规则仍为 S3 可清理，且路径仍是该规则的合法候选
pub fn revalidate_cleanable(
    rule_id: &str,
    path: &Path,
    drive: &str,
    protect_prefixes: &[String],
    blacklist: &[String],
) -> bool {
    if !path.is_dir() {
        return false;
    }
    if !path_on_drive(path, drive) {
        return false;
    }
    if is_protected_workspace(path, protect_prefixes) {
        return false;
    }
    let value = path.to_string_lossy().to_ascii_lowercase();
    if blacklist.iter().any(|b| {
        let needle = b.trim().trim_end_matches(['\\', '/']).to_ascii_lowercase();
        !needle.is_empty() && (value == needle || value.starts_with(&format!("{needle}\\")))
    }) {
        return false;
    }
    let Some(rule) = RULES.iter().find(|r| r.rule_id == rule_id) else {
        return false;
    };
    if !rule.tier.is_cleanable() {
        return false;
    }
    // 路径必须仍落在该规则解析出的候选集合中（规范化比较）
    let target = path
        .canonicalize()
        .unwrap_or_else(|_| path.to_path_buf());
    for candidate in resolve_paths(rule) {
        let cand = candidate
            .canonicalize()
            .unwrap_or_else(|_| candidate.clone());
        if cand == target {
            return true;
        }
        // 也允许 path 是候选的子路径？P2 否：只允许精确候选根
    }
    // electron cache: 候选可能很多，用结构再认一次
    if matches!(rule.kind, RuleKind::ElectronCaches(_)) {
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        if ELECTRON_CACHE_NAMES
            .iter()
            .any(|n| n.eq_ignore_ascii_case(&name))
        {
            // 父路径需在 AppData 下某已知应用内
            if value.contains("\\appdata\\") {
                return true;
            }
        }
    }
    // pnpm store 深层补丁
    if rule.rule_id == "tool-pnpm-store" {
        if value.contains("\\pnpm") && (value.ends_with("\\store") || value.contains("\\store\\")) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rulepack_version_non_empty() {
        assert!(!RULEPACK_VERSION.is_empty());
    }

    #[test]
    fn workspace_protect_matches_prefix() {
        let p = PathBuf::from(r"C:\Users\me\code\app\node_modules");
        let protect = vec![r"C:\Users\me\code\app".into()];
        assert!(is_protected_workspace(&p, &protect));
        assert!(!is_protected_workspace(&p, &[r"D:\other".into()]));
    }
}
