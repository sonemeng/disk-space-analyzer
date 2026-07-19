# 磁盘空间分析器

原生 Windows 桌面应用：分析磁盘占用、安全清理、媒体与注册表健康检查。

[English](README.en.md) | [日本語](README.ja.md) | [Русский](README.ru.md)

[![Version](https://img.shields.io/badge/version-6.2.2-blue)](https://github.com/sonemeng/disk-space-analyzer/releases)
[![Tauri](https://img.shields.io/badge/Tauri-2-green)](https://tauri.app/)
[![Platform](https://img.shields.io/badge/platform-Windows-blue)](https://github.com/sonemeng/disk-space-analyzer)

---

## 功能概览

| 模块 | 说明 |
|------|------|
| 空间概览 | 整盘扫描、容量与类别分布、空间归因与建议 |
| 清理中心 | 分类 Tab：固定白名单 / 开发可重建 / 工具·AI / 应用缓存 / 需复核 / 系统 |
| 文件审查 | 目录排行、大文件筛选、按类型浏览、多选回收站 |
| 深度分析 | 重复文件、空间趋势 Diff、文件年龄、行动清单 |
| 媒体管理 | 图片/视频/音频分析，重复与回收站处理 |
| 注册表检查 | HKCU 健康检查，备份后修复；不碰 HKLM 服务驱动 |

### 安全原则

- 删除一律进入 **Windows 回收站**（可还原），无永久删除 API
- 开发缓存需 **邻居验证**（如 `package.json` 旁的 `node_modules`）
- 模型 / 应用缓存等需 **强确认** 后才可清理
- 扫描与哈希仅在本机处理，不上传路径内容

---

## 下载

从 [Releases](https://github.com/sonemeng/disk-space-analyzer/releases) 下载最新版，例如：

- `DiskSpaceAnalyzer-6.2.2.exe`（便携版，双击运行）

适用于 Windows 10 / 11 64 位。

---

## 开发与构建

```bash
cd tauri-app
npm install
npm run tauri dev      # 开发窗口
npm run tauri build    # 正式构建
```

仅前端打包进 exe（无安装器）：

```bash
cd tauri-app
npx tauri build --no-bundle
# 产物: src-tauri/target/release/disk-analyzer.exe
```

生产前端必须使用 `vite base: './'`，否则 Tauri release 可能白屏。

---

## 主要模块说明

### 清理中心

1. 选择磁盘并完成 **完整扫描**
2. 按分类查看可清理项（本类全选不包含强确认项）
3. 可选 **预览释放量**（dry-run）
4. 确认后 **移入回收站**；热文件 / 占用文件会跳过

详见路线图与规则草案：

- [docs/ROADMAP.md](docs/ROADMAP.md)
- [docs/DEV_AI_CACHE_RULES_DRAFT.md](docs/DEV_AI_CACHE_RULES_DRAFT.md)
- [docs/TOOL_AI_COVERAGE.md](docs/TOOL_AI_COVERAGE.md)

### 媒体管理

图片相似度、音视频属性、重复媒体；统一回收站。说明见 [docs/MEDIA_CENTER.md](docs/MEDIA_CENTER.md)。

### 注册表检查

仅当前用户 HKCU；修复前强制备份。说明见 [docs/REGISTRY_CLEANER.md](docs/REGISTRY_CLEANER.md)。

### 深度分析

重复文件（大小 + SHA-256）、空间快照趋势、文件年龄。说明见 [docs/ANALYSIS_FEATURES.md](docs/ANALYSIS_FEATURES.md)。

---

## 项目结构

```
磁盘空间分析器/
├── tauri-app/                 # 主产品（Vue 3 + Tauri 2 + Rust）
│   ├── src/                   # 前端
│   └── src-tauri/src/         # 扫描 / 清理 / 媒体 / 注册表 / 规则包
├── docs/                      # 文档
├── dist/                      # 本地便携 exe（不保证纳入 git）
├── src/                       # 历史 Python 版（可选）
├── README.md
├── CHANGELOG.md
└── LICENSE
```

---

## 技术栈

- 界面：Vue 3 + TypeScript + Lucide
- 桌面：Tauri 2
- 引擎：Rust
- 平台：Windows 10 / 11 x64

---

## 更新日志

见 [CHANGELOG.md](CHANGELOG.md)。

---

## 许可

MIT License
