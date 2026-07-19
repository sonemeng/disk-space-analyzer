//! 应用与社交通讯缓存（清理中心 category = app）
//! 第一批：微信/QQ/钉钉/飞书/Telegram；默认不勾选；敏感库强确认。

use std::env;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub const RULEPACK_VERSION: &str = "0.3.0-app";
const MIN_BYTES: u64 = 5 * 1024 * 1024;
const MAX_ITEMS: usize = 80;

#[derive(Clone)]
pub struct AppCacheHit {
    pub id: String,
    pub rule_id: String,
    pub name: String,
    pub description: String,
    pub path: PathBuf,
    pub size: u64,
    pub file_count: u64,
    pub requires_strong_confirm: bool,
}

struct RuleDef {
    rule_id: &'static str,
    product: &'static str,
    title: &'static str,
    note: &'static str,
    strong: bool,
    kind: Kind,
}

enum Kind {
    LocalRel(&'static [&'static str]),
    RoamingRel(&'static [&'static str]),
    ProfileRel(&'static [&'static str]),
}

const RULES: &[RuleDef] = &[
    // —— 微信 ——
    RuleDef {
        rule_id: "app-wechat-files",
        product: "微信",
        title: "微信文件缓存",
        note: "关联微信 · 图片/视频/文件等缓存目录（非消息库本体时仍建议确认）。",
        strong: true,
        kind: Kind::ProfileRel(&[
            "Documents\\WeChat Files",
            "Documents\\xwechat_files",
        ]),
    },
    RuleDef {
        rule_id: "app-wechat-appdata",
        product: "微信",
        title: "微信应用数据",
        note: "关联微信 · AppData 缓存；需强确认。",
        strong: true,
        kind: Kind::RoamingRel(&["Tencent\\WeChat", "Tencent\\xwechat"]),
    },
    // —— QQ ——
    RuleDef {
        rule_id: "app-qq-nt",
        product: "QQ",
        title: "QQ NT 数据",
        note: "关联 QQ · 新版 NT 用户数据；需强确认。",
        strong: true,
        kind: Kind::RoamingRel(&["Tencent\\QQ", "Tencent\\QQNT", "QQ"]),
    },
    RuleDef {
        rule_id: "app-qq-documents",
        product: "QQ",
        title: "QQ 文档目录",
        note: "关联 QQ · Documents 下常见接收文件目录；需强确认。",
        strong: true,
        kind: Kind::ProfileRel(&["Documents\\Tencent Files", "Documents\\QQFiles"]),
    },
    // —— 钉钉 ——
    RuleDef {
        rule_id: "app-dingtalk",
        product: "钉钉",
        title: "钉钉缓存",
        note: "关联钉钉 · 本地缓存/文件；需强确认。",
        strong: true,
        kind: Kind::RoamingRel(&["DingTalk", "DingDing"]),
    },
    RuleDef {
        rule_id: "app-dingtalk-local",
        product: "钉钉",
        title: "钉钉本地数据",
        note: "关联钉钉 · LocalAppData；需强确认。",
        strong: true,
        kind: Kind::LocalRel(&["DingTalk", "DingDing"]),
    },
    // —— 飞书 ——
    RuleDef {
        rule_id: "app-feishu",
        product: "飞书",
        title: "飞书缓存",
        note: "关联飞书 · 本地缓存；需强确认。",
        strong: true,
        kind: Kind::RoamingRel(&["LarkShell", "Feishu", "Lark"]),
    },
    RuleDef {
        rule_id: "app-feishu-local",
        product: "飞书",
        title: "飞书本地数据",
        note: "关联飞书 · LocalAppData；需强确认。",
        strong: true,
        kind: Kind::LocalRel(&["LarkShell", "Feishu", "Lark"]),
    },
    // —— Telegram ——
    RuleDef {
        rule_id: "app-telegram",
        product: "Telegram",
        title: "Telegram 数据",
        note: "关联 Telegram Desktop · tdata 等；需强确认。",
        strong: true,
        kind: Kind::RoamingRel(&["Telegram Desktop"]),
    },
    RuleDef {
        rule_id: "app-telegram-local",
        product: "Telegram",
        title: "Telegram 本地缓存",
        note: "关联 Telegram · Local 缓存；需强确认。",
        strong: true,
        kind: Kind::LocalRel(&["Telegram Desktop"]),
    },
    // —— 企业微信 ——
    RuleDef {
        rule_id: "app-wxwork",
        product: "企业微信",
        title: "企业微信数据",
        note: "关联企业微信 · AppData 用户数据；需强确认。",
        strong: true,
        kind: Kind::RoamingRel(&["Tencent\\WXWork", "Tencent\\WeChatWork", "WXWork"]),
    },
    RuleDef {
        rule_id: "app-wxwork-local",
        product: "企业微信",
        title: "企业微信本地缓存",
        note: "关联企业微信 · LocalAppData；需强确认。",
        strong: true,
        kind: Kind::LocalRel(&["Tencent\\WXWork", "Tencent\\WeChatWork", "WXWork"]),
    },
    RuleDef {
        rule_id: "app-wxwork-docs",
        product: "企业微信",
        title: "企业微信文档目录",
        note: "关联企业微信 · Documents 常见文件目录；需强确认。",
        strong: true,
        kind: Kind::ProfileRel(&["Documents\\WXWork", "Documents\\WeChat Work Files"]),
    },
    // —— 抖音 / TikTok 桌面端（有客户端才命中）——
    RuleDef {
        rule_id: "app-douyin",
        product: "抖音",
        title: "抖音桌面端数据",
        note: "关联抖音电脑版 · 本地缓存；需强确认。",
        strong: true,
        kind: Kind::RoamingRel(&["Douyin", "douyin", "ByteDance\\Douyin", "bytedance\\douyin"]),
    },
    RuleDef {
        rule_id: "app-douyin-local",
        product: "抖音",
        title: "抖音本地缓存",
        note: "关联抖音电脑版 · LocalAppData；需强确认。",
        strong: true,
        kind: Kind::LocalRel(&["Douyin", "douyin", "ByteDance\\Douyin", "bytedance\\douyin"]),
    },
    RuleDef {
        rule_id: "app-tiktok",
        product: "TikTok",
        title: "TikTok 桌面端数据",
        note: "关联 TikTok Desktop · 本地缓存；需强确认。",
        strong: true,
        kind: Kind::RoamingRel(&["TikTok", "tiktok", "ByteDance\\TikTok"]),
    },
    RuleDef {
        rule_id: "app-tiktok-local",
        product: "TikTok",
        title: "TikTok 本地缓存",
        note: "关联 TikTok Desktop · LocalAppData；需强确认。",
        strong: true,
        kind: Kind::LocalRel(&["TikTok", "tiktok", "ByteDance\\TikTok"]),
    },
    // —— 国内长视频 / 短视频桌面端 ——
    RuleDef {
        rule_id: "app-iqiyi",
        product: "爱奇艺",
        title: "爱奇艺缓存",
        note: "关联爱奇艺 · 本地缓存/下载；需强确认。",
        strong: true,
        kind: Kind::RoamingRel(&["IqiyiVideo", "iQIYI", "IQIYI Video"]),
    },
    RuleDef {
        rule_id: "app-iqiyi-local",
        product: "爱奇艺",
        title: "爱奇艺本地数据",
        note: "关联爱奇艺 · LocalAppData；需强确认。",
        strong: true,
        kind: Kind::LocalRel(&["IqiyiVideo", "iQIYI", "IQIYI Video"]),
    },
    RuleDef {
        rule_id: "app-tencentvideo",
        product: "腾讯视频",
        title: "腾讯视频缓存",
        note: "关联腾讯视频 · 本地缓存；需强确认。",
        strong: true,
        kind: Kind::RoamingRel(&["Tencent\\qvideo", "Tencent\\QQLive", "TencentVideo"]),
    },
    RuleDef {
        rule_id: "app-tencentvideo-local",
        product: "腾讯视频",
        title: "腾讯视频本地数据",
        note: "关联腾讯视频 · LocalAppData；需强确认。",
        strong: true,
        kind: Kind::LocalRel(&["Tencent\\qvideo", "Tencent\\QQLive", "TencentVideo"]),
    },
    RuleDef {
        rule_id: "app-youku",
        product: "优酷",
        title: "优酷缓存",
        note: "关联优酷 · 本地缓存；需强确认。",
        strong: true,
        kind: Kind::RoamingRel(&["Youku", "YoukuClient", "Alibaba\\Youku"]),
    },
    RuleDef {
        rule_id: "app-youku-local",
        product: "优酷",
        title: "优酷本地数据",
        note: "关联优酷 · LocalAppData；需强确认。",
        strong: true,
        kind: Kind::LocalRel(&["Youku", "YoukuClient", "Alibaba\\Youku"]),
    },
    RuleDef {
        rule_id: "app-mgtv",
        product: "芒果TV",
        title: "芒果TV缓存",
        note: "关联芒果TV · 本地缓存；需强确认。",
        strong: true,
        kind: Kind::RoamingRel(&["MGTV", "ImgoTV", "HunanTV"]),
    },
    RuleDef {
        rule_id: "app-bilibili",
        product: "哔哩哔哩",
        title: "B站缓存",
        note: "关联哔哩哔哩 · 本地缓存/下载；需强确认。",
        strong: true,
        kind: Kind::RoamingRel(&["bilibili", "Bilibili", "com.bilibili"]),
    },
    RuleDef {
        rule_id: "app-bilibili-local",
        product: "哔哩哔哩",
        title: "B站本地数据",
        note: "关联哔哩哔哩 · LocalAppData；需强确认。",
        strong: true,
        kind: Kind::LocalRel(&["bilibili", "Bilibili", "com.bilibili"]),
    },
    RuleDef {
        rule_id: "app-kuaishou",
        product: "快手",
        title: "快手缓存",
        note: "关联快手电脑版 · 本地缓存；需强确认。",
        strong: true,
        kind: Kind::RoamingRel(&["Kwai", "Kuaishou", "kuaishou-live-partner"]),
    },
    RuleDef {
        rule_id: "app-kuaishou-local",
        product: "快手",
        title: "快手本地数据",
        note: "关联快手 · LocalAppData；需强确认。",
        strong: true,
        kind: Kind::LocalRel(&["Kwai", "Kuaishou"]),
    },
    // —— 剪辑 ——
    RuleDef {
        rule_id: "app-jianying",
        product: "剪映",
        title: "剪映缓存",
        note: "关联剪映专业版 · 草稿/缓存；需强确认。",
        strong: true,
        kind: Kind::RoamingRel(&["JianyingPro", "CapCut", "ByteDance\\JianyingPro"]),
    },
    RuleDef {
        rule_id: "app-jianying-local",
        product: "剪映",
        title: "剪映本地数据",
        note: "关联剪映 · LocalAppData；需强确认。",
        strong: true,
        kind: Kind::LocalRel(&["JianyingPro", "CapCut", "ByteDance\\JianyingPro"]),
    },
    RuleDef {
        rule_id: "app-capcut",
        product: "CapCut",
        title: "CapCut 缓存",
        note: "关联 CapCut · 草稿/缓存；需强确认。",
        strong: true,
        kind: Kind::RoamingRel(&["CapCut", "ByteDance\\CapCut"]),
    },
    // —— 会议 ——
    RuleDef {
        rule_id: "app-tencentmeeting",
        product: "腾讯会议",
        title: "腾讯会议缓存",
        note: "关联腾讯会议 · 本地缓存/录制相关；需强确认。",
        strong: true,
        kind: Kind::RoamingRel(&["Tencent\\WeMeet", "Tencent\\xwechat_work", "WeMeet"]),
    },
    RuleDef {
        rule_id: "app-tencentmeeting-local",
        product: "腾讯会议",
        title: "腾讯会议本地数据",
        note: "关联腾讯会议 · LocalAppData；需强确认。",
        strong: true,
        kind: Kind::LocalRel(&["Tencent\\WeMeet", "WeMeet"]),
    },
    RuleDef {
        rule_id: "app-zoom",
        product: "Zoom",
        title: "Zoom 数据",
        note: "关联 Zoom · 本地数据/缓存；需强确认。",
        strong: true,
        kind: Kind::RoamingRel(&["Zoom", "Zoom\\data"]),
    },
    RuleDef {
        rule_id: "app-zoom-local",
        product: "Zoom",
        title: "Zoom 本地缓存",
        note: "关联 Zoom · LocalAppData；需强确认。",
        strong: true,
        kind: Kind::LocalRel(&["Zoom"]),
    },
    RuleDef {
        rule_id: "app-teams",
        product: "Microsoft Teams",
        title: "Teams 缓存",
        note: "关联 Teams · 本地缓存；需强确认。",
        strong: true,
        kind: Kind::RoamingRel(&["Microsoft\\Teams", "Microsoft\\Teams\\Cache"]),
    },
    RuleDef {
        rule_id: "app-skype",
        product: "Skype",
        title: "Skype 数据",
        note: "关联 Skype · 本地数据；需强确认。",
        strong: true,
        kind: Kind::RoamingRel(&["Microsoft\\Skype for Desktop", "Skype"]),
    },
    // —— 社交 IM ——
    RuleDef {
        rule_id: "app-discord",
        product: "Discord",
        title: "Discord 缓存",
        note: "关联 Discord · Cache 数据较大；需强确认。",
        strong: true,
        kind: Kind::RoamingRel(&["discord"]),
    },
    RuleDef {
        rule_id: "app-discord-local",
        product: "Discord",
        title: "Discord 本地数据",
        note: "关联 Discord · LocalAppData；需强确认。",
        strong: true,
        kind: Kind::LocalRel(&["Discord", "discord"]),
    },
    RuleDef {
        rule_id: "app-whatsapp",
        product: "WhatsApp",
        title: "WhatsApp 桌面数据",
        note: "关联 WhatsApp Desktop · 本地数据；需强确认。",
        strong: true,
        kind: Kind::RoamingRel(&["WhatsApp", "WhatsApp.Desktop"]),
    },
    RuleDef {
        rule_id: "app-line",
        product: "LINE",
        title: "LINE 数据",
        note: "关联 LINE · 本地数据；需强确认。",
        strong: true,
        kind: Kind::RoamingRel(&["LINE"]),
    },
    RuleDef {
        rule_id: "app-signal",
        product: "Signal",
        title: "Signal 数据",
        note: "关联 Signal · 本地数据；需强确认。",
        strong: true,
        kind: Kind::RoamingRel(&["Signal"]),
    },
    RuleDef {
        rule_id: "app-slack",
        product: "Slack",
        title: "Slack 缓存",
        note: "关联 Slack · 本地缓存；需强确认。",
        strong: true,
        kind: Kind::RoamingRel(&["Slack"]),
    },
    RuleDef {
        rule_id: "app-slack-local",
        product: "Slack",
        title: "Slack 本地数据",
        note: "关联 Slack · LocalAppData；需强确认。",
        strong: true,
        kind: Kind::LocalRel(&["slack", "Slack"]),
    },
    // —— 内容社区桌面端（有则命中）——
    RuleDef {
        rule_id: "app-xiaohongshu",
        product: "小红书",
        title: "小红书缓存",
        note: "关联小红书电脑版 · 本地缓存；需强确认。",
        strong: true,
        kind: Kind::RoamingRel(&["XiaoHongShu", "xhs", "com.xingin.xhs"]),
    },
    RuleDef {
        rule_id: "app-weibo",
        product: "微博",
        title: "微博缓存",
        note: "关联微博桌面端 · 本地缓存；需强确认。",
        strong: true,
        kind: Kind::RoamingRel(&["Sina\\WeiboDesktop", "WeiboDesktop", "Weibo"]),
    },
    // —— 文档/知识库 ——
    RuleDef {
        rule_id: "app-notion",
        product: "Notion",
        title: "Notion 缓存",
        note: "关联 Notion · 本地缓存；需强确认。",
        strong: true,
        kind: Kind::RoamingRel(&["Notion"]),
    },
    RuleDef {
        rule_id: "app-notion-local",
        product: "Notion",
        title: "Notion 本地数据",
        note: "关联 Notion · LocalAppData；需强确认。",
        strong: true,
        kind: Kind::LocalRel(&["Notion"]),
    },
    RuleDef {
        rule_id: "app-yuque",
        product: "语雀",
        title: "语雀缓存",
        note: "关联语雀 · 本地缓存；需强确认。",
        strong: true,
        kind: Kind::RoamingRel(&["yuque-desktop", "Yuque", "larksuite-yuque"]),
    },
    RuleDef {
        rule_id: "app-shimo",
        product: "石墨文档",
        title: "石墨文档缓存",
        note: "关联石墨文档 · 本地缓存；需强确认。",
        strong: true,
        kind: Kind::RoamingRel(&["shimo-desktop", "Shimo"]),
    },
];

fn env_path(key: &str) -> Option<PathBuf> {
    env::var_os(key).map(PathBuf::from)
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

fn resolve(rule: &RuleDef) -> Vec<PathBuf> {
    let mut out = Vec::new();
    match rule.kind {
        Kind::ProfileRel(rels) => {
            if let Some(base) = env_path("USERPROFILE") {
                for rel in rels {
                    out.push(join_rel(&base, rel));
                }
            }
        }
        Kind::LocalRel(rels) => {
            if let Some(base) = env_path("LOCALAPPDATA") {
                for rel in rels {
                    out.push(join_rel(&base, rel));
                }
            }
        }
        Kind::RoamingRel(rels) => {
            if let Some(base) = env_path("APPDATA") {
                for rel in rels {
                    out.push(join_rel(&base, rel));
                }
            }
        }
    }
    out
}

fn blacklisted(path: &Path, blacklist: &[String]) -> bool {
    let value = path.to_string_lossy().to_ascii_lowercase();
    blacklist.iter().any(|b| {
        let needle = b.trim().trim_end_matches(['\\', '/']).to_ascii_lowercase();
        !needle.is_empty() && (value == needle || value.starts_with(&format!("{needle}\\")))
    })
}

pub fn discover_app_caches(drive: &str, blacklist: &[String]) -> Vec<AppCacheHit> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for rule in RULES {
        for path in resolve(rule) {
            if out.len() >= MAX_ITEMS {
                break;
            }
            if !path.is_dir() || !path_on_drive(&path, drive) || blacklisted(&path, blacklist) {
                continue;
            }
            let key = path.to_string_lossy().to_ascii_lowercase();
            if !seen.insert(key) {
                continue;
            }
            let (size, file_count) = dir_size_quick(&path);
            if size < MIN_BYTES {
                continue;
            }
            let leaf = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("");
            out.push(AppCacheHit {
                id: format!("app:{}:{}", rule.rule_id, path.to_string_lossy()),
                rule_id: rule.rule_id.into(),
                name: format!("{} · {} · {}", rule.product, rule.title, leaf),
                description: format!(
                    "应用缓存 · 关联 {} · {} · 默认不勾选",
                    rule.product, rule.note
                ),
                path,
                size,
                file_count,
                requires_strong_confirm: rule.strong,
            });
        }
    }
    out.sort_by(|a, b| b.size.cmp(&a.size));
    out.truncate(MAX_ITEMS);
    out
}

pub fn parse_app_id(id: &str) -> Option<(String, PathBuf)> {
    let rest = id.strip_prefix("app:")?;
    let (rule_id, path_str) = rest.split_once(':')?;
    if rule_id.is_empty() || path_str.is_empty() {
        return None;
    }
    Some((rule_id.to_string(), PathBuf::from(path_str)))
}

pub fn revalidate_app(
    rule_id: &str,
    path: &Path,
    drive: &str,
    blacklist: &[String],
) -> bool {
    if !path.is_dir() || !path_on_drive(path, drive) || blacklisted(path, blacklist) {
        return false;
    }
    let Some(rule) = RULES.iter().find(|r| r.rule_id == rule_id) else {
        return false;
    };
    let target = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    for candidate in resolve(rule) {
        let cand = candidate
            .canonicalize()
            .unwrap_or_else(|_| candidate.clone());
        if cand == target {
            return true;
        }
    }
    false
}
