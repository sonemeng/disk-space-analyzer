# Анализатор дискового пространства

Нативное приложение Windows для анализа занятости диска, безопасной очистки, проверки медиа и состояния реестра.

[中文](README.md) | [English](README.en.md) | [日本語](README.ja.md)

[![Version](https://img.shields.io/badge/version-6.2.2-blue)](https://github.com/sonemeng/disk-space-analyzer/releases)
[![Tauri](https://img.shields.io/badge/Tauri-2-green)](https://tauri.app/)
[![Platform](https://img.shields.io/badge/platform-Windows-blue)](https://github.com/sonemeng/disk-space-analyzer)

---

## Возможности

| Модуль | Описание |
|--------|----------|
| Обзор | Полное сканирование диска, ёмкость и категории, подсказки по распределению |
| Центр очистки | Вкладки: белый список / пересобираемые dev-кэши / tool-AI / кэш приложений / на проверку / система |
| Просмотр файлов | Топ каталогов, фильтры крупных файлов, по типам, множественный выбор в корзину |
| Глубокий анализ | Дубликаты, Diff снимков, возраст файлов, список действий |
| Медиа | Анализ изображений/видео/аудио, дубликаты, корзина |
| Реестр | Проверка HKCU с резервной копией перед исправлением; без HKLM-служб/драйверов |

### Принципы безопасности

- Удаление только в **Корзину Windows** (можно восстановить)
- Dev-кэши требуют **проверки соседних маркеров** (например, `node_modules` рядом с `package.json`)
- Модели и кэши приложений требуют **строгого подтверждения**
- Сканирование и хеширование выполняются **только локально**

---

## Загрузка

Скачайте свежую сборку на странице [Releases](https://github.com/sonemeng/disk-space-analyzer/releases), например:

- `DiskSpaceAnalyzer-6.2.2.exe` (portable, запуск двойным щелчком)

Windows 10 / 11 64-bit.

---

## Разработка и сборка

```bash
cd tauri-app
npm install
npm run tauri dev
npm run tauri build
```

Только portable exe без установщика:

```bash
cd tauri-app
npx tauri build --no-bundle
# результат: src-tauri/target/release/disk-analyzer.exe
```

Для production-фронтенда обязателен `vite base: './'`, иначе в Tauri release возможен белый экран.

---

## Модули

### Центр очистки

1. Выберите диск и выполните **полное сканирование**
2. Просмотрите пункты по категориям (выбор «все в категории» не включает пункты со строгим подтверждением)
3. При желании **предпросмотр освобождаемого объёма** (dry-run)
4. Подтвердите перемещение в **Корзину**; «горячие»/занятые файлы пропускаются

Документы:

- [docs/ROADMAP.md](docs/ROADMAP.md)
- [docs/DEV_AI_CACHE_RULES_DRAFT.md](docs/DEV_AI_CACHE_RULES_DRAFT.md)
- [docs/TOOL_AI_COVERAGE.md](docs/TOOL_AI_COVERAGE.md)

### Медиа / реестр / анализ

- Медиа: [docs/MEDIA_CENTER.md](docs/MEDIA_CENTER.md)
- Реестр: [docs/REGISTRY_CLEANER.md](docs/REGISTRY_CLEANER.md)
- Анализ: [docs/ANALYSIS_FEATURES.md](docs/ANALYSIS_FEATURES.md)

---

## Стек

- UI: Vue 3 + TypeScript + Lucide
- Desktop: Tauri 2
- Движок: Rust
- Платформа: Windows 10 / 11 x64

---

## История изменений

См. [CHANGELOG.md](CHANGELOG.md).

---

## Лицензия

MIT License
