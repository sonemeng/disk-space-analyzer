# 工具/AI 识别覆盖说明（0.5.0-p4）

> 给产品与用户看：能扫什么、不能扫什么、为何如此。

## 原则

1. **只扫有稳定本机路径的**（用户目录、AppData、常见安装布局、环境变量）。
2. **云端/纯网页 Agent**（Devin 网页、v0、Bolt.new、Lovable、Copilot Workspace 网页）本地几乎无「大缓存目录」→ **不做假规则**。
3. **IDE 插件**（Cline/Roo/Copilot 扩展）数据多在 VS Code/Cursor 扩展目录内 → 优先靠 **Electron Cache 子树** + 点目录（`.cursor` 等），不假装能拆出每个插件体积。
4. **模型/生图** 一律 **需强确认**；包管理器与 Chromium Cache **默认可清但不默认勾选**。

## 已覆盖（可落地）

### 包管理器
npm / Yarn / pnpm / pip / Cargo / Gradle / NuGet

### 编辑器 Electron Cache（仅 Cache 类子树）
Cursor、VS Code/Insiders、VSCodium、Windsurf、Trae、Qoder、Void、PearAI、Continue、Zed、Antigravity、CodeBuddy、MarsCode、Claude、ChatGPT、OpenAI、OpenCode、Hermes、AutoClaw、Codex Show Studio、ZCODE、LM Studio、TabNine、Codeium、GitHubCopilot、Amazon Q、JetBrains、CherryStudio、Kiro、Workbuddy、Fitten、通义/Comate/CodeGeeX/Kimi/星火 等常见文件夹名

### Agent / 工具用户目录（强确认）
`.cursor` `.claude` `.codex` `.continue` `.opencode` `.hermes` `.codeium` `.copilot` `.gemini` `.grok` `.kiro` `.openclaw` `.cherrystudio` `.codebuddy` `.qoder` `.windsurf` `.trae-*` `.devin` `.fitten` `.workbuddy` `.agents` `.aider` `.tabnine` 等

### 本地推理 / 模型（强确认）
Ollama、Hugging Face、PyTorch hub、LM Studio、Jan、GPT4All、AnythingLLM、LocalAI、SillyTavern、KoboldCPP、text-generation-webui

### 生图（强确认，常见布局）
ComfyUI、Stable Diffusion WebUI（A1111）常见 models/output 路径

## 刻意不做或弱覆盖

| 类型 | 原因 |
|------|------|
| Devin / Replit Agent / v0 / Bolt / Lovable / Jules 等纯云端 | 本机无稳定大目录 |
| Cline / Roo 作为 VS Code 插件的独立「整包」体积 | 数据嵌在扩展目录，无法干净拆分 |
| JetBrains AI 精确缓存 | 与 IDE 系统目录深度耦合，仅做浅层 AppData 名匹配 |
| 用户自定义模型盘（任意 D:\models） | 禁止裸 `models` 名匹配，防误伤 |

## 后续可追加方式

在 `tool_ai_rules.rs` 的 `RULES` / `ElectronCaches` / `ProfileRel` 表追加即可；发版升 `RULEPACK_VERSION`。
