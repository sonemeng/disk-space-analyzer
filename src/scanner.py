"""扫描引擎 — 多线程磁盘扫描核心"""

import os
import time
import shutil
import threading
from concurrent.futures import ThreadPoolExecutor, as_completed

from src.utils import fmt_size

MAX_WORKERS = 6


class ScanEngine:
    """磁盘扫描引擎（后台线程安全）"""

    def __init__(self, callback=None):
        """
        Args:
            callback: func(status_msg, progress_pct, item_data=None)
        """
        self.callback = callback
        self.cancelled = False
        self.results = {}       # {abs_path: (size_bytes, file_count, dir_count, label)}
        self.large_files = []   # [(abs_path, size_bytes)]
        self.file_stats = []    # [(category_name, size_bytes), ...]
        self.cat_summary = []   # [(category_name, size_bytes), ...]
        self.disk_usage = None  # shutil.disk_usage result
        self.drive = None
        self.errors = []

    # ---- 控制 ----

    def cancel(self):
        self.cancelled = True

    def _report(self, msg, pct=0):
        if self.callback and not self.cancelled:
            self.callback(msg, pct)

    # ---- 单目录扫描 ----

    def scan_dir(self, path, depth=5, timeout=8):
        """扫描单个目录，返回 (size_bytes, file_count, dir_count)"""
        total = 0
        files = 0
        dirs = 0
        start = time.time()
        try:
            base = path.rstrip('\\').count('\\')
            for root, dirs_list, files_list in os.walk(path):
                if self.cancelled or time.time() - start > timeout:
                    break
                cur = root.rstrip('\\').count('\\') - base
                if cur > depth:
                    dirs_list.clear()
                    continue
                try:
                    if os.path.islink(root):
                        dirs_list.clear()
                        continue
                except Exception:
                    dirs_list.clear()
                    continue

                dirs += len(dirs_list)
                files += len(files_list)
                for f in files_list[:3000]:
                    try:
                        fp = os.path.join(root, f)
                        if os.path.islink(fp):
                            continue
                        total += os.path.getsize(fp)
                    except Exception:
                        continue
                if dirs > 80000:
                    dirs_list.clear()
        except Exception:
            pass
        return total, files, dirs

    # ---- 主扫描流程 ----

    def scan(self, drive):
        """对指定盘符执行完整扫描"""
        self.drive = drive
        self.results = {}
        self.large_files = []
        self.cancelled = False

        d = drive.rstrip('\\') + '\\'
        self.disk_usage = shutil.disk_usage(d)
        du = self.disk_usage
        pct = du.used / du.total * 100

        self._report(
            f"📀 {drive}  总 {fmt_size(du.total)}  |  已用 {fmt_size(du.used)} ({pct:.0f}%)", 2)

        # Phase 1: 根目录下第一级
        self._report("📁 根目录扫描...", 5)
        root_dirs = []
        try:
            for item in sorted(os.listdir(drive)):
                dp = os.path.join(drive, item)
                if os.path.isdir(dp):
                    root_dirs.append(dp)
        except PermissionError:
            pass

        for i, dp in enumerate(root_dirs):
            if self.cancelled:
                return
            lbl = os.path.basename(dp)
            self._report(f"📁 {lbl}", 5 + int(i / len(root_dirs) * 20) if root_dirs else 5)
            sz, fc, dc = self.scan_dir(dp, depth=1, timeout=3)
            self.results[dp] = (sz, fc, dc, lbl)

        # Phase 2: 深入大目录
        sorted_res = sorted(
            self.results.items(), key=lambda x: x[1][0] if x[1][0] else 0, reverse=True
        )
        big_ones = [
            p for p, d in sorted_res[:8]
            if d[0] and d[0] > 200 * 1024**3
        ]
        for i, dp in enumerate(big_ones):
            if self.cancelled:
                return
            lbl = os.path.basename(dp)
            self._report(f"🔍 {lbl}", 25 + int(i / len(big_ones) * 15) if big_ones else 25)
            sz, fc, dc = self.scan_dir(dp, depth=3, timeout=6)
            self.results[dp] = (sz, fc, dc, lbl)

        # Phase 3: 用户目录 (仅C盘)
        if drive[0].upper() == os.environ.get('SYSTEMDRIVE', 'C')[0]:
            self._scan_user_profile()

            # Phase 4: 系统目录
            self._scan_system_dirs()

        # Phase 5: 大文件搜索
        self._scan_large_files()

        # Phase 6: 汇总分析
        self._report("📊 分析汇总...", 92)
        self._calc_categories()
        self._calc_file_types()

        self._report("✅ 扫描完成", 100)

    def _scan_user_profile(self):
        """扫描用户主目录"""
        home = os.path.expanduser("~")
        self._report("👤 用户目录...", 42)

        skip = {
            'Application Data', 'Local Settings', 'My Documents', 'Cookies',
            'Recent', 'NetHood', 'PrintHood', 'SendTo', 'Templates', '开始菜单',
            'Searches', 'Contacts', 'Favorites', 'Links', 'Saved Games',
        }
        user_dirs = []
        try:
            for d in sorted(os.listdir(home)):
                if d in skip:
                    continue
                dp = os.path.join(home, d)
                if os.path.isdir(dp):
                    user_dirs.append((dp, d))
        except PermissionError:
            pass

        for i, (dp, lbl) in enumerate(user_dirs):
            if self.cancelled:
                return
            self._report(f"👤 {lbl}", 42 + int(i / len(user_dirs) * 14) if user_dirs else 42)
            sz, fc, dc = self.scan_dir(dp, depth=2, timeout=4)
            if sz > 10 * 1024**2:
                self.results[dp] = (sz, fc, dc, lbl)

        # AppData 子目录深入
        for sub in [
            'AppData\\Local', 'AppData\\Roaming', 'CrossDevice',
            '.lmstudio', '.android', '.gradle', '.cache',
        ]:
            sp = os.path.join(home, sub)
            if os.path.exists(sp) and sp not in self.results:
                lbl = sub.replace('\\', '/')
                self._report(f"📦 {lbl}", 58)
                sz, fc, dc = self.scan_dir(sp, depth=3, timeout=7)
                if sz > 50 * 1024**2:
                    self.results[sp] = (sz, fc, dc, lbl)

    def _scan_system_dirs(self):
        """扫描系统关键目录"""
        self._report("🪟 系统目录...", 68)
        sys_paths = [
            os.environ.get('WINDIR', 'C:\\Windows') + '\\WinSxS',
            os.environ.get('WINDIR', 'C:\\Windows') + '\\Installer',
            os.environ.get('WINDIR', 'C:\\Windows') + '\\SoftwareDistribution',
            os.environ.get('WINDIR', 'C:\\Windows') + '\\Temp',
            'C:\\ProgramData',
        ]
        for i, dp in enumerate(sys_paths):
            if self.cancelled:
                return
            if os.path.exists(dp):
                lbl = os.path.basename(dp)
                self._report(f"🪟 {lbl}", 70 + int(i / len(sys_paths) * 10))
                sz, fc, dc = self.scan_dir(dp, depth=2, timeout=6)
                self.results[dp] = (sz, fc, dc, lbl)

    def _scan_large_files(self):
        """扫描大文件"""
        self._report("🐋 大文件搜索...", 82)
        home = os.path.expanduser("~")
        areas = [
            os.path.join(home, 'Desktop'),
            os.path.join(home, 'Downloads'),
            os.path.join(home, 'Documents'),
        ]

        all_lf = []
        for area in areas:
            if not os.path.exists(area) or self.cancelled:
                continue
            try:
                for root, dirs, files in os.walk(area):
                    if self.cancelled:
                        break
                    for f in files[:2000]:
                        try:
                            fp = os.path.join(root, f)
                            sz = os.path.getsize(fp)
                            if sz > 100 * 1024**2:
                                all_lf.append((fp, sz))
                        except Exception:
                            continue
                    if len(all_lf) > 50:
                        break
            except Exception:
                pass

        all_lf.sort(key=lambda x: x[1], reverse=True)
        self.large_files = all_lf[:25]

    # ---- 汇总 ----

    def _calc_categories(self):
        """按类别汇总空间"""
        cats = {}
        for path, (size, _, _, _) in self.results.items():
            if not size:
                continue
            p = path.lower()
            if 'windows' in p and ('winsxs' in p or 'installer' in p):
                cat = '🪟 系统'
            elif 'appdata' in p or 'roaming' in p:
                cat = '📦 应用缓存'
            elif 'temp' in p or 'cache' in p:
                cat = '🗑️ 临时文件'
            elif 'crossdevice' in p:
                cat = '📱 手机备份'
            elif any(x in p for x in ('desktop', 'documents', 'downloads', 'pictures', 'videos')):
                cat = '📂 用户文件'
            elif 'programdata' in p or 'program files' in p:
                cat = '💻 程序'
            elif any(x in p for x in ('.lmstudio', '.android', '.gradle', '.rustup', '.cargo')):
                cat = '🔧 开发工具'
            else:
                cat = '📁 其他'
            cats[cat] = cats.get(cat, 0) + size
        self.cat_summary = sorted(cats.items(), key=lambda x: x[1], reverse=True)

    def _calc_file_types(self):
        """文件类型分布分析（抽样）"""
        ext_map = {}
        for path, (size, files, _, _) in self.results.items():
            if not size or not files:
                continue
            try:
                cnt = 0
                for root, dirs2, files2 in os.walk(path):
                    if cnt > 3000:
                        break
                    for f in files2[:30]:
                        cnt += 1
                        ext = os.path.splitext(f)[1].lower() or '(无)'
                        try:
                            fp = os.path.join(root, f)
                            ext_map[ext] = ext_map.get(ext, 0) + os.path.getsize(fp)
                        except Exception:
                            pass
            except Exception:
                pass

        sorted_ext = sorted(ext_map.items(), key=lambda x: x[1], reverse=True)
        cat_groups = {}
        cat_map = {
            '.jpg': '图片', '.jpeg': '图片', '.png': '图片', '.gif': '图片',
            '.webp': '图片', '.bmp': '图片', '.svg': '图片',
            '.mp4': '视频', '.avi': '视频', '.mkv': '视频',
            '.mp3': '音频', '.wav': '音频', '.flac': '音频', '.aac': '音频',
            '.doc': '文档', '.docx': '文档', '.pdf': '文档', '.txt': '文档',
            '.xls': '文档', '.xlsx': '文档', '.ppt': '文档', '.pptx': '文档',
            '.md': '文档', '.csv': '文档',
            '.zip': '压缩包', '.rar': '压缩包', '.7z': '压缩包',
            '.tar': '压缩包', '.gz': '压缩包',
            '.exe': '程序', '.msi': '程序', '.dll': '程序',
            '.rlib': '开发', '.lib': '开发', '.pdb': '开发',
            '.js': '开发', '.ts': '开发', '.py': '开发', '.java': '开发',
            '.rs': '开发', '.go': '开发',
            '.json': '开发', '.xml': '开发', '.yaml': '开发', '.toml': '开发',
        }
        for ext, sz in sorted_ext[:30]:
            cat_groups[cat_map.get(ext, '其他')] = cat_groups.get(cat_map.get(ext, '其他'), 0) + sz

        self.file_stats = sorted(cat_groups.items(), key=lambda x: x[1], reverse=True)
