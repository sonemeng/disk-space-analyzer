//! 系统服务管理：服务列表（实时 CPU/内存）+ 启动/停止服务。
use crate::win_cmd::hidden;
use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceEntry {
    pub name: String,
    pub display_name: String,
    pub state: String,
    pub start_mode: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceOverview {
    pub cpu_percent: f64,
    pub memory_percent: f64,
    pub memory_used_bytes: u64,
    pub memory_total_bytes: u64,
    pub services: Vec<ServiceEntry>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemLoad {
    pub cpu_percent: f64,
    pub memory_percent: f64,
    pub memory_used_bytes: u64,
    pub memory_total_bytes: u64,
}

fn load_snapshot_ps() -> String {
    r#"
$cpu = (Get-CimInstance Win32_PerfFormattedData_PerfOS_Processor | Where-Object { $_.Name -eq '_Total' }).PercentProcessorTime
$os = Get-CimInstance Win32_OperatingSystem
$total = $os.TotalVisibleMemorySize * 1KB
$free = (Get-Counter '\Memory\Available MBytes' -ErrorAction Stop).CounterSamples[0].CookedValue * 1MB
[PSCustomObject]@{
    cpuPercent = [double]$cpu
    memoryPercent = [double](100 * ($total - $free) / $total)
    memoryUsedBytes = [uint64]($total - $free)
    memoryTotalBytes = [uint64]$total
} | ConvertTo-Json -Compress
"#
    .to_string()
}

/// 轻量实时采样：仅 CPU 占用与物理内存（供前端折线动画轮询使用）。
pub fn sample_system_load() -> Result<SystemLoad, String> {
    let out = hidden(Command::new("powershell"))
        .args(["-NoProfile", "-NonInteractive", "-Command", &load_snapshot_ps()])
        .output()
        .map_err(|e| format!("无法调用 PowerShell: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout);
    serde_json::from_str(&text).map_err(|_| "无法解析系统负载".into())
}

pub fn list_services() -> Result<ServiceOverview, String> {
    let load = sample_system_load()?;
    let ps = r#"
$services = Get-Service | Select-Object Name, DisplayName, Status, StartType
$services | ForEach-Object {
    [PSCustomObject]@{
        name = $_.Name
        displayName = $_.DisplayName
        state = $_.Status.ToString()
        startMode = $_.StartType.ToString()
    }
} | ConvertTo-Json -Compress -Depth 3
"#;
    let out = hidden(Command::new("powershell"))
        .args(["-NoProfile", "-NonInteractive", "-Command", ps])
        .output()
        .map_err(|e| format!("无法调用 PowerShell: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout);
    let services: Vec<ServiceEntry> = serde_json::from_str::<Vec<ServiceEntry>>(&text)
        .map_err(|_| "无法解析服务列表".to_string())?;
    Ok(ServiceOverview {
        cpu_percent: load.cpu_percent,
        memory_percent: load.memory_percent,
        memory_used_bytes: load.memory_used_bytes,
        memory_total_bytes: load.memory_total_bytes,
        services,
    })
}

pub fn set_service(name: &str, action: &str) -> Result<(), String> {
    if name.is_empty() || name.contains([' ', '&', '|', ';', '<', '>', '"', '\'']) {
        return Err("非法的服务名称".into());
    }
    let verb = match action {
        "start" => "start",
        "stop" => "stop",
        _ => return Err("仅支持 start / stop".into()),
    };
    let out = hidden(Command::new("sc.exe"))
        .args([verb, name])
        .output()
        .map_err(|e| format!("无法执行 sc.exe: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout);
    if out.status.success() {
        Ok(())
    } else {
        let detail = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let mut msg = if detail.is_empty() {
            text.trim().to_string()
        } else {
            detail.to_string()
        };
        if msg.is_empty() {
            msg = format!("操作服务 {name} 失败");
        }
        if msg.contains("要求提升") || msg.contains("拒绝访问") || msg.contains("Access is denied") {
            msg = "需要管理员权限：请右键应用图标「以管理员身份运行」后再试".into();
        } else if msg.contains("未找到") || msg.contains("does not exist") {
            msg = format!("服务 {name} 不存在");
        }
Err(msg)
    }
}

