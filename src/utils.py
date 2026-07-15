"""工具函数 — 格式化、路径处理、系统调用"""

import os
import subprocess

def fmt_size(b):
    """字节数转为人类可读格式"""
    for u in ['B', 'KB', 'MB', 'GB', 'TB']:
        if abs(b) < 1024:
            return f"{b:.1f} {u}"
        b /= 1024
    return f"{b:.1f} PB"

def fmt_gb(b):
    """字节数转 GB (float)"""
    return b / (1024**3) if b else 0

def open_in_explorer(path):
    """在 Windows 资源管理器中打开指定路径"""
    try:
        abs_path = os.path.abspath(path)
        if os.path.isdir(abs_path):
            subprocess.Popen(['explorer', abs_path])
        else:
            subprocess.Popen(['explorer', '/select,', abs_path])
    except Exception:
        pass

def path_to_file_url(path):
    """Windows 绝对路径 → file:/// 协议 URL"""
    return 'file:///' + os.path.abspath(path).replace('\\', '/')

def esc_html(s):
    """转义 HTML 特殊字符"""
    return str(s).replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;").replace('"', "&quot;")
