<script setup lang="ts">
import { computed, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import {
  AlertTriangle,
  ArchiveRestore,
  Check,
  Database,
  DatabaseSearch,
  FolderOpen,
  LoaderCircle,
  RefreshCw,
  ShieldCheck,
  Wrench,
  X,
} from '@lucide/vue'

interface RegistryIssue {
  id: string
  category: string
  name: string
  keyPath: string
  valueName?: string | null
  data: string
  reason: string
  risk: 'low' | 'review'
  fixable: boolean
}

interface RegistryReport {
  items: RegistryIssue[]
  scannedKeys: number
  fixableCount: number
  reviewCount: number
  elapsedMs: number
}

interface RegistryRepairResult {
  repaired: number
  failed: number
  backupDirectory: string
}

const isTauri = '__TAURI_INTERNALS__' in window
const report = ref<RegistryReport | null>(null)
const scanning = ref(false)
const repairing = ref(false)
const selectedIds = ref<string[]>([])
const confirmRepair = ref(false)
const error = ref('')
const notice = ref('')
const backupDirectory = ref('')

const fixableItems = computed(() => report.value?.items.filter(item => item.fixable) ?? [])
const selectedItems = computed(() => fixableItems.value.filter(item => selectedIds.value.includes(item.id)))
const allFixableSelected = computed(() => fixableItems.value.length > 0 && fixableItems.value.every(item => selectedIds.value.includes(item.id)))

function buildPreview(): RegistryReport {
  return {
    scannedKeys: 38,
    fixableCount: 2,
    reviewCount: 1,
    elapsedMs: 84,
    items: [
      { id: 'preview-1', category: '失效启动项', name: 'OldSyncAgent', keyPath: 'HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run', valueName: 'OldSyncAgent', data: 'C:\\Program Files\\OldSync\\agent.exe --background', reason: '启动目标不存在：C:\\Program Files\\OldSync\\agent.exe', risk: 'low', fixable: true },
      { id: 'preview-2', category: '失效应用路径', name: 'retired-tool.exe', keyPath: 'HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\App Paths\\retired-tool.exe', data: 'D:\\Tools\\retired-tool.exe', reason: '登记的应用程序不存在：D:\\Tools\\retired-tool.exe', risk: 'low', fixable: true },
      { id: 'preview-3', category: '卸载信息残留', name: 'Legacy Photo Tool', keyPath: 'HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\LegacyPhoto', data: 'C:\\LegacyPhoto\\uninstall.exe', reason: '安装目录和卸载程序均不存在，建议先确认软件确已移除', risk: 'review', fixable: false },
    ],
  }
}

async function scanRegistry(clearMessages = true) {
  scanning.value = true
  if (clearMessages) {
    error.value = ''
    notice.value = ''
  }
  try {
    report.value = isTauri ? await invoke<RegistryReport>('analyze_registry') : buildPreview()
    selectedIds.value = report.value.items.filter(item => item.fixable).map(item => item.id)
  } catch (value) {
    error.value = String(value)
  } finally {
    scanning.value = false
  }
}

function toggleSelection(id: string) {
  selectedIds.value = selectedIds.value.includes(id)
    ? selectedIds.value.filter(value => value !== id)
    : [...selectedIds.value, id]
}

function toggleAllFixable() {
  selectedIds.value = allFixableSelected.value ? [] : fixableItems.value.map(item => item.id)
}

async function runRepair() {
  if (!selectedIds.value.length) return
  repairing.value = true
  error.value = ''
  try {
    if (!isTauri) {
      backupDirectory.value = 'C:\\Users\\User\\AppData\\Local\\DiskAnalyzer\\registry-backups\\20260715-180000'
      notice.value = `界面预览：已备份并修复 ${selectedIds.value.length} 项。`
      confirmRepair.value = false
      return
    }
    const result = await invoke<RegistryRepairResult>('repair_registry', { ids: [...selectedIds.value] })
    backupDirectory.value = result.backupDirectory
    confirmRepair.value = false
    notice.value = `已修复 ${result.repaired} 项${result.failed ? `，${result.failed} 项未能处理` : ''}。注册表备份已保存。`
    await scanRegistry(false)
  } catch (value) {
    error.value = String(value)
  } finally {
    repairing.value = false
  }
}

async function openBackup() {
  if (!backupDirectory.value || !isTauri) return
  try {
    await invoke('open_in_explorer', { path: backupDirectory.value, selectFile: false })
  } catch (value) {
    error.value = String(value)
  }
}
</script>

<template>
  <div class="registry-center">
    <div v-if="error" class="registry-alert error"><AlertTriangle :size="17" /><span>{{ error }}</span><button title="关闭" @click="error = ''"><X :size="16" /></button></div>
    <div v-if="notice" class="registry-alert notice"><Check :size="17" /><span>{{ notice }}</span><button v-if="backupDirectory" class="backup-link" @click="openBackup"><FolderOpen :size="15" /> 查看备份</button><button title="关闭" @click="notice = ''"><X :size="16" /></button></div>

    <section class="registry-hero panel">
      <span class="registry-hero-icon"><Database :size="25" /></span>
      <div><span class="panel-kicker">Windows 当前用户</span><h2>注册表健康检查</h2><p>仅检查能够验证目标已失效的项目，不扫描系统核心键，不做“无效键数量”营销式扩大。</p></div>
      <button class="button primary" :disabled="scanning || repairing" @click="scanRegistry()"><LoaderCircle v-if="scanning" :size="16" class="spin" /><DatabaseSearch v-else :size="16" /> {{ report ? '重新检查' : '开始检查' }}</button>
    </section>

    <section class="registry-boundary">
      <div><ShieldCheck :size="18" /><span><b>低风险自动范围</b><small>失效的用户启动项、失效的 App Paths</small></span></div>
      <div><ArchiveRestore :size="18" /><span><b>修复前强制备份</b><small>完整导出涉及的注册表分支为 .reg 文件</small></span></div>
      <div><AlertTriangle :size="18" /><span><b>只建议、不自动处理</b><small>卸载残留需要人工确认软件确已移除</small></span></div>
    </section>

    <section v-if="scanning" class="registry-loading panel"><LoaderCircle :size="30" class="spin" /><h2>正在检查注册表</h2><p>读取当前用户启动项、应用路径和卸载信息，不需要管理员权限。</p></section>

    <template v-else-if="report">
      <section class="registry-metrics">
        <div><small>已检查键</small><b>{{ report.scannedKeys }}</b><span>{{ report.elapsedMs }} ms</span></div>
        <div class="safe"><small>可安全修复</small><b>{{ report.fixableCount }}</b><span>目标已确认不存在</span></div>
        <div class="review"><small>建议人工复核</small><b>{{ report.reviewCount }}</b><span>不会自动修改</span></div>
      </section>

      <section class="registry-results panel">
        <header><div><span class="panel-kicker">检查结果</span><h2>{{ report.items.length ? `${report.items.length} 个项目` : '未发现明确问题' }}</h2></div><div class="registry-result-actions"><button class="text-button" :disabled="!fixableItems.length" @click="toggleAllFixable"><Check :size="15" /> {{ allFixableSelected ? '取消全选' : '选择低风险项' }}</button><button class="button repair-button" :disabled="!selectedIds.length" @click="confirmRepair = true"><Wrench :size="16" /> 备份并修复 · {{ selectedIds.length }}</button></div></header>
        <div v-if="report.items.length" class="registry-rows">
          <article v-for="item in report.items" :key="item.id" class="registry-row" :class="{ selected: selectedIds.includes(item.id), review: !item.fixable }">
            <button v-if="item.fixable" class="registry-check" :class="{ checked: selectedIds.includes(item.id) }" :title="selectedIds.includes(item.id) ? '取消选择' : '选择修复'" @click="toggleSelection(item.id)"><Check v-if="selectedIds.includes(item.id)" :size="14" /></button>
            <span v-else class="registry-review-mark"><AlertTriangle :size="15" /></span>
            <div class="registry-copy"><div><b>{{ item.name }}</b><span :class="item.fixable ? 'safe-badge' : 'review-badge'">{{ item.category }}</span></div><p>{{ item.reason }}</p><small :title="item.keyPath">{{ item.keyPath }}{{ item.valueName ? ` · ${item.valueName}` : '' }}</small><code :title="item.data">{{ item.data }}</code></div>
            <span class="registry-action-label">{{ item.fixable ? '可备份修复' : '仅提供建议' }}</span>
          </article>
        </div>
        <div v-else class="registry-empty"><ShieldCheck :size="42" /><h2>当前检查范围没有发现问题</h2><p>注册表项目越少不代表电脑会明显变快；保持稳定比追求“清理数量”更重要。</p></div>
        <footer><ShieldCheck :size="16" /><span>不会清理系统服务、驱动、COM、文件关联或共享 DLL，也不会为了增加结果数量猜测无效项。</span></footer>
      </section>
    </template>

    <section v-else class="registry-welcome panel"><div><DatabaseSearch :size="44" /></div><h2>先做一次保守检查</h2><p>检查结果分为“可安全修复”和“建议人工复核”。修复前会先创建可双击恢复的注册表备份。</p><button class="button primary" @click="scanRegistry()"><DatabaseSearch :size="17" /> 开始注册表检查</button></section>

    <div v-if="confirmRepair" class="registry-modal" @click.self="confirmRepair = false"><section class="repair-dialog" role="dialog" aria-modal="true" aria-label="确认注册表修复"><span><ArchiveRestore :size="27" /></span><h2>备份并修复 {{ selectedItems.length }} 个项目？</h2><p>应用会先将涉及的注册表分支导出到本地备份目录。只有备份全部成功后，才会移除已确认目标不存在的用户级记录。</p><div><button class="button secondary" :disabled="repairing" @click="confirmRepair = false">取消</button><button class="button repair-button" :disabled="repairing" @click="runRepair"><LoaderCircle v-if="repairing" :size="16" class="spin" /><Wrench v-else :size="16" /> 确认备份并修复</button></div></section></div>
  </div>
</template>

<style scoped>
.registry-center{display:grid;gap:14px}.registry-alert{min-height:42px;padding:9px 12px;border-radius:5px;display:flex;align-items:center;gap:9px}.registry-alert>span{flex:1}.registry-alert>button:not(.backup-link){border:0;background:transparent;color:inherit;display:grid;place-items:center}.registry-alert.error{background:#fff0f0;border:1px solid #ffcaca;color:#a52b2b}.registry-alert.notice{background:#effaf6;border:1px solid #ccecdf;color:#087355}.backup-link{height:30px;border:1px solid #a9ddcb;background:#fff;border-radius:4px;color:#087355;display:flex;align-items:center;gap:6px;padding:0 9px}.registry-hero{display:grid;grid-template-columns:48px minmax(0,1fr) auto;align-items:center;gap:14px}.registry-hero-icon{width:48px;height:48px;border-radius:6px;background:var(--accent-soft);color:var(--accent-ink);display:grid;place-items:center}.registry-hero h2{margin:3px 0;font-size:16px}.registry-hero p{margin:0;color:#667085}.registry-boundary{display:grid;grid-template-columns:repeat(3,1fr);gap:10px}.registry-boundary>div{min-height:70px;padding:13px 15px;background:#fff;border:1px solid #e1e5e9;border-radius:6px;display:flex;align-items:flex-start;gap:10px}.registry-boundary>div:nth-child(1)>svg{color:#12a47b}.registry-boundary>div:nth-child(2)>svg{color:#3182f6}.registry-boundary>div:nth-child(3)>svg{color:#e18a00}.registry-boundary b,.registry-boundary small{display:block}.registry-boundary small{margin-top:4px;color:#7d8896;line-height:1.45}.registry-loading,.registry-welcome{min-height:390px;display:flex;flex-direction:column;align-items:center;justify-content:center;text-align:center}.registry-loading>svg{color:var(--accent)}.registry-loading h2,.registry-welcome h2{margin:14px 0 6px}.registry-loading p,.registry-welcome p{max-width:590px;color:#667085;line-height:1.7;margin:0 0 18px}.registry-welcome>div{width:86px;height:86px;border-radius:50%;background:var(--accent-soft);color:var(--accent-ink);display:grid;place-items:center}.registry-metrics{display:grid;grid-template-columns:repeat(3,1fr);background:#fff;border:1px solid #dfe3e8;border-radius:6px}.registry-metrics>div{min-height:90px;padding:15px 19px;border-right:1px solid #e7eaee}.registry-metrics>div:last-child{border:0}.registry-metrics small,.registry-metrics b,.registry-metrics span{display:block}.registry-metrics small,.registry-metrics span{color:#7d8896}.registry-metrics b{font-size:21px;margin:4px 0}.registry-metrics .safe b{color:#0b8d69}.registry-metrics .review b{color:#d18400}.registry-results{padding:0;overflow:hidden}.registry-results>header{min-height:68px;padding:13px 17px;border-bottom:1px solid #e4e7eb;display:flex;align-items:center;justify-content:space-between;gap:16px}.registry-results h2{margin:3px 0}.registry-result-actions{display:flex;align-items:center;gap:8px}.registry-result-actions .text-button{display:flex;align-items:center;gap:6px}.repair-button{height:38px;border:0;border-radius:5px;background:#eaf8f3;color:#087355;display:inline-flex;align-items:center;justify-content:center;gap:7px;padding:0 13px;font-weight:650;white-space:nowrap}.repair-button:hover{background:#d9f2e8}.repair-button:disabled{opacity:.45;cursor:not-allowed}.registry-row{min-height:100px;padding:13px 17px;display:grid;grid-template-columns:22px minmax(0,1fr) 100px;gap:12px;align-items:center;border-bottom:1px solid #edf0f2}.registry-row.selected{background:var(--accent-soft)}.registry-row.review{background:#fffdf8}.registry-check{width:20px;height:20px;padding:0;border:1px solid #b8c0ca;background:#fff;border-radius:4px;color:#fff;display:grid;place-items:center}.registry-check.checked{background:var(--accent);border-color:var(--accent)}.registry-review-mark{width:22px;height:22px;border-radius:4px;background:#fff2d9;color:#bb7600;display:grid;place-items:center}.registry-copy{min-width:0}.registry-copy>div{display:flex;align-items:center;gap:8px}.registry-copy>div span{padding:2px 6px;border-radius:3px}.safe-badge{background:#eaf8f3;color:#087355}.review-badge{background:#fff2d9;color:#9a6500}.registry-copy p{margin:5px 0;color:#475467}.registry-copy small,.registry-copy code{display:block;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.registry-copy small{color:#8b95a3}.registry-copy code{margin-top:5px;color:#667085;font:inherit;font-size:.92em}.registry-action-label{text-align:right;color:#667085}.registry-row.review .registry-action-label{color:#9a6500}.registry-empty{min-height:310px;display:flex;flex-direction:column;align-items:center;justify-content:center;text-align:center}.registry-empty>svg{color:#12a47b}.registry-empty h2{margin:13px 0 5px}.registry-empty p{color:#667085;margin:0}.registry-results>footer{min-height:44px;padding:10px 16px;background:#f7faf9;color:#4d7f6d;display:flex;align-items:center;gap:8px}.registry-modal{position:fixed;inset:0;background:#10182880;z-index:60;display:grid;place-items:center;padding:20px}.repair-dialog{width:min(460px,100%);background:#fff;border-radius:7px;padding:26px;box-shadow:0 22px 60px #10182855;text-align:center}.repair-dialog>span{width:54px;height:54px;border-radius:7px;background:#eaf4ff;color:#3182f6;display:grid;place-items:center;margin:auto}.repair-dialog h2{margin:15px 0 7px}.repair-dialog p{color:#667085;line-height:1.65;margin:0}.repair-dialog>div{display:flex;justify-content:center;gap:8px;margin-top:20px}.spin{animation:spin 1s linear infinite}@keyframes spin{to{transform:rotate(360deg)}}
@media(max-width:980px){.registry-boundary{grid-template-columns:1fr}.registry-results>header{align-items:flex-start;flex-direction:column}.registry-result-actions{width:100%}.registry-result-actions .repair-button{margin-left:auto}}
@media(max-width:800px){.registry-hero{grid-template-columns:44px 1fr}.registry-hero>.button{grid-column:1 / -1}.registry-metrics{grid-template-columns:1fr}.registry-metrics>div{border-right:0;border-bottom:1px solid #e7eaee}.registry-row{grid-template-columns:22px minmax(0,1fr)}.registry-action-label{grid-column:2;text-align:left}.registry-result-actions{flex-wrap:wrap}.registry-result-actions .repair-button{margin-left:0;flex:1}}
</style>
