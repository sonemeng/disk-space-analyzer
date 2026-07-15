use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::Instant;

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
    pub scanned_keys: u64,
    pub fixable_count: u64,
    pub review_count: u64,
    pub elapsed_ms: u128,
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
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
    use winreg::RegKey;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const RUN_ONCE: &str = r"Software\Microsoft\Windows\CurrentVersion\RunOnce";
    const APP_PATHS: &str = r"Software\Microsoft\Windows\CurrentVersion\App Paths";
    const UNINSTALL: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall";

    fn read_string(key: &RegKey, name: &str) -> Option<String> {
        key.get_value::<String, _>(name)
            .ok()
            .filter(|value| !value.trim().is_empty())
    }

    fn scan_startup(items: &mut Vec<RegistryIssue>, scanned_keys: &mut u64) {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        for relative_key in [RUN, RUN_ONCE] {
            let Ok(key) = hkcu.open_subkey_with_flags(relative_key, KEY_READ) else {
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
                items.push(RegistryIssue {
                    id: issue_id("startup", relative_key, Some(&name)),
                    category: "失效启动项".into(),
                    name: name.clone(),
                    key_path: format!(r"HKCU\{relative_key}"),
                    value_name: Some(name.clone()),
                    data: command,
                    reason: format!("启动目标不存在：{}", missing.display()),
                    risk: "low".into(),
                    fixable: true,
                    relative_key: relative_key.into(),
                    backup_key: relative_key.into(),
                    operation: RegistryOperation::DeleteValue(name),
                });
            }
        }
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
            let Some(target) = read_string(&key, "") else {
                continue;
            };
            let expanded = expand_environment_variables(&target);
            let path = PathBuf::from(expanded.trim().trim_matches('"'));
            if !path.is_absolute() || path.exists() {
                continue;
            }
            let relative_key = format!(r"{APP_PATHS}\{subkey_name}");
            items.push(RegistryIssue {
                id: issue_id("app-path", &relative_key, None),
                category: "失效应用路径".into(),
                name: subkey_name,
                key_path: format!(r"HKCU\{relative_key}"),
                value_name: None,
                data: target,
                reason: format!("登记的应用程序不存在：{}", path.display()),
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
            let install_missing = install_location
                .as_deref()
                .map(expand_environment_variables)
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
                .map(|path| !path.exists());
            let uninstaller_missing = uninstall_command
                .as_deref()
                .and_then(missing_command_target)
                .is_some();
            if install_missing != Some(true) || !uninstaller_missing {
                continue;
            }
            let relative_key = format!(r"{UNINSTALL}\{subkey_name}");
            items.push(RegistryIssue {
                id: issue_id("uninstall", &relative_key, None),
                category: "卸载信息残留".into(),
                name: display_name,
                key_path: format!(r"HKCU\{relative_key}"),
                value_name: None,
                data: uninstall_command.unwrap_or_default(),
                reason: "安装目录和卸载程序均不存在，建议先确认软件确已移除".into(),
                risk: "review".into(),
                fixable: false,
                relative_key,
                backup_key: UNINSTALL.into(),
                operation: RegistryOperation::Review,
            });
        }
    }

    pub fn scan_registry() -> Result<RegistryReport, String> {
        let started = Instant::now();
        let mut items = Vec::new();
        let mut scanned_keys = 0;
        scan_startup(&mut items, &mut scanned_keys);
        scan_app_paths(&mut items, &mut scanned_keys);
        scan_uninstall(&mut items, &mut scanned_keys);
        items.sort_by(|left, right| {
            left.fixable
                .cmp(&right.fixable)
                .reverse()
                .then_with(|| left.category.cmp(&right.category))
                .then_with(|| left.name.cmp(&right.name))
        });
        let fixable_count = items.iter().filter(|item| item.fixable).count() as u64;
        let review_count = items.len() as u64 - fixable_count;
        Ok(RegistryReport {
            items,
            scanned_keys,
            fixable_count,
            review_count,
            elapsed_ms: started.elapsed().as_millis(),
        })
    }

    fn backup_root() -> Result<PathBuf, String> {
        let base = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .ok_or("无法确定本地应用数据目录")?;
        let directory = base
            .join("DiskAnalyzer")
            .join("registry-backups")
            .join(Local::now().format("%Y%m%d-%H%M%S").to_string());
        fs::create_dir_all(&directory)
            .map_err(|error| format!("无法创建注册表备份目录: {error}"))?;
        Ok(directory)
    }

    fn export_key(relative_key: &str, destination: &Path) -> Result<(), String> {
        let source = format!(r"HKCU\{relative_key}");
        let status = Command::new("reg.exe")
            .args(["export", &source])
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

    pub fn repair_registry(ids: Vec<String>) -> Result<RegistryRepairResult, String> {
        if ids.is_empty() || ids.len() > 200 {
            return Err("请选择 1–200 个可修复项目".into());
        }
        let requested: HashSet<_> = ids.into_iter().collect();
        let report = scan_registry()?;
        let selected: Vec<_> = report
            .items
            .into_iter()
            .filter(|item| item.fixable && requested.contains(&item.id))
            .collect();
        if selected.is_empty() {
            return Err("所选项目已不存在或不属于允许修复的低风险范围".into());
        }

        let backup_directory = backup_root()?;
        let mut backup_keys: Vec<_> = selected
            .iter()
            .map(|item| item.backup_key.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        backup_keys.sort();
        for (index, key) in backup_keys.iter().enumerate() {
            export_key(key, &backup_directory.join(backup_file_name(key, index)))?;
        }

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let mut repaired = 0;
        let mut failed = 0;
        let mut opened: HashMap<String, RegKey> = HashMap::new();
        for item in selected {
            let outcome = match item.operation {
                RegistryOperation::DeleteValue(value_name) => {
                    if !opened.contains_key(&item.relative_key) {
                        match hkcu.open_subkey_with_flags(&item.relative_key, KEY_READ | KEY_WRITE)
                        {
                            Ok(key) => {
                                opened.insert(item.relative_key.clone(), key);
                            }
                            Err(_) => {
                                failed += 1;
                                continue;
                            }
                        }
                    }
                    opened
                        .get(&item.relative_key)
                        .expect("已打开的注册表键")
                        .delete_value(value_name)
                }
                RegistryOperation::DeleteKey => hkcu.delete_subkey_all(&item.relative_key),
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
pub use platform::{repair_registry, scan_registry};

#[cfg(not(windows))]
pub fn scan_registry() -> Result<RegistryReport, String> {
    Err("注册表检查仅支持 Windows".into())
}

#[cfg(not(windows))]
pub fn repair_registry(_ids: Vec<String>) -> Result<RegistryRepairResult, String> {
    Err("注册表修复仅支持 Windows".into())
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
}
