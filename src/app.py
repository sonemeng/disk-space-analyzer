"""tkinter 原生 GUI 主窗口"""

import os
import sys
import shutil
import threading
import tkinter as tk
from tkinter import ttk, messagebox
from datetime import datetime

from src import __version__, __app_name__
from src.utils import fmt_size, fmt_gb, open_in_explorer
from src.scanner import ScanEngine
from src import html_report

# ============================================================
# 主题颜色
# ============================================================
BG = "#0b1120"
CARD = "#1e293b"
FG = "#e2e8f0"
FG2 = "#94a3b8"
ACCENT = "#3b82f6"
DANGER = "#ef4444"
WARN = "#eab308"
SUCCESS = "#22c55e"
DARK = "#0f172a"


class App:
    """主应用类"""

    def __init__(self):
        self.root = tk.Tk()
        self.root.title(f"{__app_name__} v{__version__}")
        self.root.geometry("1100x720")
        self.root.minsize(900, 600)
        self.root.configure(bg=BG)

        # 尝试设置图标
        self._try_set_icon()

        # 状态
        self.engine = ScanEngine(callback=self._on_progress)
        self.scan_thread = None
        self.current_drive = tk.StringVar(value="C:")

        self._build_ui()
        self._refresh_drive_info()

    def _try_set_icon(self):
        """尝试加载应用图标"""
        # 找图标: assets/icon.ico 或打包时的内置图标
        try:
            base = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
            icon_path = os.path.join(base, "assets", "icon.ico")
            if os.path.exists(icon_path):
                self.root.iconbitmap(bitmap=icon_path)
        except Exception:
            pass

    # ==================== UI 构建 ====================

    def _build_ui(self):
        # ---- 顶栏 ----
        header = tk.Frame(self.root, bg=CARD, height=56)
        header.pack(fill="x")
        header.pack_propagate(False)

        tk.Label(header, text=f"💾 {__app_name__}",
                 font=("微软雅黑", 16, "bold"),
                 bg=CARD, fg=FG).pack(side="left", padx=20, pady=10)

        # ---- 控制栏 ----
        ctrl = tk.Frame(self.root, bg=BG, height=44)
        ctrl.pack(fill="x", padx=16, pady=(10, 2))

        tk.Label(ctrl, text="选择磁盘:", bg=BG, fg=FG2).pack(side="left")

        drives = self._detect_drives()
        self.drive_combo = ttk.Combobox(ctrl, textvariable=self.current_drive,
                                        values=drives, state="readonly", width=10)
        self.drive_combo.pack(side="left", padx=8)
        self.drive_combo.bind("<<ComboboxSelected>>",
                              lambda e: self._refresh_drive_info())

        self.scan_btn = tk.Button(ctrl, text="▶  开始扫描",
                                  font=("微软雅黑", 10, "bold"),
                                  bg=ACCENT, fg="white", padx=20, pady=3,
                                  relief="flat", cursor="hand2",
                                  activebackground="#2563eb",
                                  command=self._toggle_scan)
        self.scan_btn.pack(side="left", padx=10)

        self.export_btn = tk.Button(ctrl, text="📄 导出 HTML",
                                    font=("微软雅黑", 10),
                                    bg="#334155", fg=FG, padx=16, pady=3,
                                    relief="flat", cursor="hand2", state="disabled",
                                    command=self._export_html)
        self.export_btn.pack(side="right", padx=4)

        tk.Button(ctrl, text="📂 打开桌面", font=("微软雅黑", 10),
                  bg="#334155", fg=FG, padx=12, pady=3,
                  relief="flat", cursor="hand2",
                  command=lambda: open_in_explorer(
                      os.path.join(os.path.expanduser("~"), "Desktop"))
                  ).pack(side="right", padx=4)

        # ---- 磁盘信息条 ----
        self.info_frame = tk.Frame(self.root, bg=CARD, height=68)
        self.info_frame.pack(fill="x", padx=16, pady=2)
        self.info_frame.pack_propagate(False)

        self.info_label = tk.Label(self.info_frame,
                                   text="选择磁盘后点击「开始扫描」",
                                   font=("微软雅黑", 11), bg=CARD, fg=FG2)
        self.info_label.pack(expand=True)

        # ---- 进度 ----
        prog_frame = tk.Frame(self.root, bg=BG, height=30)
        prog_frame.pack(fill="x", padx=16, pady=(4, 0))
        prog_frame.pack_propagate(False)

        self.progress = ttk.Progressbar(prog_frame, mode="determinate")
        self.progress.pack(side="left", fill="x", expand=True)

        self.prog_label = tk.Label(prog_frame, text="", font=("微软雅黑", 9),
                                   bg=BG, fg=FG2, width=28, anchor="w")
        self.prog_label.pack(side="left", padx=8)

        # 样式
        style = ttk.Style()
        style.theme_use("default")
        style.configure("Treeview", background=CARD, foreground=FG,
                        fieldbackground=CARD, rowheight=28,
                        font=("微软雅黑", 10))
        style.configure("Treeview.Heading", background="#334155", foreground=FG,
                        font=("微软雅黑", 10, "bold"), relief="flat")
        style.map("Treeview", background=[("selected", ACCENT)])
        style.configure("TProgressbar", thickness=18, troughcolor=DARK,
                        background=ACCENT, bordercolor=BG)
        style.configure("TCombobox", fieldbackground=CARD, foreground=FG,
                        background=CARD, arrowcolor=FG)

        # ---- 主面板 ----
        main = tk.Frame(self.root, bg=BG)
        main.pack(fill="both", expand=True, padx=16, pady=(4, 12))

        # 左侧: 目录树
        left = tk.Frame(main, bg=BG)
        left.pack(side="left", fill="both", expand=True)

        tk.Label(left, text="📁 目录占用排行 (双击跳转)",
                 font=("微软雅黑", 11, "bold"),
                 bg=BG, fg=FG).pack(anchor="w")

        tree_frame = tk.Frame(left, bg=CARD)
        tree_frame.pack(fill="both", expand=True, pady=(4, 0))

        cols = ("rank", "name", "size", "percent")
        self.tree = ttk.Treeview(tree_frame, columns=cols, show="headings",
                                  height=18, selectmode="browse")
        self.tree.heading("rank", text="#")
        self.tree.heading("name", text="目录")
        self.tree.heading("size", text="大小")
        self.tree.heading("percent", text="占比")
        self.tree.column("rank", width=38, anchor="center")
        self.tree.column("name", width=380, anchor="w")
        self.tree.column("size", width=100, anchor="e")
        self.tree.column("percent", width=68, anchor="e")

        vsb = ttk.Scrollbar(tree_frame, orient="vertical", command=self.tree.yview)
        self.tree.configure(yscrollcommand=vsb.set)
        self.tree.pack(side="left", fill="both", expand=True)
        vsb.pack(side="right", fill="y")
        self.tree.bind("<Double-1>", self._on_tree_double)

        # 右侧: 统计面板
        right = tk.Frame(main, bg=BG, width=270)
        right.pack(side="right", fill="y", padx=(12, 0))
        right.pack_propagate(False)

        tk.Label(right, text="📊 统计概览",
                 font=("微软雅黑", 11, "bold"),
                 bg=BG, fg=FG).pack(anchor="w")

        self.stat_frame = tk.Frame(right, bg=CARD)
        self.stat_frame.pack(fill="both", expand=True, pady=(4, 0))

        self.stat_container = tk.Frame(self.stat_frame, bg=CARD)
        self.stat_container.pack(fill="both", expand=True, padx=8, pady=8)

        self.stat_empty = tk.Label(self.stat_container,
                                    text="扫描完成后\n这里会显示详细统计",
                                    font=("微软雅黑", 10), bg=CARD, fg=FG2)
        self.stat_empty.pack(expand=True)

    @staticmethod
    def _detect_drives():
        import string
        drives = []
        for l in string.ascii_uppercase:
            d = f"{l}:"
            if os.path.exists(d + "\\"):
                drives.append(d)
        return drives

    def _refresh_drive_info(self):
        d = self.current_drive.get()
        if not d or not os.path.exists(d + "\\"):
            return
        try:
            du = shutil.disk_usage(d + "\\")
            total = du.total / (1024**3)
            used = du.used / (1024**3)
            free = du.free / (1024**3)
            pct = du.used / du.total * 100
            status = "充足 ✅" if pct < 70 else "偏紧 ⚠️" if pct < 85 else "不足 ❌"
            color = FG if pct < 85 else DANGER
            self.info_label.config(
                text=f"  {d}    总容量: {total:.1f} GB  |  已用: {used:.1f} GB ({pct:.0f}%)  |  剩余: {free:.1f} GB  |  状态: {status}",
                fg=color)
        except Exception:
            self.info_label.config(text=f"  {d}  无法读取")

    # ==================== 扫描控制 ====================

    def _toggle_scan(self):
        if self.scan_thread and self.scan_thread.is_alive():
            self.engine.cancel()
            self.scan_btn.config(text="⏹ 取消中...", state="disabled")
            self.root.after(2000, self._reset_btn)
            return

        drive = self.current_drive.get()
        if not drive:
            messagebox.showwarning("提示", "请先选择一个磁盘")
            return

        # 清空旧数据
        self._clear_results()
        self.progress["value"] = 0
        self.prog_label.config(text="正在扫描...")
        self.scan_btn.config(text="⏹ 取消", bg=DANGER)
        self.export_btn.config(state="disabled")

        self.engine = ScanEngine(callback=self._on_progress)
        self.scan_thread = threading.Thread(
            target=self._do_scan, args=(drive + "\\",), daemon=True)
        self.scan_thread.start()

    def _do_scan(self, drive):
        try:
            self.engine.scan(drive)
        except Exception as e:
            self.root.after(0, lambda: self._show_error(str(e)))

    def _show_error(self, msg):
        self.prog_label.config(text=f"❌ {msg}", fg=DANGER)
        self._reset_btn()

    def _reset_btn(self):
        self.scan_btn.config(text="▶  开始扫描", state="normal", bg=ACCENT)

    def _on_progress(self, msg, pct):
        def update():
            self.progress["value"] = pct
            self.prog_label.config(text=msg[:50])
            if pct >= 100:
                self._on_complete()
        self.root.after(0, update)

    def _on_complete(self):
        self._reset_btn()
        self.export_btn.config(state="normal")
        self._populate_tree()
        self._populate_stats()

    def _clear_results(self):
        for i in self.tree.get_children():
            self.tree.delete(i)
        for w in self.stat_container.winfo_children():
            w.destroy()
        self.stat_empty = tk.Label(self.stat_container, text="扫描中...",
                                    font=("微软雅黑", 10), bg=CARD, fg=FG2)
        self.stat_empty.pack(expand=True)

    # ==================== 结果填充 ====================

    def _populate_tree(self):
        for i in self.tree.get_children():
            self.tree.delete(i)

        items = [(p, d) for p, d in self.engine.results.items() if d[0] is not None]
        items.sort(key=lambda x: x[1][0], reverse=True)
        disk_used = self.engine.disk_usage.used if self.engine.disk_usage else 1

        for rank, (path, (size, _, _, label)) in enumerate(items[:50], 1):
            pct_disk = size / disk_used * 100 if disk_used else 0
            display = (label if label else os.path.basename(path))[:45]
            tag = ""
            if pct_disk > 10:
                tag = "danger"
            elif pct_disk > 5:
                tag = "warn"
            self.tree.insert("", "end", values=(
                rank, f"  {display}", fmt_size(size), f"{pct_disk:.1f}%"
            ), tags=(tag,), iid=path)

        self.tree.tag_configure("danger", foreground=DANGER)
        self.tree.tag_configure("warn", foreground=WARN)

    def _populate_stats(self):
        for w in self.stat_container.winfo_children():
            w.destroy()

        engine = self.engine

        # 磁盘概况
        if engine.disk_usage:
            du = engine.disk_usage
            total_g = du.total / (1024**3)
            used_g = du.used / (1024**3)
            free_g = du.free / (1024**3)
            pct = du.used / du.total * 100

            self._stat_card("💿 总容量", f"{total_g:.0f} GB")
            self._stat_card("📌 已使用", f"{used_g:.0f} GB  ({pct:.0f}%)")
            self._stat_card("🟢 剩余", f"{free_g:.0f} GB")

            # 使用率条
            bar_frame = tk.Frame(self.stat_container, bg=CARD)
            bar_frame.pack(fill="x", pady=6)
            tk.Label(bar_frame, text="使用率", font=("微软雅黑", 9),
                     bg=CARD, fg=FG2).pack(anchor="w")

            bar_bg = tk.Frame(bar_frame, bg=DARK, height=12)
            bar_bg.pack(fill="x", pady=2)
            bar_bg.pack_propagate(False)

            color = SUCCESS if pct < 70 else WARN if pct < 85 else DANGER
            fill = tk.Frame(bar_bg, bg=color, width=int(pct * 2.4))
            fill.pack(side="left", fill="y")

        # 类别汇总
        if engine.cat_summary:
            self._sep("类别汇总")
            max_cat = engine.cat_summary[0][1]
            for cat, sz in engine.cat_summary:
                bar_pct = sz / max_cat * 100
                frame = tk.Frame(self.stat_container, bg=CARD)
                frame.pack(fill="x", pady=1)
                tk.Label(frame, text=cat, font=("微软雅黑", 9),
                         bg=CARD, fg=FG, anchor="w").pack(side="left")
                tk.Label(frame, text=fmt_size(sz), font=("微软雅黑", 9),
                         bg=CARD, fg=FG2, anchor="e").pack(side="right")
                bar = tk.Frame(frame, bg=DARK, height=4)
                bar.pack(fill="x", pady=1)
                bar.pack_propagate(False)
                fill_bar = tk.Frame(bar, bg=ACCENT, width=int(bar_pct * 2))
                fill_bar.pack(side="left", fill="y")

            # ---- 饼图 ----
            self._sep("占比饼图")
            canvas = tk.Canvas(self.stat_container, bg=CARD, highlightthickness=0)
            canvas.pack(pady=8, expand=True, fill="both")
            size = 200
            center = size // 2
            radius = center - 10
            start_angle = 0
            colors = ["#3b82f6", "#22c55e", "#f97316", "#ef4444", "#8b5cf6", "#eab308"]
            for idx, (cat, sz) in enumerate(engine.cat_summary):
                extent = sz / max_cat * 360
                canvas.create_arc((center - radius, center - radius,
                                   center + radius, center + radius),
                                  start=start_angle, extent=extent,
                                  fill=colors[idx % len(colors)], outline="")
                start_angle += extent

        # 大文件
        if engine.large_files:
            self._sep("大文件 TOP 10")
            for fp, sz in engine.large_files[:10]:
                name = os.path.basename(fp)
                if len(name) > 22:
                    name = name[:20] + "…"
                frame = tk.Frame(self.stat_container, bg=CARD)
                frame.pack(fill="x", pady=1)
                lbl = tk.Label(frame, text=f"🐋 {name}",
                               font=("微软雅黑", 9),
                               bg=CARD, fg=FG, anchor="w", cursor="hand2")
                lbl.pack(side="left")
                lbl.bind("<Button-1>",
                         lambda e, p=os.path.dirname(fp): open_in_explorer(p))
                tk.Label(frame, text=fmt_size(sz), font=("微软雅黑", 9),
                         bg=CARD, fg=FG2, anchor="e").pack(side="right")

    def _stat_card(self, label, value):
        frame = tk.Frame(self.stat_container, bg=DARK, height=40)
        frame.pack(fill="x", pady=2)
        frame.pack_propagate(False)
        tk.Label(frame, text=label, font=("微软雅黑", 9),
                 bg=DARK, fg=FG2).pack(side="left", padx=8)
        tk.Label(frame, text=value, font=("微软雅黑", 12, "bold"),
                 bg=DARK, fg=FG).pack(side="right", padx=8)

    def _sep(self, title):
        tk.Label(self.stat_container, text=f"━━━ {title} ━━━",
                 font=("微软雅黑", 9), bg=CARD, fg=FG2).pack(pady=(10, 4))

    # ==================== 交互 ====================

    def _on_tree_double(self, _event):
        sel = self.tree.selection()
        if sel:
            open_in_explorer(sel[0])

    # ==================== 导出 ====================

    def _export_html(self):
        if not self.engine.disk_usage:
            messagebox.showwarning("提示", "请先完成一次扫描")
            return

        desktop = os.path.join(os.path.expanduser("~"), "Desktop")
        fname = f"disk_report_{datetime.now().strftime('%Y%m%d_%H%M%S')}.html"
        path = os.path.join(desktop, fname)

        try:
            html_report.generate(self.engine, path)
            self.prog_label.config(text="✅ 报告已导出")
            if messagebox.askyesno("完成",
                                   f"报告已保存到桌面:\n{path}\n\n是否在浏览器中打开？"):
                import webbrowser
                webbrowser.open(f"file://{path}")
        except Exception as e:
            messagebox.showerror("导出失败", str(e))

    # ==================== 运行 ====================

    def run(self):
        self.root.mainloop()


def main():
    app = App()
    app.run()
    return 0


if __name__ == "__main__":
    sys.exit(main())
