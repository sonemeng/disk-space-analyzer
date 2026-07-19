# Disk Space Analyzer

Native Windows desktop app for disk usage analysis, safe cleanup, media review, and registry health checks.

[中文](README.md) | [日本語](README.ja.md) | [Русский](README.ru.md)

[![Version](https://img.shields.io/badge/version-6.2.2-blue)](https://github.com/sonemeng/disk-space-analyzer/releases)
[![Tauri](https://img.shields.io/badge/Tauri-2-green)](https://tauri.app/)
[![Platform](https://img.shields.io/badge/platform-Windows-blue)](https://github.com/sonemeng/disk-space-analyzer)

---

## Features

| Module | Description |
|--------|-------------|
| Overview | Full-disk scan, capacity/category breakdown, attribution tips |
| Cleanup Center | Category tabs: fixed whitelist / rebuildable dev / tool-AI / app cache / review / system |
| File Review | Directory ranking, large-file filters, type browser, multi-select recycle |
| Deep Analysis | Duplicates, snapshot Diff, file age, action checklist |
| Media Center | Image/video/audio analysis, duplicates, recycle bin |
| Registry Check | HKCU health checks with backup-before-repair; no HKLM services/drivers |

### Safety principles

- Deletions go to the **Windows Recycle Bin** only (recoverable)
- Dev caches require **neighbor validation** (e.g. `node_modules` next to `package.json`)
- Model / app caches require **strong confirmation**
- Scanning and hashing stay **on-device**; no path/content upload

---

## Download

Get the latest build from [Releases](https://github.com/sonemeng/disk-space-analyzer/releases), e.g.:

- `DiskSpaceAnalyzer-6.2.2.exe` (portable; double-click to run)

Windows 10 / 11 64-bit.

---

## Development

```bash
cd tauri-app
npm install
npm run tauri dev
npm run tauri build
```

Portable binary without installer:

```bash
cd tauri-app
npx tauri build --no-bundle
# output: src-tauri/target/release/disk-analyzer.exe
```

Production frontend must use `vite base: './'` to avoid a blank white window in Tauri release builds.

---

## Module notes

### Cleanup Center

1. Select a drive and run a **full scan**
2. Review items by category (select-all applies only to the current tab; strong-confirm items excluded)
3. Optional **preview freeable size** (dry-run)
4. Confirm to move into **Recycle Bin**; hot/in-use files are skipped

See also:

- [docs/ROADMAP.md](docs/ROADMAP.md)
- [docs/DEV_AI_CACHE_RULES_DRAFT.md](docs/DEV_AI_CACHE_RULES_DRAFT.md)
- [docs/TOOL_AI_COVERAGE.md](docs/TOOL_AI_COVERAGE.md)

### Media Center

Similarity, metadata, duplicates; recycle-bin only. Details: [docs/MEDIA_CENTER.md](docs/MEDIA_CENTER.md).

### Registry Check

Current-user HKCU only; forced backup before repair. Details: [docs/REGISTRY_CLEANER.md](docs/REGISTRY_CLEANER.md).

### Deep Analysis

Duplicates (size + SHA-256), snapshots, file age. Details: [docs/ANALYSIS_FEATURES.md](docs/ANALYSIS_FEATURES.md).

---

## Project layout

```
disk-space-analyzer/
├── tauri-app/          # Main product (Vue 3 + Tauri 2 + Rust)
├── docs/
├── dist/               # Local portable exe (may be untracked)
├── src/                # Legacy Python edition (optional)
├── README.md
├── CHANGELOG.md
└── LICENSE
```

---

## Stack

- UI: Vue 3 + TypeScript + Lucide
- Desktop: Tauri 2
- Engine: Rust
- Platform: Windows 10 / 11 x64

---

## Changelog

See [CHANGELOG.md](CHANGELOG.md).

---

## License

MIT License
