//! 进程管理：进程列表（CPU 采样/内存/路径）。
use crate::win_cmd::hidden;
use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessEntry {
    pub pid: u64,
    pub name: String,
    pub cpu_percent: f64,
    pub memory_bytes: u64,
    pub path: Option<String>,
    pub company: Option<String>,
    pub window_title: Option<String>,
}

pub fn list_processes() -> Result<Vec<ProcessEntry>, String> {
    let ps = r#"
$p1 = Get-Process -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 900
$p2 = Get-Process -ErrorAction SilentlyContinue
$cores = [Environment]::ProcessorCount
$map = @{}
foreach ($proc in $p2) {
    $before = $p1 | Where-Object { $_.Id -eq $proc.Id } | Select-Object -First 1
    $cpu = 0.0
    if ($before -and $proc.CPU -ge $before.CPU) {
        $cpu = [double](($proc.CPU - $before.CPU) * 100.0 / 0.9 / $cores)
        if ($cpu -gt 100) { $cpu = 100 }
    }
    $map[$proc.Id] = [PSCustomObject]@{
        pid = [uint64]$proc.Id
        name = $proc.ProcessName
        cpuPercent = [Math]::Round($cpu, 2)
        memoryBytes = [uint64]$proc.WorkingSet64
        path = $proc.Path
        company = $proc.Company
        windowTitle = $proc.MainWindowTitle
    }
}
@($map.Values) | ConvertTo-Json -Compress -Depth 3
"#;
    let out = hidden(Command::new("powershell"))
        .args(["-NoProfile", "-NonInteractive", "-Command", ps])
        .output()
        .map_err(|e| format!("无法调用 PowerShell: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout);
    serde_json::from_str::<Vec<ProcessEntry>>(&text)
        .map_err(|_| "无法解析进程列表".into())
}