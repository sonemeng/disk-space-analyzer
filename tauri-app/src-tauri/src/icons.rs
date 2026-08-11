//! 应用图标提取：批量 ExtractAssociatedIcon → 32×32 PNG data-url（一次 PowerShell 会话）。
use crate::win_cmd::hidden;
use std::collections::HashMap;
use std::process::Command;

/// 批量提取图标的文件路径的关联图标；路径不存在/提取失败的项目不在结果中。
pub fn extract_icons(paths: Vec<String>) -> Result<HashMap<String, String>, String> {
    if paths.is_empty() {
        return Ok(HashMap::new());
    }
    let list_json =
        serde_json::to_string(&paths).map_err(|_| "无法序列化图标路径".to_string())?;
    let ps = format!(
        r##"
Add-Type -AssemblyName System.Drawing
$paths = @'
{0}
'@ | ConvertFrom-Json
$result = @{{}}
foreach ($p in $paths) {{
    try {{
        $resolved = [Environment]::ExpandEnvironmentVariables($p)
        if (-not (Test-Path -LiteralPath $resolved)) {{ continue }}
        $icon = [System.Drawing.Icon]::ExtractAssociatedIcon($resolved)
        if ($null -eq $icon) {{ continue }}
        $bmp = $icon.ToBitmap()
        $bmp32 = New-Object System.Drawing.Bitmap 32,32
        $g = [System.Drawing.Graphics]::FromImage($bmp32)
        $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
        $g.DrawImage($bmp, 0, 0, 32, 32)
        $g.Dispose(); $bmp.Dispose(); $icon.Dispose()
        $ms = New-Object System.IO.MemoryStream
        $bmp32.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
        $result[$p] = 'data:image/png;base64,' + [Convert]::ToBase64String($ms.ToArray())
        $ms.Dispose(); $bmp32.Dispose()
    }} catch {{}}
}}
$result | ConvertTo-Json -Compress
"##,
        list_json
    );
    let out = hidden(Command::new("powershell"))
        .args(["-NoProfile", "-NonInteractive", "-Command", &ps])
        .output()
        .map_err(|e| format!("无法调用 PowerShell: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout);
    serde_json::from_str(&text).map_err(|_| "无法解析图标提取结果".into())
}