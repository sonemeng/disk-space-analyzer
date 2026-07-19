# 开发者 / AI 缓存识别草案（仅设计，未实现）

> 状态：**草案 v0.2** · 2026-07-18（已并入产品决策）  
> 原则：**主干稳、枝干活**——框架构建产物为长期主干；AI/工具路径为可热更新枝干。  
> 先可解释、默认可恢复、默认不勾选高风险；P1 只读，再开删除。  
> 相关现状：`dev_rules.rs`（邻域校验）、`fixed_cleanup_definitions`（固定白名单）、清理中心分组 UI。
---

## 1. 目标与非目标

### 目标

1. 让开发者能**看见**「工具缓存 / AI 相关缓存 / 可重建产物」占了多少空间。  
2. 在**不误删源码、模型权重（默认）、用户数据**的前提下，给出分级处理入口。  
3. 每条命中规则**可解释**（匹配了哪条规则、为何是 safe/review）。  
4. 与现有体系兼容：固定白名单 / 开发可重建 / 需复核 / 永不碰。

### 非目标（v1 不做）

- 不做内容语义 / ML 判断「是不是垃圾」。  
- 不自动永久删除；不默认勾选 AI 类与开发类。  
- 不扫描用户私有对话全文内容；只认**路径与邻域标记**。  
- 不替代 Windows 存储感知 / 浏览器自带清理的全部能力。  
- **不把「追新 AI 平台路径」当作主算法**（见 §1.1）。

---

## 1.1 战略方向：主干稳、枝干活（回答「AI 更新太快怎么办」）

### 你的判断成立

| 层 | 变化速度 | 例子 | 在产品中的角色 |
|----|----------|------|----------------|
| **主干 A：框架 / 构建产物** | 慢（年） | `node_modules`、`target`、`__pycache__`、`.next`、`obj`/`bin` | **长期主算法**；邻域标记稳定，误删面可控 |
| **主干 B：包管理器缓存** | 中（年） | npm/yarn/pnpm store、pip、cargo registry、gradle | **第二主干**；路径相对稳定，清了可重下 |
| **枝干 C：编辑器 Electron Cache** | 中 | `Cache`/`Code Cache`/`GPUCache` 子树 | 模式稳定（Chromium 壳），厂商名会变 |
| **枝干 D：AI Agent / 本地模型** | **很快** | Cursor、新 Agent、Ollama、HF、明年新平台 | **可插拔规则表**，不是写死在核心逻辑里 |

**结论：**  
- **父/祖先路径片段 alone 追 AI 新品 → 一定会过时。**  
- 正确做法不是放弃路径，而是：  
  1. **核心引擎只做稳定的「模式」**（邻域校验、Cache 子树、包缓存根、保护带）；  
  2. **具体厂商路径放在「规则包」里**，可版本化更新，不必等发版重写算法；  
  3. **新平台未收录时**：靠大文件审查 / 文件夹分析 / 用户黑名单与手动回收兜底，而不是瞎猜。

### 路径片段还能不能判到新平台？

| 情况 | 能否判到 | 靠什么 |
|------|----------|--------|
| 已收录厂商 + 路径未大改 | 能 | 规则包精确路径 / 环境变量 |
| 已收录但目录改名 | 部分漏 | 规则包小版本更新；P1 只读暴露漏检 |
| **全新 Agent 平台** | **默认不能** | 不承诺；靠通用枝干 + 人工加规则 |
| 新平台仍是 Electron/Chromium 壳 | 能判到 **Cache 类** | 通用模式：`…\<App>\Cache` 且在 Local/Roaming AppData |
| 新平台把数据放进项目目录 | 危险 | **禁止**泛匹配；项目内默认 S0/S2 |

### 通用模式（抗厂商更名，优先实现）

这些**不依赖「叫不叫 Cursor」**：

1. **构建产物邻域校验**（已有）— 主干，不变。  
2. **包管理器约定根** — `%LOCALAPPDATA%\npm-cache`、`.cargo\registry`、pnpm store 等。  
3. **Electron/Chromium 缓存子树** — 仅当 basename ∈ `{Cache, Code Cache, GPUCache, ShaderCache, logs, Crashpad}` **且** 位于 `%APPDATA%|<App>|` 或 `%LOCALAPPDATA%|<App>|` 下，**禁止**整棵 User Data。  
4. **环境变量根** — `OLLAMA_MODELS`、`HF_HOME`、`CARGO_HOME`、`GOMODCACHE`、`NUGET_PACKAGES`…（平台换皮也常留 env）。  
5. **活跃工作区 S0** — 与厂商无关，防误伤当前项目。

厂商专用表（Cursor/Claude/…）= **枝干规则包**，允许滞后；主干不受影响。

### 规则包形态（设计，P1/P2 起）

```text
规则包 = 内置默认 JSON/表 + 可选本地覆盖
字段：id, layer(A|B|C|D), patterns[], env_keys[], markers[],
      tier(S0–S4), whole_dir, confirm_level, min_bytes, note
更新：随应用发版；远期可「只更新规则、不换主程序」（非 v1 必做）
未命中：不猜测；用户仍可在文件审查里手动处理
```

**产品承诺话术建议：**  
「开发构建与包缓存：稳定识别。工具/AI 缓存：持续扩充规则；新平台可能先显示在大文件里，确认后再加入规则包。」

---

## 2. 分级模型（强制）

| 等级 | 代码意向 | 清理中心默认 | 执行方式 | 说明 |
|------|----------|--------------|----------|------|
| **S0 永不碰** | `never` | 不展示为可清理，或仅「保护中」 | 禁止 `clean_items` | 系统、源码树、活跃工作区、用户明确保护 |
| **S1 仅展示** | `info` | 不进清理列表 | 无 | 体积归因用，引导去审查/手动 |
| **S2 需复核** | `review` | 单独分组，**无默认勾选**，无整目录一键 | 仅跳转审查 / 打开资源管理器 / 可选「移入回收站」且强确认 | AI 模型、数据集、不确定缓存 |
| **S3 可重建·高置信** | `safe` + `developer` | 列表可见，**默认不勾选** | 现有开发项流程：再校验 + 热保护 + 回收站 | 与现 `node_modules`/`target` 同档 |
| **S4 固定安全** | `safe` + `fixed` | 可默认勾选（保持现状） | 按文件年龄过滤 + 回收站 | Temp、浏览器 Cache 等 |

**硬规则：**

- 新 AI 规则 **默认不得直接进 S4**。  
- 新 AI 规则 **首版最多进 S2**；经观察误报率低、且可明确「删了能再下」的，再评估升 S3。  
- 任何 `whole_dir` 删除必须：邻域标记 + 删除前再分类 + 热保护 + 回收站。

---

## 3. 识别流水线（设计）

```text
候选路径
  → ① 保护前缀 / 源码名 / 用户黑名单 / 活跃工作区？ → S0
  → ② 命中固定白名单模板？ → S4（现有逻辑）
  → ③ 命中高置信可重建规则（邻域 OK）？ → S3
  → ④ 命中 AI/工具缓存规则？
        · 可证明「纯缓存且可再生」→ 候选 S3（v1 仍建议先 S2）
        · 可能含模型/数据/密钥/对话 → S2
        · 仅名称像、邻域不足 → S1 或忽略
  → ⑤ 否则不进清理中心（可在文件夹分析给 hint）
```

**匹配维度（只允许这些，v1）：**

| 维度 | 用法 |
|------|------|
| 目录/文件 **basename** | 主匹配 |
| **父/祖先路径片段** | 如 `.cursor`、`Ollama`、`HuggingFace` |
| **邻域标记文件** | 如 `package.json`、`Cargo.toml`；AI 侧可用「厂商目录结构」 |
| **位于已知根** | `%LOCALAPPDATA%`、`%USERPROFILE%\.cache`、项目扫描根 |
| **年龄 / 热更新** | 执行门闩，不作唯一分类依据 |
| **最小体积** | 与现开发项类似（建议 ≥ 5–20 MB 才进列表，防噪音） |

**明确不用：** 文件内容、模型 magic、对话 JSON 解析（隐私与误判风险）。

---

## 4. 规则表

### 4.1 已有：开发可重建（维持，作对照）

| ID | 路径特征 | 邻域条件 | 建议等级 | 说明 |
|----|----------|----------|----------|------|
| `dev-node-modules` | `**/node_modules` | 父有 package/lock | **S3** | 已实现 |
| `dev-rust-target` | `**/target` | Cargo/pom/gradle | **S3** | 已实现 |
| `dev-pycache` | `__pycache__` 等 | py 工程或 `.py` | **S3** | 已实现 |
| `dev-next-turbo` | `.next` `.turbo` 等 | 前端标记 | **S3** | 已实现 |
| `dev-dotnet-obj-bin` | `obj`/`bin` | `*.csproj` | **S3** | 已实现 |
| `dev-dist-hint` | `dist`/`build`/`out` | 有工程标记 | **S1** | 已实现：不进一键 |
| `dev-vendor-hint` | `vendor` | 有工程标记 | **S1** | 已实现 |

### 4.2 新增：通用开发工具缓存（建议 v1 进 S3 或 S2）

| ID | 典型位置（Windows） | 邻域 / 约束 | 建议等级 | 可再生？ | 风险 |
|----|---------------------|-------------|----------|----------|------|
| `tool-npm-cache` | `%LOCALAPPDATA%\npm-cache` | 路径片段 `npm-cache` | **S3** | 是，`npm install` | 低；清后装包变慢 |
| `tool-yarn-cache` | `%LOCALAPPDATA%\Yarn\Cache` | `Yarn` | **S3** | 是 | 低 |
| `tool-pnpm-store` | `pnpm-store` / `.pnpm-store` | 路径+可选 `pnpm` 配置 | **S3**（已决） | 是，store 共享 | 中：清后全项目重下；**默认不勾 + 文案强调** |
| `tool-pip-cache` | `%LOCALAPPDATA%\pip\Cache` | `pip\Cache` | **S3** | 是 | 低 |
| `tool-cargo-registry` | `%USERPROFILE%\.cargo\registry` | `.cargo` + registry | **S3**（已决） | 是 | 中：体积大、编译变慢；仅 registry 子树，**不动**乱放的源码 |
| `tool-cargo-git` | `.cargo\git` | 同上 | **S3**（已决） | 是 | 中：与 registry 同策略 |
| `tool-gradle-caches` | `.gradle\caches` | `.gradle` | **S3** | 是 | 低–中 |
| `tool-nuget-cache` | `NuGet\Cache` 等 | 路径 | **S3** | 是 | 低 |
| `tool-composer-cache` | 用户 composer cache | 路径 | **S3** | 是 | 低 |
| `tool-go-mod-cache` | `%GOMODCACHE%` / `go-build` | `go` 环境路径 | **S2** | 是 | 中：路径因环境而异 |
| `tool-docker-data` | Docker desktop data | 厂商路径 | **S0/S2** | 视内容 | **高**：可能含镜像/卷数据；v1 **只 S2 展示或跳过** |

**v1 落地子集（已决）：**  
- **S3：** npm / yarn / **pnpm-store** / pip / gradle / nuget / **cargo registry+git**（均默认不勾；pnpm/cargo 文案加粗风险）  
- **S2 强确认可回收：** Ollama / HF / Torch / LM Studio 等模型树  
- **仍不做一键：** Docker 数据盘、项目内配置目录
### 4.3 新增：编辑器 / AI Agent 相关（核心草案）

> 路径随版本会变，表中为**常见约定**；实现时必须「路径片段 + 目录结构」双条件，禁止单靠单词 `cache`。

| ID | 产品/场景 | 典型路径片段（示例） | 建议等级 | 为何 |
|----|-----------|----------------------|----------|------|
| `ai-cursor-cache` | Cursor | `%APPDATA%\Cursor\Cache`、`Code Cache`、`GPUCache` | **S3** | 与浏览器内核缓存同类，可再生 |
| `ai-cursor-logs` | Cursor | `logs`、`Crashpad` | **S3** | 日志/崩溃，低价值 |
| `ai-cursor-user-data` | Cursor | `User\workspaceStorage`、`User\globalStorage` | **S2** | 可能含工作区状态/扩展数据 |
| `ai-cursor-chat` | Cursor 对话/索引 | 含 `chat`/`composer`/`aichat` 等 storage 键目录 | **S2** | 用户资产；**禁止 S3** |
| `ai-vscode-cache` | VS Code / 衍生 | `%APPDATA%\Code\Cache` 等 | **S3** | 同浏览器类缓存 |
| `ai-vscode-globalstorage` | VS Code | `User\globalStorage` | **S2** | 扩展状态 |
| `ai-copilot-cache` | GitHub Copilot | 扩展 storage 下 cache 子目录（需结构确认） | **S2** | 边界不清时复核 |
| `ai-continue-cache` | Continue 等 | `.continue` 下 index/cache | **S2** | 索引可重建，但可能含代码片段索引 |
| `ai-aider-cache` | Aider 等 | `.aider*` 缓存目录 | **S2** | 先复核 |
| `ai-claude-desktop` | Claude 桌面 | 厂商 AppData 下 Cache | **S3**（仅 Cache 子树） | 非 Cache 子树 → S2 |
| `ai-chatgpt-desktop` | ChatGPT 桌面 | 同上策略 | **S3** 仅 Cache / **S2** 其余 | |
| `ai-ollama-models` | Ollama | `.ollama\models` 或 `OLLAMA_MODELS` | **S2 + 强确认可回收**（已决） | 禁止默认勾选；二次确认文案含「需重新下载」 |
| `ai-ollama-blobs` | Ollama blobs | models 下 blobs | **S2 + 强确认可回收** | 同上 |
| `ai-hf-hub` | Hugging Face Hub | `.cache\huggingface\hub` 或 `HF_HOME` | **S2 + 强确认可回收**（已决） | 同上 |
| `ai-hf-transformers` | transformers 缓存 | `transformers` 缓存目录 | **S2 + 强确认可回收** | 同上 |
| `ai-torch-hub` | Torch hub | `.cache\torch` | **S2 + 强确认可回收** | 同上 |
| `ai-lmstudio` | LM Studio | 模型目录（用户可配） | **S2 + 强确认可回收** | 路径不固定，禁止升 S3 |
| `ai-localai` / `ai-jan` 等 | 本地推理 UI | 各自 models | **S2** | 同上 |
| `ai-embed-index` | 各类向量/代码索引 | `*.index`、`lancedb`、`chroma` 在工具目录下 | **S2** | 可重建但重建慢；可能含代码衍生 |
| `ai-agent-workdir` | Agent 工作目录 | 项目内 `.agent` / `tmp/agent` | **S1/S2** | 易与用户文件混放 → 默认不自动删 |
| `ai-project-cursor` | 项目内 | 项目根 `.cursor` | **S2** | 规则/索引可能用户需要 |
| `ai-project-github-copilot` | 项目内 | `.github/copilot` 等 | **S0/S1** | 多为配置，不是缓存 |

### 4.4 固定安全扩展（可选，仍走 S4）

| ID | 路径 | 等级 | 备注 |
|----|------|------|------|
| 现有 `user-temp` 等 | 保持 | **S4** | 不动策略 |
| `win-prefetch` 等 | 系统向 | **S0** | **不要**收进清理 |
| `thumbnail-cache` | Explorer 缩略图 | **S2 或 S3** | 可再生；影响体验小 → 可 S3 但需年龄 |

---

## 5. 「进 review 还是 safe」判定树

```text
IF 路径 ∈ 系统保护 / 源码名 / 用户黑名单 / 活跃工作区
  → S0

IF 明确是「浏览器内核式 Cache/Code Cache/GPUCache/日志」
   AND 位于已知编辑器/Electron 应用的 Cache 子树
  → S3（safe/developer 或 fixed 子类）
  → 默认不勾选（若挂在 developer 组）

IF 明确是包管理器缓存目录（npm/yarn/pnpm-store/pip/gradle/nuget/cargo registry|git…）
   AND 路径落在约定位置或 env 根
   AND 不是 docker 数据盘
  → S3，默认不勾选（pnpm/cargo 额外风险文案）

IF 是模型权重、HF hub、Ollama models、数据集
  → S2（review）+ **允许强确认后进回收站**（已决）
  → UI：展示占用 + 打开目录 + 强制确认词/勾选「我了解需重新下载」
  → 禁止默认勾选；禁止无二次确认
  → 对话 storage / 不确定目录：仍可只读或 S2 但不提供整目录回收
IF 仅名称像 cache，邻域不足
  → S1 或忽略
```

**升档条件（S2 → S3）必须同时满足：**

1. 社区/文档确认「删了只影响速度，不影响工程正确性与用户资产」；  
2. 路径结构稳定 ≥ 2 个大版本；  
3. 内部试用 N 台机器误报为 0；  
4. 删除前样本清单（前 20 个文件）对用户可见；  
5. 产品文案写清「将重新下载」。

**降档条件（任何 S3 → S2/S0）：**

- 出现用户报告误删源码/模型/密钥；  
- 厂商把用户数据放进 Cache 目录；  
- 路径与用户文档库无法区分。

---

## 6. UI / 产品表现（设计）

### 清理中心分组（已决命名）

1. **固定可清理**（S4）— 现有，默认可勾  
2. **开发可重建**（S3）— 框架产物 + 包管理器缓存；默认不勾  
3. **工具/AI 缓存**（S2 为主，部分编辑器 Cache 可挂 S3）— **已决名称**；默认不勾；模型类强确认  
4. **系统工具** — 跳转 Windows  

### 每条目前展示字段

- 标题、路径、体积  
- **规则 ID**（如 `ai-ollama-models`）+ **规则包版本**  
- 等级徽章：`可重建` / `需确认` / `重新下载成本高`  
- 一句话原因：「位于 Ollama models，删除后需重新拉取模型」  
- 操作：打开目录 ·（S3）勾选回收 ·（S2）「我了解风险后移入回收站」

### 设置项（可选）

- 「显示工具/AI 缓存候选」默认开  
- 「允许模型缓存强确认回收」默认 **开**（已决允许；仍须二次确认，无默认勾选）  
- 清理黑名单继续生效，且对 S2/S3 均拦截  
- **保护当前打开的项目路径**（S0）— **已决要做**，与 P1 同期或紧前  

---

## 7. 安全门闩（相对现网增强点）

| 门闩 | 现网 | 草案要求 |
|------|------|----------|
| 只回收站 | ✅ | 保持 |
| 开发默认不勾 | ✅ | AI 组同样 |
| 删除前再校验 | ✅ 开发项 | S3 AI/工具同样 |
| 热保护 2h | ✅ | 保持；模型目录建议「24h 内有写入则跳过/警告」 |
| 黑名单 | ✅ | 保持 |
| 可解释规则 ID | 弱 | **每条必须带 ruleId + 规则包版本** |
| 样本清单 | 无 | S2/S3 执行前展示 TOP 路径 |
| dry-run | 有 | S2 强确认前必须预览体积/样本 |
| 活跃工作区保护 | 无 | **已决：P1 同期做** 当前打开项目路径前缀 → S0 |

---

## 8. 分阶段落地（仍不写代码，只排期）

| 阶段 | 内容 | 验收 |
|------|------|------|
| **P0 设计冻结** | 本文 v0.2 + 已决 5 项 | ✅ 决策已写入 |
| **P1 只读发现** | 扫描+展示体积/ruleId/**不能删**；分组名「工具/AI 缓存」；**当前项目路径 S0** | 单独发一版目视验收识别对错 |
| **P2 S3 包缓存+编辑器 Cache** | npm/yarn/**pnpm**/pip/gradle/nuget/**cargo** + Electron Cache 子树可回收 | 默认不勾；误删=0；可回收站找回 |
| **P3 S2 模型强确认回收** | Ollama/HF/Torch/LM Studio：展示+打开+**强确认进回收站** | 无默认勾选；确认词/勾选风险；活动日志记 ruleId |
| **P4 规则包迭代** | 按反馈加新 Agent；升/降档 | 新平台靠规则包，不改核心引擎 |

**明确：P1 未完成前，不对工具/AI 项执行真实删除。**  
**优先级：** 主干 A/B（构建+包缓存）> 通用 Cache 模式 > 厂商 AI 表。

---

## 9. v1 推荐「做 / 不做」清单

### 做（已决）

- **主干：** 现有构建产物规则保持并强化可解释  
- 编辑器 **Cache / Code Cache / GPUCache / logs**（通用模式）→ S3  
- npm / yarn / **pnpm-store** / pip / gradle / nuget / **cargo registry+git** → S3（默认不勾）  
- Ollama / HF / Torch / LM Studio → **S2 + 强确认可回收**  
- 规则 ID + 原因文案 + 打开目录  
- **当前打开项目路径 → S0**  
- **P1 只读包**单独验收  

### 不做（v1）

- 项目内 `.cursor` 规则/配置一键删  
- Docker 数据盘一键删  
- 任意名为 `models` 的目录  
- 用户 `Documents` 下模糊匹配  
- 对话历史 storage 进 S3  
- 未收录新 Agent 的猜测删除  

---

## 10. 误判场景与对策

| 场景 | 风险 | 对策 |
|------|------|------|
| 用户把模型手拷到 `Downloads\models` | 误当 AI 缓存 | 仅匹配**厂商约定根**，不匹配裸 `models` |
| HF 缓存里有未提交的本地改动 | 误删 | S2 + 强确认；不升 S3 |
| pnpm 全局 store 被清 | 所有项目重下 | **S3 但默认不勾** + 文案；用户黑名单 |
| cargo registry 被清 | 编译全量重下 | 同上；仅 registry/git 子树 |
| Electron `User Data` 整目录当 Cache | 毁掉配置/登录 | **只允许 Cache 子树 S3**，User 级 S2/S0 |
| 公司内网包缓存 | 清后无法重下 | 黑名单 + 工作区 S0 |
| 路径中文/自定义 Ollama home | 漏检 | 读 `OLLAMA_MODELS` / `HF_HOME` 等（P3） |
| 明年新 Agent 未收录 | 漏检 | **可接受**；大文件审查兜底 + 规则包追加，不猜删 |

---

## 11. 与现有模块边界

| 模块 | 关系 |
|------|------|
| 清理中心 | 主入口：S3/S4 执行；S2 展示 |
| 文件审查 | S2 可「在审查中打开该树」 |
| 深度分析·归因 | 可增加「工具/AI 缓存」区域条（只读） |
| 活动日志 | 记录 ruleId、路径、回收结果 |
| 注册表专家模式 | **无关**；不复用专家门闩语义，避免概念混淆 |

---

## 12. 已决事项（v0.2）

| # | 问题 | 决定 |
|---|------|------|
| 1 | Ollama/HF 等模型 | **S2 + 强确认后可进回收站**（无默认勾选） |
| 2 | pnpm-store / cargo registry | **坚持 S3**（默认不勾 + 风险文案） |
| 3 | P1 只读发现 | **单独发一版**做识别验收 |
| 4 | 分组名称 | **`工具/AI 缓存`** |
| 5 | 当前项目路径 | **自动 S0 保护**（与 P1 同期） |

**战略已决：** 识别以**编程框架构建产物 + 包管理器缓存**为长期主干；AI 厂商路径为**可更新规则包枝干**，不承诺覆盖未来所有新平台。

### 产品答疑（2026-07 补充）

**Q: 软件装在 D/E，缓存是不是还在 C？**  
多数是。安装目录在 D/E 时，Windows 用户级缓存仍常在 `C:\Users\…\AppData` 与 `C:\Users\…\.xxx`（npm/cargo/ollama/编辑器 Cache 等）。故清理中心看 **C:** 才有工具/AI 体积；D/E 接近 0B 往往正常。

**Q: Cursor / Codex / OpenCode / Windsurf / Claude… 要不要每个都写规则？**  
**不必作为主干。** 优先：  
1) 框架构建产物（node_modules/target…）；  
2) 包管理器缓存（npm/pnpm/cargo…）；  
3) **通用 Electron Cache 子树**（不依赖厂商名也能扫到 Cache/Code Cache/GPUCache）；  
厂商专用表只作枝干补漏，允许滞后。

**Q: S0 还要再放宽吗？**  
**不要再放宽到「几乎不保护」。** 0.2.1 已收紧：禁止 Users/用户主目录当 S0。保持：  
- 足够深的项目路径才保护；  
- 用户黑名单强制保护；  
- 工具缓存根（AppData 浅层）永不因 TOP 被整树 S0。  
再放宽只会增加误删当前工程风险。
---

## 13. 一句话总结

> **主干（构建产物 / 包缓存）用稳定邻域算法，长期可靠；  
> 枝干（工具/AI）用规则包 + 通用 Cache/环境变量模式，允许滞后；  
> 模型可强确认回收，构建/包缓存可 S3 但默认不勾；  
> 当前项目与源码 S0；先 P1 只读，再开放删除。**

---

## 修订记录

| 版本 | 日期 | 说明 |
|------|------|------|
| v0.1 | 2026-07-18 | 初稿：分级、规则表、判定树、分阶段、待决问题 |
| v0.2 | 2026-07-18 | 主干/枝干战略；规则包；并入 5 项产品决策；P1–P4 调整 |
