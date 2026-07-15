"""HTML 报告生成器"""

import os
from datetime import datetime

from src.utils import fmt_size, path_to_file_url, esc_html


def generate(scanner, output_path):
    """生成 HTML 报告"""
    du = scanner.disk_usage
    if not du:
        raise ValueError("请先完成扫描")

    total_g = du.total / (1024**3)
    used_g = du.used / (1024**3)
    free_g = du.free / (1024**3)
    pct = du.used / du.total * 100

    items = [(p, d) for p, d in scanner.results.items() if d[0] is not None]
    items.sort(key=lambda x: x[1][0], reverse=True)
    max_size = items[0][1][0] if items else 1

    # ---- 目录排行 ----
    dir_rows = ""
    for i, (p, (sz, fc, dc, lbl)) in enumerate(items[:50], 1):
        dp = sz / du.used * 100 if du.used else 0
        bar_pct = sz / max_size * 100
        display = (lbl if lbl else os.path.basename(p))[:45]
        url = path_to_file_url(p)
        color = '#ef4444' if dp > 10 else '#eab308' if dp > 5 else '#3b82f6'
        cls = ''
        if dp > 10: cls = ' class="d"'
        elif dp > 5: cls = ' class="w"'
        dir_rows += f'''<tr{cls}>
<td style="text-align:center;color:#64748b;">{i}</td>
<td><a href="{url}" style="color:#93c5fd;text-decoration:none;" title="{esc_html(p)}">📁 {esc_html(display)}</a></td>
<td style="font-weight:600;">{fmt_size(sz)}</td>
<td style="color:#94a3b8;">{dp:.1f}%</td>
<td><div style="height:16px;background:#0f172a;border-radius:4px;"><div style="height:100%;width:{bar_pct:.0f}%;background:{color};border-radius:4px;"></div></div></td>
</tr>'''

    # ---- 大文件 ----
    file_rows = ""
    for i, (fp, sz) in enumerate(scanner.large_files[:20], 1):
        url = path_to_file_url(os.path.dirname(fp))
        name = os.path.basename(fp)[:50]
        file_rows += f'''<tr>
<td style="text-align:center;color:#64748b;">{i}</td>
<td><a href="{url}" style="color:#93c5fd;text-decoration:none;" title="{esc_html(fp)}">📄 {esc_html(name)}</a></td>
<td style="font-weight:600;">{fmt_size(sz)}</td>
</tr>'''

    # ---- 类别 ----
    cat_color = {
        '🪟 系统': '#ef4444', '📦 应用缓存': '#eab308', '🗑️ 临时文件': '#f97316',
        '📱 手机备份': '#8b5cf6', '📂 用户文件': '#3b82f6', '💻 程序': '#22c55e',
        '🔧 开发工具': '#06b6d4', '📁 其他': '#64748b',
    }
    cat_rows = ""
    if scanner.cat_summary:
        max_cat = scanner.cat_summary[0][1]
        for cat, sz in scanner.cat_summary:
            c = cat_color.get(cat, '#64748b')
            bar_cat = sz / max_cat * 100
            cat_rows += f'''<tr>
<td>{cat}</td><td style="font-weight:600;">{fmt_size(sz)}</td>
<td><div style="height:16px;background:#0f172a;border-radius:4px;"><div style="height:100%;width:{bar_cat:.0f}%;background:{c};border-radius:4px;"></div></div></td>
</tr>'''

    # ---- 文件类型 ----
    ft_rows = ""
    ft_colors = {
        '图片': '#22c55e', '视频': '#8b5cf6', '音频': '#f97316', '文档': '#3b82f6',
        '压缩包': '#eab308', '程序': '#ef4444', '开发': '#06b6d4', '其他': '#64748b',
    }
    if scanner.file_stats:
        max_ft = scanner.file_stats[0][1]
        for cat, sz in scanner.file_stats:
            c = ft_colors.get(cat, '#64748b')
            bar_ft = sz / max_ft * 100
            ft_rows += f'''<tr>
<td>{cat}</td><td style="font-weight:600;">{fmt_size(sz)}</td>
<td><div style="height:16px;background:#0f172a;border-radius:4px;"><div style="height:100%;width:{bar_ft:.0f}%;background:{c};border-radius:4px;"></div></div></td>
</tr>'''

    total_files = sum(d[1] for d in scanner.results.values() if d[1] is not None)

    html = f'''<!DOCTYPE html>
<html lang="zh-CN">
<head><meta charset="UTF-8"><meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>磁盘分析报告 - {scanner.drive}</title>
<style>
body{{background:#0b1120;color:#e2e8f0;font-family:-apple-system,"Noto Sans SC","Microsoft YaHei",sans-serif;padding:20px;}}
h1{{font-size:22px;margin-bottom:4px;}}
.sub{{color:#64748b;font-size:13px;margin-bottom:24px;}}
.card{{background:#1e293b;border-radius:12px;padding:20px;margin-bottom:16px;}}
.card h2{{font-size:15px;margin-bottom:12px;}}
table{{width:100%;border-collapse:collapse;font-size:13px;}}
th{{text-align:left;padding:8px;color:#64748b;font-weight:500;font-size:11px;border-bottom:1px solid #334155;}}
td{{padding:8px;border-bottom:1px solid #1a2332;}}
tr:hover td{{background:#1a2332;}}
tr.d td{{color:#fca5a5;}} tr.w td{{color:#fde68a;}}
a:hover{{text-decoration:underline!important;}}
.metrics{{display:flex;gap:16px;flex-wrap:wrap;}}
.metric{{background:#0f172a;border-radius:8px;padding:14px 20px;text-align:center;flex:1;}}
.metric .v{{font-size:24px;font-weight:700;}}
.metric .l{{font-size:11px;color:#64748b;margin-top:2px;}}
.footer{{text-align:center;color:#334155;font-size:11px;margin-top:30px;}}
</style></head>
<body>
<h1>💾 {scanner.drive} 磁盘分析报告</h1>
<div class="sub">{datetime.now().strftime('%Y-%m-%d %H:%M:%S')} · {total_files:,} 个文件</div>
<div class="card">
<div class="metrics">
<div class="metric"><div class="v">{total_g:.0f} GB</div><div class="l">总容量</div></div>
<div class="metric"><div class="v">{used_g:.0f} GB</div><div class="l">已使用 ({pct:.0f}%)</div></div>
<div class="metric"><div class="v">{free_g:.0f} GB</div><div class="l">剩余</div></div>
</div></div>
<div class="card">
<h2>📁 目录排行 TOP 50 <span style="font-size:11px;color:#64748b;">(点击跳转)</span></h2>
<table><thead><tr><th>#</th><th>目录</th><th>大小</th><th>占比</th><th>条</th></tr></thead><tbody>{dir_rows}</tbody></table>
</div>
<div class="card"><h2>📊 类别汇总</h2>
<table><thead><tr><th>类别</th><th>大小</th><th>分布</th></tr></thead><tbody>{cat_rows}</tbody></table></div>
<div class="card"><h2>🧩 文件类型</h2>
<table><thead><tr><th>类别</th><th>大小</th><th>分布</th></tr></thead><tbody>{ft_rows}</tbody></table></div>
<div class="card"><h2>🐋 大文件 TOP 20</h2>
<table><thead><tr><th>#</th><th>文件</th><th>大小</th></tr></thead><tbody>{file_rows}</tbody></table></div>
<div class="footer">磁盘空间分析器 v3.0 · 只读分析</div>
</body></html>'''

    with open(output_path, "w", encoding="utf-8") as f:
        f.write(html)

    return output_path
