//! 端口占用查看：netstat -ano 解析 + tasklist 进程名映射 + taskkill 终止。
use crate::win_cmd::hidden;
use serde::Serialize;
use std::collections::HashMap;
use std::process::Command;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortEntry {
    pub protocol: String,
    pub local_address: String,
    pub local_port: String,
    pub remote_address: String,
    pub remote_port: String,
    pub state: String,
    pub pid: u64,
    pub process: String,
    /// 进程可执行文件路径（用于前端提取真实图标），未知为 None
    pub path: Option<String>,
}

fn split_host_port(s: &str) -> (String, String) {
    if let Some(rest) = s.strip_prefix('[') {
        // [::1]:8080
        if let Some((addr, port)) = rest.split_once("]:") {
            return (addr.to_string(), port.to_string());
        }
        (s.to_string(), String::new())
    } else if let Some((addr, port)) = s.rsplit_once(':') {
        (addr.to_string(), port.to_string())
    } else {
        (s.to_string(), String::new())
    }
}

pub fn list_ports() -> Result<Vec<PortEntry>, String> {
    let out = hidden(Command::new("netstat"))
        .arg("-ano")
        .output()
        .map_err(|e| format!("无法执行 netstat: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout);
    let mut entries: Vec<PortEntry> = Vec::new();
    for line in text.lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 2 {
            continue;
        }
        let protocol = cols[0].to_uppercase();
        if protocol != "TCP" && protocol != "UDP" {
            continue;
        }
        let local = cols[1];
        let remote = cols.get(2).copied().unwrap_or("*");
        let mut state = String::new();
        let pid = if protocol == "TCP" {
            state = cols.get(3).copied().unwrap_or("").to_string();
            cols.last().and_then(|p| p.parse().ok()).unwrap_or(0)
        } else {
            cols.last().and_then(|p| p.parse().ok()).unwrap_or(0)
        };
        let (local_address, local_port) = split_host_port(local);
        let (remote_address, remote_port) = split_host_port(remote);
        entries.push(PortEntry {
            protocol: protocol.clone(),
            local_address,
            local_port,
            remote_address,
            remote_port,
            state,
            pid,
            process: String::new(),
            path: None,
        });
    }
    let names = process_name_map();
    let mut seen_pids = Vec::new();
    for entry in &mut entries {
        if let Some(name) = names.get(&entry.pid) {
            entry.process = name.clone();
        }
        if entry.pid > 0 && entry.path.is_none() && !seen_pids.contains(&entry.pid) {
            seen_pids.push(entry.pid);
        }
    }
    let pid_paths = pid_path_map(&seen_pids);
    for entry in &mut entries {
        if let Some(path) = pid_paths.get(&entry.pid) {
            entry.path = Some(path.clone());
        }
    }
    // 常用排序：监听优先，其次已建立，最后其他
    fn rank(state: &str) -> u8 {
        match state {
            "LISTENING" => 0,
            "ESTABLISHED" => 1,
            _ => 2,
        }
    }
    entries.sort_by(|a, b| {
        rank(&a.state)
            .cmp(&rank(&b.state))
            .then_with(|| a.local_port.parse::<u32>().unwrap_or(0).cmp(&b.local_port.parse::<u32>().unwrap_or(0)))
    });
    Ok(entries)
}

fn process_name_map() -> HashMap<u64, String> {
    let mut map = HashMap::new();
    let out = match hidden(Command::new("tasklist")).args(["/FO", "CSV", "/NH"]).output() {
        Ok(o) => o,
        Err(_) => return map,
    };
    let text = String::from_utf8_lossy(&out.stdout);
    for line in text.lines() {
        let csv = line.trim();
        let cols: Vec<&str> = csv
            .split("\",\"")
            .map(|c| c.trim_matches('"'))
            .collect();
        if cols.len() >= 5 {
            if let Ok(pid) = cols[1].parse::<u64>() {
                let name = cols[0].split('.').next().unwrap_or("").to_string();
                if !name.is_empty() {
                    map.entry(pid).or_insert(name);
                }
            }
        }
    }
    map
}

fn pid_path_map(pids: &[u64]) -> HashMap<u64, String> {
    let mut map = HashMap::new();
    if pids.is_empty() {
        return map;
    }
    // Get-Process -Id 一次取全部目标进程的可执行路径（-ErrorAction 静默缺进程）
    let list = pids
        .iter()
        .map(|p| p.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let ps = format!(
        "Get-Process -Id {list} -ErrorAction SilentlyContinue | ForEach-Object {{ \"$($_.Id)|$($_.Path)\" }}"
    );
    let out = match hidden(Command::new("powershell"))
        .args(["-NoProfile", "-NonInteractive", "-Command", &ps])
        .output()
    {
        Ok(o) => o,
        Err(_) => return map,
    };
    let text = String::from_utf8_lossy(&out.stdout);
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((pid, path)) = line.split_once('|') else {
            continue;
        };
        let path = path.trim();
        if path.is_empty() {
            continue;
        }
        if let Ok(pid) = pid.trim().parse::<u64>() {
            map.entry(pid).or_insert(path.to_string());
        }
    }
    map
}

pub fn kill_process(pid: u64, force: bool) -> Result<(), String> {
    if pid == 0 {
        return Err("PID 0 为系统内核占位进程，不能终止".into());
    }
    let mut cmd = Command::new("taskkill");
    cmd.arg("/PID").arg(pid.to_string());
    if force {
        cmd.arg("/F");
    }
    let out = hidden(cmd).output().map_err(|e| format!("无法执行 taskkill: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        let detail = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(if detail.is_empty() {
            format!("终止进程失败 (PID {pid})，可能权限不足")
        } else {
            format!("终止失败 (PID {pid})：{detail}")
        })
    }
}