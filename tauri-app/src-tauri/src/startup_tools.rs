//! 启动项管理：注册表 Run/RunOnce + 启动文件夹。
//! 禁用 = 从注册表删除值或文件夹改名，原始值备份到应用数据目录 JSON 可随时恢复。
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RegType};
use winreg::reg_key::HKEY;
use winreg::{RegKey, RegValue};

static TOOLS_ROOT: OnceLock<PathBuf> = OnceLock::new();

pub fn init(root: PathBuf) -> Result<(), String> {
    if TOOLS_ROOT.set(root.clone()).is_err() {
        return Ok(());
    }
    fs::create_dir_all(&root).map_err(|e| format!("无法创建系统工具数据目录: {e}"))
}

fn data_dir() -> Result<&'static PathBuf, String> {
    TOOLS_ROOT.get().ok_or_else(|| "启动项管理未初始化".into())
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupItem {
    pub name: String,
    pub command: String,
    pub location: String,
    pub enabled: bool,
    pub file_path: Option<String>,
    /// data-url 图标（png base64），提取失败为 None
    pub icon: Option<String>,
    /// 唯一标识：reg:<HKLM|HKCU>:<键>:<值名> 或 folder:<绝对路径>
    pub key: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartupBackup {
    key: String,
    name: String,
    command: String,
    location: String,
    file_path: Option<String>,
    is_folder: bool,
}

fn read_backups() -> Vec<StartupBackup> {
    let path = data_dir()
        .map(|d| d.join("startup-backups.json"))
        .unwrap_or_default();
    fs::read_to_string(&path)
        .ok()
        .and_then(|c| serde_json::from_str(&c).ok())
        .unwrap_or_default()
}

fn write_backups(backups: &[StartupBackup]) -> Result<(), String> {
    let path = data_dir()?.join("startup-backups.json");
    let content =
        serde_json::to_string_pretty(backups).map_err(|e| format!("序列化备份失败: {e}"))?;
    fs::write(path, content).map_err(|e| format!("保存备份失败: {e}"))
}

/// REG_SZ/REG_EXPAND_SZ 为 UTF-16LE 宽字符，正确解码并去末尾 \0。
fn decode_reg_string(bytes: &[u8]) -> Option<String> {
    if bytes.is_empty() || bytes.len() % 2 != 0 {
        return None;
    }
    let u16s: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    let mut s = String::from_utf16(&u16s).ok()?;
    while s.ends_with('\0') {
        s.pop();
    }
    Some(s)
}

fn collect_registry_entries(hive: HKEY, key_path: &str, label: &str, out: &mut Vec<StartupItem>) {
    let Ok(reg) = RegKey::predef(hive).open_subkey(key_path) else {
        return;
    };
    for (name, value) in reg.enum_values().filter_map(|v| v.ok()) {
        if !matches!(value.vtype, RegType::REG_SZ | RegType::REG_EXPAND_SZ) {
            continue;
        }
        let text = decode_reg_string(&value.bytes)
            .or_else(|| {
                Some(
                    String::from_utf8_lossy(&value.bytes)
                        .trim_end_matches('\0')
                        .to_string(),
                )
            })
            .unwrap_or_default();
        if text.trim().is_empty() || name.starts_with("__da_disabled_") {
            continue;
        }
        let hive_label = if hive == HKEY_LOCAL_MACHINE { "HKLM" } else { "HKCU" };
        out.push(StartupItem {
            name: name.clone(),
            command: text,
            location: label.to_string(),
            enabled: true,
            file_path: None,
            icon: None,
            key: format!("reg:{hive_label}:{key_path}:{name}"),
        });
    }
}

fn startup_folders() -> Vec<(PathBuf, &'static str)> {
    let mut folders = Vec::new();
    if let Some(appdata) = std::env::var_os("APPDATA") {
        folders.push((
            PathBuf::from(appdata).join("Microsoft\\Windows\\Start Menu\\Programs\\Startup"),
            "启动文件夹 · 当前用户",
        ));
    }
    folders.push((
        PathBuf::from(r"C:\ProgramData\Microsoft\Windows\Start Menu\Programs\Startup"),
        "启动文件夹 · 本机",
    ));
    folders
}

fn collect_folder_entries(out: &mut Vec<StartupItem>) {
    for (folder, label) in startup_folders() {
        let Ok(entries) = fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with("__da_disabled_") {
                continue;
            }
            out.push(StartupItem {
                name,
                command: path.to_string_lossy().into_owned(),
                location: label.to_string(),
                enabled: true,
                file_path: Some(path.to_string_lossy().into_owned()),
                icon: None,
                key: format!("folder:{}", path.to_string_lossy()),
            });
        }
    }
}

/// 解析 reg:<hive>:<键>:<值名>
fn parse_reg_key(key: &str) -> Option<(HKEY, String, String)> {
    let rest = key.strip_prefix("reg:")?;
    let (hive, rest) = rest.split_once(':')?;
    let hive = if hive == "HKLM" {
        HKEY_LOCAL_MACHINE
    } else if hive == "HKCU" {
        HKEY_CURRENT_USER
    } else {
        return None;
    };
    let (reg_path, name) = rest.rsplit_once(':')?;
    Some((hive, reg_path.to_string(), name.to_string()))
}

pub fn list_startup_items() -> Result<Vec<StartupItem>, String> {
    let mut items = Vec::new();
    collect_registry_entries(
        HKEY_CURRENT_USER,
        r"Software\Microsoft\Windows\CurrentVersion\Run",
        "注册表 · 当前用户 Run",
        &mut items,
    );
    collect_registry_entries(
        HKEY_LOCAL_MACHINE,
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run",
        "注册表 · 本机 Run",
        &mut items,
    );
    collect_registry_entries(
        HKEY_CURRENT_USER,
        r"Software\Microsoft\Windows\CurrentVersion\RunOnce",
        "注册表 · 当前用户 RunOnce",
        &mut items,
    );
    collect_registry_entries(
        HKEY_LOCAL_MACHINE,
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\RunOnce",
        "注册表 · 本机 RunOnce",
        &mut items,
    );
    collect_folder_entries(&mut items);

    let backups = read_backups();
    for item in &mut items {
        if backups.iter().any(|b| b.key == item.key) {
            item.enabled = false;
        }
    }
    items.sort_by(|a, b| {
        a.location
            .cmp(&b.location)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    enrich_icons(&mut items);
    Ok(items)
}

/// 从注册表命令中解析出可提取图标的文件路径（与 folder 条目共用）。
fn resolve_command_path(cmd: &str) -> Option<String> {
    let cmd = cmd.trim();
    if cmd.is_empty() {
        return None;
    }
    let head = if let Some(rest) = cmd.strip_prefix('"') {
        rest.split('"').next().unwrap_or("")
    } else {
        cmd.split_whitespace().next().unwrap_or("")
    };
    if head.is_empty() {
        return None;
    }
    if head.eq_ignore_ascii_case("start") {
        return match cmd.find('"') {
            Some(i) => {
                let rest = &cmd[i + 1..];
                match rest.find('"') {
                    Some(j) => Some(rest[..j].to_string()),
                    None => None,
                }
            }
            None => None,
        };
    }
    let head = head.to_string();
    // 兼容坏写入的 \\ 路径（如 "D:\\Weixin\Weixin.exe"），UNC 开头不动
    if !head.starts_with('\\') {
        return Some(head.replace("\\\\", "\\"));
    }
    Some(head)
}

/// 批量提取程序关联图标（复用 icons 模块），失败项保持 None。
fn enrich_icons(items: &mut [StartupItem]) {
    let mut targets: Vec<(usize, String)> = Vec::new();
    for (i, item) in items.iter().enumerate() {
        let path = match &item.file_path {
            Some(f) if !f.trim().is_empty() => f.clone(),
            _ => match resolve_command_path(&item.command) {
                Some(p) => p,
                None => continue,
            },
        };
        targets.push((i, path));
    }
    if targets.is_empty() {
        return;
    }
    let paths = targets.iter().map(|(_, p)| p.clone()).collect::<Vec<_>>();
    let Ok(map) = crate::icons::extract_icons(paths) else {
        return;
    };
    for (i, path) in targets {
        if let Some(icon) = map.get(&path) {
            items[i].icon = Some(icon.clone());
        }
    }
}

pub fn disable_startup_item(key: &str) -> Result<(), String> {
    let mut backups = read_backups();
    if backups.iter().any(|b| b.key == key) {
        return Ok(());
    }
    if let Some((hive, reg_path, name)) = parse_reg_key(key) {
        let reg = RegKey::predef(hive)
            .open_subkey(&reg_path)
            .map_err(|e| format!("打开注册表项失败: {e}"))?;
        let value = reg
            .get_raw_value(&name)
            .map_err(|e| format!("读取注册表值失败: {e}"))?;
        let text = decode_reg_string(&value.bytes)
            .or_else(|| {
                Some(
                    String::from_utf8_lossy(&value.bytes)
                        .trim_end_matches('\0')
                        .to_string(),
                )
            })
            .unwrap_or_default();
        reg.delete_value(&name)
            .map_err(|e| format!("删除注册表值失败: {e}"))?;
        backups.push(StartupBackup {
            key: key.to_string(),
            name,
            command: text,
            location: reg_path,
            file_path: None,
            is_folder: false,
        });
        write_backups(&backups)?;
        return Ok(());
    }
    if let Some(path) = key.strip_prefix("folder:") {
        let path = PathBuf::from(path);
        if !path.exists() {
            return Err("启动文件夹内未找到该文件".into());
        }
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let parent = path.parent().ok_or_else(|| "路径无效".to_string())?;
        let dest = parent.join(format!("__da_disabled_{name}"));
        if dest.exists() {
            return Err("目标位置已存在同名文件".into());
        }
        fs::rename(&path, &dest).map_err(|e| format!("移动启动项失败: {e}"))?;
        backups.push(StartupBackup {
            key: key.to_string(),
            name,
            command: String::new(),
            location: String::new(),
            file_path: Some(dest.to_string_lossy().into_owned()),
            is_folder: true,
        });
        write_backups(&backups)?;
        return Ok(());
    }
    Err("无效的启动项标识".into())
}

pub fn enable_startup_item(key: &str) -> Result<(), String> {
    let mut backups = read_backups();
    let Some(pos) = backups.iter().position(|b| b.key == key) else {
        return Err("未找到该启动项的禁用备份".into());
    };
    let backup = backups.remove(pos);
    if backup.is_folder {
        let dest_str = backup
            .file_path
            .as_deref()
            .ok_or_else(|| "路径缺失".to_string())?;
        let dest = PathBuf::from(dest_str);
        let parent = dest.parent().ok_or_else(|| "路径无效".to_string())?;
        let original = parent.join(backup.name.trim_start_matches("__da_disabled_"));
        if original.exists() {
            backups.push(backup);
            write_backups(&backups)?;
            return Err("原位置已存在同名文件，恢复中止".into());
        }
        fs::rename(&dest, &original).map_err(|e| format!("恢复启动项失败: {e}"))?;
    } else {
        let (hive, reg_path, _name) =
            parse_reg_key(key).ok_or_else(|| "备份标识无效".to_string())?;
        let reg = RegKey::predef(hive)
            .open_subkey(&reg_path)
            .map_err(|e| format!("打开注册表项失败: {e}"))?;
        let mut bytes = backup.command.clone().into_bytes();
        bytes.push(0);
        reg.set_raw_value(
            &backup.name,
            &RegValue {
                vtype: RegType::REG_SZ,
                bytes,
            },
        )
        .map_err(|e| format!("写回注册表失败: {e}"))?;
    }
    write_backups(&backups)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reg_key_roundtrip() {
        let (hive, path, name) =
            parse_reg_key(r"reg:HKCU:Software\x:y:MyApp").expect("parse");
        assert_eq!(hive, HKEY_CURRENT_USER);
        assert_eq!(path, r"Software\x:y");
        assert_eq!(name, "MyApp");
        let bad = parse_reg_key("folder:C:\\x");
        assert!(bad.is_none());
    }

    #[test]
    fn folder_key_roundtrip() {
        let k = "folder:C:\\Users\\x\\test.lnk";
        assert!(k.starts_with("folder:"));
    }
}
