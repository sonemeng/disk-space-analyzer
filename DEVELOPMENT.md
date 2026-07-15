# 开发文档 — 磁盘空间分析器

## 技术架构

```
磁盘空间分析器/
├── src/                  # 源码
│   ├── __init__.py       # 版本号 & 元信息
│   ├── __main__.py       # 入口（python -m src）
│   ├── app.py            # tkinter GUI 主窗口
│   ├── scanner.py        # 扫描引擎（多线程）
│   ├── html_report.py    # HTML 报告生成器
│   └── utils.py          # 工具函数
├── assets/               # 资源
│   ├── icon-5.1.png      # 高分辨率图标源文件
│   └── icon.ico          # Windows 16–256px 多尺寸图标
├── scripts/              # 构建脚本
│   └── build.bat         # 一键打包 .exe
├── tests/                # 单元测试
└── docs/                 # 扩展文档
```

### Tauri 6.0

```
tauri-app/
├── src/App.vue                 # Vue 3 工作台、磁盘视图与设置状态
├── src/MediaCenter.vue         # 媒体管理、筛选、预览和回收站交互
├── src-tauri/src/main.rs       # 磁盘扫描器、系统集成和报告导出
├── src-tauri/src/media.rs      # 媒体属性、哈希、缩略图和回收站后端
├── src-tauri/tauri.conf.json   # 窗口与打包配置
├── package.json                # Vite/Tauri 前端工具链
└── vite.config.ts
```

Rust 扫描在后台阻塞任务中执行，通过 `scan-progress` 事件更新前端。扫描取消使用共享原子标记。清理命令只接受固定候选 ID，并在 Rust 端重新解析白名单路径；前端不能请求删除任意文件。

界面偏好由 `App.vue` 统一管理，并通过 HTML `data-*` 属性驱动 CSS 变量。设置保存在以下本地存储键中：

- `disk-analyzer-theme`
- `disk-analyzer-font-scale`
- `disk-analyzer-icon-scale`
- `disk-analyzer-density`
- `disk-analyzer-sidebar-collapsed`
- `disk-analyzer-advanced-settings`

高级设置会作为显式参数传入 Rust 命令；后端仍会验证范围、数量、路径和上下限，不能依赖前端绕过安全限制。

深度分析事件和命令：

- `duplicate-progress` / `find_duplicates`：大小预筛与 SHA-256 重复检测
- `save_snapshot` / `get_snapshots`：本地 JSON 快照，按设置保留 10–100 条
- `ScanResult.age_buckets`：扫描过程中同步累积文件年龄容量
- `folder-progress` / `analyze_folder`：指定目录逐层下钻
- `media-progress` / `scan_media`：媒体属性、图片感知哈希与缩略图
- `recycle_media`：验证媒体文件后调用 Windows 回收站
- `clear_snapshots`：清除指定盘或全部本地快照
- `check_for_updates`：读取公开 GitHub Release 版本
- `export_diagnostics`：导出本地版本、平台、设置与快照状态

媒体依赖：

- `img_hash` / `image`：图片解码、缩略图和感知哈希
- `lofty`：音频属性解析
- `rayon`：受设置控制的媒体并发分析
- `trash`：Windows 回收站
- `ureq` / `semver`：更新检查与版本比较

## 依赖

**运行时**：无外部依赖（纯 Python 标准库）
- `tkinter` / `ttk` — GUI
- `os` / `shutil` — 文件系统
- `threading` / `concurrent.futures` — 并行扫描
- `subprocess` / `webbrowser` — 系统交互

**开发时**：
- `pyinstaller>=6.0` — 打包 .exe

## 模块说明

### app.py — 主窗口
- `App` 类管理整个 tkinter 窗口的生命周期
- 使用 `ScanEngine(callback=…)` 启动后台扫描线程
- 扫描完成后用 `_populate_tree()` 和 `_populate_stats()` 填充结果

### scanner.py — 扫描引擎
- `ScanEngine` 在后台线程运行
- `scan(drive)` 分 6 个阶段执行：
  1. Phase 1: 根目录下一级（depth=1，快速概览）
  2. Phase 2: 深入 >200GB 的大目录（depth=3）
  3. Phase 3: 用户目录 + AppData（C盘专用）
  4. Phase 4: 系统目录（WinSxS/Installer/ProgramData）
  5. Phase 5: 大文件搜索（Desktop/Downloads/Documents）
  6. Phase 6: 类别汇总 + 文件类型统计

### html_report.py — HTML 生成
- 纯函数 `generate(scanner, output_path)`
- 生成深色主题的自包含 HTML 报告
- 目录/大文件均包含 `file:///` 可点击链接

## 构建

### Tauri 桌面版

```bash
cd tauri-app
npm install --cache .npm-cache
npm run tauri dev

# 完整发布构建
npm run tauri build

# 仅验证桌面二进制，不生成安装器
npm run tauri -- build --debug --no-bundle
```

发布前如需重新生成应用图标：

```bash
python scripts\generate_icon.py
```

脚本使用高分辨率源图生成 `assets/icon-5.1.png`，并将 16/24/32/48/64/128/256px 图层写入 `assets/icon.ico`。Tauri 和 PyInstaller 均使用该 ICO。

### 打包 .exe

```bash
scripts\build.bat
# 或手动：
pyinstaller --onefile --windowed --icon=assets\icon.ico --name "磁盘空间分析器" src\__main__.py
```

输出在 `dist/磁盘空间分析器.exe`

### 开发调试

```bash
# 直接运行源码（无控制台用 pythonw）
python src\__main__.py

# 或使用模块方式
python -m src
```

## 扩展指南

### 添加新功能

1. **新扫描策略** → 在 `scanner.py` 中新增 `_scan_xxx()` 方法，在 `scan()` 中调用
2. **新图表类型** → 在 `app.py` 的 `_populate_stats()` 中添加 tkinter 组件
3. **新导出格式** → 新建 `src/csv_report.py`，在 `app.py` 的导出逻辑中增加选项

### 代码规范

- 中文字符串：中文变量用 Unicode，用户界面文字用中文
- 工具函数放在 `utils.py`，不散落在其他模块
- 函数名用小写蛇形

## 已知限制

- 仅支持 Windows（使用 `explorer.exe` 跳转，`os.walk` 的行为也依赖 Windows）
- WinSxS 扫描较慢（目录结构深、文件数量大）
- tkinter 在深色主题下的样式定制有限
