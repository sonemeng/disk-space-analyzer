use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::Instant;
use tauri::{AppHandle, Emitter};

/// startup | app-path | uninstall | history | user-command | help | fonts | sound
/// 进阶另含: hklm-startup | hklm-app-path | hklm-uninstall | com-server | shared-dll
#[derive(Clone, Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RegistryScanOptions {
    /// 为空则扫描全部支持的分类（受 mode 限制）
    #[serde(default)]
    pub categories: Vec<String>,
    /// 命中 name / keyPath / data 任一子串则跳过（不区分大小写）
    #[serde(default)]
    pub exclusions: Vec<String>,
    /// current | all（all 需管理员，扫描 HKEY_USERS 下各 SID）
    #[serde(default = "default_user_scope")]
    pub user_scope: String,
    /// basic | advanced | expert
    /// - basic: 用户级路径铁证
    /// - advanced: + HKLM/COM/SharedDLL 等，高风险默认不勾选
    /// - expert: 含 advanced + 服务/驱动只读 + 文件关联可选删 + SharedDLL 缺失项极严格可删
    #[serde(default = "default_scan_mode")]
    pub mode: String,
}

fn default_user_scope() -> String {
    "current".into()
}

fn default_scan_mode() -> String {
    "basic".into()
}

fn is_advanced_mode(mode: &str) -> bool {
    mode.eq_ignore_ascii_case("advanced")
        || mode.eq_ignore_ascii_case("pro")
        || mode.eq_ignore_ascii_case("expert")
}

fn is_expert_mode(mode: &str) -> bool {
    mode.eq_ignore_ascii_case("expert")
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryCategorySummary {
    pub id: String,
    pub name: String,
    pub total: u64,
    pub fixable: u64,
    pub review: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryBackupInfo {
    pub id: String,
    pub path: String,
    pub label: String,
    pub created_at: String,
    pub file_count: u64,
    pub kind: String,
}

fn category_enabled(categories: &[String], id: &str) -> bool {
    categories.is_empty()
        || categories
            .iter()
            .any(|value| value.eq_ignore_ascii_case(id))
}

fn is_excluded(exclusions: &[String], name: &str, key_path: &str, data: &str) -> bool {
    if exclusions.is_empty() {
        return false;
    }
    let haystack = format!("{name}\n{key_path}\n{data}").to_ascii_lowercase();
    exclusions.iter().any(|entry| {
        let needle = entry.trim().to_ascii_lowercase();
        !needle.is_empty() && haystack.contains(&needle)
    })
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryIssue {
    pub id: String,
    pub category: String,
    pub name: String,
    pub key_path: String,
    pub value_name: Option<String>,
    pub data: String,
    pub reason: String,
    pub risk: String,
    pub fixable: bool,
    #[serde(skip_serializing)]
    relative_key: String,
    #[serde(skip_serializing)]
    backup_key: String,
    #[serde(skip_serializing)]
    operation: RegistryOperation,
}

#[derive(Clone)]
enum RegistryOperation {
    DeleteValue(String),
    DeleteKey,
    Review,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryReport {
    pub items: Vec<RegistryIssue>,
    pub categories: Vec<RegistryCategorySummary>,
    pub scanned_keys: u64,
    pub fixable_count: u64,
    pub review_count: u64,
    pub elapsed_ms: u128,
    pub user_scope: String,
    pub elevated: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryRepairResult {
    pub repaired: u64,
    pub failed: u64,
    pub backup_directory: String,
}

fn issue_id(kind: &str, key: &str, value: Option<&str>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(kind.as_bytes());
    hasher.update([0]);
    hasher.update(key.as_bytes());
    hasher.update([0]);
    hasher.update(value.unwrap_or_default().as_bytes());
    format!("{kind}-{:x}", hasher.finalize())
}

fn expand_environment_variables(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find('%') {
        result.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('%') else {
            result.push_str(&rest[start..]);
            return result;
        };
        let name = &after[..end];
        if let Ok(replacement) = std::env::var(name) {
            result.push_str(&replacement);
        } else {
            result.push('%');
            result.push_str(name);
            result.push('%');
        }
        rest = &after[end + 1..];
    }
    result.push_str(rest);
    result
}

fn executable_from_command(command: &str) -> Option<PathBuf> {
    let expanded = expand_environment_variables(command);
    let trimmed = expanded.trim();
    if trimmed.is_empty() {
        return None;
    }

    let candidate = if let Some(quoted) = trimmed.strip_prefix('"') {
        quoted.find('"').map(|end| &quoted[..end])?
    } else {
        let lower = trimmed.to_ascii_lowercase();
        [".exe", ".com", ".bat", ".cmd"]
            .iter()
            .filter_map(|extension| lower.find(extension).map(|index| index + extension.len()))
            .min()
            .map(|end| &trimmed[..end])
            .unwrap_or_else(|| trimmed.split_whitespace().next().unwrap_or_default())
    };
    let path = PathBuf::from(candidate.trim().trim_matches('"'));
    path.is_absolute().then_some(path)
}

fn missing_command_target(command: &str) -> Option<PathBuf> {
    executable_from_command(command).filter(|path| !path.exists())
}

#[cfg(windows)]
mod platform {
    use super::*;
    use chrono::Local;
    use std::collections::{HashMap, HashSet};
    use std::fs;
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, HKEY_USERS, KEY_READ, KEY_WRITE};
    use winreg::RegKey;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const RUN_ONCE: &str = r"Software\Microsoft\Windows\CurrentVersion\RunOnce";
    const POLICIES_RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Policies\Explorer\Run";
    const APP_PATHS: &str = r"Software\Microsoft\Windows\CurrentVersion\App Paths";
    const UNINSTALL: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall";
    const TYPED_PATHS: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\TypedPaths";
    const RUN_MRU: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\RunMRU";
    const USER_CLASSES_APPS: &str = r"Software\Classes\Applications";

    fn read_string(key: &RegKey, name: &str) -> Option<String> {
        key.get_value::<String, _>(name)
            .ok()
            .filter(|value| !value.trim().is_empty())
    }

    fn absolute_missing_path(value: &str) -> Option<PathBuf> {
        let expanded = expand_environment_variables(value);
        let trimmed = expanded.trim().trim_matches('"');
        if trimmed.is_empty() {
            return None;
        }
        // 支持 "C:\path\file.exe" 或裸绝对路径（可带尾部参数时交给 missing_command_target）
        let path = PathBuf::from(trimmed);
        if path.is_absolute() && !path.exists() {
            Some(path)
        } else {
            None
        }
    }

    fn hive_issue_id(hive: &str, kind: &str, key: &str, value: Option<&str>) -> String {
        issue_id(&format!("{hive}|{kind}"), key, value)
    }

    fn push_startup_value(
        items: &mut Vec<RegistryIssue>,
        hive_prefix: &str,
        relative_key: &str,
        name: String,
        command: String,
        can_fix: bool,
    ) {
        let Some(missing) = missing_command_target(&command) else {
            return;
        };
        items.push(RegistryIssue {
            id: hive_issue_id(hive_prefix, "startup", relative_key, Some(&name)),
            category: "无效的启动程序".into(),
            name: name.clone(),
            key_path: format!(r"{hive_prefix}\{relative_key}"),
            value_name: Some(name.clone()),
            data: command,
            reason: format!("启动目标不存在：{}", missing.display()),
            risk: if can_fix { "low" } else { "review" }.into(),
            fixable: can_fix,
            relative_key: relative_key.into(),
            backup_key: relative_key.into(),
            operation: if can_fix {
                RegistryOperation::DeleteValue(name)
            } else {
                RegistryOperation::Review
            },
        });
    }

    fn scan_startup_on(
        root: &RegKey,
        hive_prefix: &str,
        items: &mut Vec<RegistryIssue>,
        scanned_keys: &mut u64,
        can_fix: bool,
    ) {
        for relative_key in [RUN, RUN_ONCE, POLICIES_RUN] {
            let Ok(key) = root.open_subkey_with_flags(relative_key, KEY_READ) else {
                continue;
            };
            *scanned_keys += 1;
            for (name, _) in key.enum_values().flatten() {
                let Some(command) = read_string(&key, &name) else {
                    continue;
                };
                push_startup_value(items, hive_prefix, relative_key, name, command, can_fix);
            }
        }
    }

    fn scan_startup(items: &mut Vec<RegistryIssue>, scanned_keys: &mut u64) {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        scan_startup_on(&hkcu, "HKCU", items, scanned_keys, true);
    }

    fn scan_app_paths(items: &mut Vec<RegistryIssue>, scanned_keys: &mut u64) {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let Ok(parent) = hkcu.open_subkey_with_flags(APP_PATHS, KEY_READ) else {
            return;
        };
        *scanned_keys += 1;
        for subkey_name in parent.enum_keys().flatten() {
            let Ok(key) = parent.open_subkey_with_flags(&subkey_name, KEY_READ) else {
                continue;
            };
            *scanned_keys += 1;
            let default_target = read_string(&key, "");
            let path_value = read_string(&key, "Path");
            let default_missing = default_target
                .as_deref()
                .and_then(|value| {
                    let expanded = expand_environment_variables(value);
                    let path = PathBuf::from(expanded.trim().trim_matches('"'));
                    path.is_absolute()
                        .then_some(path)
                        .filter(|path| !path.exists())
                });
            let path_missing = path_value.as_deref().and_then(absolute_missing_path);
            let Some(missing) = default_missing.or(path_missing) else {
                continue;
            };
            let relative_key = format!(r"{APP_PATHS}\{subkey_name}");
            let data = default_target
                .or(path_value)
                .unwrap_or_else(|| missing.display().to_string());
            items.push(RegistryIssue {
                id: issue_id("app-path", &relative_key, None),
                category: "无效的应用程序路径".into(),
                name: subkey_name,
                key_path: format!(r"HKCU\{relative_key}"),
                value_name: None,
                data,
                reason: format!("登记的应用程序或目录不存在：{}", missing.display()),
                risk: "low".into(),
                fixable: true,
                relative_key,
                backup_key: APP_PATHS.into(),
                operation: RegistryOperation::DeleteKey,
            });
        }
    }

    fn scan_uninstall(items: &mut Vec<RegistryIssue>, scanned_keys: &mut u64) {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let Ok(parent) = hkcu.open_subkey_with_flags(UNINSTALL, KEY_READ) else {
            return;
        };
        *scanned_keys += 1;
        for subkey_name in parent.enum_keys().flatten() {
            let Ok(key) = parent.open_subkey_with_flags(&subkey_name, KEY_READ) else {
                continue;
            };
            *scanned_keys += 1;
            if key.get_value::<u32, _>("SystemComponent").unwrap_or(0) == 1 {
                continue;
            }
            let Some(display_name) = read_string(&key, "DisplayName") else {
                continue;
            };
            let install_location = read_string(&key, "InstallLocation");
            let uninstall_command = read_string(&key, "UninstallString");
            let install_path = install_location
                .as_deref()
                .map(expand_environment_variables)
                .map(|value| PathBuf::from(value.trim().trim_matches('"')))
                .filter(|path| path.is_absolute());
            let install_missing = install_path.as_ref().map(|path| !path.exists());
            let missing_uninstaller = uninstall_command
                .as_deref()
                .and_then(missing_command_target);
            // 放宽：卸载程序绝对路径失效，且（安装目录失效 或 未登记安装目录）
            let report = match (install_missing, missing_uninstaller.as_ref()) {
                (Some(true), Some(missing)) => Some(format!(
                    "安装目录与卸载程序均不存在（{}）",
                    missing.display()
                )),
                (None, Some(missing)) => Some(format!(
                    "卸载程序不存在且未登记有效安装目录（{}）",
                    missing.display()
                )),
                (Some(true), None) if uninstall_command.is_none() => {
                    Some("安装目录不存在且无卸载命令".into())
                }
                _ => None,
            };
            let Some(reason) = report else {
                continue;
            };
            let relative_key = format!(r"{UNINSTALL}\{subkey_name}");
            items.push(RegistryIssue {
                id: issue_id("uninstall", &relative_key, None),
                category: "无效的卸载程序".into(),
                name: display_name,
                key_path: format!(r"HKCU\{relative_key}"),
                value_name: None,
                data: uninstall_command.unwrap_or_default(),
                reason: format!("{reason}；建议确认软件确已移除后再处理"),
                risk: "review".into(),
                fixable: false,
                relative_key,
                backup_key: UNINSTALL.into(),
                operation: RegistryOperation::Review,
            });
        }
    }

    /// 运行对话框历史 / 资源管理器输入路径：仅处理绝对路径且目标已不存在的项
    fn scan_history(items: &mut Vec<RegistryIssue>, scanned_keys: &mut u64) {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        // TypedPaths: url1 = C:\somewhere
        if let Ok(key) = hkcu.open_subkey_with_flags(TYPED_PATHS, KEY_READ) {
            *scanned_keys += 1;
            for (name, _) in key.enum_values().flatten() {
                let Some(value) = read_string(&key, &name) else {
                    continue;
                };
                let Some(missing) = absolute_missing_path(&value) else {
                    continue;
                };
                items.push(RegistryIssue {
                    id: issue_id("history", TYPED_PATHS, Some(&name)),
                    category: "无效的历史记录".into(),
                    name: name.clone(),
                    key_path: format!(r"HKCU\{TYPED_PATHS}"),
                    value_name: Some(name.clone()),
                    data: value,
                    reason: format!("输入过的路径已不存在：{}", missing.display()),
                    risk: "low".into(),
                    fixable: true,
                    relative_key: TYPED_PATHS.into(),
                    backup_key: TYPED_PATHS.into(),
                    operation: RegistryOperation::DeleteValue(name),
                });
            }
        }
        // RunMRU: a=notepad.exe\1 或 a=C:\gone.exe\1
        scan_string_mru_key(
            &hkcu,
            RUN_MRU,
            items,
            scanned_keys,
            true,
            "运行历史中的目标不存在",
        );
        // 常见应用程序「最近文件」字符串路径 + 部分 Explorer MRU
        for relative in [
            r"Software\Microsoft\Windows\CurrentVersion\Applets\Paint\Recent File List",
            r"Software\Microsoft\Windows\CurrentVersion\Applets\Wordpad\Recent File List",
            r"Software\Microsoft\Windows\CurrentVersion\Applets\Regedit\Favorites",
            r"Software\Microsoft\Windows\CurrentVersion\Explorer\Map Network Drive MRU",
            r"Software\Microsoft\Windows\CurrentVersion\Explorer\ComDlg32\LastVisitedPidlMRULegacy",
            r"Software\Microsoft\Windows\CurrentVersion\Explorer\ComDlg32\OpenSavePidlMRU\*",
            r"Software\Microsoft\Windows\CurrentVersion\Explorer\Wallpapers\Images",
            r"Software\Microsoft\Office\16.0\Common\Open Find\Microsoft Word\Settings\Save As\File Name MRU",
            r"Software\Microsoft\Office\16.0\Common\Open Find\Microsoft Excel\Settings\Save As\File Name MRU",
            r"Software\Microsoft\Office\15.0\Common\Open Find\Microsoft Word\Settings\Save As\File Name MRU",
            r"Software\Microsoft\Windows\CurrentVersion\Explorer\RecentDocs",
            // MuiCache 多为显示名，非路径；仅在进阶由其它逻辑覆盖，此处不扫以免噪音
        ] {
            if relative.contains('*') {
                // OpenSavePidlMRU 下按扩展名子键枚举
                if let Some(parent) = relative.strip_suffix(r"\*") {
                    scan_mru_tree(&hkcu, parent, items, scanned_keys, 2, "打开/保存历史路径已不存在");
                }
                continue;
            }
            scan_string_mru_key(
                &hkcu,
                relative,
                items,
                scanned_keys,
                false,
                "最近文件/路径已不存在",
            );
        }
    }

    /// 递归浅扫 MRU 树中的字符串绝对路径（深度有限）
    fn scan_mru_tree(
        root: &RegKey,
        relative_key: &str,
        items: &mut Vec<RegistryIssue>,
        scanned_keys: &mut u64,
        depth: usize,
        reason_prefix: &str,
    ) {
        if depth == 0 {
            return;
        }
        let Ok(key) = root.open_subkey_with_flags(relative_key, KEY_READ) else {
            return;
        };
        *scanned_keys += 1;
        for (name, _) in key.enum_values().flatten() {
            if name.eq_ignore_ascii_case("MRUList") || name.eq_ignore_ascii_case("MRUListEx") {
                continue;
            }
            let Some(value) = read_string(&key, &name) else {
                continue;
            };
            let Some(missing) =
                missing_command_target(&value).or_else(|| absolute_missing_path(&value))
            else {
                continue;
            };
            items.push(RegistryIssue {
                id: issue_id("history", relative_key, Some(&name)),
                category: "无效的历史记录".into(),
                name: name.clone(),
                key_path: format!(r"HKCU\{relative_key}"),
                value_name: Some(name.clone()),
                data: value,
                reason: format!("{reason_prefix}：{}", missing.display()),
                risk: "low".into(),
                fixable: true,
                relative_key: relative_key.into(),
                backup_key: relative_key.into(),
                operation: RegistryOperation::DeleteValue(name),
            });
        }
        if depth > 1 {
            for sub in key.enum_keys().flatten().take(40) {
                let child = format!(r"{relative_key}\{sub}");
                scan_mru_tree(root, &child, items, scanned_keys, depth - 1, reason_prefix);
            }
        }
    }

    fn scan_string_mru_key(
        root: &RegKey,
        relative_key: &str,
        items: &mut Vec<RegistryIssue>,
        scanned_keys: &mut u64,
        strip_run_mru_suffix: bool,
        reason_prefix: &str,
    ) {
        let Ok(key) = root.open_subkey_with_flags(relative_key, KEY_READ) else {
            return;
        };
        *scanned_keys += 1;
        for (name, _) in key.enum_values().flatten() {
            if name.eq_ignore_ascii_case("MRUList") || name.eq_ignore_ascii_case("MRUListEx") {
                continue;
            }
            let Some(value) = read_string(&key, &name) else {
                continue;
            };
            let cleaned = if strip_run_mru_suffix {
                value.trim_end_matches(['\\', '1']).trim_end_matches('\\')
            } else {
                value.as_str()
            };
            let Some(missing) = missing_command_target(cleaned)
                .or_else(|| absolute_missing_path(cleaned))
            else {
                continue;
            };
            items.push(RegistryIssue {
                id: issue_id("history", relative_key, Some(&name)),
                category: "无效的历史记录".into(),
                name: name.clone(),
                key_path: format!(r"HKCU\{relative_key}"),
                value_name: Some(name.clone()),
                data: value,
                reason: format!("{reason_prefix}：{}", missing.display()),
                risk: "low".into(),
                fixable: true,
                relative_key: relative_key.into(),
                backup_key: relative_key.into(),
                operation: RegistryOperation::DeleteValue(name),
            });
        }
    }

    /// 用户级 Applications 打开命令指向不存在的程序
    fn scan_user_commands(items: &mut Vec<RegistryIssue>, scanned_keys: &mut u64) {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let Ok(apps) = hkcu.open_subkey_with_flags(USER_CLASSES_APPS, KEY_READ) else {
            return;
        };
        *scanned_keys += 1;
        for app_name in apps.enum_keys().flatten().take(400) {
            let relative_command =
                format!(r"{USER_CLASSES_APPS}\{app_name}\shell\open\command");
            let Ok(key) = hkcu.open_subkey_with_flags(&relative_command, KEY_READ) else {
                continue;
            };
            *scanned_keys += 1;
            let Some(command) = read_string(&key, "") else {
                continue;
            };
            let Some(missing) = missing_command_target(&command) else {
                continue;
            };
            items.push(RegistryIssue {
                id: issue_id("user-command", &relative_command, None),
                category: "无效的文件类型".into(),
                name: app_name,
                key_path: format!(r"HKCU\{relative_command}"),
                value_name: None,
                data: command,
                reason: format!("打开方式指向的程序不存在：{}", missing.display()),
                risk: "low".into(),
                fixable: true,
                relative_key: relative_command.clone(),
                backup_key: relative_command,
                operation: RegistryOperation::DeleteKey,
            });
        }
    }

    /// 帮助文件路径：HKCU App Paths 旁或 Windows Help 用户覆盖中的绝对路径
    fn scan_help_files(items: &mut Vec<RegistryIssue>, scanned_keys: &mut u64) {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        // 用户级 HTML Help / 自定义帮助关联
        const HELP_ROOTS: &[&str] = &[
            r"Software\Microsoft\Windows\HTML Help",
            r"Software\Classes\.chm",
            r"Software\Classes\.hlp",
        ];
        for root in HELP_ROOTS {
            let Ok(key) = hkcu.open_subkey_with_flags(root, KEY_READ) else {
                continue;
            };
            *scanned_keys += 1;
            for (name, _) in key.enum_values().flatten() {
                let Some(value) = read_string(&key, &name) else {
                    continue;
                };
                let Some(missing) = absolute_missing_path(&value)
                    .or_else(|| missing_command_target(&value))
                else {
                    continue;
                };
                let lower = missing.to_string_lossy().to_ascii_lowercase();
                if !(lower.ends_with(".chm")
                    || lower.ends_with(".hlp")
                    || lower.ends_with(".html")
                    || lower.ends_with(".htm"))
                {
                    continue;
                }
                items.push(RegistryIssue {
                    id: issue_id("help", root, Some(&name)),
                    category: "无效的帮助文件".into(),
                    name: name.clone(),
                    key_path: format!(r"HKCU\{root}"),
                    value_name: Some(name.clone()),
                    data: value,
                    reason: format!("帮助文件路径不存在：{}", missing.display()),
                    risk: "low".into(),
                    fixable: true,
                    relative_key: (*root).into(),
                    backup_key: (*root).into(),
                    operation: RegistryOperation::DeleteValue(name),
                });
            }
        }
    }

    /// 用户字体：注册表指向的字体文件不存在
    fn scan_fonts(items: &mut Vec<RegistryIssue>, scanned_keys: &mut u64) {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        const FONTS: &str = r"Software\Microsoft\Windows NT\CurrentVersion\Fonts";
        // 用户字体更多在 HKLM；HKCU 下若有覆盖则检查
        let Ok(key) = hkcu.open_subkey_with_flags(FONTS, KEY_READ) else {
            return;
        };
        *scanned_keys += 1;
        let fonts_dir = std::env::var_os("WINDIR")
            .map(PathBuf::from)
            .map(|p| p.join("Fonts"));
        for (name, _) in key.enum_values().flatten() {
            let Some(value) = read_string(&key, &name) else {
                continue;
            };
            let path = {
                let expanded = expand_environment_variables(&value);
                let trimmed = expanded.trim().trim_matches('"');
                let p = PathBuf::from(trimmed);
                if p.is_absolute() {
                    p
                } else if let Some(dir) = &fonts_dir {
                    dir.join(trimmed)
                } else {
                    continue;
                }
            };
            if path.exists() {
                continue;
            }
            items.push(RegistryIssue {
                id: issue_id("fonts", FONTS, Some(&name)),
                category: "无效的字体".into(),
                name: name.clone(),
                key_path: format!(r"HKCU\{FONTS}"),
                value_name: Some(name.clone()),
                data: value,
                reason: format!("字体文件不存在：{}", path.display()),
                risk: "review".into(),
                fixable: false,
                relative_key: FONTS.into(),
                backup_key: FONTS.into(),
                operation: RegistryOperation::Review,
            });
        }
    }

    /// 声音事件：.wav 等绝对路径失效
    fn scan_sound_events(items: &mut Vec<RegistryIssue>, scanned_keys: &mut u64) {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        const SCHEMES: &str = r"AppEvents\Schemes\Apps";
        let Ok(apps) = hkcu.open_subkey_with_flags(SCHEMES, KEY_READ) else {
            return;
        };
        *scanned_keys += 1;
        for app_name in apps.enum_keys().flatten().take(80) {
            let Ok(app_key) = apps.open_subkey_with_flags(&app_name, KEY_READ) else {
                continue;
            };
            *scanned_keys += 1;
            for event_name in app_key.enum_keys().flatten().take(40) {
                let Ok(event_key) = app_key.open_subkey_with_flags(&event_name, KEY_READ) else {
                    continue;
                };
                for scheme_name in event_key.enum_keys().flatten().take(8) {
                    let Ok(scheme_key) = event_key.open_subkey_with_flags(&scheme_name, KEY_READ)
                    else {
                        continue;
                    };
                    *scanned_keys += 1;
                    let Some(value) = read_string(&scheme_key, "") else {
                        continue;
                    };
                    let Some(missing) = absolute_missing_path(&value) else {
                        continue;
                    };
                    let lower = missing.to_string_lossy().to_ascii_lowercase();
                    if !(lower.ends_with(".wav")
                        || lower.ends_with(".mp3")
                        || lower.ends_with(".wma"))
                    {
                        continue;
                    }
                    let relative_key =
                        format!(r"{SCHEMES}\{app_name}\{event_name}\{scheme_name}");
                    items.push(RegistryIssue {
                        id: issue_id("sound", &relative_key, None),
                        category: "无效的声音事件".into(),
                        name: format!("{app_name} / {event_name}"),
                        key_path: format!(r"HKCU\{relative_key}"),
                        value_name: None,
                        data: value,
                        reason: format!("声音文件不存在：{}", missing.display()),
                        risk: "low".into(),
                        fixable: true,
                        relative_key: relative_key.clone(),
                        backup_key: relative_key,
                        operation: RegistryOperation::DeleteKey,
                    });
                }
            }
        }
    }

    fn run_categories_on_current_user(
        categories: &[String],
        items: &mut Vec<RegistryIssue>,
        scanned_keys: &mut u64,
        app: Option<&AppHandle>,
    ) {
        let steps: [(&str, &str, fn(&mut Vec<RegistryIssue>, &mut u64)); 8] = [
            ("startup", "失效启动项", scan_startup),
            ("app-path", "应用路径", scan_app_paths),
            ("uninstall", "卸载信息", scan_uninstall),
            ("history", "历史记录", scan_history),
            ("user-command", "打开方式", scan_user_commands),
            ("help", "帮助文件", scan_help_files),
            ("fonts", "字体", scan_fonts),
            ("sound", "声音事件", scan_sound_events),
        ];
        let enabled: Vec<_> = steps
            .into_iter()
            .filter(|(id, _, _)| category_enabled(categories, id))
            .collect();
        let total = enabled.len().max(1);
        for (index, (_, label, scan_fn)) in enabled.into_iter().enumerate() {
            let pct = 8 + ((index * 28) / total) as u8;
            emit_registry_progress(app, &format!("正在检查：{label}"), pct.min(36));
            scan_fn(items, scanned_keys);
            emit_registry_progress(
                app,
                &format!("{label}完成 · 累计命中 {}", items.len()),
                (8 + (((index + 1) * 28) / total) as u8).min(36),
            );
        }
    }

    fn scan_hklm_startup(items: &mut Vec<RegistryIssue>, scanned_keys: &mut u64, elevated: bool) {
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        for relative_key in [
            r"Software\Microsoft\Windows\CurrentVersion\Run",
            r"Software\Microsoft\Windows\CurrentVersion\RunOnce",
            r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Run",
        ] {
            let Ok(key) = hklm.open_subkey_with_flags(relative_key, KEY_READ) else {
                continue;
            };
            *scanned_keys += 1;
            for (name, _) in key.enum_values().flatten() {
                let Some(command) = read_string(&key, &name) else {
                    continue;
                };
                let Some(missing) = missing_command_target(&command) else {
                    continue;
                };
                // 系统级默认仅建议；管理员下仍保持 review，避免误删服务相关启动
                // 管理员 + 进阶：允许勾选删除（前端二次确认）；非管理员仅建议
                let can_fix = elevated;
                items.push(RegistryIssue {
                    id: hive_issue_id("HKLM", "hklm-startup", relative_key, Some(&name)),
                    category: "无效的启动程序".into(),
                    name: format!("[系统] {name}"),
                    key_path: format!(r"HKLM\{relative_key}"),
                    value_name: Some(name.clone()),
                    data: command,
                    reason: format!(
                        "系统启动项目标不存在：{}{}",
                        missing.display(),
                        if can_fix {
                            "（进阶·可勾选删除，需二次确认）"
                        } else {
                            "（进阶·只读，修改需管理员）"
                        }
                    ),
                    risk: if can_fix { "medium" } else { "review" }.into(),
                    fixable: can_fix,
                    relative_key: relative_key.into(),
                    backup_key: format!(r"HKLM\{relative_key}"),
                    operation: if can_fix {
                        RegistryOperation::DeleteValue(name)
                    } else {
                        RegistryOperation::Review
                    },
                });
            }
        }
    }

    fn scan_hklm_app_paths(items: &mut Vec<RegistryIssue>, scanned_keys: &mut u64) {
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        for relative_parent in [
            r"Software\Microsoft\Windows\CurrentVersion\App Paths",
            r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\App Paths",
        ] {
            let Ok(parent) = hklm.open_subkey_with_flags(relative_parent, KEY_READ) else {
                continue;
            };
            *scanned_keys += 1;
            for subkey_name in parent.enum_keys().flatten().take(500) {
                let Ok(key) = parent.open_subkey_with_flags(&subkey_name, KEY_READ) else {
                    continue;
                };
                *scanned_keys += 1;
                let Some(target) = read_string(&key, "") else {
                    continue;
                };
                let expanded = expand_environment_variables(&target);
                let path = PathBuf::from(expanded.trim().trim_matches('"'));
                if !path.is_absolute() || path.exists() {
                    continue;
                }
                let relative_key = format!(r"{relative_parent}\{subkey_name}");
                let can_fix = is_elevated();
                items.push(RegistryIssue {
                    id: hive_issue_id("HKLM", "hklm-app-path", &relative_key, None),
                    category: "无效的应用程序路径".into(),
                    name: format!("[系统] {subkey_name}"),
                    key_path: format!(r"HKLM\{relative_key}"),
                    value_name: None,
                    data: target,
                    reason: format!(
                        "系统 App Paths 目标不存在：{}{}",
                        path.display(),
                        if can_fix {
                            "（进阶·可勾选删除，需二次确认）"
                        } else {
                            "（进阶·只读，修改需管理员）"
                        }
                    ),
                    risk: if can_fix { "medium" } else { "review" }.into(),
                    fixable: can_fix,
                    relative_key: relative_key.clone(),
                    backup_key: format!(r"HKLM\{relative_parent}"),
                    operation: if can_fix {
                        RegistryOperation::DeleteKey
                    } else {
                        RegistryOperation::Review
                    },
                });
            }
        }
    }

    fn scan_hklm_uninstall(items: &mut Vec<RegistryIssue>, scanned_keys: &mut u64) {
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        for relative_parent in [
            r"Software\Microsoft\Windows\CurrentVersion\Uninstall",
            r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
        ] {
            let Ok(parent) = hklm.open_subkey_with_flags(relative_parent, KEY_READ) else {
                continue;
            };
            *scanned_keys += 1;
            for subkey_name in parent.enum_keys().flatten().take(800) {
                let Ok(key) = parent.open_subkey_with_flags(&subkey_name, KEY_READ) else {
                    continue;
                };
                *scanned_keys += 1;
                if key.get_value::<u32, _>("SystemComponent").unwrap_or(0) == 1 {
                    continue;
                }
                let Some(display_name) = read_string(&key, "DisplayName") else {
                    continue;
                };
                let uninstall_command = read_string(&key, "UninstallString");
                let install_location = read_string(&key, "InstallLocation");
                let install_missing = install_location
                    .as_deref()
                    .map(expand_environment_variables)
                    .map(|v| PathBuf::from(v.trim().trim_matches('"')))
                    .filter(|p| p.is_absolute())
                    .map(|p| !p.exists());
                let uninstaller_missing = uninstall_command
                    .as_deref()
                    .and_then(missing_command_target);
                let reason = match (install_missing, uninstaller_missing.as_ref()) {
                    (Some(true), Some(m)) => format!(
                        "系统卸载信息：安装目录与卸载程序均不存在（{}）",
                        m.display()
                    ),
                    (None, Some(m)) => format!(
                        "系统卸载信息：卸载程序不存在（{}）",
                        m.display()
                    ),
                    (Some(true), None) if uninstall_command.is_none() => {
                        "系统卸载信息：安装目录不存在且无卸载命令".into()
                    }
                    _ => continue,
                };
                let relative_key = format!(r"{relative_parent}\{subkey_name}");
                items.push(RegistryIssue {
                    id: hive_issue_id("HKLM", "hklm-uninstall", &relative_key, None),
                    category: "无效的卸载程序".into(),
                    name: format!("[系统] {display_name}"),
                    key_path: format!(r"HKLM\{relative_key}"),
                    value_name: None,
                    data: uninstall_command.unwrap_or_default(),
                    reason: format!("{reason}（进阶·仅建议）"),
                    risk: "review".into(),
                    fixable: false,
                    relative_key,
                    backup_key: relative_parent.into(),
                    operation: RegistryOperation::Review,
                });
            }
        }
    }

    /// COM 本地服务器：InprocServer32 / LocalServer32 绝对路径不存在（只读建议）
    fn scan_com_servers(items: &mut Vec<RegistryIssue>, scanned_keys: &mut u64, elevated: bool) {
        let roots: Vec<(&str, RegKey)> = {
            let mut v = vec![("HKCU", RegKey::predef(HKEY_CURRENT_USER))];
            if elevated {
                v.push(("HKLM", RegKey::predef(HKEY_LOCAL_MACHINE)));
            }
            v
        };
        for (hive_name, hive) in roots {
            for clsid_root in [
                r"Software\Classes\CLSID",
                r"Software\Classes\WOW6432Node\CLSID",
            ] {
                let Ok(parent) = hive.open_subkey_with_flags(clsid_root, KEY_READ) else {
                    continue;
                };
                *scanned_keys += 1;
                // 限制数量，避免全盘 CLSID 过慢
                for clsid in parent.enum_keys().flatten().take(if elevated { 2_000 } else { 800 }) {
                    for server in ["InprocServer32", "LocalServer32"] {
                        let relative = format!(r"{clsid_root}\{clsid}\{server}");
                        let Ok(key) = hive.open_subkey_with_flags(&relative, KEY_READ) else {
                            continue;
                        };
                        *scanned_keys += 1;
                        let Some(value) = read_string(&key, "") else {
                            continue;
                        };
                        // 跳过系统已知相对名
                        let lower = value.to_ascii_lowercase();
                        if lower == "combase.dll"
                            || lower == "ole32.dll"
                            || !value.contains(':')
                        {
                            continue;
                        }
                        let Some(missing) = missing_command_target(&value)
                            .or_else(|| absolute_missing_path(&value))
                        else {
                            continue;
                        };
                        // HKCU 可删；HKLM 需管理员且标 medium（前端二次确认）
                        let can_fix = hive_name == "HKCU" || elevated;
                        items.push(RegistryIssue {
                            id: hive_issue_id(hive_name, "com-server", &relative, None),
                            category: "无效的 ActiveX/COM".into(),
                            name: format!("{clsid} / {server}"),
                            key_path: format!(r"{hive_name}\{relative}"),
                            value_name: None,
                            data: value,
                            reason: format!(
                                "COM 服务器文件不存在：{}{}",
                                missing.display(),
                                if can_fix {
                                    "（进阶·可勾选删除，高风险，需二次确认）"
                                } else {
                                    "（进阶·只读，修改需管理员）"
                                }
                            ),
                            risk: "medium".into(),
                            fixable: can_fix,
                            relative_key: relative.clone(),
                            backup_key: format!(r"{hive_name}\{relative}"),
                            operation: if can_fix {
                                RegistryOperation::DeleteKey
                            } else {
                                RegistryOperation::Review
                            },
                        });
                    }
                }
            }
        }
    }

    /// SharedDLLs：路径不存在的共享 DLL 登记
    /// - 进阶：只读
    /// - 专家+管理员：可删除该**注册表值**（不改引用计数数字，整值删除），critical
    fn scan_shared_dlls(
        items: &mut Vec<RegistryIssue>,
        scanned_keys: &mut u64,
        expert: bool,
        elevated: bool,
    ) {
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        const SHARED: &str = r"Software\Microsoft\Windows\CurrentVersion\SharedDLLs";
        let Ok(key) = hklm.open_subkey_with_flags(SHARED, KEY_READ) else {
            return;
        };
        *scanned_keys += 1;
        for (name, value) in key.enum_values().flatten().take(1_500) {
            let path = PathBuf::from(name.trim().trim_matches('"'));
            if !path.is_absolute() || path.exists() {
                continue;
            }
            let count = match value {
                winreg::RegValue { bytes, vtype } => {
                    use winreg::enums::RegType;
                    if matches!(vtype, RegType::REG_DWORD) && bytes.len() >= 4 {
                        u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
                    } else {
                        0
                    }
                }
            };
            let can_fix = expert && elevated;
            items.push(RegistryIssue {
                id: hive_issue_id("HKLM", "shared-dll", SHARED, Some(&name)),
                category: "无效的共享 DLL".into(),
                name: path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| name.clone()),
                key_path: format!(r"HKLM\{SHARED}"),
                value_name: Some(name.clone()),
                data: format!("{name} (引用计数 {count})"),
                reason: if can_fix {
                    format!(
                        "SharedDLLs 登记文件不存在：{name}（专家·可删该注册表值，非改计数；需确认词）"
                    )
                } else {
                    format!(
                        "SharedDLLs 登记文件不存在：{name}（{}）",
                        if expert {
                            "专家·需管理员才可删"
                        } else {
                            "进阶·仅建议"
                        }
                    )
                },
                risk: "critical".into(),
                fixable: can_fix,
                relative_key: SHARED.into(),
                backup_key: format!(r"HKLM\{SHARED}"),
                operation: if can_fix {
                    RegistryOperation::DeleteValue(name)
                } else {
                    RegistryOperation::Review
                },
            });
        }
    }

    /// 服务：ImagePath/ServiceDll 指向不存在 → 只读（绝不自动删）
    fn scan_services_readonly(items: &mut Vec<RegistryIssue>, scanned_keys: &mut u64) {
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        const SERVICES: &str = r"SYSTEM\CurrentControlSet\Services";
        let Ok(parent) = hklm.open_subkey_with_flags(SERVICES, KEY_READ) else {
            return;
        };
        *scanned_keys += 1;
        for svc_name in parent.enum_keys().flatten().take(800) {
            let relative = format!(r"{SERVICES}\{svc_name}");
            let Ok(key) = parent.open_subkey_with_flags(&svc_name, KEY_READ) else {
                continue;
            };
            *scanned_keys += 1;
            // 跳过驱动类型中的内核关键可在 display 里提示；仍只读
            let image = read_string(&key, "ImagePath");
            let Some(image) = image else {
                continue;
            };
            // \SystemRoot\... 或 %SystemRoot% 展开
            let normalized = image
                .trim()
                .trim_start_matches('\\')
                .replace("SystemRoot\\", "%SystemRoot%\\")
                .replace("systemroot\\", "%SystemRoot%\\");
            let Some(missing) = missing_command_target(&normalized)
                .or_else(|| absolute_missing_path(&normalized))
            else {
                continue;
            };
            // 排除明显的系统驱动路径误报：若在 drivers 且我们解析失败已在 missing 中
            items.push(RegistryIssue {
                id: hive_issue_id("HKLM", "service", &relative, Some("ImagePath")),
                category: "无效的服务".into(),
                name: format!("[服务] {svc_name}"),
                key_path: format!(r"HKLM\{relative}"),
                value_name: Some("ImagePath".into()),
                data: image,
                reason: format!(
                    "服务 ImagePath 指向不存在：{}（专家·只读，禁止自动删除以防无法开机）",
                    missing.display()
                ),
                risk: "critical".into(),
                fixable: false,
                relative_key: relative.clone(),
                backup_key: format!(r"HKLM\{relative}"),
                operation: RegistryOperation::Review,
            });
        }
    }

    /// 驱动服务（Type=1/2）只读
    fn scan_drivers_readonly(items: &mut Vec<RegistryIssue>, scanned_keys: &mut u64) {
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        const SERVICES: &str = r"SYSTEM\CurrentControlSet\Services";
        let Ok(parent) = hklm.open_subkey_with_flags(SERVICES, KEY_READ) else {
            return;
        };
        *scanned_keys += 1;
        for svc_name in parent.enum_keys().flatten().take(800) {
            let Ok(key) = parent.open_subkey_with_flags(&svc_name, KEY_READ) else {
                continue;
            };
            let typ = key.get_value::<u32, _>("Type").unwrap_or(0);
            // 1 kernel driver, 2 file system driver
            if typ != 1 && typ != 2 {
                continue;
            }
            *scanned_keys += 1;
            let Some(image) = read_string(&key, "ImagePath") else {
                continue;
            };
            let normalized = image
                .trim()
                .trim_start_matches('\\')
                .replace("SystemRoot\\", "%SystemRoot%\\")
                .replace("??\\", "");
            let Some(missing) = missing_command_target(&normalized)
                .or_else(|| absolute_missing_path(&normalized))
            else {
                continue;
            };
            let relative = format!(r"{SERVICES}\{svc_name}");
            items.push(RegistryIssue {
                id: hive_issue_id("HKLM", "driver", &relative, Some("ImagePath")),
                category: "无效的驱动程序".into(),
                name: format!("[驱动] {svc_name}"),
                key_path: format!(r"HKLM\{relative}"),
                value_name: Some("ImagePath".into()),
                data: image,
                reason: format!(
                    "驱动 ImagePath 指向不存在：{}（专家·只读，删除可能导致无法启动）",
                    missing.display()
                ),
                risk: "critical".into(),
                fixable: false,
                relative_key: relative.clone(),
                backup_key: format!(r"HKLM\{relative}"),
                operation: RegistryOperation::Review,
            });
        }
    }

    /// 文件关联：shell\open\command 目标不存在（用户级可删；系统级专家+管理员可删）
    fn scan_file_associations(
        items: &mut Vec<RegistryIssue>,
        scanned_keys: &mut u64,
        elevated: bool,
    ) {
        // 用户类
        scan_assoc_under(
            "HKCU",
            RegKey::predef(HKEY_CURRENT_USER),
            r"Software\Classes",
            items,
            scanned_keys,
            true,
        );
        if elevated {
            scan_assoc_under(
                "HKLM",
                RegKey::predef(HKEY_LOCAL_MACHINE),
                r"Software\Classes",
                items,
                scanned_keys,
                true,
            );
        }
    }

    fn scan_assoc_under(
        hive_name: &str,
        hive: RegKey,
        classes_rel: &str,
        items: &mut Vec<RegistryIssue>,
        scanned_keys: &mut u64,
        allow_fix: bool,
    ) {
        let Ok(classes) = hive.open_subkey_with_flags(classes_rel, KEY_READ) else {
            return;
        };
        *scanned_keys += 1;
        // 扩展名与少量 ProgID
        for name in classes.enum_keys().flatten().take(600) {
            // .xxx 或明确的 ProgID
            if !(name.starts_with('.') || name.contains("File") || name.ends_with("file")) {
                continue;
            }
            let cmd_rel = format!(r"{classes_rel}\{name}\shell\open\command");
            let Ok(cmd_key) = hive.open_subkey_with_flags(&cmd_rel, KEY_READ) else {
                continue;
            };
            *scanned_keys += 1;
            let Some(command) = read_string(&cmd_key, "") else {
                continue;
            };
            let Some(missing) = missing_command_target(&command) else {
                continue;
            };
            let can_fix = allow_fix && (hive_name == "HKCU" || is_elevated());
            items.push(RegistryIssue {
                id: hive_issue_id(hive_name, "file-assoc", &cmd_rel, None),
                category: "无效的文件关联".into(),
                name: name.clone(),
                key_path: format!(r"{hive_name}\{cmd_rel}"),
                value_name: None,
                data: command,
                reason: format!(
                    "文件关联打开命令目标不存在：{}{}",
                    missing.display(),
                    if can_fix {
                        "（专家·可勾选删除，需二次确认）"
                    } else {
                        "（专家·只读）"
                    }
                ),
                risk: if hive_name == "HKLM" {
                    "medium".into()
                } else {
                    "low".into()
                },
                fixable: can_fix,
                relative_key: cmd_rel.clone(),
                backup_key: format!(r"{hive_name}\{cmd_rel}"),
                operation: if can_fix {
                    RegistryOperation::DeleteKey
                } else {
                    RegistryOperation::Review
                },
            });
        }
    }

    /// 只读：HKLM 字体 / Installer / App Paths 旁路径 / 图像文件执行选项等
    fn scan_hklm_readonly_extras(items: &mut Vec<RegistryIssue>, scanned_keys: &mut u64) {
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        // 字体
        const FONTS: &str = r"Software\Microsoft\Windows NT\CurrentVersion\Fonts";
        if let Ok(key) = hklm.open_subkey_with_flags(FONTS, KEY_READ) {
            *scanned_keys += 1;
            let fonts_dir = std::env::var_os("WINDIR")
                .map(PathBuf::from)
                .map(|p| p.join("Fonts"));
            for (name, _) in key.enum_values().flatten().take(400) {
                let Some(value) = read_string(&key, &name) else {
                    continue;
                };
                let path = {
                    let expanded = expand_environment_variables(&value);
                    let trimmed = expanded.trim().trim_matches('"');
                    let p = PathBuf::from(trimmed);
                    if p.is_absolute() {
                        p
                    } else if let Some(dir) = &fonts_dir {
                        dir.join(trimmed)
                    } else {
                        continue;
                    }
                };
                if path.exists() {
                    continue;
                }
                items.push(RegistryIssue {
                    id: hive_issue_id("HKLM", "fonts", FONTS, Some(&name)),
                    category: "无效的字体".into(),
                    name: format!("[系统] {name}"),
                    key_path: format!(r"HKLM\{FONTS}"),
                    value_name: Some(name.clone()),
                    data: value,
                    reason: format!("系统字体文件不存在：{}（进阶·只读）", path.display()),
                    risk: "review".into(),
                    fixable: false,
                    relative_key: FONTS.into(),
                    backup_key: format!(r"HKLM\{FONTS}"),
                    operation: RegistryOperation::Review,
                });
            }
        }
        // 安装程序文件夹残留路径
        const INSTALLER_FOLDERS: &str =
            r"Software\Microsoft\Windows\CurrentVersion\Installer\Folders";
        if let Ok(key) = hklm.open_subkey_with_flags(INSTALLER_FOLDERS, KEY_READ) {
            *scanned_keys += 1;
            for (name, _) in key.enum_values().flatten().take(300) {
                let path = PathBuf::from(name.trim().trim_matches('"'));
                if !path.is_absolute() || path.exists() {
                    continue;
                }
                items.push(RegistryIssue {
                    id: hive_issue_id("HKLM", "installer-folder", INSTALLER_FOLDERS, Some(&name)),
                    category: "无效的卸载程序".into(),
                    name: path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| name.clone()),
                    key_path: format!(r"HKLM\{INSTALLER_FOLDERS}"),
                    value_name: Some(name.clone()),
                    data: name.clone(),
                    reason: format!("Installer 登记的文件夹不存在：{name}（进阶·只读）"),
                    risk: "review".into(),
                    fixable: false,
                    relative_key: INSTALLER_FOLDERS.into(),
                    backup_key: format!(r"HKLM\{INSTALLER_FOLDERS}"),
                    operation: RegistryOperation::Review,
                });
            }
        }
        // Image File Execution Options：Debugger 指向不存在的程序（常见残留）
        const IFEO: &str = r"Software\Microsoft\Windows NT\CurrentVersion\Image File Execution Options";
        if let Ok(parent) = hklm.open_subkey_with_flags(IFEO, KEY_READ) {
            *scanned_keys += 1;
            for app_name in parent.enum_keys().flatten().take(200) {
                let relative = format!(r"{IFEO}\{app_name}");
                let Ok(key) = parent.open_subkey_with_flags(&app_name, KEY_READ) else {
                    continue;
                };
                *scanned_keys += 1;
                let Some(debugger) = read_string(&key, "Debugger") else {
                    continue;
                };
                let Some(missing) = missing_command_target(&debugger)
                    .or_else(|| absolute_missing_path(&debugger))
                else {
                    continue;
                };
                items.push(RegistryIssue {
                    id: hive_issue_id("HKLM", "ifeo", &relative, Some("Debugger")),
                    category: "无效的文件类型".into(),
                    name: format!("[系统] IFEO · {app_name}"),
                    key_path: format!(r"HKLM\{relative}"),
                    value_name: Some("Debugger".into()),
                    data: debugger,
                    reason: format!(
                        "映像劫持 Debugger 指向不存在：{}（进阶·只读）",
                        missing.display()
                    ),
                    risk: "review".into(),
                    fixable: false,
                    relative_key: relative.clone(),
                    backup_key: format!(r"HKLM\{relative}"),
                    operation: RegistryOperation::Review,
                });
            }
        }
        // Windows 帮助文件（系统级）
        const HELP: &str = r"Software\Microsoft\Windows\HTML Help";
        if let Ok(key) = hklm.open_subkey_with_flags(HELP, KEY_READ) {
            *scanned_keys += 1;
            for (name, _) in key.enum_values().flatten().take(100) {
                let Some(value) = read_string(&key, &name) else {
                    continue;
                };
                let Some(missing) = absolute_missing_path(&value)
                    .or_else(|| missing_command_target(&value))
                else {
                    continue;
                };
                items.push(RegistryIssue {
                    id: hive_issue_id("HKLM", "help", HELP, Some(&name)),
                    category: "无效的帮助文件".into(),
                    name: format!("[系统] {name}"),
                    key_path: format!(r"HKLM\{HELP}"),
                    value_name: Some(name.clone()),
                    data: value,
                    reason: format!("系统帮助文件不存在：{}（进阶·只读）", missing.display()),
                    risk: "review".into(),
                    fixable: false,
                    relative_key: HELP.into(),
                    backup_key: format!(r"HKLM\{HELP}"),
                    operation: RegistryOperation::Review,
                });
            }
        }
    }

    fn run_advanced_categories(
        categories: &[String],
        items: &mut Vec<RegistryIssue>,
        scanned_keys: &mut u64,
        elevated: bool,
    ) {
        if category_enabled(categories, "hklm-startup") || category_enabled(categories, "startup") {
            // 进阶：系统启动项始终附加（即使只勾了 startup 也扫 HKLM，便于对比 Wise）
            if is_advanced_category_request(categories, "hklm-startup", "startup") {
                scan_hklm_startup(items, scanned_keys, elevated);
            }
        }
        if is_advanced_category_request(categories, "hklm-app-path", "app-path") {
            scan_hklm_app_paths(items, scanned_keys);
        }
        if is_advanced_category_request(categories, "hklm-uninstall", "uninstall") {
            scan_hklm_uninstall(items, scanned_keys);
        }
        if category_enabled(categories, "com-server") || categories.is_empty() {
            if categories.is_empty() || category_enabled(categories, "com-server") {
                scan_com_servers(items, scanned_keys, elevated);
            }
        }
        if category_enabled(categories, "shared-dll") || categories.is_empty() {
            if categories.is_empty() || category_enabled(categories, "shared-dll") {
                // expert 标志在外层传入前先用 false，真正专家扫描在 scan_registry 再调
                scan_shared_dlls(items, scanned_keys, false, elevated);
            }
        }
        // 字体/Installer 路径：挂在 fonts / uninstall 分类下展示
        if categories.is_empty()
            || category_enabled(categories, "fonts")
            || category_enabled(categories, "uninstall")
        {
            scan_hklm_readonly_extras(items, scanned_keys);
        }
    }

    fn run_expert_categories(
        categories: &[String],
        items: &mut Vec<RegistryIssue>,
        scanned_keys: &mut u64,
        elevated: bool,
    ) {
        // 先按进阶跑一遍（含 SharedDLL 只读）
        run_advanced_categories(categories, items, scanned_keys, elevated);
        // 覆盖 SharedDLL 为专家策略：移除只读条目后重扫
        items.retain(|i| i.category != "无效的共享 DLL");
        if categories.is_empty() || category_enabled(categories, "shared-dll") {
            scan_shared_dlls(items, scanned_keys, true, elevated);
        }
        if categories.is_empty() || category_enabled(categories, "service") {
            scan_services_readonly(items, scanned_keys);
        }
        if categories.is_empty() || category_enabled(categories, "driver") {
            scan_drivers_readonly(items, scanned_keys);
        }
        if categories.is_empty() || category_enabled(categories, "file-assoc") {
            scan_file_associations(items, scanned_keys, elevated);
        }
    }

    fn is_advanced_category_request(categories: &[String], advanced_id: &str, basic_id: &str) -> bool {
        categories.is_empty()
            || category_enabled(categories, advanced_id)
            || category_enabled(categories, basic_id)
    }

    /// 管理员 + 所有用户：扫描 HKEY_USERS 下真实用户 SID 的启动项（其它用户仅作展示/建议）
    fn scan_other_users_startup(
        items: &mut Vec<RegistryIssue>,
        scanned_keys: &mut u64,
        elevated: bool,
    ) {
        if !elevated {
            return;
        }
        let users = RegKey::predef(HKEY_USERS);
        *scanned_keys += 1;
        for sid in users.enum_keys().flatten().take(64) {
            // 跳过 .DEFAULT、_Classes、短 SID
            if !sid.starts_with("S-1-5-21-") || sid.contains("_Classes") {
                continue;
            }
            let Ok(user_root) = users.open_subkey_with_flags(&sid, KEY_READ) else {
                continue;
            };
            *scanned_keys += 1;
            let prefix = format!(r"HKEY_USERS\{sid}");
            // 其它用户默认不自动修复（避免误改他人配置）；仅当 elevated 时也仍标为可修当前可写项——此处一律 review 更安全
            scan_startup_on(&user_root, &prefix, items, scanned_keys, false);
        }
    }

    fn emit_registry_progress(app: Option<&AppHandle>, message: &str, percentage: u8) {
        let Some(app) = app else {
            return;
        };
        let _ = app.emit(
            "registry-progress",
            serde_json::json!({
                "message": message,
                "percentage": percentage,
            }),
        );
    }

    pub fn scan_registry(
        options: RegistryScanOptions,
        app: Option<AppHandle>,
    ) -> Result<RegistryReport, String> {
        let started = Instant::now();
        let mut items = Vec::new();
        let mut scanned_keys = 0;
        let elevated = is_elevated();
        let want_all = options.user_scope.eq_ignore_ascii_case("all");
        let advanced = is_advanced_mode(&options.mode);
        let expert = is_expert_mode(&options.mode);
        let app_ref = app.as_ref();

        emit_registry_progress(app_ref, "正在检查当前用户…", 6);
        run_categories_on_current_user(&options.categories, &mut items, &mut scanned_keys, app_ref);
        emit_registry_progress(
            app_ref,
            &format!("当前用户完成 · 已查 {} 键，命中 {}", scanned_keys, items.len()),
            if expert {
                38
            } else if advanced {
                42
            } else {
                88
            },
        );

        if want_all && category_enabled(&options.categories, "startup") {
            emit_registry_progress(app_ref, "正在检查其他用户启动项…", 45);
            scan_other_users_startup(&mut items, &mut scanned_keys, elevated);
            emit_registry_progress(
                app_ref,
                &format!("其他用户启动项完成 · 命中 {}", items.len()),
                52,
            );
        }

        if expert {
            emit_registry_progress(app_ref, "专家：HKLM 启动/路径/卸载…", 55);
            run_expert_categories(
                &options.categories,
                &mut items,
                &mut scanned_keys,
                elevated,
            );
            emit_registry_progress(
                app_ref,
                &format!(
                    "专家完成 · 已检查 {} 键，命中 {} 项",
                    scanned_keys,
                    items.len()
                ),
                90,
            );
        } else if advanced {
            emit_registry_progress(app_ref, "进阶：HKLM 启动项与 App Paths…", 50);
            run_advanced_categories(
                &options.categories,
                &mut items,
                &mut scanned_keys,
                elevated,
            );
            emit_registry_progress(
                app_ref,
                &format!("进阶完成 · 已检查 {} 键，命中 {} 项", scanned_keys, items.len()),
                88,
            );
        }

        if !options.exclusions.is_empty() {
            emit_registry_progress(app_ref, "正在应用排除列表…", 92);
            items.retain(|item| {
                !is_excluded(
                    &options.exclusions,
                    &item.name,
                    &item.key_path,
                    &item.data,
                )
            });
        }
        items.sort_by(|left, right| {
            left.fixable
                .cmp(&right.fixable)
                .reverse()
                .then_with(|| left.category.cmp(&right.category))
                .then_with(|| left.name.cmp(&right.name))
        });
        let fixable_count = items.iter().filter(|item| item.fixable).count() as u64;
        let review_count = items.len() as u64 - fixable_count;
        let categories = summarize_categories(&items);
        emit_registry_progress(app_ref, "注册表检查完成", 100);
        Ok(RegistryReport {
            items,
            categories,
            scanned_keys,
            fixable_count,
            review_count,
            elapsed_ms: started.elapsed().as_millis(),
            user_scope: if want_all { "all".into() } else { "current".into() },
            elevated,
        })
    }

    fn summarize_categories(items: &[RegistryIssue]) -> Vec<RegistryCategorySummary> {
        // 对齐专业注册表工具的分类命名（仅 HKCU 铁证路径）
        let order = [
            ("无效的启动程序", "startup"),
            ("无效的应用程序路径", "app-path"),
            ("无效的卸载程序", "uninstall"),
            ("无效的历史记录", "history"),
            ("无效的文件类型", "user-command"),
            ("无效的帮助文件", "help"),
            ("无效的字体", "fonts"),
            ("无效的声音事件", "sound"),
            ("无效的 ActiveX/COM", "com-server"),
            ("无效的共享 DLL", "shared-dll"),
            ("无效的文件关联", "file-assoc"),
            ("无效的服务", "service"),
            ("无效的驱动程序", "driver"),
        ];
        let mut out = Vec::new();
        for (name, id) in order {
            let group: Vec<_> = items.iter().filter(|i| i.category == name).collect();
            if group.is_empty() {
                continue;
            }
            let fixable = group.iter().filter(|i| i.fixable).count() as u64;
            out.push(RegistryCategorySummary {
                id: id.into(),
                name: name.into(),
                total: group.len() as u64,
                fixable,
                review: group.len() as u64 - fixable,
            });
        }
        out
    }

    fn is_elevated() -> bool {
        Command::new("net")
            .args(["session"])
            .creation_flags(CREATE_NO_WINDOW)
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

    fn backups_root() -> Result<PathBuf, String> {
        let base = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .ok_or("无法确定本地应用数据目录")?;
        let directory = base.join("DiskAnalyzer").join("registry-backups");
        fs::create_dir_all(&directory)
            .map_err(|error| format!("无法创建注册表备份根目录: {error}"))?;
        Ok(directory)
    }

    fn backup_root() -> Result<PathBuf, String> {
        let directory = backups_root()?.join(Local::now().format("%Y%m%d-%H%M%S").to_string());
        fs::create_dir_all(&directory)
            .map_err(|error| format!("无法创建注册表备份目录: {error}"))?;
        Ok(directory)
    }

    fn export_key(source: &str, destination: &Path) -> Result<(), String> {
        let status = Command::new("reg.exe")
            .args(["export", source])
            .arg(destination)
            .arg("/y")
            .creation_flags(CREATE_NO_WINDOW)
            .status()
            .map_err(|error| format!("无法启动注册表备份: {error}"))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("注册表备份失败：{source}"))
        }
    }

    fn export_relative_hkcu(relative_key: &str, destination: &Path) -> Result<(), String> {
        export_key(&format!(r"HKCU\{relative_key}"), destination)
    }

    pub fn list_registry_backups() -> Result<Vec<RegistryBackupInfo>, String> {
        let root = backups_root()?;
        let mut list = Vec::new();
        let Ok(entries) = fs::read_dir(&root) else {
            return Ok(list);
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let meta_path = path.join("backup.json");
            let (label, created_at, kind) = if meta_path.is_file() {
                let text = fs::read_to_string(&meta_path).unwrap_or_default();
                let value: serde_json::Value =
                    serde_json::from_str(&text).unwrap_or(serde_json::json!({}));
                (
                    value["label"]
                        .as_str()
                        .unwrap_or("注册表备份")
                        .to_string(),
                    value["createdAt"]
                        .as_str()
                        .unwrap_or("")
                        .to_string(),
                    value["kind"].as_str().unwrap_or("repair").to_string(),
                )
            } else {
                (
                    "注册表备份".into(),
                    path.file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    "repair".into(),
                )
            };
            let file_count = fs::read_dir(&path)
                .map(|rd| {
                    rd.flatten()
                        .filter(|e| {
                            e.path()
                                .extension()
                                .and_then(|x| x.to_str())
                                .map(|x| x.eq_ignore_ascii_case("reg"))
                                .unwrap_or(false)
                        })
                        .count() as u64
                })
                .unwrap_or(0);
            if file_count == 0 && !meta_path.is_file() {
                continue;
            }
            list.push(RegistryBackupInfo {
                id: path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                path: path.display().to_string(),
                label,
                created_at,
                file_count,
                kind,
            });
        }
        list.sort_by(|a, b| b.id.cmp(&a.id));
        Ok(list)
    }

    pub fn create_full_registry_backup(
        label: Option<String>,
        destination_dir: Option<String>,
    ) -> Result<RegistryBackupInfo, String> {
        let stamp = Local::now().format("%Y%m%d-%H%M%S").to_string();
        let directory = if let Some(dest) = destination_dir.filter(|v| !v.trim().is_empty()) {
            let base = PathBuf::from(dest.trim());
            if !base.is_absolute() {
                return Err("备份目标必须是绝对路径".into());
            }
            if base.exists() && !base.is_dir() {
                return Err("备份目标必须是文件夹".into());
            }
            fs::create_dir_all(&base).map_err(|e| format!("无法创建备份目标目录: {e}"))?;
            let dir = base.join(format!("DiskAnalyzer-registry-{stamp}"));
            fs::create_dir_all(&dir).map_err(|e| format!("无法创建备份子目录: {e}"))?;
            dir
        } else {
            let dir = backups_root()?.join(&stamp);
            fs::create_dir_all(&dir).map_err(|e| format!("无法创建注册表备份目录: {e}"))?;
            dir
        };
        let keys = [
            RUN,
            RUN_ONCE,
            POLICIES_RUN,
            APP_PATHS,
            UNINSTALL,
            TYPED_PATHS,
            RUN_MRU,
            USER_CLASSES_APPS,
            r"Software\Microsoft\Windows\CurrentVersion\Explorer",
            r"Software\Microsoft\Windows\CurrentVersion\Applets",
            r"AppEvents\Schemes\Apps",
        ];
        let mut exported = 0_u64;
        for (index, key) in keys.iter().enumerate() {
            let dest = directory.join(backup_file_name(key, index));
            // 部分键可能不存在，跳过失败
            if export_relative_hkcu(key, &dest).is_ok() {
                exported += 1;
            }
        }
        if exported == 0 {
            let _ = fs::remove_dir_all(&directory);
            return Err("未能导出任何注册表分支，请检查权限".into());
        }
        let label = label
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "完整备份（修复前保护）".into());
        let created_at = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let meta = serde_json::json!({
            "label": label,
            "createdAt": created_at,
            "kind": "full",
            "fileCount": exported,
            "app": "DiskAnalyzer",
        });
        fs::write(
            directory.join("backup.json"),
            serde_json::to_string_pretty(&meta).unwrap_or_default(),
        )
        .map_err(|e| format!("无法写入备份元数据: {e}"))?;
        Ok(RegistryBackupInfo {
            id: directory
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or(stamp),
            path: directory.display().to_string(),
            label,
            created_at,
            file_count: exported,
            kind: "full".into(),
        })
    }

    fn is_our_backup_dir(directory: &Path) -> bool {
        if backups_root()
            .ok()
            .and_then(|root| {
                let dir = directory.canonicalize().ok()?;
                let root = root.canonicalize().ok()?;
                Some(dir.starts_with(root))
            })
            .unwrap_or(false)
        {
            return true;
        }
        // 用户自选目录：必须有我们写入的 backup.json
        let meta_path = directory.join("backup.json");
        if !meta_path.is_file() {
            return false;
        }
        fs::read_to_string(meta_path)
            .ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            .map(|v| {
                v["app"].as_str() == Some("DiskAnalyzer")
                    || v["kind"].as_str() == Some("full")
                    || v["kind"].as_str() == Some("repair")
            })
            .unwrap_or(false)
    }

    pub fn restore_registry_backup(path: String) -> Result<String, String> {
        let directory = PathBuf::from(path);
        if !directory.is_dir() {
            return Err("备份目录不存在".into());
        }
        if !is_our_backup_dir(&directory) {
            return Err(
                "只能恢复本应用创建的备份（默认备份目录，或含 DiskAnalyzer backup.json 的自选目录）"
                    .into(),
            );
        }
        let mut files: Vec<_> = fs::read_dir(&directory)
            .map_err(|e| format!("无法读取备份目录: {e}"))?
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.extension()
                    .and_then(|x| x.to_str())
                    .map(|x| x.eq_ignore_ascii_case("reg"))
                    .unwrap_or(false)
            })
            .collect();
        files.sort();
        if files.is_empty() {
            return Err("备份目录中没有 .reg 文件".into());
        }
        let mut ok = 0_u64;
        let mut failed = 0_u64;
        for file in &files {
            let status = Command::new("reg.exe")
                .args(["import"])
                .arg(file)
                .creation_flags(CREATE_NO_WINDOW)
                .status();
            match status {
                Ok(s) if s.success() => ok += 1,
                _ => failed += 1,
            }
        }
        if ok == 0 {
            return Err("注册表恢复失败，请以管理员身份重试或手动双击 .reg 文件".into());
        }
        Ok(format!(
            "已将备份写回注册表原位置（导入 {ok} 个 .reg 文件{}）。这会覆盖当前对应键值，不是把文件复制回文件夹。",
            if failed > 0 {
                format!("，{failed} 个失败")
            } else {
                String::new()
            }
        ))
    }

    fn backup_file_name(key: &str, index: usize) -> String {
        let leaf = key
            .rsplit('\u{5c}')
            .next()
            .unwrap_or("registry")
            .chars()
            .filter(|character| {
                character.is_ascii_alphanumeric() || *character == '-' || *character == '_'
            })
            .collect::<String>();
        format!(
            "{:02}-{}.reg",
            index + 1,
            if leaf.is_empty() { "registry" } else { &leaf }
        )
    }

    fn parse_hive_prefix(key_path: &str) -> (&'static str, &str) {
        if let Some(rest) = key_path.strip_prefix(r"HKLM\") {
            ("HKLM", rest)
        } else if let Some(rest) = key_path.strip_prefix(r"HKEY_LOCAL_MACHINE\") {
            ("HKLM", rest)
        } else if let Some(rest) = key_path.strip_prefix(r"HKCU\") {
            ("HKCU", rest)
        } else if let Some(rest) = key_path.strip_prefix(r"HKEY_CURRENT_USER\") {
            ("HKCU", rest)
        } else {
            ("HKCU", key_path)
        }
    }

    fn open_hive_key(hive: &str, relative: &str, write: bool) -> Result<RegKey, String> {
        let flags = if write {
            KEY_READ | KEY_WRITE
        } else {
            KEY_READ
        };
        let root = match hive {
            "HKLM" => RegKey::predef(HKEY_LOCAL_MACHINE),
            _ => RegKey::predef(HKEY_CURRENT_USER),
        };
        root.open_subkey_with_flags(relative, flags)
            .map_err(|e| format!("无法打开 {hive}\\{relative}: {e}"))
    }

    pub fn repair_registry(ids: Vec<String>) -> Result<RegistryRepairResult, String> {
        if ids.is_empty() || ids.len() > 200 {
            return Err("请选择 1–200 个可修复项目".into());
        }
        let requested: HashSet<_> = ids.into_iter().collect();
        // 修复前用进阶+当前用户再扫，覆盖 HKCU/HKLM 可修项
        let report = scan_registry(
            RegistryScanOptions {
                categories: vec![],
                exclusions: vec![],
                user_scope: "current".into(),
                mode: "advanced".into(),
            },
            None,
        )?;
        let selected: Vec<_> = report
            .items
            .into_iter()
            .filter(|item| item.fixable && requested.contains(&item.id))
            .collect();
        if selected.is_empty() {
            return Err("所选项目已不存在或不属于允许修复的范围".into());
        }

        let backup_directory = backup_root()?;
        let mut backup_sources: Vec<_> = selected
            .iter()
            .map(|item| item.backup_key.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        backup_sources.sort();
        for (index, source) in backup_sources.iter().enumerate() {
            let dest = backup_directory.join(backup_file_name(source, index));
            // backup_key 可能是 HKLM\... 或相对 HKCU 路径
            let full = if source.starts_with("HKLM\\")
                || source.starts_with("HKCU\\")
                || source.starts_with("HKEY_")
            {
                source.clone()
            } else {
                format!(r"HKCU\{source}")
            };
            let _ = export_key(&full, &dest);
        }
        let meta = serde_json::json!({
            "label": "修复前自动备份",
            "createdAt": Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            "kind": "repair",
            "fileCount": backup_sources.len(),
            "app": "DiskAnalyzer",
        });
        let _ = fs::write(
            backup_directory.join("backup.json"),
            serde_json::to_string_pretty(&meta).unwrap_or_default(),
        );

        let mut repaired = 0;
        let mut failed = 0;
        let mut opened: HashMap<String, RegKey> = HashMap::new();
        for item in selected {
            let (hive, rel_from_path) = parse_hive_prefix(&item.key_path);
            // relative_key 对 HKCU 是相对路径；对 HKLM 也可能已是完整相对
            let relative = if item.relative_key.starts_with("Software")
                || item.relative_key.starts_with("AppEvents")
            {
                item.relative_key.clone()
            } else {
                rel_from_path.to_string()
            };
            let cache_key = format!("{hive}\\{relative}");
            let outcome = match &item.operation {
                RegistryOperation::DeleteValue(value_name) => {
                    if !opened.contains_key(&cache_key) {
                        match open_hive_key(hive, &relative, true) {
                            Ok(key) => {
                                opened.insert(cache_key.clone(), key);
                            }
                            Err(_) => {
                                failed += 1;
                                continue;
                            }
                        }
                    }
                    opened
                        .get(&cache_key)
                        .expect("已打开的注册表键")
                        .delete_value(value_name)
                }
                RegistryOperation::DeleteKey => {
                    let root = match hive {
                        "HKLM" => RegKey::predef(HKEY_LOCAL_MACHINE),
                        _ => RegKey::predef(HKEY_CURRENT_USER),
                    };
                    root.delete_subkey_all(&relative)
                }
                RegistryOperation::Review => {
                    failed += 1;
                    continue;
                }
            };
            if outcome.is_ok() {
                repaired += 1;
            } else {
                failed += 1;
            }
        }
        Ok(RegistryRepairResult {
            repaired,
            failed,
            backup_directory: backup_directory.display().to_string(),
        })
    }
}

#[cfg(windows)]
pub use platform::{
    create_full_registry_backup, list_registry_backups, repair_registry, restore_registry_backup,
    scan_registry,
};

#[cfg(not(windows))]
pub fn scan_registry(
    _options: RegistryScanOptions,
    _app: Option<AppHandle>,
) -> Result<RegistryReport, String> {
    Err("注册表检查仅支持 Windows".into())
}

#[cfg(not(windows))]
pub fn repair_registry(_ids: Vec<String>) -> Result<RegistryRepairResult, String> {
    Err("注册表修复仅支持 Windows".into())
}

#[cfg(not(windows))]
pub fn list_registry_backups() -> Result<Vec<RegistryBackupInfo>, String> {
    Err("注册表备份列表仅支持 Windows".into())
}

#[cfg(not(windows))]
pub fn create_full_registry_backup(
    _label: Option<String>,
    _destination_dir: Option<String>,
) -> Result<RegistryBackupInfo, String> {
    Err("注册表完整备份仅支持 Windows".into())
}

#[cfg(not(windows))]
pub fn restore_registry_backup(_path: String) -> Result<String, String> {
    Err("注册表恢复仅支持 Windows".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_quoted_and_unquoted_executables() {
        assert_eq!(
            executable_from_command(r#""C:\Program Files\Tool\tool.exe" --silent"#).unwrap(),
            PathBuf::from(r"C:\Program Files\Tool\tool.exe")
        );
        assert_eq!(
            executable_from_command(r"C:\Tools\agent.exe /background").unwrap(),
            PathBuf::from(r"C:\Tools\agent.exe")
        );
    }

    #[test]
    fn ignores_non_absolute_commands() {
        assert!(executable_from_command("rundll32.exe shell32.dll").is_none());
    }

    #[test]
    fn category_filter_defaults_to_all() {
        assert!(category_enabled(&[], "startup"));
        assert!(category_enabled(&["startup".into()], "startup"));
        assert!(!category_enabled(&["startup".into()], "uninstall"));
    }

    #[test]
    fn exclusion_matches_name_or_path() {
        let exclusions = vec!["OldSync".into(), r"App Paths".into()];
        assert!(is_excluded(
            &exclusions,
            "OldSyncAgent",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
            r"C:\missing.exe"
        ));
        assert!(is_excluded(
            &exclusions,
            "tool.exe",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\App Paths\tool.exe",
            r"D:\gone.exe"
        ));
        assert!(!is_excluded(
            &exclusions,
            "KeepMe",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
            r"C:\ok.exe"
        ));
    }

    #[test]
    fn advanced_mode_scans_at_least_as_many_keys_as_basic() {
        let basic = crate::registry::scan_registry(
            crate::registry::RegistryScanOptions {
                categories: vec![],
                exclusions: vec![],
                user_scope: "current".into(),
                mode: "basic".into(),
            },
            None,
        )
        .unwrap();
        let advanced = crate::registry::scan_registry(
            crate::registry::RegistryScanOptions {
                categories: vec![],
                exclusions: vec![],
                user_scope: "current".into(),
                mode: "advanced".into(),
            },
            None,
        )
        .unwrap();
        eprintln!(
            "BASIC total={} keys={} | ADVANCED total={} keys={}",
            basic.items.len(),
            basic.scanned_keys,
            advanced.items.len(),
            advanced.scanned_keys
        );
        assert!(advanced.scanned_keys >= basic.scanned_keys);
    }
}