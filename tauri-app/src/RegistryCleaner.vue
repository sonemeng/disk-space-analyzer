<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { open } from '@tauri-apps/plugin-dialog'
import { appendActivity, formatDetailLines, isDetailedActivityLogEnabled } from './activityLog'
import {
  AlertTriangle,
  ArchiveRestore,
  Ban,
  Check,
  ChevronRight,
  Database,
  DatabaseSearch,
  FolderOpen,
  History,
  LoaderCircle,
  Plus,
  RotateCcw,
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
  risk: 'low' | 'medium' | 'review' | 'critical'
  fixable: boolean
}

interface RegistryCategorySummary {
  id: string
  name: string
  total: number
  fixable: number
  review: number
}

interface RegistryReport {
  items: RegistryIssue[]
  categories: RegistryCategorySummary[]
  scannedKeys: number
  fixableCount: number
  reviewCount: number
  elapsedMs: number
  userScope: string
  elevated: boolean
}

interface RegistryRepairResult {
  repaired: number
  failed: number
  backupDirectory: string
}

interface RegistryBackupInfo {
  id: string
  path: string
  label: string
  createdAt: string
  fileCount: number
  kind: string
}

type CategoryId =
  | 'startup'
  | 'app-path'
  | 'uninstall'
  | 'history'
  | 'user-command'
  | 'help'
  | 'fonts'
  | 'sound'
  | 'com-server'
  | 'shared-dll'
  | 'file-assoc'
  | 'service'
  | 'driver'
type UserScope = 'current' | 'all'
type ScanMode = 'basic' | 'advanced' | 'expert'

const BASIC_CATEGORY_OPTIONS: Array<{ id: CategoryId; label: string }> = [
  { id: 'startup', label: '无效的启动程序' },
  { id: 'app-path', label: '无效的应用程序路径' },
  { id: 'uninstall', label: '无效的卸载程序' },
  { id: 'history', label: '无效的历史记录' },
  { id: 'user-command', label: '无效的文件类型' },
  { id: 'help', label: '无效的帮助文件' },
  { id: 'fonts', label: '无效的字体' },
  { id: 'sound', label: '无效的声音事件' },
]

const ADVANCED_ONLY_OPTIONS: Array<{ id: CategoryId; label: string }> = [
  { id: 'com-server', label: '无效的 ActiveX/COM' },
  { id: 'shared-dll', label: '无效的共享 DLL' },
]

const EXPERT_ONLY_OPTIONS: Array<{ id: CategoryId; label: string }> = [
  { id: 'file-assoc', label: '无效的文件关联' },
  { id: 'service', label: '无效的服务（只读）' },
  { id: 'driver', label: '无效的驱动程序（只读）' },
]

const STORAGE_KEY = 'disk-analyzer-registry-prefs'
const LAST_FULL_BACKUP_KEY = 'disk-analyzer-registry-last-full-backup'
const BACKUP_WARN_DAYS = 180
const isTauri = '__TAURI_INTERNALS__' in window

const report = ref<RegistryReport | null>(null)
const scanning = ref(false)
const repairing = ref(false)
const backingUp = ref(false)
const restoring = ref(false)
const selectedIds = ref<string[]>([])
const confirmRepair = ref(false)
const expertConfirmText = ref('')
const confirmRestore = ref<RegistryBackupInfo | null>(null)
const error = ref('')
const notice = ref('')
const backupDirectory = ref('')
const backups = ref<RegistryBackupInfo[]>([])
const activeCategory = ref<string | null>(null)
const resultRiskFilter = ref<'all' | 'low' | 'high' | 'critical' | 'review'>('all')
const resultScopeFilter = ref<'all' | 'hkcu' | 'hklm'>('all')
const resultQuery = ref('')
const expertGateAck = ref(false)
const showExpertGate = ref(false)
const showRepairLog = ref(false)
interface RepairLogEntry {
  id: string
  at: string
  mode: string
  repaired: number
  failed: number
  backupDirectory: string
  names: string[]
}
const repairLog = ref<RepairLogEntry[]>([])
const userScope = ref<UserScope>('current')
const scanMode = ref<ScanMode>('basic')
const showCategoryMenu = ref(false)
const showBackupMenu = ref(false)

const CATEGORY_OPTIONS = computed(() => {
  if (scanMode.value === 'expert') {
    return [...BASIC_CATEGORY_OPTIONS, ...ADVANCED_ONLY_OPTIONS, ...EXPERT_ONLY_OPTIONS]
  }
  if (scanMode.value === 'advanced') {
    return [...BASIC_CATEGORY_OPTIONS, ...ADVANCED_ONLY_OPTIONS]
  }
  return BASIC_CATEGORY_OPTIONS
})

/** 选中的检查分类（默认全选基础分类） */
const selectedCategories = ref<CategoryId[]>(BASIC_CATEGORY_OPTIONS.map(item => item.id))
const exclusions = ref<string[]>([])
const exclusionInput = ref('')
const lastFullBackupAt = ref<string | null>(null)
const progress = ref({ message: '等待开始检查', percentage: 0 })
let unlistenProgress: UnlistenFn | undefined

const enabledCategories = computed(() => {
  const allowed = new Set(CATEGORY_OPTIONS.value.map(item => item.id))
  return selectedCategories.value.filter(id => allowed.has(id))
})
const categoryLabel = computed(() => {
  const opts = CATEGORY_OPTIONS.value
  if (enabledCategories.value.length === opts.length) return '全部分类'
  if (enabledCategories.value.length === 0) return '请选择分类'
  if (enabledCategories.value.length === 1) {
    return opts.find(item => item.id === enabledCategories.value[0])?.label || '1 个分类'
  }
  return `已选 ${enabledCategories.value.length} 个分类`
})

const fixableItems = computed(() => report.value?.items.filter(item => item.fixable) ?? [])
const filteredItems = computed(() => {
  if (!report.value) return []
  let list = report.value.items
  if (activeCategory.value) {
    const name = report.value.categories.find(c => c.id === activeCategory.value)?.name
    if (name) list = list.filter(item => item.category === name)
  }
  if (resultRiskFilter.value === 'low') {
    list = list.filter(item => item.fixable && !isHighRiskItem(item))
  } else if (resultRiskFilter.value === 'high') {
    list = list.filter(item => item.fixable && isHighRiskItem(item) && !isCriticalItem(item))
  } else if (resultRiskFilter.value === 'critical') {
    list = list.filter(item => isCriticalItem(item))
  } else if (resultRiskFilter.value === 'review') {
    list = list.filter(item => !item.fixable)
  }
  if (resultScopeFilter.value === 'hkcu') {
    list = list.filter(item => item.keyPath.startsWith('HKCU') || item.keyPath.startsWith('HKEY_CURRENT_USER'))
  } else if (resultScopeFilter.value === 'hklm') {
    list = list.filter(item => item.keyPath.startsWith('HKLM') || item.keyPath.startsWith('HKEY_LOCAL_MACHINE'))
  }
  const needle = resultQuery.value.trim().toLowerCase()
  if (needle) {
    list = list.filter(item =>
      `${item.name}\n${item.keyPath}\n${item.data}\n${item.reason}\n${item.category}`.toLowerCase().includes(needle),
    )
  }
  return list
})
const selectedItems = computed(() => fixableItems.value.filter(item => selectedIds.value.includes(item.id)))
const selectedHighRiskItems = computed(() => selectedItems.value.filter(item => isHighRiskItem(item)))
const selectedCriticalItems = computed(() => selectedItems.value.filter(item => isCriticalItem(item)))
const expertConfirmOk = computed(() => {
  if (!selectedCriticalItems.value.length) return true
  return expertConfirmText.value.trim() === '确认删除高风险项'
})
const allFixableSelected = computed(() => {
  const visibleFixable = filteredItems.value.filter(item => item.fixable)
  return visibleFixable.length > 0 && visibleFixable.every(item => selectedIds.value.includes(item.id))
})
const canScan = computed(() => enabledCategories.value.length > 0)
const needsFullBackup = computed(() => {
  if (!lastFullBackupAt.value) return true
  const t = Date.parse(lastFullBackupAt.value)
  if (Number.isNaN(t)) return true
  return Date.now() - t > BACKUP_WARN_DAYS * 24 * 60 * 60 * 1000
})
const lastFullBackupLabel = computed(() => {
  if (!lastFullBackupAt.value) return '尚未完整备份'
  const t = Date.parse(lastFullBackupAt.value)
  if (Number.isNaN(t)) return lastFullBackupAt.value
  return new Date(t).toLocaleString('zh-CN')
})

function loadPrefs() {
  try {
    const raw = JSON.parse(localStorage.getItem(STORAGE_KEY) || '{}') as {
      categories?: string[]
      userScope?: UserScope
      mode?: ScanMode
      exclusions?: string[]
      expertGateAck?: boolean
    }
    if (raw.mode === 'advanced' || raw.mode === 'basic' || raw.mode === 'expert') scanMode.value = raw.mode
    if (Array.isArray(raw.categories) && raw.categories.length) {
      const allowed = new Set(
        [...BASIC_CATEGORY_OPTIONS, ...ADVANCED_ONLY_OPTIONS, ...EXPERT_ONLY_OPTIONS].map(item => item.id),
      )
      selectedCategories.value = raw.categories.filter((id): id is CategoryId => allowed.has(id as CategoryId))
      if (!selectedCategories.value.length) {
        selectedCategories.value = BASIC_CATEGORY_OPTIONS.map(item => item.id)
      }
    }
    if (raw.userScope === 'all' || raw.userScope === 'current') userScope.value = raw.userScope
    if (Array.isArray(raw.exclusions)) {
      exclusions.value = raw.exclusions.filter(value => typeof value === 'string' && value.trim())
    }
    if (typeof raw.expertGateAck === 'boolean') expertGateAck.value = raw.expertGateAck
  } catch {
    localStorage.removeItem(STORAGE_KEY)
  }
  lastFullBackupAt.value = localStorage.getItem(LAST_FULL_BACKUP_KEY)
  try {
    const logs = JSON.parse(localStorage.getItem('disk-analyzer-registry-repair-log') || '[]') as RepairLogEntry[]
    if (Array.isArray(logs)) repairLog.value = logs.slice(0, 50)
  } catch {
    repairLog.value = []
  }
}

function persistPrefs() {
  localStorage.setItem(STORAGE_KEY, JSON.stringify({
    categories: selectedCategories.value,
    userScope: userScope.value,
    mode: scanMode.value,
    exclusions: exclusions.value,
    expertGateAck: expertGateAck.value,
  }))
}

function persistRepairLog() {
  localStorage.setItem('disk-analyzer-registry-repair-log', JSON.stringify(repairLog.value.slice(0, 50)))
}

watch([selectedCategories, userScope, scanMode, exclusions, expertGateAck], persistPrefs, { deep: true })

watch(scanMode, mode => {
  if (mode === 'expert') {
    selectedCategories.value = CATEGORY_OPTIONS.value.map(item => item.id)
    if (!expertGateAck.value || needsFullBackup.value) {
      showExpertGate.value = true
    }
    notice.value = '已切换专家模式：含服务/驱动只读、文件关联与 SharedDLL 极严格策略。首次使用需确认门闩并建议完整备份。'
  } else if (mode === 'advanced') {
    selectedCategories.value = CATEGORY_OPTIONS.value.map(item => item.id)
    notice.value = '已切换进阶模式：默认全部分类；HKLM/COM/SharedDLL 等。高风险橙色，默认不勾选，可手动勾选并二次确认。'
  } else {
    selectedCategories.value = BASIC_CATEGORY_OPTIONS.map(item => item.id)
  }
  report.value = null
  resultRiskFilter.value = 'all'
  resultScopeFilter.value = 'all'
  resultQuery.value = ''
})

function acknowledgeExpertGate(withBackup: boolean) {
  expertGateAck.value = true
  showExpertGate.value = false
  if (withBackup) void createFullBackup(false)
  notice.value = withBackup
    ? '专家门闩已确认，并开始完整备份。备份完成后再扫描更安全。'
    : '专家门闩已确认。仍强烈建议先完整备份再扫描/修复。'
}

function pushRepairLog(entry: Omit<RepairLogEntry, 'id' | 'at'>) {
  const row: RepairLogEntry = {
    id: `${Date.now()}-${Math.random().toString(36).slice(2, 7)}`,
    at: new Date().toISOString(),
    ...entry,
  }
  repairLog.value = [row, ...repairLog.value].slice(0, 50)
  persistRepairLog()
  appendActivity(
    'registry',
    `注册表修复：成功 ${entry.repaired}，失败 ${entry.failed}（${entry.mode}）`,
    entry.names.join('、') || entry.backupDirectory,
    { backupDirectory: entry.backupDirectory, mode: entry.mode },
  )
}

function toggleCategory(id: CategoryId) {
  if (selectedCategories.value.includes(id)) {
    if (enabledCategories.value.length === 1 && enabledCategories.value[0] === id) return
    selectedCategories.value = selectedCategories.value.filter(item => item !== id)
  } else {
    selectedCategories.value = [...selectedCategories.value, id]
  }
}

function selectAllCategories() {
  selectedCategories.value = CATEGORY_OPTIONS.value.map(item => item.id)
}

function buildPreviewCategories(items: RegistryIssue[]): RegistryCategorySummary[] {
  return CATEGORY_OPTIONS.value
    .map(meta => {
      const group = items.filter(item => item.category === meta.label)
      const fixable = group.filter(item => item.fixable).length
      return {
        id: meta.id,
        name: meta.label,
        total: group.length,
        fixable,
        review: group.length - fixable,
      }
    })
    .filter(item => item.total > 0)
}

function buildPreview(): RegistryReport {
  const all: RegistryIssue[] = [
    { id: 'preview-1', category: '无效的启动程序', name: 'OldSyncAgent', keyPath: 'HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run', valueName: 'OldSyncAgent', data: 'C:\\Program Files\\OldSync\\agent.exe --background', reason: '启动目标不存在', risk: 'low', fixable: true },
    { id: 'preview-2', category: '无效的应用程序路径', name: 'retired-tool.exe', keyPath: 'HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\App Paths\\retired-tool.exe', data: 'D:\\Tools\\retired-tool.exe', reason: '登记的应用程序不存在', risk: 'low', fixable: true },
    { id: 'preview-3', category: '无效的卸载程序', name: 'Legacy Photo Tool', keyPath: 'HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\LegacyPhoto', data: 'C:\\LegacyPhoto\\uninstall.exe', reason: '安装目录和卸载程序均不存在', risk: 'review', fixable: false },
    { id: 'preview-4', category: '无效的历史记录', name: 'url1', keyPath: 'HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\TypedPaths', valueName: 'url1', data: 'D:\\OldFolder', reason: '输入过的路径已不存在', risk: 'low', fixable: true },
    { id: 'preview-5', category: '无效的文件类型', name: 'gone-app.exe', keyPath: 'HKCU\\Software\\Classes\\Applications\\gone-app.exe\\shell\\open\\command', data: '"C:\\Gone\\app.exe" "%1"', reason: '打开方式指向的程序不存在', risk: 'low', fixable: true },
    { id: 'preview-6', category: '无效的声音事件', name: 'Explorer / SystemAsterisk', keyPath: 'HKCU\\AppEvents\\Schemes\\Apps\\Explorer\\SystemAsterisk\\.Current', data: 'C:\\Windows\\Media\\missing.wav', reason: '声音文件不存在', risk: 'low', fixable: true },
    { id: 'preview-7', category: '无效的 ActiveX/COM', name: '{GUID} / InprocServer32', keyPath: 'HKLM\\Software\\Classes\\CLSID\\{GUID}\\InprocServer32', data: 'C:\\Gone\\com.dll', reason: 'COM 服务器文件不存在（进阶·仅建议）', risk: 'review', fixable: false },
    { id: 'preview-8', category: '无效的共享 DLL', name: 'old.dll', keyPath: 'HKLM\\Software\\Microsoft\\Windows\\CurrentVersion\\SharedDLLs', data: 'C:\\Windows\\System32\\old.dll (引用计数 1)', reason: 'SharedDLLs 登记的文件不存在（进阶·仅建议）', risk: 'review', fixable: false },
  ]
  if (scanMode.value === 'expert') {
    all.push(
      { id: 'preview-9', category: '无效的文件关联', name: '.old', keyPath: 'HKCU\\Software\\Classes\\.old\\shell\\open\\command', data: '"C:\\Gone\\open.exe" "%1"', reason: '文件关联打开命令目标不存在', risk: 'low', fixable: true },
      { id: 'preview-10', category: '无效的服务', name: '[服务] OldSvc', keyPath: 'HKLM\\SYSTEM\\CurrentControlSet\\Services\\OldSvc', data: 'C:\\Gone\\svc.exe', reason: '服务 ImagePath 指向不存在（专家·只读）', risk: 'critical', fixable: false },
      { id: 'preview-11', category: '无效的驱动程序', name: '[驱动] OldDrv', keyPath: 'HKLM\\SYSTEM\\CurrentControlSet\\Services\\OldDrv', data: 'C:\\Windows\\System32\\drivers\\old.sys', reason: '驱动 ImagePath 指向不存在（专家·只读）', risk: 'critical', fixable: false },
    )
  }
  const map: Record<string, CategoryId> = Object.fromEntries(
    [...BASIC_CATEGORY_OPTIONS, ...ADVANCED_ONLY_OPTIONS, ...EXPERT_ONLY_OPTIONS].map(item => [item.label, item.id]),
  ) as Record<string, CategoryId>
  let items = all.filter(item => {
    const id = map[item.category]
    if (!id) return false
    if (scanMode.value === 'basic' && (ADVANCED_ONLY_OPTIONS.some(a => a.id === id) || EXPERT_ONLY_OPTIONS.some(a => a.id === id))) return false
    if (scanMode.value === 'advanced' && EXPERT_ONLY_OPTIONS.some(a => a.id === id)) return false
    return enabledCategories.value.includes(id)
  })
  if (exclusions.value.length) {
    const needles = exclusions.value.map(value => value.toLowerCase())
    items = items.filter(item => {
      const hay = `${item.name}\n${item.keyPath}\n${item.data}`.toLowerCase()
      return !needles.some(needle => needle && hay.includes(needle))
    })
  }
  return {
    scannedKeys: scanMode.value === 'advanced' ? 4200 : 256,
    fixableCount: items.filter(item => item.fixable).length,
    reviewCount: items.filter(item => !item.fixable).length,
    elapsedMs: 96,
    items,
    categories: buildPreviewCategories(items),
    userScope: userScope.value,
    elevated: false,
  }
}

function addExclusion() {
  const value = exclusionInput.value.trim()
  if (!value) {
    error.value = '请先输入要排除的关键字（软件名或路径片段）'
    return
  }
  if (exclusions.value.some(entry => entry.toLowerCase() === value.toLowerCase())) {
    notice.value = `排除列表中已有：${value}`
    exclusionInput.value = ''
    return
  }
  exclusions.value = [...exclusions.value, value]
  exclusionInput.value = ''
  notice.value = `已加入排除：${value}`
  error.value = ''
}

function removeExclusion(value: string) {
  exclusions.value = exclusions.value.filter(entry => entry !== value)
}

function addExclusionFromItem(item: RegistryIssue) {
  const value = item.name.trim()
  if (!value) return
  if (!exclusions.value.some(entry => entry.toLowerCase() === value.toLowerCase())) {
    exclusions.value = [...exclusions.value, value]
    notice.value = `已加入排除列表：${value}（下次检查将跳过）`
  }
}

function selectCategory(id: string | null) {
  activeCategory.value = activeCategory.value === id ? null : id
}

function isHighRiskItem(item: RegistryIssue) {
  return item.risk === 'medium' || item.risk === 'critical' || item.keyPath.startsWith('HKLM') || item.keyPath.startsWith('HKEY_LOCAL_MACHINE')
}

function isCriticalItem(item: RegistryIssue) {
  return item.risk === 'critical' || item.category.includes('共享 DLL') || item.category.includes('服务') || item.category.includes('驱动')
}

function itemRiskBadge(item: RegistryIssue) {
  if (!item.fixable && isCriticalItem(item)) return { className: 'critical-badge', text: '极高风险·只读' }
  if (!item.fixable) return { className: 'review-badge', text: '仅建议' }
  if (item.risk === 'critical') return { className: 'critical-badge', text: '极高风险' }
  if (isHighRiskItem(item)) return { className: 'danger-badge', text: '高风险' }
  return { className: 'safe-badge', text: '低风险' }
}

function itemActionLabel(item: RegistryIssue) {
  if (!item.fixable) return '仅建议'
  if (isHighRiskItem(item)) return '可修·高风险'
  return '可备份修复'
}

function closeMenus() {
  showCategoryMenu.value = false
  showBackupMenu.value = false
}

function onDocumentPointerDown(event: MouseEvent) {
  const target = event.target as HTMLElement | null
  if (!target) return
  if (!target.closest('.dropdown') && !target.closest('.backup-dropdown')) {
    closeMenus()
  }
}

async function loadBackups() {
  try {
    const local = isTauri
      ? await invoke<RegistryBackupInfo[]>('list_registry_backups')
      : [{
          id: '20260715-180000',
          path: 'C:\\Users\\User\\AppData\\Local\\DiskAnalyzer\\registry-backups\\20260715-180000',
          label: '完整备份（修复前保护）',
          createdAt: '2026-07-15 18:00:00',
          fileCount: 8,
          kind: 'full',
        }]
    const external = loadRememberedExternalBackups()
    const seen = new Set<string>()
    const merged: RegistryBackupInfo[] = []
    for (const item of [...local, ...external]) {
      const key = item.path.toLowerCase()
      if (seen.has(key)) continue
      seen.add(key)
      merged.push(item)
    }
    backups.value = merged
  } catch {
    backups.value = loadRememberedExternalBackups()
  }
}

async function scanRegistry(clearMessages = true) {
  if (!canScan.value) {
    error.value = '请至少勾选一个检查分类'
    return
  }
  if (scanMode.value === 'expert' && (!expertGateAck.value || needsFullBackup.value)) {
    showExpertGate.value = true
    error.value = needsFullBackup.value
      ? '专家模式要求先完成完整备份（或确认门闩并备份）'
      : '请先确认专家模式门闩'
    return
  }
  scanning.value = true
  activeCategory.value = null
  resultRiskFilter.value = 'all'
  resultScopeFilter.value = 'all'
  resultQuery.value = ''
  progress.value = {
    message: scanMode.value === 'expert' ? '正在启动专家扫描…' : scanMode.value === 'advanced' ? '正在启动进阶扫描…' : '正在启动检查…',
    percentage: 2,
  }
  if (clearMessages) {
    error.value = ''
    notice.value = ''
  }
  try {
    report.value = isTauri
      ? await invoke<RegistryReport>('analyze_registry', {
          options: {
            categories: enabledCategories.value,
            exclusions: exclusions.value,
            userScope: userScope.value,
            mode: scanMode.value,
          },
        })
      : buildPreview()
    progress.value = { message: '检查完成', percentage: 100 }
    if (!report.value.categories) {
      report.value.categories = buildPreviewCategories(report.value.items)
    }
    // 默认只勾选低风险用户级项；进阶 medium/HKLM 需手动勾选
    selectedIds.value = report.value.items
      .filter(item => item.fixable && !isHighRiskItem(item))
      .map(item => item.id)

    const catLines = (report.value.categories || []).map(
      cat => `${cat.name}：${cat.total}（可修 ${cat.fixable} · 复核 ${cat.review}）`,
    )
    const sampleNames = report.value.items.slice(0, 12).map(item =>
      `${item.name} · ${item.category}${item.fixable ? '' : '（仅建议）'}`,
    )
    const summary = `模式 ${scanMode.value} · 命中 ${report.value.items.length} · 可修 ${report.value.fixableCount} · 复核 ${report.value.reviewCount} · ${report.value.scannedKeys} 键 · ${report.value.elapsedMs} ms`
    const detail = isDetailedActivityLogEnabled()
      ? formatDetailLines([
          summary,
          `范围：${report.value.userScope === 'all' ? '所有用户' : '当前用户'}${report.value.elevated ? ' · 已提升权限' : ''}`,
          '分类：',
          ...catLines,
          '样例：',
          ...sampleNames,
        ])
      : summary
    appendActivity(
      'registry',
      `注册表检查完成：命中 ${report.value.items.length} 项（可修 ${report.value.fixableCount}）`,
      detail,
      {
        mode: scanMode.value,
        total: report.value.items.length,
        fixable: report.value.fixableCount,
        scannedKeys: report.value.scannedKeys,
      },
    )

    if (userScope.value === 'all' && !report.value.elevated) {
      notice.value = '已选择「所有用户」，但当前未以管理员运行：仅扫描当前用户。请右键「以管理员身份运行」后可附加其他用户启动项（其它用户项仅建议、不自动修复）。'
    } else if (userScope.value === 'all' && report.value.elevated) {
      notice.value = '已在管理员模式下扫描：当前用户全部分类 + 其他用户启动项（其他用户结果仅建议）。'
    } else if (clearMessages) {
      notice.value = `检查完成：命中 ${report.value.items.length} 项（可修 ${report.value.fixableCount}）。点击通知可看详情。`
    }
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
  const visibleFixable = filteredItems.value.filter(item => item.fixable)
  const allOn = visibleFixable.length > 0 && visibleFixable.every(item => selectedIds.value.includes(item.id))
  if (allOn) {
    const drop = new Set(visibleFixable.map(item => item.id))
    selectedIds.value = selectedIds.value.filter(id => !drop.has(id))
  } else {
    const merge = new Set([...selectedIds.value, ...visibleFixable.map(item => item.id)])
    selectedIds.value = [...merge]
  }
}

async function createFullBackup(customDir = false) {
  showBackupMenu.value = false
  backingUp.value = true
  error.value = ''
  try {
    let destinationDir: string | null = null
    if (customDir) {
      if (!isTauri) {
        destinationDir = 'D:\\RegistryBackups'
      } else {
        const selected = await open({ directory: true, multiple: false, title: '选择注册表备份保存文件夹' })
        if (typeof selected !== 'string') {
          backingUp.value = false
          return
        }
        destinationDir = selected
      }
    }
    const info = isTauri
      ? await invoke<RegistryBackupInfo>('create_registry_backup', {
          label: customDir ? '完整备份（自选目录）' : '完整备份（用户手动）',
          destinationDir,
        })
      : {
          id: 'preview-full',
          path: destinationDir
            ? `${destinationDir}\\DiskAnalyzer-registry-preview`
            : 'C:\\Users\\User\\AppData\\Local\\DiskAnalyzer\\registry-backups\\preview',
          label: customDir ? '完整备份（自选目录）' : '完整备份（用户手动）',
          createdAt: new Date().toLocaleString('zh-CN'),
          fileCount: 8,
          kind: 'full',
        }
    const iso = new Date().toISOString()
    lastFullBackupAt.value = iso
    localStorage.setItem(LAST_FULL_BACKUP_KEY, iso)
    // 自选目录也记入本机列表，重装后仍可从「导入已有备份」找回
    if (customDir && info.path) rememberExternalBackup(info)
    backupDirectory.value = info.path
    notice.value = `完整备份已保存到：${info.path}（${info.fileCount} 个 .reg）。恢复 = 写回注册表原键。`
    appendActivity('backup', '注册表完整备份已保存', info.path, { fileCount: info.fileCount })
    await loadBackups()
  } catch (value) {
    error.value = String(value)
  } finally {
    backingUp.value = false
  }
}

const EXTERNAL_BACKUPS_KEY = 'disk-analyzer-registry-external-backups'

function rememberExternalBackup(info: RegistryBackupInfo) {
  try {
    const raw = JSON.parse(localStorage.getItem(EXTERNAL_BACKUPS_KEY) || '[]') as RegistryBackupInfo[]
    const next = [info, ...raw.filter(item => item.path !== info.path)].slice(0, 30)
    localStorage.setItem(EXTERNAL_BACKUPS_KEY, JSON.stringify(next))
  } catch {
    localStorage.setItem(EXTERNAL_BACKUPS_KEY, JSON.stringify([info]))
  }
}

function loadRememberedExternalBackups(): RegistryBackupInfo[] {
  try {
    const raw = JSON.parse(localStorage.getItem(EXTERNAL_BACKUPS_KEY) || '[]') as RegistryBackupInfo[]
    return Array.isArray(raw) ? raw.filter(item => item && typeof item.path === 'string') : []
  } catch {
    return []
  }
}

/** 重装软件后：手动选择以前的备份文件夹，加入列表并可用于恢复 */
async function importExistingBackup() {
  showBackupMenu.value = false
  error.value = ''
  try {
    if (!isTauri) {
      const demo: RegistryBackupInfo = {
        id: 'imported-demo',
        path: 'D:\\OldBackups\\DiskAnalyzer-registry-demo',
        label: '导入的旧备份（预览）',
        createdAt: new Date().toLocaleString('zh-CN'),
        fileCount: 5,
        kind: 'full',
      }
      rememberExternalBackup(demo)
      await loadBackups()
      notice.value = '界面预览：已导入备份路径到列表'
      return
    }
    const selected = await open({
      directory: true,
      multiple: false,
      title: '选择以前的备份文件夹（内含 .reg 与 backup.json）',
    })
    if (typeof selected !== 'string') return
    // 先尝试 restore 校验是否为本应用备份（不真恢复，只检测）——用 list 思路：直接加入，恢复时后端校验
    const info: RegistryBackupInfo = {
      id: selected.split(/[/\\]/).filter(Boolean).pop() || selected,
      path: selected,
      label: '导入的备份',
      createdAt: '',
      fileCount: 0,
      kind: 'full',
    }
    rememberExternalBackup(info)
    await loadBackups()
    notice.value = `已加入备份列表：${selected}。可点「恢复」写回注册表；若目录无效，恢复时会提示。`
  } catch (value) {
    error.value = String(value)
  }
}

async function runRestore() {
  if (!confirmRestore.value) return
  restoring.value = true
  error.value = ''
  try {
    const message = isTauri
      ? await invoke<string>('restore_registry_backup', { path: confirmRestore.value.path })
      : '界面预览：已模拟导入备份'
    notice.value = message
    confirmRestore.value = null
    await loadBackups()
  } catch (value) {
    error.value = String(value)
  } finally {
    restoring.value = false
  }
}

async function openPath(path: string) {
  if (!isTauri || !path) return
  try {
    await invoke('open_in_explorer', { path, selectFile: false })
  } catch (value) {
    error.value = String(value)
  }
}

async function runRepair() {
  if (!selectedIds.value.length) return
  if (!expertConfirmOk.value) {
    error.value = '删除极高风险项前，请输入确认词：确认删除高风险项'
    return
  }
  repairing.value = true
  error.value = ''
  try {
    if (!isTauri) {
      backupDirectory.value = 'C:\\Users\\User\\AppData\\Local\\DiskAnalyzer\\registry-backups\\20260715-180000'
      notice.value = `界面预览：已备份并修复 ${selectedIds.value.length} 项。`
      confirmRepair.value = false
      return
    }
    const names = selectedItems.value.map(item => item.name).slice(0, 20)
    const result = await invoke<RegistryRepairResult>('repair_registry', { ids: [...selectedIds.value] })
    backupDirectory.value = result.backupDirectory
    confirmRepair.value = false
    expertConfirmText.value = ''
    notice.value = `已修复 ${result.repaired} 项${result.failed ? `，${result.failed} 项未能处理` : ''}。注册表备份已保存。`
    pushRepairLog({
      mode: scanMode.value,
      repaired: result.repaired,
      failed: result.failed,
      backupDirectory: result.backupDirectory,
      names,
    })
    await Promise.all([scanRegistry(false), loadBackups()])
  } catch (value) {
    error.value = String(value)
  } finally {
    repairing.value = false
  }
}

onMounted(async () => {
  loadPrefs()
  await loadBackups()
  document.addEventListener('pointerdown', onDocumentPointerDown, true)
  if (isTauri) {
    unlistenProgress = await listen<{ message: string; percentage: number }>('registry-progress', event => {
      progress.value = {
        message: event.payload.message,
        percentage: event.payload.percentage ?? 0,
      }
    })
  }
})
onBeforeUnmount(() => {
  document.removeEventListener('pointerdown', onDocumentPointerDown, true)
  unlistenProgress?.()
})
</script>

<template>
  <div class="registry-page registry-center">
    <div v-if="error" class="registry-page registry-alert error"><AlertTriangle :size="17" /><span>{{ error }}</span><button title="关闭" @click="error = ''"><X :size="16" /></button></div>
    <div v-if="notice" class="registry-page registry-alert notice"><Check :size="17" /><span>{{ notice }}</span><button v-if="backupDirectory" class="backup-link" @click="openPath(backupDirectory)"><FolderOpen :size="15" /> 查看备份</button><button title="关闭" @click="notice = ''"><X :size="16" /></button></div>

    <section class="registry-page registry-hero panel">
      <span class="registry-page registry-hero-icon"><Database :size="25" /></span>
      <div>
        <span class="panel-kicker">注册表 · 可点分类看明细 · 备份可恢复</span>
        <h2>注册表健康检查</h2>
        <p>铁证路径失效才报项。可按用户范围与分类扫描；首次或久未备份时建议先完整备份。</p>
      </div>
      <button class="button primary" :disabled="scanning || repairing || !canScan" @click="scanRegistry()">
        <LoaderCircle v-if="scanning" :size="16" class="spin" />
        <DatabaseSearch v-else :size="16" />
        {{ report ? '重新检查' : '开始检查' }}
      </button>
    </section>

    <section v-if="needsFullBackup" class="backup-prompt panel">
      <div class="backup-prompt-copy">
        <ArchiveRestore :size="22" />
        <div>
          <b>建议先完整备份注册表</b>
          <small>首次使用，或距离上次完整备份已超过约半年（上次：{{ lastFullBackupLabel }}）。备份后可在下方一键恢复。</small>
        </div>
      </div>
      <button class="button secondary" :disabled="backingUp" @click="createFullBackup(false)">
        <LoaderCircle v-if="backingUp" :size="15" class="spin" />
        <ArchiveRestore v-else :size="15" />
        立即完整备份
      </button>
    </section>

    <section class="registry-page registry-options panel">
      <div class="options-toolbar">
        <label class="field">
          <span>扫描模式</span>
          <select v-model="scanMode" :disabled="scanning">
            <option value="basic">初级（安全·推荐）</option>
            <option value="advanced">进阶（更广·高风险需确认）</option>
            <option value="expert">专家（含服务/驱动只读·极严格）</option>
          </select>
        </label>
        <label class="field">
          <span>用户范围</span>
          <select v-model="userScope" :disabled="scanning">
            <option value="current">当前用户</option>
            <option value="all">所有用户（建议管理员）</option>
          </select>
        </label>
        <div class="field category-field">
          <span>检查分类</span>
          <div class="dropdown" @keydown.esc="showCategoryMenu = false">
            <button type="button" class="dropdown-trigger" :disabled="scanning" @click="showCategoryMenu = !showCategoryMenu">
              <span>{{ categoryLabel }}</span>
              <ChevronRight :size="16" class="chev" :class="{ open: showCategoryMenu }" />
            </button>
            <div v-if="showCategoryMenu" class="dropdown-menu">
              <button type="button" class="dropdown-item all" @click="selectAllCategories">全选</button>
              <button
                v-for="item in CATEGORY_OPTIONS"
                :key="item.id"
                type="button"
                class="dropdown-item"
                :class="{ checked: selectedCategories.includes(item.id) }"
                @click="toggleCategory(item.id)"
              >
                <Check v-if="selectedCategories.includes(item.id)" :size="14" />
                <i v-else />
                <span>{{ item.label }}</span>
              </button>
            </div>
          </div>
        </div>
        <div class="field exclusion-field">
          <span>排除关键字</span>
          <div class="exclusion-add">
            <input v-model="exclusionInput" type="text" maxlength="120" placeholder="软件名 / 路径片段" :disabled="scanning" @keydown.enter.prevent="addExclusion" />
            <button class="button secondary compact" type="button" :disabled="scanning" @click="addExclusion"><Plus :size="15" /> 添加</button>
          </div>
        </div>
      </div>
      <div v-if="exclusions.length" class="exclusion-chips">
        <span v-for="entry in exclusions" :key="entry"><Ban :size="13" />{{ entry }}<button type="button" title="移除" @click="removeExclusion(entry)"><X :size="13" /></button></span>
      </div>
    </section>

    <section class="backup-panel panel">
      <header>
        <div><span class="panel-kicker">备份与恢复</span><h2>注册表备份</h2></div>
        <div class="backup-actions">
          <div class="dropdown backup-dropdown">
            <button type="button" class="button secondary compact dropdown-trigger-btn" :disabled="backingUp" @click="showBackupMenu = !showBackupMenu">
              <LoaderCircle v-if="backingUp" :size="14" class="spin" />
              <ArchiveRestore v-else :size="14" />
              备份
              <ChevronRight :size="14" class="chev" :class="{ open: showBackupMenu }" />
            </button>
            <div v-if="showBackupMenu" class="dropdown-menu backup-menu">
              <button type="button" class="dropdown-item" :disabled="backingUp" @click="createFullBackup(false)"><ArchiveRestore :size="14" /> 备份到默认位置</button>
              <button type="button" class="dropdown-item" :disabled="backingUp" @click="createFullBackup(true)"><FolderOpen :size="14" /> 备份到指定文件夹…</button>
              <button type="button" class="dropdown-item" @click="importExistingBackup"><History :size="14" /> 导入已有备份文件夹…</button>
            </div>
          </div>
          <button class="text-button" @click="loadBackups"><History :size="14" /> 刷新</button>
        </div>
      </header>
      <div v-if="backups.length" class="backup-list">
        <article v-for="item in backups" :key="item.path || item.id" class="backup-row">
          <div>
            <b>{{ item.label || '注册表备份' }}</b>
            <small :title="item.path">{{ item.createdAt || item.id }} · {{ item.fileCount ? `${item.fileCount} 个 .reg · ` : '' }}{{ item.kind === 'full' ? '完整' : item.kind === 'repair' ? '修复前' : '导入' }} · {{ item.path }}</small>
          </div>
          <div class="backup-row-actions">
            <button type="button" class="text-button" @click="openPath(item.path)"><FolderOpen :size="14" /> 打开</button>
            <button type="button" class="text-button restore" @click="confirmRestore = item"><RotateCcw :size="14" /> 恢复</button>
          </div>
        </article>
      </div>
      <p v-else class="backup-empty">列表为空。可用「备份」下拉：默认位置 / 指定文件夹；重装后用「导入已有备份文件夹」找回以前的备份。</p>
    </section>

    <section v-if="scanning" class="registry-page registry-loading panel">
      <LoaderCircle :size="30" class="spin" />
      <h2>正在检查注册表</h2>
      <p>{{ progress.message }}</p>
      <div class="registry-page registry-progress"><i><em :style="{ width: `${progress.percentage}%` }" /></i><b>{{ progress.percentage }}%</b></div>
      <small>{{ scanMode === 'expert' ? '专家模式含服务/驱动只读与更多系统项' : scanMode === 'advanced' ? '进阶模式会扫更多历史与系统项' : '初级模式仅扫用户级安全项' }}</small>
    </section>

    <template v-else-if="report">
      <section class="registry-page registry-metrics">
        <div><small>已检查键</small><b>{{ report.scannedKeys }}</b><span>{{ report.elapsedMs }} ms · {{ report.userScope === 'all' ? '所有用户' : '当前用户' }}</span></div>
        <button type="button" class="metric-btn safe" :class="{ active: activeCategory === null && resultRiskFilter === 'all' }" @click="activeCategory = null; resultRiskFilter = 'all'"><small>可安全修复</small><b>{{ report.fixableCount }}</b><span>点击查看全部</span></button>
        <div class="review"><small>建议人工复核</small><b>{{ report.reviewCount }}</b><span>不会自动修改</span></div>
      </section>

      <section v-if="report.categories?.length" class="category-cards">
        <button
          v-for="cat in report.categories"
          :key="cat.id"
          type="button"
          class="category-card"
          :class="{ active: activeCategory === cat.id }"
          @click="selectCategory(cat.id)"
        >
          <div class="category-card-top">
            <b>{{ cat.name }}</b>
            <ChevronRight :size="16" />
          </div>
          <strong>{{ cat.total }}</strong>
          <small>可修 {{ cat.fixable }} · 复核 {{ cat.review }}</small>
        </button>
      </section>

      <section class="registry-page registry-filter-bar panel">
        <label class="field compact">
          <span>风险</span>
          <select v-model="resultRiskFilter">
            <option value="all">全部风险</option>
            <option value="low">低风险可修</option>
            <option value="high">高风险可修</option>
            <option value="critical">极高风险</option>
            <option value="review">仅建议</option>
          </select>
        </label>
        <label class="field compact">
          <span>范围</span>
          <select v-model="resultScopeFilter">
            <option value="all">HKCU + HKLM</option>
            <option value="hkcu">仅 HKCU</option>
            <option value="hklm">仅 HKLM</option>
          </select>
        </label>
        <label class="field compact grow">
          <span>关键字</span>
          <input v-model="resultQuery" type="search" placeholder="名称 / 路径 / 数据 / 原因" />
        </label>
        <button type="button" class="text-button" @click="showRepairLog = true"><History :size="14" /> 修复日志 · {{ repairLog.length }}</button>
      </section>

      <section class="registry-page registry-results panel">
        <header>
          <div>
            <span class="panel-kicker">{{ activeCategory ? (report.categories.find(c => c.id === activeCategory)?.name || '分类明细') : '筛选结果' }}</span>
            <h2>{{ filteredItems.length }} 个项目</h2>
          </div>
          <div class="registry-page registry-result-actions">
            <button v-if="activeCategory || resultRiskFilter !== 'all' || resultScopeFilter !== 'all' || resultQuery" class="text-button" @click="activeCategory = null; resultRiskFilter = 'all'; resultScopeFilter = 'all'; resultQuery = ''">清除筛选</button>
            <button class="text-button" :disabled="!filteredItems.some(i => i.fixable)" @click="toggleAllFixable"><Check :size="15" /> {{ allFixableSelected ? '取消全选' : '选择可见可修项' }}</button>
            <button class="button repair-button" :disabled="!selectedIds.length" @click="confirmRepair = true"><Wrench :size="16" /> 备份并修复 · {{ selectedIds.length }}</button>
          </div>
        </header>
        <div v-if="filteredItems.length" class="registry-page registry-rows">
          <article
            v-for="item in filteredItems"
            :key="item.id"
            class="registry-page registry-row"
            :class="{
              selected: selectedIds.includes(item.id),
              review: !item.fixable,
              danger: item.fixable && isHighRiskItem(item),
            }"
          >
            <button v-if="item.fixable" class="registry-page registry-check" :class="{ checked: selectedIds.includes(item.id), danger: isHighRiskItem(item) }" @click="toggleSelection(item.id)"><Check v-if="selectedIds.includes(item.id)" :size="14" /></button>
            <span v-else class="registry-page registry-review-mark"><AlertTriangle :size="15" /></span>
            <div class="registry-page registry-copy">
              <div>
                <b>{{ item.name }}</b>
                <span class="category-chip">{{ item.category }}</span>
                <span :class="itemRiskBadge(item).className">{{ itemRiskBadge(item).text }}</span>
              </div>
              <p>{{ item.reason }}</p>
              <small :title="item.keyPath">{{ item.keyPath }}{{ item.valueName ? ` · ${item.valueName}` : '' }}</small>
              <code :title="item.data">{{ item.data }}</code>
            </div>
            <div class="registry-page registry-row-actions">
              <button type="button" class="text-button" @click="addExclusionFromItem(item)"><Ban :size="14" /> 排除</button>
              <span class="registry-page registry-action-label" :class="{ danger: item.fixable && isHighRiskItem(item) }">{{ itemActionLabel(item) }}</span>
            </div>
          </article>
        </div>
        <div v-else class="registry-page registry-empty"><ShieldCheck :size="42" /><h2>此分类下没有项目</h2><p>可切换其他分类卡片，或调整检查范围后重新扫描。</p></div>
        <footer><ShieldCheck :size="16" /><span>绿色低风险可默认勾选修复；橙色/系统项需手动勾选并二次确认。备份与恢复见上方面板。</span></footer>
      </section>
    </template>

    <section v-else class="registry-page registry-welcome panel">
      <div><DatabaseSearch :size="44" /></div>
      <h2>选择范围后开始检查</h2>
      <p>建议先完整备份。检查结果按分类卡片展示，点击即可查看该类下的具体问题项。</p>
      <div class="welcome-actions">
        <button class="button secondary" :disabled="backingUp" @click="createFullBackup(false)"><ArchiveRestore :size="16" /> 先备份</button>
        <button class="button primary" :disabled="!canScan" @click="scanRegistry()"><DatabaseSearch :size="17" /> 开始检查</button>
      </div>
    </section>

    <div v-if="showExpertGate" class="registry-page registry-modal" @click.self="showExpertGate = false">
      <section class="repair-dialog" role="dialog" aria-modal="true" aria-label="专家模式门闩">
        <span class="restore-icon"><AlertTriangle :size="27" /></span>
        <h2>进入专家模式前请确认</h2>
        <p>专家模式会扫描服务/驱动（只读）及更多系统项。SharedDLL 等极高风险删除需确认词。建议先完整备份；未备份时不建议扫描后直接修复。</p>
        <ul class="high-risk-list">
          <li>服务 / 驱动：只读，不会删除</li>
          <li>文件关联 / COM / HKLM：可修项默认不勾选</li>
          <li>SharedDLL：仅删缺失文件对应注册表值</li>
        </ul>
        <div>
          <button class="button secondary" @click="scanMode = 'advanced'; showExpertGate = false">改用进阶</button>
          <button class="button secondary" @click="acknowledgeExpertGate(false)">已知风险，继续</button>
          <button class="button repair-button" :disabled="backingUp" @click="acknowledgeExpertGate(true)"><ArchiveRestore :size="16" /> 备份并确认</button>
        </div>
      </section>
    </div>

    <div v-if="showRepairLog" class="registry-page registry-modal" @click.self="showRepairLog = false">
      <section class="repair-dialog repair-log-dialog" role="dialog" aria-modal="true" aria-label="修复日志">
        <button class="dialog-close" type="button" @click="showRepairLog = false"><X :size="18" /></button>
        <span><History :size="27" /></span>
        <h2>注册表修复日志</h2>
        <p>本机记录最近修复操作，便于溯源（最多 50 条）。</p>
        <div class="repair-log-list">
          <article v-for="item in repairLog" :key="item.id" class="repair-log-row">
            <div><b>{{ item.mode }} · 修复 {{ item.repaired }} / 失败 {{ item.failed }}</b><small>{{ new Date(item.at).toLocaleString('zh-CN') }}</small></div>
            <p>{{ item.names.join('、') || '（无名称摘要）' }}</p>
            <button v-if="item.backupDirectory" type="button" class="text-button" @click="openPath(item.backupDirectory)"><FolderOpen :size="14" /> 打开备份</button>
          </article>
          <div v-if="!repairLog.length" class="no-matches">暂无修复记录</div>
        </div>
        <div>
          <button class="button secondary" @click="repairLog = []; persistRepairLog()">清空</button>
          <button class="button primary" @click="showRepairLog = false">关闭</button>
        </div>
      </section>
    </div>

    <div v-if="confirmRepair" class="registry-page registry-modal" @click.self="confirmRepair = false">
      <section class="repair-dialog" role="dialog" aria-modal="true">
        <span><ArchiveRestore :size="27" /></span>
        <h2>备份并修复 {{ selectedItems.length }} 个项目？</h2>
        <p>将先导出备份，再删除所选失效项。可在备份列表恢复。</p>
        <div v-if="selectedHighRiskItems.length" class="high-risk-warn">
          <AlertTriangle :size="16" />
          <div>
            <b>含 {{ selectedHighRiskItems.length }} 个高风险/系统项</b>
            <small>包括 COM、HKLM 启动项、文件关联、SharedDLL 等。误删可能导致软件异常。</small>
          </div>
        </div>
        <div v-if="selectedCriticalItems.length" class="critical-risk-warn">
          <AlertTriangle :size="16" />
          <div>
            <b>含 {{ selectedCriticalItems.length }} 个极高风险项（如 SharedDLL）</b>
            <small>服务/驱动不会被删除（只读）。SharedDLL 仅删除缺失文件对应的注册表值。请输入确认词：</small>
            <input v-model="expertConfirmText" class="expert-confirm-input" type="text" placeholder="确认删除高风险项" :disabled="repairing" />
          </div>
        </div>
        <ul v-if="selectedHighRiskItems.length" class="high-risk-list">
          <li v-for="item in selectedHighRiskItems.slice(0, 8)" :key="item.id">{{ item.name }} · {{ item.category }}</li>
          <li v-if="selectedHighRiskItems.length > 8">…还有 {{ selectedHighRiskItems.length - 8 }} 项</li>
        </ul>
        <div>
          <button class="button secondary" :disabled="repairing" @click="confirmRepair = false; expertConfirmText = ''">取消</button>
          <button class="button repair-button" :disabled="repairing || !expertConfirmOk" @click="runRepair"><LoaderCircle v-if="repairing" :size="16" class="spin" /><Wrench v-else :size="16" /> {{ selectedCriticalItems.length ? '输入确认词后删除' : selectedHighRiskItems.length ? '已知风险，确认修复' : '确认备份并修复' }}</button>
        </div>
      </section>
    </div>

    <div v-if="confirmRestore" class="registry-page registry-modal" @click.self="confirmRestore = null">
      <section class="repair-dialog" role="dialog" aria-modal="true">
        <span class="restore-icon"><RotateCcw :size="27" /></span>
        <h2>恢复此备份？</h2>
        <p>
          {{ confirmRestore.label }}（{{ confirmRestore.createdAt || confirmRestore.id }}）。
          将把 .reg <b>写回注册表原来的键</b>（reg import）。
          若你备份后几乎没改注册表，再恢复通常几乎无感（相当于用同样内容覆盖一遍）；
          若已做过「备份并修复」删过项，恢复会把当时导出的键值装回去。
        </p>
        <div>
          <button class="button secondary" :disabled="restoring" @click="confirmRestore = null">取消</button>
          <button class="button repair-button" :disabled="restoring" @click="runRestore"><LoaderCircle v-if="restoring" :size="16" class="spin" /><RotateCcw v-else :size="16" /> 确认恢复</button>
        </div>
      </section>
    </div>
  </div>
</template>

<style scoped>
.registry-center{display:grid;gap:14px}
.registry-alert{min-height:42px;padding:9px 12px;border-radius:5px;display:flex;align-items:center;gap:9px}
.registry-alert>span{flex:1}
.registry-alert>button:not(.backup-link){border:0;background:transparent;color:inherit;display:grid;place-items:center}
.registry-alert.error{background:#fff0f0;border:1px solid #ffcaca;color:#a52b2b}
.registry-alert.notice{background:#effaf6;border:1px solid #ccecdf;color:#087355}
.backup-link{height:30px;border:1px solid #a9ddcb;background:#fff;border-radius:4px;color:#087355;display:flex;align-items:center;gap:6px;padding:0 9px}
.registry-hero{display:grid;grid-template-columns:48px minmax(0,1fr) auto;align-items:center;gap:14px}
.registry-hero-icon{width:48px;height:48px;border-radius:6px;background:var(--accent-soft);color:var(--accent-ink);display:grid;place-items:center}
.registry-hero h2{margin:3px 0;font-size:16px}
.registry-hero p{margin:0;color:#667085}
.backup-prompt{display:flex;align-items:center;justify-content:space-between;gap:14px;padding:14px 16px;border:1px solid #f0d9a8;background:#fffaf0}
.backup-prompt-copy{display:flex;align-items:flex-start;gap:12px;min-width:0}
.backup-prompt-copy>svg{color:#c27803;flex:none;margin-top:2px}
.backup-prompt b,.backup-prompt small{display:block}
.backup-prompt small{margin-top:4px;color:#8a7348;line-height:1.45}
.registry-options{display:grid;gap:12px;padding:16px 18px}
.options-toolbar{display:grid;grid-template-columns:minmax(140px,.85fr) minmax(140px,.85fr) minmax(180px,1.05fr) minmax(200px,1.15fr);gap:12px;align-items:end}
.field{display:grid;gap:6px;min-width:0}
.field>span{font-size:12px;color:#667085;font-weight:600}
.field select,.dropdown-trigger{height:38px;border:1px solid #d7dce2;border-radius:5px;background:#fff;color:#344054;padding:0 12px;width:100%;outline:0}
.field select:focus,.dropdown-trigger:focus{border-color:var(--accent)}
.dropdown{position:relative}
.dropdown-trigger{display:flex;align-items:center;justify-content:space-between;gap:8px;text-align:left;cursor:pointer}
.dropdown-trigger .chev{transform:rotate(90deg);transition:transform .15s;color:#98a2b3}
.dropdown-trigger .chev.open{transform:rotate(-90deg)}
.dropdown-menu{position:absolute;z-index:20;top:calc(100% + 4px);left:0;right:0;max-height:280px;overflow:auto;background:#fff;border:1px solid #e1e5e9;border-radius:8px;box-shadow:0 12px 28px #1018281f;padding:6px}
.dropdown-item{width:100%;height:36px;border:0;border-radius:5px;background:transparent;display:flex;align-items:center;gap:8px;padding:0 10px;color:#475467;text-align:left}
.dropdown-item:hover{background:#f5f7fa}
.dropdown-item.checked{color:var(--accent-ink);background:var(--accent-soft)}
.dropdown-item.all{font-weight:650;color:#344054;border-bottom:1px solid #edf0f2;margin-bottom:4px;border-radius:0}
.dropdown-item i{width:14px;height:14px;border:1px solid #d0d5dd;border-radius:3px;flex:none}
.exclusion-add{display:grid;grid-template-columns:minmax(0,1fr) auto;gap:8px}
.exclusion-add input{height:38px;border:1px solid #d7dce2;border-radius:5px;padding:0 12px;outline:0}
.exclusion-add input:focus{border-color:var(--accent)}
.exclusion-chips{display:flex;flex-wrap:wrap;gap:8px}
.exclusion-chips>span{display:inline-flex;align-items:center;gap:6px;padding:5px 8px;border-radius:999px;background:#f2f4f7;color:#475467;font-size:12px}
.exclusion-chips button{border:0;background:transparent;color:#98a2b3;display:grid;place-items:center}
.backup-panel{padding:0;overflow:hidden}
.backup-panel>header{display:flex;align-items:center;justify-content:space-between;gap:12px;padding:14px 16px;border-bottom:1px solid #edf0f2}
.backup-panel h2{margin:3px 0 0;font-size:15px}
.backup-actions{display:flex;align-items:center;gap:8px;position:relative}
.backup-dropdown{position:relative}
.dropdown-trigger-btn{display:inline-flex;align-items:center;gap:6px}
.dropdown-trigger-btn .chev{transform:rotate(90deg);transition:transform .15s}
.dropdown-trigger-btn .chev.open{transform:rotate(-90deg)}
.backup-menu{right:0;left:auto;min-width:220px;top:calc(100% + 4px)}
.backup-list{display:grid}
.backup-row{display:flex;align-items:center;justify-content:space-between;gap:12px;padding:12px 16px;border-bottom:1px solid #f0f2f5}
.backup-row:last-child{border:0}
.backup-row b,.backup-row small{display:block}
.backup-row small{margin-top:3px;color:#7d8896}
.backup-row-actions{display:flex;gap:6px}
.backup-row-actions .restore{color:#0f8f6b}
.backup-empty{margin:0;padding:16px;color:#98a2b3;text-align:center}
.registry-loading,.registry-welcome{min-height:280px;display:flex;flex-direction:column;align-items:center;justify-content:center;text-align:center;padding:24px}
.registry-loading>svg{color:var(--accent)}
.registry-welcome>div{width:86px;height:86px;border-radius:50%;background:var(--accent-soft);color:var(--accent-ink);display:grid;place-items:center}
.registry-welcome h2,.registry-loading h2{margin:14px 0 6px}
.registry-welcome p,.registry-loading p{max-width:560px;color:#667085;line-height:1.7;margin:0 0 12px}
.registry-progress{width:min(420px,100%);display:grid;grid-template-columns:1fr auto;gap:10px;align-items:center;margin:8px 0}
.registry-progress>i{display:block;height:6px;background:#edf0f2;border-radius:4px;overflow:hidden}
.registry-progress>i em{display:block;height:100%;background:var(--accent-gradient,var(--accent));transition:width .2s}
.registry-progress b{color:var(--accent-ink,#344054);font-size:12px}
.registry-loading small{color:#98a2b3}
.welcome-actions{display:flex;gap:10px}
.registry-metrics{display:grid;grid-template-columns:repeat(3,1fr);background:#fff;border:1px solid #dfe3e8;border-radius:6px;overflow:hidden}
.registry-metrics>div,.metric-btn{min-height:88px;padding:16px 18px;border-right:1px solid #e7eaee;text-align:left}
.registry-metrics>div:last-child,.metric-btn:last-child{border:0}
.registry-metrics small,.registry-metrics b,.registry-metrics span,.metric-btn small,.metric-btn b,.metric-btn span{display:block}
.registry-metrics b,.metric-btn b{margin:6px 0 4px;font-size:22px}
.registry-metrics span,.metric-btn span{color:#7d8896;font-size:12px}
.metric-btn{border:0;background:#fff;cursor:pointer}
.metric-btn.safe b{color:#12a47b}
.metric-btn.active{background:#f3fbf8;box-shadow:inset 0 0 0 2px #12a47b55}
.registry-metrics .review b{color:#e18a00}
.category-cards{display:grid;grid-template-columns:repeat(auto-fill,minmax(160px,1fr));gap:10px}
.category-card{border:1px solid #e1e5e9;border-radius:8px;background:#fff;padding:14px 14px 12px;text-align:left;cursor:pointer;transition:border-color .15s,box-shadow .15s,transform .12s}
.category-card:hover{border-color:var(--accent);box-shadow:0 6px 16px #10182812;transform:translateY(-1px)}
.category-card.active{border-color:var(--accent);background:var(--accent-soft);box-shadow:0 0 0 1px var(--accent)}
.category-card-top{display:flex;align-items:center;justify-content:space-between;gap:8px;color:#475467}
.category-card strong{display:block;margin:8px 0 4px;font-size:24px;color:#1d2939}
.category-card small{color:#7d8896}
.registry-results>header{display:flex;align-items:center;justify-content:space-between;gap:14px;padding:16px 18px;border-bottom:1px solid #edf0f2}
.registry-results h2{margin:3px 0 0}
.registry-result-actions{display:flex;align-items:center;gap:10px;flex-wrap:wrap}
.repair-button{background:#12a47b;color:#fff;border:0}
.repair-button:hover{background:#0f8f6b}
.repair-button:disabled{opacity:.55}
.registry-rows{display:grid}
.registry-row{display:grid;grid-template-columns:28px minmax(0,1fr) auto;gap:12px;align-items:start;padding:14px 18px;border-bottom:1px solid #edf0f2}
.registry-row:last-child{border:0}
.registry-row.selected{background:#f3fbf8}
.registry-row.review{background:#fffaf3}
.registry-row.danger{background:#fff7ed}
.registry-row.danger.selected{background:#ffedd5}
.registry-check,.registry-review-mark{width:24px;height:24px;border-radius:5px;display:grid;place-items:center;margin-top:2px}
.registry-check{border:1px solid #d0d5dd;background:#fff}
.registry-check.checked{border-color:#12a47b;background:#12a47b;color:#fff}
.registry-check.checked.danger{border-color:#ea580c;background:#ea580c}
.registry-review-mark{background:#fff5e6;color:#e18a00}
.registry-copy{min-width:0}
.registry-copy>div{display:flex;align-items:center;gap:8px;flex-wrap:wrap}
.registry-copy p{margin:6px 0;color:#475467;line-height:1.5}
.registry-copy small{display:block;color:#98a2b3;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.registry-copy code{display:block;margin-top:6px;padding:6px 8px;border-radius:4px;background:#f8fafc;color:#52606d;font-size:11px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.category-chip,.safe-badge,.review-badge,.danger-badge{font-size:11px;padding:2px 7px;border-radius:999px}
.category-chip{background:#eef2f6;color:#52606d}
.safe-badge{background:#e8fbf4;color:#0f8f6b}
.review-badge{background:#fff5e6;color:#c27803}
.danger-badge{background:#ffedd5;color:#c2410c;font-weight:650}
.critical-badge{background:#fee2e2;color:#b91c1c;font-weight:700}
.critical-risk-warn{display:flex;gap:10px;text-align:left;padding:10px 12px;margin:0 0 10px;border-radius:6px;background:#fef2f2;border:1px solid #fecaca;color:#991b1b}
.critical-risk-warn b,.critical-risk-warn small{display:block}
.critical-risk-warn small{margin-top:4px;line-height:1.45}
.expert-confirm-input{width:100%;margin-top:8px;height:36px;border:1px solid #fca5a5;border-radius:5px;padding:0 10px;outline:0}
.expert-confirm-input:focus{border-color:#dc2626;box-shadow:0 0 0 3px #fecaca88}
.registry-row-actions{display:grid;gap:8px;justify-items:end}
.registry-action-label{font-size:12px;color:#7d8896;white-space:nowrap}
.registry-action-label.danger{color:#c2410c;font-weight:650}
.registry-empty{padding:48px 20px;text-align:center;color:#667085}
.registry-empty h2{margin:12px 0 8px}
.registry-results>footer{display:flex;align-items:flex-start;gap:8px;padding:12px 18px;border-top:1px solid #edf0f2;color:#667085;font-size:12px;line-height:1.5}
.registry-modal{position:fixed;inset:0;background:#10182866;display:grid;place-items:center;z-index:50;padding:20px}
.repair-dialog{width:min(440px,100%);background:#fff;border-radius:10px;padding:28px 24px;text-align:center;box-shadow:0 20px 50px #10182833}
.repair-dialog>span{width:56px;height:56px;margin:0 auto 12px;border-radius:50%;background:#e8fbf4;color:#12a47b;display:grid;place-items:center}
.repair-dialog>span.restore-icon{background:#eef4ff;color:#3b6fd9}
.repair-dialog h2{margin:0 0 8px}
.repair-dialog p{margin:0 0 14px;color:#667085;line-height:1.6}
.repair-dialog>div:last-child{display:flex;justify-content:center;gap:10px}
.high-risk-warn{display:flex;gap:10px;text-align:left;padding:10px 12px;margin:0 0 10px;border-radius:6px;background:#fff5e6;border:1px solid #f0d9a8;color:#8a5a00}
.high-risk-warn b,.high-risk-warn small{display:block}
.high-risk-warn small{margin-top:4px;line-height:1.45}
.high-risk-list{text-align:left;margin:0 0 14px;padding:0 0 0 18px;color:#667085;font-size:12px;max-height:120px;overflow:auto}
@media(max-width:980px){.options-toolbar{grid-template-columns:1fr}.registry-results>header{align-items:flex-start;flex-direction:column}.registry-result-actions{width:100%}.registry-result-actions .repair-button{margin-left:auto}}
@media(max-width:800px){.registry-hero{grid-template-columns:44px 1fr}.registry-hero>.button{grid-column:1 / -1}.backup-prompt{flex-direction:column;align-items:stretch}.registry-metrics{grid-template-columns:1fr}.registry-metrics>div,.metric-btn{border-right:0;border-bottom:1px solid #e7eaee}.registry-row{grid-template-columns:22px minmax(0,1fr)}.registry-row-actions{grid-column:2;justify-items:start}.exclusion-add{grid-template-columns:1fr}}

/* registry glass helpers (U1) */
.registry-page .panel,.registry-options,.backup-panel,.registry-results.panel,.category-card,.registry-filter-bar{background:color-mix(in srgb,#fff 52%,transparent)!important;backdrop-filter:blur(14px);-webkit-backdrop-filter:blur(14px);border-color:color-mix(in srgb,#fff 50%,#e4e7eb)!important;box-shadow:0 10px 28px #1018280d, inset 0 1px 0 #ffffffa8}
.registry-metrics{background:color-mix(in srgb,#fff 54%,transparent)!important;backdrop-filter:blur(14px);border:1px solid color-mix(in srgb,#fff 50%,#e4e7eb);border-radius:14px;box-shadow:0 10px 28px #1018280d}
.registry-row{background:color-mix(in srgb,#fff 48%,transparent);border-radius:10px;transition:background .16s ease, transform .16s ease}
.registry-row:hover{background:color-mix(in srgb,var(--accent-soft) 42%, #ffffffcc)}
.registry-modal{backdrop-filter:blur(6px)}
.repair-dialog,.registry-modal .repair-dialog{background:color-mix(in srgb,#fff 72%,transparent)!important;backdrop-filter:blur(18px)}

/* mild-debox-reg */
.registry-options.panel{background:transparent!important;border:0!important;box-shadow:none!important;backdrop-filter:none!important;padding:8px 2px 12px!important}
.registry-filter-bar,.registry-filter-bar.panel{background:transparent!important;border:0!important;box-shadow:none!important;backdrop-filter:none!important}
.registry-results>header{background:transparent!important;border-bottom:0!important}
.backup-panel>header{background:transparent!important;border-bottom:1px solid color-mix(in srgb,#fff 30%, #edf0f2)!important}
.category-card{box-shadow:none!important}

/* U1.8: registry chrome sweep */
.registry-filter-bar,
.registry-filter-bar.panel,
.registry-options.panel{
  background:transparent!important;
  border:0!important;
  box-shadow:none!important;
  backdrop-filter:none!important;
  -webkit-backdrop-filter:none!important;
}
.registry-filter-bar.panel{padding:8px 2px 12px!important}
.registry-hero.panel{box-shadow:0 8px 24px #1018280a!important}
.registry-results.panel .registry-row{
  border-color:color-mix(in srgb, var(--u1-border, #edf0f2) 70%, transparent)!important;
  box-shadow:none!important;
}
.exclusion-chips{display:flex;flex-wrap:wrap;gap:8px;background:transparent!important;padding:0!important;border:0!important}
.exclusion-chips>span{
  background:color-mix(in srgb,#fff 58%, transparent)!important;
  border:1px solid var(--u1-border-chip, var(--u1-border, #d7dce2));
  border-radius:999px!important;
}

/* keyword input border match global */
.registry-filter-bar input,
.registry-filter-bar .field input,
.registry-filter-bar input[type="search"],
.field input,
.field.grow input{
  border:1px solid var(--u1-border, #e4e7eb)!important;
  border-color:var(--u1-border, #e4e7eb)!important;
  box-shadow:none!important;
  background:color-mix(in srgb,#fff 92%, transparent)!important;
  border-radius:10px!important;
  color:#344054!important;
}
.registry-filter-bar input:focus,
.field input:focus{
  border-color:color-mix(in srgb, var(--accent) 50%, #d7dce2)!important;
  box-shadow:0 0 0 3px color-mix(in srgb, var(--accent-soft) 75%, transparent)!important;
  outline:none!important;
}
.registry-filter-bar select,
.field select{
  border:1px solid var(--u1-border, #e4e7eb)!important;
  border-radius:10px!important;
}
</style>
