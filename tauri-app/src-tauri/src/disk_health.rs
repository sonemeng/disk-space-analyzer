//! 磁盘健康（WMI）+ 磁盘读写测速。
use crate::win_cmd::hidden;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskHealthItem {
    pub index: u32,
    pub model: String,
    pub serial: String,
    pub interface: String,
    pub media_type: String,
    pub size_bytes: u64,
    pub status: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskHealthReport {
    pub drives: Vec<DiskHealthItem>,
    /// None = 无法读取 SMART（非管理员常见）
    pub smart_ok: Option<bool>,
}

pub fn disk_health() -> Result<DiskHealthReport, String> {
    let ps = "Get-CimInstance Win32_DiskDrive | Select-Object Index,Model,SerialNumber,InterfaceType,MediaType,Size,Status | ConvertTo-Json -Compress";
    let out = hidden(Command::new("powershell"))
        .args(["-NoProfile", "-NonInteractive", "-Command", ps])
        .output()
        .map_err(|e| format!("无法调用 PowerShell: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout);
    let mut drives = Vec::new();
    if let Ok(list) = serde_json::from_str::<Vec<serde_json::Value>>(&text) {
        for item in list {
            drives.push(parse_health(&item));
        }
    } else if let Ok(single) = serde_json::from_str::<serde_json::Value>(&text) {
        if single.is_object() {
            drives.push(parse_health(&single));
        }
    }
    Ok(DiskHealthReport {
        smart_ok: get_smart_health(),
        drives,
    })
}

fn parse_health(v: &serde_json::Value) -> DiskHealthItem {
    let str_of = |k: &str| -> String {
        v.get(k)
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .trim()
            .to_string()
    };
    let num = |k: &str| -> u64 {
        v.get(k)
            .and_then(|x| x.as_str())
            .and_then(|s| s.parse().ok())
            .or_else(|| v.get(k).and_then(|x| x.as_u64()))
            .unwrap_or(0)
    };
    DiskHealthItem {
        index: v.get("Index").and_then(|x| x.as_u64()).unwrap_or(0) as u32,
        model: str_of("Model"),
        serial: str_of("SerialNumber"),
        interface: str_of("Interface"),
        media_type: str_of("MediaType"),
        size_bytes: num("Size"),
        status: str_of("Status"),
    }
}

fn get_smart_health() -> Option<bool> {
    let ps = "Get-PhysicalDisk | Select-Object -ExpandProperty HealthStatus -Unique";
    let out = hidden(Command::new("powershell"))
        .args(["-NoProfile", "-NonInteractive", "-Command", ps])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let clean: String = text
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(",");
    if clean.contains("Healthy") {
        Some(true)
    } else if clean.contains("Warning")
        || clean.to_lowercase().contains("unhealthy")
        || clean.contains("Fail")
    {
        Some(false)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// 磁盘读写测速
// ---------------------------------------------------------------------------

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeedTestResult {
    pub drive: String,
    /// MB/s
    pub seq_write_mbps: f64,
    pub seq_read_mbps: f64,
    /// 4K 随机读 IOPS
    pub rand_read_4k_iops: u64,
    /// 4K 随机读带宽 MB/s
    pub rand_read_4k_mbps: f64,
    pub test_bytes: u64,
}

pub fn run_speed_test(drive: &str) -> Result<SpeedTestResult, String> {
    let drive_letter = drive.trim_end_matches(':').to_uppercase();
    if drive_letter.len() != 1 {
        return Err("无效的盘符".into());
    }
    let path = PathBuf::from(format!("{drive_letter}:\\__da_speed_test.tmp"));
    let _ = fs::remove_file(&path);

    const SEQ_BYTES: u64 = 192 * 1024 * 1024;
    let chunk: usize = 16 * 1024 * 1024;
    let buf = vec![0x5Au8; chunk];

    // 顺序写
    let write_result = (|| -> Result<f64, String> {
        let mut file =
            fs::File::create(&path).map_err(|e| format!("创建测试文件失败: {e}"))?;
        let start = Instant::now();
        let mut written: u64 = 0;
        while written < SEQ_BYTES {
            file.write_all(&buf)
                .map_err(|e| format!("写入失败: {e}"))?;
            written += chunk as u64;
        }
        let _ = file.sync_all();
        let elapsed = start.elapsed().as_secs_f64().max(0.001);
        Ok(SEQ_BYTES as f64 / elapsed / 1024.0 / 1024.0)
    })();

    let seq_write = match write_result {
        Ok(v) => v,
        Err(e) => {
            let _ = fs::remove_file(&path);
            let low = e.to_lowercase();
            if low.contains("denied") || low.contains("只读") || low.contains("read-only") {
                return Err(format!(
                    "{drive_letter}: 不可写（可能是只读介质或需要管理员），测速仅支持可写磁盘"
                ));
            }
            return Err(e);
        }
    };

    // 顺序读
    let seq_read = (|| -> f64 {
        let Ok(mut file) = fs::File::open(&path) else {
            return 0.0;
        };
        let mut read_buf = vec![0u8; chunk];
        let start = Instant::now();
        let mut total: u64 = 0;
        loop {
            let Ok(n) = file.read(&mut read_buf) else { break };
            if n == 0 {
                break;
            }
            total += n as u64;
        }
        let elapsed = start.elapsed().as_secs_f64().max(0.001);
        total as f64 / elapsed / 1024.0 / 1024.0
    })();

    // 4K 随机读（2000 次均匀位置）
    let (rand_iops, rand_mbps) = {
        let Ok(mut file) = fs::File::open(&path) else {
            return Ok(SpeedTestResult {
                drive: drive_letter.clone(),
                seq_write_mbps: seq_write,
                seq_read_mbps: seq_read,
                rand_read_4k_iops: 0,
                rand_read_4k_mbps: 0.0,
                test_bytes: 0,
            });
        };
        let len = file.metadata().map(|m| m.len()).unwrap_or(0).max(1);
        let mut read_buf = vec![0u8; 4096];
        let start = Instant::now();
        let step = (len / 2000).max(4096);
        let mut i: u64 = 0;
        while i < 2000 {
            let mut pos = (i * step) % len;
            pos -= pos % 4096;
            if file.seek(SeekFrom::Start(pos)).is_ok() {
                let _ = file.read(&mut read_buf);
            }
            i += 1;
        }
        let elapsed = start.elapsed().as_secs_f64().max(0.001);
        let iops = (2000.0 / elapsed) as u64;
        (iops, iops as f64 * 4096.0 / 1024.0 / 1024.0)
    };

    let _ = fs::remove_file(&path);
    Ok(SpeedTestResult {
        drive: drive_letter,
        seq_write_mbps: seq_write,
        seq_read_mbps: seq_read,
        rand_read_4k_iops: rand_iops,
        rand_read_4k_mbps: rand_mbps,
        test_bytes: SEQ_BYTES,
    })
}