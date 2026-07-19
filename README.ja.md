# ディスク容量アナライザー

Windows ネイティブのデスクトップアプリです。ディスク使用量の分析、安全なクリーンアップ、メディア確認、レジストリの健全性チェックに対応します。

[中文](README.md) | [English](README.en.md) | [Русский](README.ru.md)

[![Version](https://img.shields.io/badge/version-6.2.2-blue)](https://github.com/sonemeng/disk-space-analyzer/releases)
[![Tauri](https://img.shields.io/badge/Tauri-2-green)](https://tauri.app/)
[![Platform](https://img.shields.io/badge/platform-Windows-blue)](https://github.com/sonemeng/disk-space-analyzer)

---

## 機能概要

| モジュール | 説明 |
|------------|------|
| 概要 | ディスク全体スキャン、容量・カテゴリ分布、空間の帰属ヒント |
| クリーンアップ | タブ分類：固定ホワイトリスト / 開発再構築可能 / ツール・AI / アプリキャッシュ / 要確認 / システム |
| ファイル確認 | ディレクトリ順位、大容量ファイル絞り込み、種類別表示、複数選択してごみ箱へ |
| 詳細分析 | 重複ファイル、スナップショット Diff、ファイル経過、アクションリスト |
| メディア | 画像・動画・音声の分析、重複、ごみ箱処理 |
| レジストリ | HKCU の健全性確認、バックアップ後に修復（HKLM サービス/ドライバは対象外） |

### 安全方針

- 削除は **Windows のごみ箱** のみ（復元可能）
- 開発キャッシュは **隣接マーカー検証**（例: `package.json` 横の `node_modules`）
- モデル / アプリキャッシュは **強い確認** が必要
- スキャンとハッシュは **端末内のみ**、パスや内容はアップロードしない

---

## ダウンロード

[Releases](https://github.com/sonemeng/disk-space-analyzer/releases) から最新版を取得してください。例:

- `DiskSpaceAnalyzer-6.2.2.exe`（ポータブル版、ダブルクリックで実行）

Windows 10 / 11 64 ビット向け。

---

## 開発とビルド

```bash
cd tauri-app
npm install
npm run tauri dev
npm run tauri build
```

インストーラなしの実行ファイルのみ:

```bash
cd tauri-app
npx tauri build --no-bundle
# 出力: src-tauri/target/release/disk-analyzer.exe
```

本番フロントは `vite base: './'` が必須です。設定しないと Tauri release で白画面になることがあります。

---

## モジュール補足

### クリーンアップ

1. ドライブを選び **フルスキャン**
2. 分類ごとに確認（「この分類を全選択」は強い確認項目を含まない）
3. 任意で **解放量プレビュー**（dry-run）
4. 確認後に **ごみ箱へ移動**；使用中/ホットなファイルはスキップ

関連ドキュメント:

- [docs/ROADMAP.md](docs/ROADMAP.md)
- [docs/DEV_AI_CACHE_RULES_DRAFT.md](docs/DEV_AI_CACHE_RULES_DRAFT.md)
- [docs/TOOL_AI_COVERAGE.md](docs/TOOL_AI_COVERAGE.md)

### メディア / レジストリ / 詳細分析

- メディア: [docs/MEDIA_CENTER.md](docs/MEDIA_CENTER.md)
- レジストリ: [docs/REGISTRY_CLEANER.md](docs/REGISTRY_CLEANER.md)
- 詳細分析: [docs/ANALYSIS_FEATURES.md](docs/ANALYSIS_FEATURES.md)

---

## 技術スタック

- UI: Vue 3 + TypeScript + Lucide
- デスクトップ: Tauri 2
- エンジン: Rust
- 対応: Windows 10 / 11 x64

---

## 変更履歴

[CHANGELOG.md](CHANGELOG.md) を参照。

---

## ライセンス

MIT License
