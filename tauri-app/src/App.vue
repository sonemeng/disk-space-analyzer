<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { open } from '@tauri-apps/plugin-dialog'
import {
  AlertTriangle,
  ArrowLeft,
  BarChart3,
  Ban,
  CalendarClock,
  ChartNoAxesCombined,
  Check,
  ChevronRight,
  CircleStop,
  Download,
  Database,
  Fingerprint,
  ExternalLink,
  FileSearch,
  FileText,
  FolderCog,
  FolderOpen,
  FolderSearch,
  FolderTree,
  Gauge,
  HardDrive,
  History,
  Info,
  LayoutDashboard,
  Library,
  LoaderCircle,
  Palette,
  PanelLeftClose,
  PanelLeftOpen,
  Play,
  RefreshCw,
  Recycle,
  Search,
  Settings,
  SlidersHorizontal,
  ShieldCheck,
  Sparkles,
  Trash2,
  Undo2,
  Wrench,
  X,
} from '@lucide/vue'
import MediaCenter from './MediaCenter.vue'
import RegistryCleaner from './RegistryCleaner.vue'
import {
  appendActivity,
  clearActivityLog,
  exportActivityLogText,
  formatDetailLines,
  isDetailedActivityLogEnabled,
  kindLabel,
  loadActivityLog,
  migrateLegacyMessageLog,
  removeActivity,
  setDetailedActivityLogEnabled,
  type ActivityEntry,
  type ActivityKind,
} from './activityLog'

interface DiskUsage { total: number; used: number; free: number }
interface DirectoryItem { path: string; name: string; size: number; fileCount: number; dirCount: number }
interface LargeFile { path: string; name: string; size: number; modifiedDays?: number | null }
interface CategoryItem { name: string; size: number; color: string }
interface AgeBucket { id: string; label: string; size: number; fileCount: number; color: string }
interface ScanResult {
  drive: string
  usage: DiskUsage
  directories: DirectoryItem[]
  largeFiles: LargeFile[]
  categories: CategoryItem[]
  fileTypes: CategoryItem[]
  ageBuckets: AgeBucket[]
  scannedFiles: number
  scannedDirs: number
  elapsedMs: number
  skippedItems: number
}
interface ScanProgress { message: string; percentage: number; currentPath?: string }
interface CleanupItem {
  id: string
  name: string
  description: string
  path: string
  size: number
  fileCount: number
  action: 'safe' | 'review' | 'system'
  risk: 'low' | 'medium'
  category: 'fixed' | 'developer' | 'toolai' | 'app'
  ruleId?: string
  rulepackVersion?: string
  readonly?: boolean
  requiresStrongConfirm?: boolean
}
interface CleanupReport {
  items: CleanupItem[]
  safeBytes: number
  reviewBytes: number
  developerBytes: number
  toolAiBytes?: number
  appCacheBytes?: number
  rulepackVersion?: string
}
type CleanupTab = 'all' | 'fixed' | 'developer' | 'toolai' | 'app' | 'review' | 'system'
interface CleanupResult { freedBytes: number; deletedFiles: number; failedItems: number; dryRun: boolean; skippedHot: number }
interface RecycleStoredItem { original: string; bin: string }
interface RecycleEntry {
  id: string
  createdAt: string
  source: string
  label: string
  totalBytes: number
  fileCount: number
  items: RecycleStoredItem[]
}
interface CleanupSnapshotPath { path: string; size: number; modifiedDays?: number | null }
interface CleanupSnapshotEntry { source: string; label: string; paths: CleanupSnapshotPath[] }
interface CleanupSnapshot { id: string; createdAt: string; drive: string; entries: CleanupSnapshotEntry[] }
interface FolderItem {
  path: string
  name: string
  size: number
  fileCount: number
  dirCount: number
  kind: 'directory' | 'file' | 'link'
  risk: 'rebuildable' | 'review' | 'protected'
  recommendation: string
}
interface FolderAnalysis {
  path: string
  name: string
  totalSize: number
  fileCount: number
  dirCount: number
  children: FolderItem[]
  largeFiles: LargeFile[]
  elapsedMs: number
  skippedItems: number
}
interface DuplicateGroup { hash: string; size: number; files: string[]; wastedBytes: number }
interface DuplicateReport {
  scope: string
  groups: DuplicateGroup[]
  scannedFiles: number
  hashedFiles: number
  duplicateFiles: number
  wastedBytes: number
  elapsedMs: number
  skippedItems: number
}
interface ScanSnapshot {
  id: string
  createdAt: string
  drive: string
  total: number
  used: number
  free: number
  scannedFiles: number
  directories: DirectoryItem[]
  fileTypes: CategoryItem[]
  ageBuckets: AgeBucket[]
}
interface UpdateStatus {
  currentVersion: string
  latestVersion?: string | null
  available: boolean
  releaseUrl?: string | null
  message: string
}
interface AdvancedSettings {
  exclusions: string[]
  cleanupBlacklist: string[]
  largeFileMb: number
  scanThreads: number
  snapshotLimit: number
  reportDirectory: string
  recyclePolicy: 'confirm' | 'direct'
  autoCheckUpdates: boolean
}

const APP_VERSION = typeof __APP_VERSION__ !== 'undefined' ? __APP_VERSION__ : '6.2.3'
const isTauri = '__TAURI_INTERNALS__' in window
type ThemeId = 'ocean' | 'forest' | 'coral' | 'cherry' | 'graphite' | 'mintrose' | 'lavenderteal'
type FontScale = 'small' | 'standard' | 'large'
type IconScale = 'compact' | 'standard' | 'large'
type UiDensity = 'compact' | 'comfortable'
const themeOptions: Array<{ id: ThemeId; name: string; colors: string[] }> = [
  { id: 'ocean', name: '海盐蓝', colors: ['#3182f6', '#eaf4ff', '#70d6ff'] },
  { id: 'forest', name: '活力青绿', colors: ['#12a47b', '#e8fbf4', '#8ee3c3'] },
  { id: 'coral', name: '橘子汽水', colors: ['#ff6846', '#fff0eb', '#ffb347'] },
  { id: 'cherry', name: '莓果红', colors: ['#e94b72', '#fff0f5', '#ff8fab'] },
  { id: 'graphite', name: '晴空灰', colors: ['#667085', '#f4f6f8', '#b8c4ce'] },
  { id: 'mintrose', name: '薄荷玫瑰', colors: ['#A9F1DF', '#FFBBBB', '#173b37'] },
  { id: 'lavenderteal', name: '薰衣草青', colors: ['#D8B5FF', '#1EAE98', '#151c2a'] },
]
const activeTheme = ref<ThemeId>('ocean')
const showSettings = ref(false)
const settingsTab = ref<'appearance' | 'scanning' | 'system' | 'activity' | 'about'>('appearance')
const activityFilter = ref<'all' | ActivityKind>('all')
const activityLog = ref<ActivityEntry[]>([])
const detailedActivityLog = ref(false)
const pullSide = ref<'left' | 'right'>('left')
const pullEdgeHot = ref(false)
const fontScale = ref<FontScale>('standard')
const iconScale = ref<IconScale>('standard')
const uiDensity = ref<UiDensity>('comfortable')
/** 界面通透度 0=更透 100=更实，写入 CSS --u1-glass-pct */
const glassStrength = ref(48)
/** 边框清晰度 0=几乎无框 100=清晰描边，主要作用于面板/列表结构线 */
const borderStrength = ref(42)
const exclusionPaths = ref<string[]>([])
const cleanupBlacklist = ref<string[]>([])
const largeFileMb = ref(100)
const scanThreads = ref(6)
const snapshotLimit = ref(30)
const reportDirectory = ref('')
const recyclePolicy = ref<'confirm' | 'direct'>('confirm')
const autoCheckUpdates = ref(true)
const updateStatus = ref<UpdateStatus | null>(null)
const settingsBusy = ref('')
const confirmClearHistory = ref(false)
const sidebarCollapsed = ref(false)
const mediaNew = ref(localStorage.getItem('disk-analyzer-seen-media-v6') !== '1')
const drives = ref<string[]>([])
const selectedDrive = ref('C:')
const usage = ref<DiskUsage | null>(null)
const result = ref<ScanResult | null>(null)
const cleanup = ref<CleanupReport | null>(null)
/** 按盘缓存：换盘不丢上次完整扫描与清理结果，重扫才刷新 */
const resultsByDrive = ref<Record<string, ScanResult>>({})
const cleanupByDrive = ref<Record<string, CleanupReport>>({})
const selectedCleanupByDrive = ref<Record<string, string[]>>({})
const scanning = ref(false)
const folderAnalyzing = ref(false)
const cleaning = ref(false)
const previewingCleanup = ref(false)
const loadingDrives = ref(true)
const loadingCleanup = ref(false)
const page = ref<'overview' | 'cleanup' | 'files' | 'insights' | 'media' | 'registry'>('overview')
const analysisTab = ref<'duplicates' | 'history' | 'age' | 'attribution' | 'types' | 'actions'>('duplicates')
const fileTab = ref<'directories' | 'files' | 'types'>('directories')
const attributionFocus = ref<'regions' | 'projects'>('regions')
const query = ref('')
const fileSizeFilter = ref<'all' | '100mb' | '500mb' | '1gb'>('all')
const fileAgeFilter = ref<'all' | 'year' | 'old'>('all')
const fileSort = ref<'size' | 'age' | 'name'>('size')
const selectedFilePaths = ref<string[]>([])
const selectedDuplicatePaths = ref<string[]>([])
const selectedAgeBucket = ref<string | null>(null)
const recyclingFiles = ref(false)
const confirmRecycleFiles = ref(false)
const recycleSource = ref<'files' | 'duplicates'>('files')
const error = ref('')
const notice = ref('')
const noticeFading = ref(false)
const errorFading = ref(false)
const showMessageLog = ref(false)
const selectedActivity = ref<ActivityEntry | null>(null)
const statusPanel = ref<'none' | 'notify' | 'quick'>('none')
const unreadActivityCount = ref(0)
const pullDistance = ref(0)
const pullDragging = ref(false)
let pullStartY = 0
let pullActive = false
const selectedCleanup = ref<string[]>([])
const confirmCleanup = ref(false)
const cleanupTab = ref<CleanupTab>('fixed')
const modelStrongConfirm = ref(false)
const modelConfirmPhrase = ref('')
const highlightCleanupPath = ref('')
const showRecycleBin = ref(false)
const recycleEntries = ref<RecycleEntry[]>([])
const recycleTotalBytes = ref(0)
const recycleBusy = ref('')
const systemRecycleBytes = ref(0)
const confirmEmptySystemBin = ref(false)
let emptyBinConfirmTimer: number | undefined
const cleanupProgress = ref<{ message: string; percent: number } | null>(null)
const showCleanupHistory = ref(false)
const cleanupHistory = ref<CleanupSnapshot[]>([])
const cleanupHistoryBusy = ref('')
const progress = ref<ScanProgress>({ message: '等待开始扫描', percentage: 0 })
const folderProgress = ref<ScanProgress>({ message: '等待选择文件夹', percentage: 0 })
const folderAnalysis = ref<FolderAnalysis | null>(null)
const folderHistory = ref<string[]>([])
const duplicateScanning = ref(false)
const duplicateProgress = ref<ScanProgress>({ message: '等待开始重复检测', percentage: 0 })
const duplicateReport = ref<DuplicateReport | null>(null)
const duplicateMinSize = ref(10 * 1024 * 1024)
const snapshots = ref<ScanSnapshot[]>([])
const snapshotsLoading = ref(false)
const fileTypeFilter = ref<string>('all')
let cleanupRequestId = 0
let unlisten: UnlistenFn | undefined
let unlistenFolder: UnlistenFn | undefined
let unlistenDuplicate: UnlistenFn | undefined
let unlistenCleanup: UnlistenFn | undefined
let noticeTimer: ReturnType<typeof setTimeout> | undefined
let noticeFadeTimer: ReturnType<typeof setTimeout> | undefined
let errorTimer: ReturnType<typeof setTimeout> | undefined
let errorFadeTimer: ReturnType<typeof setTimeout> | undefined

const currentUsage = computed(() => result.value?.usage ?? usage.value)
const isSystemDrive = computed(() => selectedDrive.value.toUpperCase() === 'C:')
const largestDirectory = computed(() => result.value?.directories[0] ?? null)
const displayCategories = computed(() => isSystemDrive.value ? result.value?.categories ?? [] : result.value?.fileTypes ?? [])
const usedPercent = computed(() => currentUsage.value?.total ? Math.round(currentUsage.value.used / currentUsage.value.total * 100) : 0)
const selectedCleanupItems = computed(() => cleanup.value?.items.filter(item => selectedCleanup.value.includes(item.id)) ?? [])
const selectedCleanupBytes = computed(() => selectedCleanupItems.value.reduce((sum, item) => sum + item.size, 0))
const safeItems = computed(() => cleanup.value?.items.filter(item => item.action === 'safe' && item.size > 0) ?? [])
const fixedSafeItems = computed(() => cleanup.value?.items.filter(item => item.category === 'fixed' && item.action === 'safe') ?? [])
const developerItems = computed(() => cleanup.value?.items.filter(item => item.category === 'developer') ?? [])
const toolAiItems = computed(() => cleanup.value?.items.filter(item => item.category === 'toolai') ?? [])
const toolAiNormalItems = computed(() => toolAiItems.value.filter(item => !item.requiresStrongConfirm))
const toolAiModelItems = computed(() => toolAiItems.value.filter(item => !!item.requiresStrongConfirm))
const appCacheItems = computed(() => cleanup.value?.items.filter(item => item.category === 'app') ?? [])
const appCacheNormalItems = computed(() => appCacheItems.value.filter(item => !item.requiresStrongConfirm))
const appCacheStrongItems = computed(() => appCacheItems.value.filter(item => !!item.requiresStrongConfirm))
const reviewItems = computed(() => cleanup.value?.items.filter(item => item.action === 'review' && item.category !== 'toolai' && item.category !== 'app') ?? [])
const toolAiTotalBytes = computed(() => toolAiItems.value.reduce((sum, item) => sum + (item.size || 0), 0))
const appCacheTotalBytes = computed(() => appCacheItems.value.reduce((sum, item) => sum + (item.size || 0), 0))
const otherReviewBytes = computed(() => reviewItems.value.reduce((sum, item) => sum + (item.size || 0), 0))
const selectedHasModelItems = computed(() => selectedCleanupItems.value.some(item => item.requiresStrongConfirm))
const selectedModelBytes = computed(() => selectedCleanupItems.value.filter(item => item.requiresStrongConfirm).reduce((s, i) => s + i.size, 0))
const systemItems = computed(() => cleanup.value?.items.filter(item => item.action === 'system') ?? [])
/** 当前 Tab 可见条目 */
const visibleCleanupItems = computed(() => {
  const tab = cleanupTab.value
  if (tab === 'all') return cleanup.value?.items ?? []
  if (tab === 'fixed') return fixedSafeItems.value
  if (tab === 'developer') return developerItems.value
  if (tab === 'toolai') return toolAiItems.value
  if (tab === 'app') return appCacheItems.value
  if (tab === 'review') return reviewItems.value
  if (tab === 'system') return systemItems.value
  return []
})
/** 本类可一键勾选：safe 且非强确认 */
const selectableInTab = computed(() => {
  const tab = cleanupTab.value
  const pool = tab === 'all'
    ? safeItems.value
    : visibleCleanupItems.value.filter(item => item.action === 'safe' && item.size > 0)
  return pool.filter(item => !item.requiresStrongConfirm)
})
const allTabSelectableSelected = computed(() =>
  selectableInTab.value.length > 0 && selectableInTab.value.every(item => selectedCleanup.value.includes(item.id)),
)
const cleanupTabOptions = computed(() => [
  { id: 'all' as CleanupTab, label: '全部', count: cleanup.value?.items.length ?? 0, bytes: cleanup.value?.safeBytes ?? 0 },
  { id: 'fixed' as CleanupTab, label: '固定白名单', count: fixedSafeItems.value.length, bytes: fixedSafeItems.value.reduce((s, i) => s + i.size, 0) },
  { id: 'developer' as CleanupTab, label: '开发可重建', count: developerItems.value.length, bytes: developerItems.value.reduce((s, i) => s + i.size, 0) },
  { id: 'toolai' as CleanupTab, label: '工具/AI', count: toolAiItems.value.length, bytes: toolAiTotalBytes.value },
  { id: 'app' as CleanupTab, label: '应用缓存', count: appCacheItems.value.length, bytes: appCacheTotalBytes.value },
  { id: 'review' as CleanupTab, label: '需复核', count: reviewItems.value.length, bytes: otherReviewBytes.value },
  { id: 'system' as CleanupTab, label: '系统工具', count: systemItems.value.length, bytes: 0 },
].filter(tab => tab.id === 'all' || tab.count > 0))
const hasScanForDrive = computed(() => !!result.value || !!resultsByDrive.value[selectedDrive.value])
const filteredDirectories = computed(() => {
  const needle = query.value.trim().toLocaleLowerCase()
  return result.value?.directories.filter(item => !needle || `${item.name} ${item.path}`.toLocaleLowerCase().includes(needle)) ?? []
})
const filteredFiles = computed(() => {
  const needle = query.value.trim().toLocaleLowerCase()
  let list = [...(result.value?.largeFiles ?? [])]
  if (needle) list = list.filter(item => `${item.name} ${item.path}`.toLocaleLowerCase().includes(needle))
  if (fileSizeFilter.value === '100mb') list = list.filter(item => item.size >= 100 * 1024 * 1024)
  if (fileSizeFilter.value === '500mb') list = list.filter(item => item.size >= 500 * 1024 * 1024)
  if (fileSizeFilter.value === '1gb') list = list.filter(item => item.size >= 1024 * 1024 * 1024)
  if (fileAgeFilter.value === 'year') list = list.filter(item => (item.modifiedDays ?? 0) >= 90)
  if (fileAgeFilter.value === 'old') list = list.filter(item => (item.modifiedDays ?? 0) >= 365)
  list.sort((a, b) => {
    if (fileSort.value === 'name') return a.name.localeCompare(b.name, 'zh-CN')
    if (fileSort.value === 'age') return (b.modifiedDays ?? -1) - (a.modifiedDays ?? -1)
    return b.size - a.size
  })
  return list
})
const selectedFileItems = computed(() => filteredFiles.value.filter(item => selectedFilePaths.value.includes(item.path)))
const selectedFileBytes = computed(() => selectedFileItems.value.reduce((sum, item) => sum + item.size, 0))
const allFilteredFilesSelected = computed(() => filteredFiles.value.length > 0 && filteredFiles.value.every(item => selectedFilePaths.value.includes(item.path)))
const maxDirectorySize = computed(() => Math.max(...filteredDirectories.value.map(item => item.size), 1))
const categoryTotal = computed(() => displayCategories.value.reduce((sum, item) => sum + item.size, 0))
const categoryGradient = computed(() => {
  if (!displayCategories.value.length || !categoryTotal.value) return '#e7eaee 0 100%'
  let cursor = 0
  return displayCategories.value.map(item => {
    const start = cursor
    cursor += item.size / categoryTotal.value * 100
    return `${item.color} ${start.toFixed(1)}% ${cursor.toFixed(1)}%`
  }).join(', ')
})
const maxFolderItemSize = computed(() => Math.max(...(folderAnalysis.value?.children.map(item => item.size) ?? []), 1))
const ageTotal = computed(() => result.value?.ageBuckets.reduce((sum, item) => sum + item.size, 0) ?? 0)
const maxAgeSize = computed(() => Math.max(...(result.value?.ageBuckets.map(item => item.size) ?? []), 1))
const longUnusedFiles = computed(() => result.value?.largeFiles.filter(file => (file.modifiedDays ?? 0) >= 365).sort((a, b) => b.size - a.size) ?? [])
const ageBucketFiles = computed(() => {
  if (!result.value || !selectedAgeBucket.value) return [] as LargeFile[]
  const id = selectedAgeBucket.value
  return result.value.largeFiles
    .filter(file => {
      const days = file.modifiedDays
      if (id === 'unknown') return days == null
      if (days == null) return false
      if (id === 'recent') return days < 30
      if (id === 'quarter') return days >= 30 && days < 90
      if (id === 'year') return days >= 90 && days < 365
      if (id === 'old') return days >= 365
      return false
    })
    .sort((a, b) => b.size - a.size)
})
const selectedDuplicateBytes = computed(() => {
  if (!duplicateReport.value) return 0
  let total = 0
  for (const group of duplicateReport.value.groups) {
    for (const file of group.files) {
      if (selectedDuplicatePaths.value.includes(file)) total += group.size
    }
  }
  return total
})
const selectedDuplicateCount = computed(() => selectedDuplicatePaths.value.length)
const recycleTargetPaths = computed(() => recycleSource.value === 'duplicates' ? selectedDuplicatePaths.value : selectedFilePaths.value)
const recycleTargetBytes = computed(() => recycleSource.value === 'duplicates' ? selectedDuplicateBytes.value : selectedFileBytes.value)
const filteredActivityLog = computed(() => {
  if (activityFilter.value === 'all') return activityLog.value
  return activityLog.value.filter(item => item.kind === activityFilter.value)
})
const unreadBadge = computed(() => Math.min(unreadActivityCount.value, 99))
const shadeOpen = computed(() => statusPanel.value === 'notify' || statusPanel.value === 'quick')
/** 通知中心列表：最多 12 条 */
const shadeNotifyItems = computed(() => activityLog.value.slice(0, 12))
const shadeStyle = computed(() => {
  // 全宽下拉帘：跟手拖动 + 松手弹性动画（非拖动时用 CSS transition）
  const sidebarWidth = sidebarCollapsed.value ? 72 : 232
  if (typeof document !== 'undefined') {
    document.documentElement.style.setProperty('--shade-left', `${sidebarWidth}px`)
  }
  const open = shadeOpen.value
  const dragging = pullDragging.value
  const travel = typeof window !== 'undefined' ? Math.min(window.innerHeight * 0.92, window.innerHeight - 24) : 640
  let dist = 0
  if (dragging) {
    dist = Math.max(0, Math.min(travel, pullDistance.value))
  } else if (open) {
    dist = travel
  }
  const visible = open || dragging || dist > 0
  return {
    left: `${sidebarWidth}px`,
    right: '0px',
    width: 'auto',
    maxHeight: '92vh',
    height: visible ? `${travel}px` : undefined,
    transform: `translateY(${visible ? dist - travel : -travel}px)`,
    transition: dragging
      ? 'none'
      : 'transform .32s cubic-bezier(.22,.9,.3,1), opacity .28s ease',
    opacity: visible ? 1 : 0,
    pointerEvents: (visible ? 'auto' : 'none') as 'auto' | 'none',
  }
})
const latestSnapshot = computed(() => snapshots.value[snapshots.value.length - 1] ?? null)
const previousSnapshot = computed(() => snapshots.value[snapshots.value.length - 2] ?? null)
const snapshotDelta = computed(() => latestSnapshot.value && previousSnapshot.value ? latestSnapshot.value.used - previousSnapshot.value.used : 0)
const snapshotDiff = computed(() => {
  const newer = latestSnapshot.value
  const older = previousSnapshot.value
  if (!newer || !older) return null as null | {
    usedDelta: number
    filesDelta: number
    dirGrown: Array<{ name: string; path: string; delta: number; size: number }>
    dirShrunk: Array<{ name: string; path: string; delta: number; size: number }>
    typeGrown: Array<{ name: string; delta: number; size: number }>
  }
  const oldDirs = new Map(older.directories.map(item => [item.path.toLowerCase(), item]))
  const dirChanges = newer.directories.map(item => {
    const prev = oldDirs.get(item.path.toLowerCase())
    const prevSize = prev?.size ?? 0
    return { name: item.name, path: item.path, size: item.size, delta: item.size - prevSize }
  })
  const oldTypes = new Map(older.fileTypes.map(item => [item.name, item.size]))
  const typeChanges = newer.fileTypes.map(item => ({
    name: item.name,
    size: item.size,
    delta: item.size - (oldTypes.get(item.name) ?? 0),
  }))
  return {
    usedDelta: newer.used - older.used,
    filesDelta: newer.scannedFiles - older.scannedFiles,
    dirGrown: [...dirChanges].filter(item => item.delta > 0).sort((a, b) => b.delta - a.delta).slice(0, 6),
    dirShrunk: [...dirChanges].filter(item => item.delta < 0).sort((a, b) => a.delta - b.delta).slice(0, 6),
    typeGrown: [...typeChanges].filter(item => item.delta !== 0).sort((a, b) => Math.abs(b.delta) - Math.abs(a.delta)).slice(0, 6),
  }
})
function fileKindLabel(name: string) {
  const ext = name.includes('.') ? name.split('.').pop()!.toLowerCase() : ''
  if (['exe', 'msi', 'msix', 'apk', 'dmg', 'pkg'].includes(ext)) return '安装包'
  if (['iso', 'img', 'vhd', 'vhdx', 'wim'].includes(ext)) return '镜像'
  if (['zip', '7z', 'rar', 'tar', 'gz', 'bz2', 'xz', 'cab'].includes(ext)) return '压缩包'
  if (['mp4', 'mkv', 'avi', 'mov', 'wmv', 'flv', 'webm', 'm4v'].includes(ext)) return '视频'
  if (['mp3', 'flac', 'wav', 'aac', 'm4a', 'ogg', 'wma'].includes(ext)) return '音频'
  if (['jpg', 'jpeg', 'png', 'gif', 'webp', 'bmp', 'tiff', 'psd', 'raw'].includes(ext)) return '图片'
  if (['pdf', 'doc', 'docx', 'xls', 'xlsx', 'ppt', 'pptx', 'txt', 'md'].includes(ext)) return '文档'
  return '其他'
}
const fileTypeGroups = computed(() => {
  const map = new Map<string, { name: string; count: number; size: number; files: LargeFile[] }>()
  for (const file of result.value?.largeFiles ?? []) {
    const kind = fileKindLabel(file.name)
    const entry = map.get(kind) ?? { name: kind, count: 0, size: 0, files: [] as LargeFile[] }
    entry.count += 1
    entry.size += file.size
    entry.files.push(file)
    map.set(kind, entry)
  }
  return [...map.values()].sort((a, b) => b.size - a.size)
})
const typedLargeFiles = computed(() => {
  if (fileTypeFilter.value === 'all') return filteredFiles.value
  return filteredFiles.value.filter(file => fileKindLabel(file.name) === fileTypeFilter.value)
})

function normalizeWinPath(path: string) {
  return path.replace(/\//g, '\\').replace(/\\+$/, '')
}

function classifyRegion(path: string): { id: string; label: string; color: string } {
  const p = normalizeWinPath(path).toLowerCase()
  const drive = p.slice(0, 2)
  if (p.includes('\\users\\') && (p.includes('\\appdata\\local\\temp') || p.includes('\\appdata\\local\\packages') || p.includes('\\appdata\\roaming'))) {
    return { id: 'user-cache', label: '用户缓存/应用数据', color: '#f59e0b' }
  }
  if (p.includes('\\users\\') && (p.includes('\\downloads') || p.includes('\\desktop') || p.includes('\\documents') || p.includes('\\videos') || p.includes('\\pictures') || p.includes('\\music'))) {
    return { id: 'user-files', label: '用户文档与下载', color: '#3b82f6' }
  }
  if (p.includes('\\users\\')) return { id: 'user-other', label: '其他用户目录', color: '#6366f1' }
  if (p.includes('\\program files') || p.includes('\\program files (x86)') || p.includes('\\windows\\') || p.includes('\\programdata\\')) {
    return { id: 'system-apps', label: '系统与已装程序', color: '#ef4444' }
  }
  if (p.includes('\\$recycle.bin') || p.includes('\\system volume information')) {
    return { id: 'system-meta', label: '系统元数据', color: '#94a3b8' }
  }
  // 数据盘常见开发/资料根
  if (/\\(projects?|code|repos?|dev|src|work|workspace|github|gitlab)\\/i.test(p) || p.includes('\\node_modules') || p.includes('\\target\\')) {
    return { id: 'projects', label: '项目与开发', color: '#10b981' }
  }
  if (p.includes('\\games') || p.includes('\\steam') || p.includes('\\epic') || p.includes('\\xbox')) {
    return { id: 'games', label: '游戏', color: '#a855f7' }
  }
  return { id: 'other', label: `${drive.toUpperCase()} 其他`, color: '#64748b' }
}

function detectProjectRoot(path: string): { key: string; name: string; root: string } | null {
  const full = normalizeWinPath(path)
  const parts = full.split('\\').filter(Boolean)
  if (parts.length < 2) return null
  // 从路径向上猜项目根：含 projects/code 等下的第一层子目录
  const lower = parts.map(p => p.toLowerCase())
  const hubIdx = lower.findIndex(p => ['projects', 'project', 'code', 'repos', 'repo', 'dev', 'work', 'workspace', 'github', 'src'].includes(p))
  if (hubIdx >= 0 && hubIdx + 1 < parts.length) {
    const root = parts.slice(0, hubIdx + 2).join('\\')
    return { key: root.toLowerCase(), name: parts[hubIdx + 1], root }
  }
  // node_modules / target 的父级
  const nm = lower.lastIndexOf('node_modules')
  if (nm > 0) {
    const root = parts.slice(0, nm).join('\\')
    return { key: root.toLowerCase(), name: parts[nm - 1], root }
  }
  const tg = lower.lastIndexOf('target')
  if (tg > 0) {
    const root = parts.slice(0, tg).join('\\')
    return { key: root.toLowerCase(), name: parts[tg - 1], root }
  }
  // 否则用盘符下第二层目录作弱项目
  if (parts.length >= 3 && parts[0].endsWith(':')) {
    const root = parts.slice(0, 2).join('\\')
    const name = parts[1]
    if (!['windows', 'program files', 'program files (x86)', 'programdata', 'users', '$recycle.bin'].includes(name.toLowerCase())) {
      return { key: root.toLowerCase(), name, root }
    }
  }
  return null
}

const regionAttribution = computed(() => {
  const map = new Map<string, { id: string; label: string; color: string; size: number; count: number; samples: string[] }>()
  const dirs = result.value?.directories ?? []
  for (const dir of dirs) {
    const region = classifyRegion(dir.path)
    const entry = map.get(region.id) ?? { ...region, size: 0, count: 0, samples: [] as string[] }
    entry.size += dir.size
    entry.count += 1
    if (entry.samples.length < 3) entry.samples.push(dir.path)
    map.set(region.id, entry)
  }
  // 补充大文件未进目录 TOP 的零头（按文件所属路径）
  for (const file of result.value?.largeFiles ?? []) {
    const region = classifyRegion(file.path)
    if (!map.has(region.id)) {
      map.set(region.id, { ...region, size: file.size, count: 1, samples: [file.path] })
    }
  }
  const list = [...map.values()].sort((a, b) => b.size - a.size)
  const total = list.reduce((s, i) => s + i.size, 0) || 1
  return list.map(item => ({ ...item, share: item.size / total }))
})

const projectAttribution = computed(() => {
  const map = new Map<string, { key: string; name: string; root: string; size: number; count: number; kinds: Set<string> }>()
  for (const dir of result.value?.directories ?? []) {
    const proj = detectProjectRoot(dir.path)
    if (!proj) continue
    const entry = map.get(proj.key) ?? { ...proj, size: 0, count: 0, kinds: new Set<string>() }
    entry.size += dir.size
    entry.count += 1
    if (dir.path.toLowerCase().includes('node_modules')) entry.kinds.add('Node 依赖')
    if (dir.path.toLowerCase().includes('\\target')) entry.kinds.add('构建产物')
    if (dir.name.toLowerCase() === 'dist' || dir.name.toLowerCase() === 'build') entry.kinds.add('产出目录')
    map.set(proj.key, entry)
  }
  for (const file of result.value?.largeFiles ?? []) {
    const proj = detectProjectRoot(file.path)
    if (!proj) continue
    const entry = map.get(proj.key) ?? { ...proj, size: 0, count: 0, kinds: new Set<string>() }
    // 避免与目录重复夸张双计：仅当项目尚未有目录条目时用文件补
    if (entry.count === 0) {
      entry.size += file.size
      entry.count += 1
    }
    entry.kinds.add(fileKindLabel(file.name))
    map.set(proj.key, entry)
  }
  return [...map.values()]
    .map(item => ({
      key: item.key,
      name: item.name,
      root: item.root,
      size: item.size,
      count: item.count,
      tags: [...item.kinds].slice(0, 3),
    }))
    .sort((a, b) => b.size - a.size)
    .slice(0, 12)
})

const attributionTotal = computed(() => regionAttribution.value.reduce((s, i) => s + i.size, 0))

const actionChecklist = computed(() => {
  const items: Array<{ id: string; title: string; detail: string; priority: 'high' | 'medium' | 'low'; action: () => void }> = []
  if ((cleanup.value?.safeBytes ?? 0) > 0) {
    items.push({
      id: 'cleanup',
      title: '清理中心可处理项',
      detail: `约 ${formatSize(cleanup.value!.safeBytes)}（白名单 + 开发可重建）`,
      priority: 'high',
      action: () => { page.value = 'cleanup' },
    })
  }
  const big = result.value?.largeFiles.filter(f => f.size >= 1024 ** 3).length ?? 0
  if (big > 0) {
    items.push({
      id: 'large',
      title: `${big} 个超过 1 GB 的文件`,
      detail: '可在文件审查中筛选并移入回收站',
      priority: 'high',
      action: () => { goLargeFilesReview(); fileSizeFilter.value = '1gb' },
    })
  }
  if ((duplicateReport.value?.wastedBytes ?? 0) > 0) {
    items.push({
      id: 'dup',
      title: '重复文件可释放空间',
      detail: `约 ${formatSize(duplicateReport.value!.wastedBytes)} · ${duplicateReport.value!.groups.length} 组`,
      priority: 'medium',
      action: () => { analysisTab.value = 'duplicates' },
    })
  } else {
    items.push({
      id: 'dup-scan',
      title: '检测重复文件',
      detail: '内容完全一致的副本，适合找可删副本',
      priority: 'medium',
      action: () => { analysisTab.value = 'duplicates' },
    })
  }
  if (longUnusedFiles.value.length) {
    items.push({
      id: 'old',
      title: `${longUnusedFiles.value.length} 个一年未改的大文件`,
      detail: '不等于可删，建议先确认用途',
      priority: 'low',
      action: () => { analysisTab.value = 'age'; selectedAgeBucket.value = 'old' },
    })
  }
  if (projectAttribution.value.length) {
    items.push({
      id: 'proj',
      title: '项目占用可下钻',
      detail: `识别到 ${projectAttribution.value.length} 个项目簇`,
      priority: 'low',
      action: () => { analysisTab.value = 'attribution'; attributionFocus.value = 'projects' },
    })
  }
  return items
})

const pageTitle = computed(() => ({ overview: '空间概览', cleanup: '清理中心', files: '文件审查', insights: '深度分析', media: '媒体管理', registry: '注册表检查' }[page.value]))
const scanOptions = computed(() => ({
  exclusions: exclusionPaths.value,
  largeFileBytes: largeFileMb.value * 1024 * 1024,
}))

function formatSize(bytes = 0) {
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  let value = bytes
  let unit = 0
  while (value >= 1024 && unit < units.length - 1) { value /= 1024; unit += 1 }
  return `${value.toFixed(unit < 2 ? 0 : 1)} ${units[unit]}`
}

function formatCount(value = 0) { return new Intl.NumberFormat('zh-CN').format(value) }

function refreshActivityLog() {
  activityLog.value = loadActivityLog()
}

function loadUnreadCount() {
  const n = Number(localStorage.getItem('disk-analyzer-activity-unread') || '0')
  unreadActivityCount.value = Number.isFinite(n) && n > 0 ? Math.floor(n) : 0
}

function persistUnreadCount() {
  localStorage.setItem('disk-analyzer-activity-unread', String(unreadActivityCount.value))
}

function bumpUnread() {
  unreadActivityCount.value += 1
  persistUnreadCount()
}

function markActivitySeen() {
  if (unreadActivityCount.value === 0) return
  unreadActivityCount.value = 0
  persistUnreadCount()
}

function openActivityLog(filter: 'all' | ActivityKind = 'all') {
  activityFilter.value = filter
  selectedActivity.value = null
  refreshActivityLog()
  markActivitySeen()
  statusPanel.value = 'none'
  pullDistance.value = 0
  showMessageLog.value = true
}

function openActivityDetail(item: ActivityEntry, opts?: { removeFromLog?: boolean }) {
  selectedActivity.value = item
  markActivitySeen()
  // 通知中心点击：从列表移除（活动日志里也不再显示该条）
  if (opts?.removeFromLog !== false && statusPanel.value === 'notify') {
    removeActivity(item.id)
    refreshActivityLog()
  }
  statusPanel.value = 'none'
  pullDistance.value = 0
  showMessageLog.value = true
}

/** 清空通知中心列表（静默，不写入新通知） */
function clearShadeNotifications() {
  clearActivityLog()
  refreshActivityLog()
  unreadActivityCount.value = 0
  persistUnreadCount()
}

function openLatestNoticeDetail() {
  refreshActivityLog()
  const latest = activityLog.value.find(item => item.kind === 'notice' || item.kind === 'error') || activityLog.value[0]
  if (latest) openActivityDetail(latest)
  else openActivityLog()
}

function toggleStatusPanel(which: 'notify' | 'quick') {
  statusPanel.value = statusPanel.value === which ? 'none' : which
  if (statusPanel.value === 'notify') {
    refreshActivityLog()
    markActivitySeen()
  }
  pullDistance.value = 0
}

function closeStatusPanel() {
  statusPanel.value = 'none'
  pullDistance.value = 0
  pullDragging.value = false
}

function openActivityInSettings() {
  showMessageLog.value = false
  selectedActivity.value = null
  statusPanel.value = 'none'
  pullDistance.value = 0
  markActivitySeen()
  settingsTab.value = 'activity'
  showSettings.value = true
  refreshActivityLog()
}

function shadeTravel() {
  return Math.min(window.innerHeight * 0.92, window.innerHeight - 24)
}

function detachPullListeners() {
  window.removeEventListener('pointermove', onPullMove)
  window.removeEventListener('pointerup', onPullEnd)
  window.removeEventListener('pointercancel', onPullEnd)
}

/** 安卓式：打开后可在帘底部空白/底栏/手柄处上滑关闭；关闭时顶部下拉打开 */
function onPullStart(event: PointerEvent, side?: 'left' | 'right') {
  if (showSettings.value || showMessageLog.value || confirmCleanup.value || confirmRecycleFiles.value || showRecycleBin.value || showCleanupHistory.value) return
  const target = event.target as HTMLElement | null
  const open = statusPanel.value === 'notify' || statusPanel.value === 'quick'
  if (open) {
    // 列表条目/控件不抢拖动手势；底栏、手柄、标题空白、帘底部可上滑
    if (target?.closest('button, a, input, select, textarea, .status-item, .quick-link, .toggle-switch, .chip-btn, .quick-toggle, .quick-slider')) return
    if (!target?.closest('.shade-handle, .shade-head, .shade-foot, .shade-dismiss, .status-shade, .pull-edge')) return
  } else {
    if (target?.closest('.modal-backdrop, .settings-backdrop, button, input, select, a, .table-wrap, .dialog-actions')) {
      if (!target?.closest('.pull-edge')) return
    }
    if (event.clientY > 56 && !target?.closest('.pull-edge')) return
  }
  const width = window.innerWidth || 1200
  const sidebarWidth = sidebarCollapsed.value ? 72 : 232
  const contentX = event.clientX - sidebarWidth
  const contentW = Math.max(320, width - sidebarWidth)
  if (open) {
    pullSide.value = statusPanel.value === 'quick' ? 'right' : 'left'
  } else {
    pullSide.value = side || (contentX < contentW / 2 ? 'left' : 'right')
  }
  pullActive = true
  pullDragging.value = true
  pullStartY = event.clientY
  const travel = shadeTravel()
  pullDistance.value = open ? travel : 0
  try {
    (event.currentTarget as HTMLElement)?.setPointerCapture?.(event.pointerId)
  } catch { /* ignore */ }
  detachPullListeners()
  window.addEventListener('pointermove', onPullMove, { passive: false })
  window.addEventListener('pointerup', onPullEnd)
  window.addEventListener('pointercancel', onPullEnd)
}

function onPullMove(event: PointerEvent) {
  if (!pullActive) return
  event.preventDefault?.()
  const delta = event.clientY - pullStartY
  const travel = shadeTravel()
  const open = statusPanel.value === 'notify' || statusPanel.value === 'quick'
  if (open) {
    // 上滑 delta 为负 → 距离减小 → 帘跟手上移
    pullDistance.value = Math.max(0, Math.min(travel, travel + delta))
  } else {
    pullDistance.value = Math.max(0, Math.min(travel, delta))
  }
}

function onPullEnd() {
  if (!pullActive) return
  pullActive = false
  const travel = shadeTravel()
  const open = statusPanel.value === 'notify' || statusPanel.value === 'quick'
  const dist = pullDistance.value
  detachPullListeners()
  // 先结束 dragging，让 transition 接管弹性动画
  pullDragging.value = false
  if (open) {
    // 轻推上滑超过约 12% 即关闭（安卓感）
    if (dist < travel * 0.88) {
      closeStatusPanel()
    } else {
      pullDistance.value = travel
    }
  } else if (dist > Math.min(72, travel * 0.07)) {
    statusPanel.value = pullSide.value === 'right' ? 'quick' : 'notify'
    if (statusPanel.value === 'notify') {
      refreshActivityLog()
      markActivitySeen()
    }
    pullDistance.value = travel
  } else {
    pullDistance.value = 0
  }
}

function openPathFromMeta(path: unknown) {
  if (typeof path === 'string' && path.trim()) void openPath(path, false)
}

function activityMetaLines(item: ActivityEntry) {
  if (!item.meta) return [] as Array<{ key: string; value: string }>
  return Object.entries(item.meta)
    .filter(([, v]) => v !== undefined && v !== null && String(v).length)
    .map(([key, value]) => ({ key, value: String(value) }))
}

function exportActivityLogFile() {
  const text = exportActivityLogText(activityLog.value)
  const blob = new Blob([text || '（暂无记录）'], { type: 'text/plain;charset=utf-8' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = `disk-analyzer-activity-${new Date().toISOString().slice(0, 19).replace(/[:T]/g, '-')}.txt`
  a.click()
  URL.revokeObjectURL(url)
  showNotice('活动日志已导出到下载目录（浏览器/系统默认下载位置）')
}

function clearAllActivity() {
  clearActivityLog()
  unreadActivityCount.value = 0
  persistUnreadCount()
  refreshActivityLog()
  showNotice('活动日志已清空')
}

function dismissNotice(immediate = false) {
  if (noticeTimer) clearTimeout(noticeTimer)
  if (noticeFadeTimer) clearTimeout(noticeFadeTimer)
  noticeTimer = undefined
  noticeFadeTimer = undefined
  if (immediate || !notice.value) {
    notice.value = ''
    noticeFading.value = false
    return
  }
  noticeFading.value = true
  noticeFadeTimer = setTimeout(() => {
    notice.value = ''
    noticeFading.value = false
  }, 420)
}

function dismissError(immediate = false) {
  if (errorTimer) clearTimeout(errorTimer)
  if (errorFadeTimer) clearTimeout(errorFadeTimer)
  errorTimer = undefined
  errorFadeTimer = undefined
  if (immediate || !error.value) {
    error.value = ''
    errorFading.value = false
    return
  }
  errorFading.value = true
  errorFadeTimer = setTimeout(() => {
    error.value = ''
    errorFading.value = false
  }, 420)
}

function showNotice(
  text: string,
  holdMs = 4200,
  kind: ActivityKind = 'notice',
  detail?: string,
  meta?: ActivityEntry['meta'],
) {
  if (noticeTimer) clearTimeout(noticeTimer)
  if (noticeFadeTimer) clearTimeout(noticeFadeTimer)
  noticeFading.value = false
  notice.value = text
  appendActivity(kind, text, detail, meta)
  bumpUnread()
  refreshActivityLog()
  noticeTimer = setTimeout(() => dismissNotice(false), holdMs)
}

function handleError(value: unknown) {
  const text = typeof value === 'string' ? value : value instanceof Error ? value.message : '操作未能完成'
  if (errorTimer) clearTimeout(errorTimer)
  if (errorFadeTimer) clearTimeout(errorFadeTimer)
  errorFading.value = false
  error.value = text
  appendActivity('error', text)
  bumpUnread()
  refreshActivityLog()
  errorTimer = setTimeout(() => dismissError(false), 8000)
}

function toggleDetailedActivityLog() {
  detailedActivityLog.value = !detailedActivityLog.value
  setDetailedActivityLogEnabled(detailedActivityLog.value)
  showNotice(
    detailedActivityLog.value
      ? '已开启详细活动记录：清理/回收等会记下项目清单（本机存储，条目有上限）'
      : '已关闭详细活动记录：仅保存摘要，更省空间',
    4200,
    'system',
  )
}
function applyTheme(theme: ThemeId) {
  activeTheme.value = theme
  document.documentElement.dataset.accent = theme
  localStorage.setItem('disk-analyzer-theme', theme)
}

function applyGlassStrength(value: number) {
  const n = Math.max(0, Math.min(100, Math.round(value)))
  glassStrength.value = n
  // 映射到白底占比：更透 28% → 更实 82%
  const pct = Math.round(28 + n * 0.54)
  document.documentElement.style.setProperty('--u1-glass-pct', String(pct))
  document.documentElement.style.setProperty('--u1-glass-pct-strong', String(Math.min(92, pct + 14)))
  document.documentElement.style.setProperty('--u1-blur-px', `${Math.round(12 + (100 - n) * 0.14)}px`)
  localStorage.setItem('disk-analyzer-glass-strength', String(n))
}

function applyBorderStrength(value: number) {
  const n = Math.max(0, Math.min(100, Math.round(value)))
  borderStrength.value = n
  // 用深色基线 #52606f，避免浅色 sidebar-border 导致“几乎看不出”
  // 0 → 完全透明；100 → 接近实线深灰
  const alpha = Math.round(n * 0.95)
  const hair = Math.round(n * 0.5)
  const chip = Math.round(n * 0.88)
  const width = n < 8 ? 0 : n < 40 ? 1 : n < 75 ? 1.2 : 1.5
  document.documentElement.style.setProperty('--u1-border-a', String(alpha))
  document.documentElement.style.setProperty('--u1-border-hair-a', String(hair))
  document.documentElement.style.setProperty('--u1-border-chip-a', String(chip))
  document.documentElement.style.setProperty('--u1-border-w', `${width}px`)
  document.documentElement.style.setProperty(
    '--u1-border',
    alpha <= 0 ? 'transparent' : `color-mix(in srgb, #52606f ${alpha}%, transparent)`,
  )
  document.documentElement.style.setProperty(
    '--u1-border-hair',
    hair <= 0 ? 'transparent' : `color-mix(in srgb, #667085 ${hair}%, transparent)`,
  )
  document.documentElement.style.setProperty(
    '--u1-border-chip',
    chip <= 0 ? 'transparent' : `color-mix(in srgb, #475467 ${chip}%, transparent)`,
  )
  document.documentElement.style.setProperty(
    '--u1-glass-border',
    alpha <= 0 ? 'transparent' : `color-mix(in srgb, #52606f ${Math.round(alpha * 0.75)}%, transparent)`,
  )
  // 用 data 属性辅助 CSS 极值样式
  document.documentElement.dataset.borderLevel = n < 12 ? 'none' : n < 45 ? 'soft' : n < 78 ? 'medium' : 'hard'
  localStorage.setItem('disk-analyzer-border-strength', String(n))
}
function applyFontScale(value: FontScale) {
  fontScale.value = value
  document.documentElement.dataset.fontSize = value
  localStorage.setItem('disk-analyzer-font-scale', value)
}
function applyIconScale(value: IconScale) {
  iconScale.value = value
  document.documentElement.dataset.iconSize = value
  localStorage.setItem('disk-analyzer-icon-scale', value)
}
function applyDensity(value: UiDensity) {
  uiDensity.value = value
  document.documentElement.dataset.density = value
  localStorage.setItem('disk-analyzer-density', value)
}
function advancedSettings(): AdvancedSettings {
  return {
    exclusions: exclusionPaths.value,
    cleanupBlacklist: cleanupBlacklist.value,
    largeFileMb: largeFileMb.value,
    scanThreads: scanThreads.value,
    snapshotLimit: snapshotLimit.value,
    reportDirectory: reportDirectory.value,
    recyclePolicy: recyclePolicy.value,
    autoCheckUpdates: autoCheckUpdates.value,
  }
}
function loadAdvancedSettings() {
  try {
    const saved = JSON.parse(localStorage.getItem('disk-analyzer-advanced-settings') || '{}') as Partial<AdvancedSettings>
    exclusionPaths.value = Array.isArray(saved.exclusions) ? saved.exclusions.filter(value => typeof value === 'string') : []
    cleanupBlacklist.value = Array.isArray(saved.cleanupBlacklist) ? saved.cleanupBlacklist.filter(value => typeof value === 'string') : []
    largeFileMb.value = [50, 100, 500, 1024].includes(saved.largeFileMb ?? 0) ? saved.largeFileMb! : 100
    scanThreads.value = [2, 4, 6, 8].includes(saved.scanThreads ?? 0) ? saved.scanThreads! : 6
    snapshotLimit.value = [10, 30, 60, 100].includes(saved.snapshotLimit ?? 0) ? saved.snapshotLimit! : 30
    reportDirectory.value = typeof saved.reportDirectory === 'string' ? saved.reportDirectory : ''
    recyclePolicy.value = saved.recyclePolicy === 'direct' ? 'direct' : 'confirm'
    autoCheckUpdates.value = saved.autoCheckUpdates !== false
  } catch { localStorage.removeItem('disk-analyzer-advanced-settings') }
}
function persistAdvancedSettings() {
  localStorage.setItem('disk-analyzer-advanced-settings', JSON.stringify(advancedSettings()))
}
function toggleSidebar() {
  sidebarCollapsed.value = !sidebarCollapsed.value
  localStorage.setItem('disk-analyzer-sidebar-collapsed', String(sidebarCollapsed.value))
  document.documentElement.style.setProperty('--shade-left', `${sidebarCollapsed.value ? 72 : 232}px`)
}

function openMediaCenter() {
  page.value = 'media'
  mediaNew.value = false
  localStorage.setItem('disk-analyzer-seen-media-v6', '1')
}

async function refreshUsage() {
  usage.value = null
  if (!isTauri) {
    usage.value = { total: 321_156_481_536, used: 250_181_844_992, free: 70_974_636_544 }
    return
  }
  try { usage.value = await invoke<DiskUsage>('get_disk_usage', { drive: selectedDrive.value }) }
  catch (value) { handleError(value) }
}

async function loadDrives() {
  loadingDrives.value = true
  error.value = ''
  try {
    if (isTauri) {
      const detected = await Promise.race<string[]>([
        invoke<string[]>('get_drives'),
        new Promise<string[]>(resolve => window.setTimeout(() => resolve([]), 1500)),
      ])
      drives.value = detected.length ? detected : ['C:']
      if (!detected.length) showNotice('盘符检测响应超时，已使用系统盘 C:，不影响扫描。')
    } else drives.value = ['C:', 'D:', 'E:']
    if (!drives.value.includes(selectedDrive.value)) selectedDrive.value = drives.value[0]
  } catch (value) { handleError(value) }
  finally { loadingDrives.value = false }
  await refreshUsage()
}

function itemBelongsToDrive(item: CleanupItem, drive: string): boolean {
  const letter = drive.trim().replace(/[:\\/]/g, '').toUpperCase()
  if (!letter) return false
  // 系统清理入口只挂在 C:
  if (item.action === 'system') return letter === 'C'
  const path = (item.path || '').trim()
  if (!path || path.startsWith('Windows ')) return letter === 'C'
  return path.length >= 2 && path[1] === ':' && path[0].toUpperCase() === letter
}

function filterCleanupForDrive(report: CleanupReport, drive: string): CleanupReport {
  const items = report.items.filter(item => itemBelongsToDrive(item, drive))
  const safeBytes = items.filter(item => item.action === 'safe').reduce((sum, item) => sum + item.size, 0)
  const reviewBytes = items.filter(item => item.action === 'review').reduce((sum, item) => sum + item.size, 0)
  const developerBytes = items.filter(item => item.category === 'developer').reduce((sum, item) => sum + item.size, 0)
  const toolAiBytes = items.filter(item => item.category === 'toolai').reduce((sum, item) => sum + item.size, 0)
  const appCacheBytes = items.filter(item => item.category === 'app').reduce((sum, item) => sum + item.size, 0)
  return {
    items,
    safeBytes,
    reviewBytes,
    developerBytes,
    toolAiBytes,
    appCacheBytes,
    rulepackVersion: report.rulepackVersion || '',
  }
}


/** 仅保护「具体项目路径」，禁止把 Users / 用户主目录 等宽前缀当 S0（否则工具/AI 全被滤掉显示 0B） */
function isBroadProtectPath(path: string): boolean {
  const n = path.trim().replace(/[\\/]+$/, '').toLowerCase()
  if (!n || n.length < 6) return true
  // 盘符根 C: 或 C:\
  if (/^[a-z]:$/i.test(n) || /^[a-z]:\\$/i.test(path.trim())) return true
  const parts = n.split(/[\\/]/).filter(Boolean)
  // 段数太少：C:\Users、C:\Users\name、C:\Program Files 等
  if (parts.length <= 2) return true
  // 用户主目录本身（C:\Users\Administrator）
  if (parts.length === 3 && parts[1] === 'users') return true
  // AppData 根与常见工具缓存根：绝不因 TOP 目录而整树保护
  if (n.includes('\\appdata\\local') && parts.length <= 5) return true
  if (n.includes('\\appdata\\roaming') && parts.length <= 5) return true
  if (parts[parts.length - 1] === 'appdata') return true
  return false
}

/** 当前打开项目 / 足够深的扫描 TOP → S0；黑名单仍完整生效 */
function buildProtectPrefixes(): string[] {
  const prefixes: string[] = []
  const push = (p?: string | null, force = false) => {
    if (!p) return
    const n = p.trim().replace(/[\\/]+$/, '')
    if (n.length < 4) return
    if (!force && isBroadProtectPath(n)) return
    if (!prefixes.some(x => x.toLowerCase() === n.toLowerCase())) prefixes.push(n)
  }
  // 文件夹下钻：仅当路径足够深（像具体项目）
  push(folderAnalysis.value?.path)
  // 扫描 TOP：只收「像项目」的深路径，不收 Users 根
  for (const d of (result.value?.directories ?? []).slice(0, 12)) {
    const p = d.path || ''
    const lower = p.toLowerCase()
    const projectLike = /\\(projects?|code|repos?|dev|src|work|workspace|github|gitlab|desktop|documents)\\/i.test(p)
      || lower.includes('\\node_modules')
      || lower.includes('\\.git')
    if (projectLike || p.split(/[\\/]/).filter(Boolean).length >= 5) push(p)
  }
  // 用户清理黑名单：强制加入（用户明确排除）
  for (const b of cleanupBlacklist.value) push(b, true)
  try {
    const raw = localStorage.getItem('disk-analyzer-protect-prefixes')
    if (raw) {
      const arr = JSON.parse(raw) as string[]
      if (Array.isArray(arr)) arr.forEach(p => push(p, true))
    }
  } catch { /* ignore */ }
  return prefixes
}
async function loadCleanup() {
  const drive = selectedDrive.value
  const requestId = ++cleanupRequestId
  if (!isTauri) {
    const isC = drive.toUpperCase() === 'C:'
    const preview: CleanupReport = {
      safeBytes: isC ? 4_563_402_752 : 3_758_096_384,
      reviewBytes: isC ? 12_884_901_888 : 0,
      developerBytes: isC ? 0 : 3_758_096_384,
      items: [
        ...(isC
          ? [
              { id: 'user-temp', name: '用户临时文件', description: '应用安装、解压和运行产生的过期临时文件', path: 'C:\\Users\\User\\AppData\\Local\\Temp', size: 1_827_160_064, fileCount: 2841, action: 'safe' as const, risk: 'low' as const, category: 'fixed' as const },
              { id: 'browser-cache', name: '浏览器缓存', description: 'Chrome 与 Edge 可重新下载的网页缓存，清理前建议关闭浏览器', path: 'C:\\Users\\User\\AppData\\Local\\Google\\Chrome\\User Data\\Default\\Cache', size: 2_521_653_248, fileCount: 1743, action: 'safe' as const, risk: 'low' as const, category: 'fixed' as const },
              { id: 'crash-dumps', name: '程序崩溃转储', description: '用于故障诊断的旧转储文件，不影响程序正常运行', path: 'C:\\Users\\User\\AppData\\Local\\CrashDumps', size: 214_589_440, fileCount: 8, action: 'safe' as const, risk: 'low' as const, category: 'fixed' as const },
              { id: 'large-downloads', name: '下载目录大文件', description: '下载目录中超过 100 MB 的内容，需要确认用途后手动处理', path: 'C:\\Users\\User\\Downloads', size: 12_884_901_888, fileCount: 11, action: 'review' as const, risk: 'medium' as const, category: 'fixed' as const },
              { id: 'windows-storage', name: 'Windows 系统清理', description: '更新缓存、旧系统文件和回收站应交给 Windows 存储设置处理', path: 'Windows 设置 > 系统 > 存储 > 临时文件', size: 0, fileCount: 0, action: 'system' as const, risk: 'medium' as const, category: 'fixed' as const },
            ]
          : []),
        { id: `dev:${drive}\\Projects\\app\\node_modules`, name: 'Node.js 依赖 · node_modules', description: '前端/Node 依赖目录；邻居验证通过；将整目录移入回收站，可还原', path: `${drive}\\Projects\\app\\node_modules`, size: 2_147_483_648, fileCount: 42000, action: 'safe', risk: 'low', category: 'developer' },
        { id: `dev:${drive}\\Projects\\rust-app\\target`, name: '构建产物 (target) · target', description: 'Rust 构建输出；邻居验证通过；将整目录移入回收站，可还原', path: `${drive}\\Projects\\rust-app\\target`, size: 1_610_612_736, fileCount: 18000, action: 'safe', risk: 'low', category: 'developer' },
      ],
    }
    preview.developerBytes = preview.items.filter(i => i.category === 'developer').reduce((s, i) => s + i.size, 0)
    preview.safeBytes = preview.items.filter(i => i.action === 'safe').reduce((s, i) => s + i.size, 0)
    if (requestId !== cleanupRequestId || selectedDrive.value !== drive) return
    cleanup.value = filterCleanupForDrive(preview, drive)
    cleanupByDrive.value = { ...cleanupByDrive.value, [drive]: cleanup.value }
    selectedCleanup.value = cleanup.value.items.filter(item => item.action === 'safe' && item.category === 'fixed' && item.size > 0).map(item => item.id)
    selectedCleanupByDrive.value = { ...selectedCleanupByDrive.value, [drive]: [...selectedCleanup.value] }
    return
  }
  loadingCleanup.value = true
  try {
    let report: CleanupReport
    try {
      report = await invoke<CleanupReport>('analyze_cleanup', {
        drive,
        options: {
          blacklist: cleanupBlacklist.value,
          protectPrefixes: buildProtectPrefixes(),
        },
      })
    } catch {
      // 兼容旧参数签名
      report = await invoke<CleanupReport>('analyze_cleanup', { drive })
    }
    // 丢弃过期请求，避免从 C 切到 E 时旧结果写回
    if (requestId !== cleanupRequestId || selectedDrive.value !== drive) return
    cleanup.value = filterCleanupForDrive(report, drive)
    cleanupByDrive.value = { ...cleanupByDrive.value, [drive]: cleanup.value }
    const devCount = cleanup.value.items.filter(item => item.category === 'developer').length
    if (devCount > 0 || cleanup.value.items.length) {
      const names = cleanup.value.items.map(item =>
        `${item.name}${item.category === 'developer' ? '（开发）' : ''} · ${formatSize(item.size)}`,
      )
      const detail = detailedActivityLog.value
        ? formatDetailLines([
            `盘符 ${drive}`,
            `合计 ${cleanup.value.items.length} 项 · 可处理 ${formatSize(cleanup.value.safeBytes)} · 开发 ${formatSize(cleanup.value.developerBytes)}`,
            ...names,
          ])
        : `盘符 ${drive} · 可处理 ${formatSize(cleanup.value.safeBytes)} · 开发目录 ${devCount} 个`
      showNotice(
        `清理分析完成：${cleanup.value.items.length} 项（含 ${devCount} 个开发可重建目录）`,
        4200,
        'cleanup',
        detail,
        { drive, itemCount: cleanup.value.items.length, developerCount: devCount },
      )
    }
    selectedCleanup.value = cleanup.value.items
      .filter(item => item.action === 'safe' && item.size > 0 && item.category === 'fixed')
      .map(item => item.id)
    selectedCleanupByDrive.value = { ...selectedCleanupByDrive.value, [drive]: [...selectedCleanup.value] }
    ensureCleanupTabVisible()
  } catch (value) {
    if (requestId === cleanupRequestId) handleError(value)
  } finally {
    if (requestId === cleanupRequestId) loadingCleanup.value = false
  }
}

function persistCurrentDriveState() {
  const drive = selectedDrive.value
  if (result.value) resultsByDrive.value = { ...resultsByDrive.value, [drive]: result.value }
  if (cleanup.value) cleanupByDrive.value = { ...cleanupByDrive.value, [drive]: cleanup.value }
  selectedCleanupByDrive.value = { ...selectedCleanupByDrive.value, [drive]: [...selectedCleanup.value] }
}

async function selectDrive(drive: string) {
  if (drive === selectedDrive.value) return
  persistCurrentDriveState()
  selectedDrive.value = drive
  // 恢复该盘上次完整扫描与清理结果；无缓存则空状态，需重新扫描
  result.value = resultsByDrive.value[drive] ?? null
  cleanup.value = cleanupByDrive.value[drive] ?? null
  selectedCleanup.value = selectedCleanupByDrive.value[drive] ? [...selectedCleanupByDrive.value[drive]] : []
  loadingCleanup.value = false
  cleanupRequestId += 1
  folderAnalysis.value = null
  folderHistory.value = []
  page.value = 'overview'
  await refreshUsage()
  // 不在此处 loadCleanup：仅完整扫描后写入缓存
  void loadSnapshots()
}

async function startScan(resume = false) {
  const drive = selectedDrive.value
  if (!isTauri) {
    result.value = buildPreviewScan(drive)
    resultsByDrive.value = { ...resultsByDrive.value, [drive]: result.value }
    usage.value = result.value.usage
    await loadSnapshots()
    await loadCleanup()
    showNotice('当前显示界面预览数据；Tauri 程序会读取真实磁盘。')
    return
  }
  scanning.value = true
  result.value = null
  cleanup.value = null
  pendingResume.value = null
  selectedCleanup.value = []
  selectedFilePaths.value = []
  selectedDuplicatePaths.value = []
  selectedAgeBucket.value = null
  // 清除本盘旧缓存，避免展示过期清理项
  const nextResults = { ...resultsByDrive.value }
  const nextCleanup = { ...cleanupByDrive.value }
  delete nextResults[drive]
  delete nextCleanup[drive]
  resultsByDrive.value = nextResults
  cleanupByDrive.value = nextCleanup
  error.value = ''
  dismissNotice(true)
  query.value = ''
  progress.value = { message: '正在启动完整扫描', percentage: 1 }
  try {
    result.value = await invoke<ScanResult>('start_scan', { drive, options: scanOptions.value, resume })
    resultsByDrive.value = { ...resultsByDrive.value, [drive]: result.value }
    usage.value = result.value.usage
    try {
      await invoke<ScanSnapshot>('save_snapshot', { result: result.value, limit: snapshotLimit.value })
      await loadSnapshots()
    } catch (snapshotError) {
      showNotice(`扫描完成，但历史快照保存失败：${String(snapshotError)}`, 5000, 'scan')
    }
    // 整盘扫描完成后再分析可清理项并写入按盘缓存
    await loadCleanup()
    if (result.value) {
      const topDirs = result.value.directories.slice(0, 8).map((d, i) =>
        `${i + 1}. ${d.name} · ${formatSize(d.size)} · ${d.path}`,
      )
      const topFiles = result.value.largeFiles.slice(0, 8).map((f, i) =>
        `${i + 1}. ${f.name} · ${formatSize(f.size)}`,
      )
      const scanDetail = detailedActivityLog.value
        ? formatDetailLines([
            `盘符 ${drive}`,
            `已用 ${formatSize(result.value.usage.used)} / 共 ${formatSize(result.value.usage.total)}（${Math.round(result.value.usage.used / Math.max(result.value.usage.total, 1) * 100)}%）`,
            `文件 ${formatCount(result.value.scannedFiles)} · 目录 ${formatCount(result.value.scannedDirs)} · 用时 ${(result.value.elapsedMs / 1000).toFixed(1)}s · 跳过 ${formatCount(result.value.skippedItems)}`,
            '目录 TOP：',
            ...topDirs,
            '大文件 TOP：',
            ...topFiles,
          ])
        : `盘符 ${drive} · 已用 ${formatSize(result.value.usage.used)} · 文件 ${formatCount(result.value.scannedFiles)} · ${(result.value.elapsedMs / 1000).toFixed(1)}s`
      showNotice(
        `扫描完成：${drive} 已用 ${formatSize(result.value.usage.used)}，发现 ${result.value.largeFiles.length} 个大文件`,
        4800,
        'scan',
        scanDetail,
        {
          drive,
          scannedFiles: result.value.scannedFiles,
          largeFiles: result.value.largeFiles.length,
          elapsedMs: result.value.elapsedMs,
        },
      )
    }
  } catch (value) {
    if (String(value).includes('扫描已取消')) showNotice('扫描已取消。进度已保存，下次可点「继续扫描」接着扫。', 4200, 'scan')
    else handleError(value)
  } finally { scanning.value = false }
  checkPendingResume()
}

interface PendingResumeState {
  drive: string
  startedAt?: string
  completedRoots?: unknown[]
  completedFiles?: number
}

const pendingResume = ref<PendingResumeState | null>(null)

async function checkPendingResume() {
  if (!isTauri) return
  try {
    const state = await invoke<PendingResumeState | null>('has_pending_scan')
    pendingResume.value = state
  } catch { pendingResume.value = null }
}

function resumeScan() {
  if (pendingResume.value) selectedDrive.value = pendingResume.value.drive
  void startScan(true)
}

function buildPreviewScan(drive: string): ScanResult {
  const directories = [
    { path: `${drive}\\Media`, name: 'Media', size: 92_342_435_840, fileCount: 18420, dirCount: 814 },
    { path: `${drive}\\Backups`, name: 'Backups', size: 61_816_012_800, fileCount: 3420, dirCount: 96 },
    { path: `${drive}\\Games`, name: 'Games', size: 42_949_672_960, fileCount: 82040, dirCount: 3840 },
    { path: `${drive}\\Projects`, name: 'Projects', size: 13_958_643_712, fileCount: 40580, dirCount: 7290 },
  ]
  return {
    drive,
    usage: { total: 320_083_263_488, used: 217_325_846_528, free: 102_757_416_960 },
    directories,
    largeFiles: [
      { path: `${drive}\\Media\\video-archive.mkv`, name: 'video-archive.mkv', size: 12_884_901_888, modifiedDays: 742 },
      { path: `${drive}\\Backups\\backup.iso`, name: 'backup.iso', size: 8_589_934_592, modifiedDays: 518 },
    ],
    categories: [{ name: '其他', size: 211_066_765_312, color: '#64748b' }],
    fileTypes: [
      { name: '视频', size: 78_315_929_600, color: '#8b5cf6' },
      { name: '压缩与镜像', size: 55_834_574_848, color: '#eab308' },
      { name: '程序文件', size: 38_654_705_664, color: '#ef4444' },
      { name: '图片', size: 21_474_836_480, color: '#22c55e' },
      { name: '开发文件', size: 16_106_127_360, color: '#06b6d4' },
      { name: '其他文件', size: 6_939_672_576, color: '#64748b' },
    ],
    ageBuckets: [
      { id: 'recent', label: '最近 30 天', size: 47_244_640_256, fileCount: 48620, color: '#22c55e' },
      { id: 'quarter', label: '30–90 天', size: 32_212_254_720, fileCount: 31840, color: '#3b82f6' },
      { id: 'year', label: '90–365 天', size: 61_847_347_200, fileCount: 42900, color: '#eab308' },
      { id: 'old', label: '超过 1 年', size: 76_021_604_352, fileCount: 21080, color: '#ef4444' },
      { id: 'unknown', label: '时间未知', size: 0, fileCount: 20, color: '#64748b' },
    ],
    scannedFiles: 144460,
    scannedDirs: 12040,
    elapsedMs: 8420,
    skippedItems: 2,
  }
}

function buildPreviewSnapshots(drive: string): ScanSnapshot[] {
  const usedValues = [188, 194, 201, 198, 202.4].map(value => value * 1024 ** 3)
  return usedValues.map((used, index) => ({
    id: `preview-${index}`,
    createdAt: new Date(Date.now() - (usedValues.length - 1 - index) * 7 * 86_400_000).toISOString(),
    drive,
    total: 298.1 * 1024 ** 3,
    used,
    free: 298.1 * 1024 ** 3 - used,
    scannedFiles: 130_000 + index * 3_600,
    directories: [{ path: `${drive}\\Media`, name: 'Media', size: (72 + index * 3.5) * 1024 ** 3, fileCount: 18000, dirCount: 800 }],
    fileTypes: [],
    ageBuckets: [],
  }))
}

async function loadSnapshots() {
  snapshotsLoading.value = true
  try {
    snapshots.value = isTauri
      ? await invoke<ScanSnapshot[]>('get_snapshots', { drive: selectedDrive.value })
      : buildPreviewSnapshots(selectedDrive.value)
  } catch (value) { handleError(value) }
  finally { snapshotsLoading.value = false }
}

function buildPreviewDuplicates(scope: string): DuplicateReport {
  return {
    scope,
    scannedFiles: 144_460,
    hashedFiles: 86,
    duplicateFiles: 7,
    wastedBytes: 15_676_653_568,
    elapsedMs: 12_840,
    skippedItems: 2,
    groups: [
      { hash: '7a9f0b22d52c6ea8660f...', size: 6_442_450_944, wastedBytes: 6_442_450_944, files: [`${scope}Backups\\archive.iso`, `${scope}Downloads\\archive-copy.iso`] },
      { hash: '2cf24dba5fb0a30e26e8...', size: 4_831_838_208, wastedBytes: 4_831_838_208, files: [`${scope}Media\\video-final.mkv`, `${scope}Media\\export\\video-final.mkv`] },
      { hash: '486ea46224d1bb4fb680...', size: 2_201_182_208, wastedBytes: 4_402_364_416, files: [`${scope}Projects\\assets.zip`, `${scope}Backups\\assets.zip`, `${scope}Downloads\\assets (1).zip`] },
    ],
  }
}

async function scanDuplicates(path = `${selectedDrive.value}\\`) {
  page.value = 'insights'
  analysisTab.value = 'duplicates'
  duplicateScanning.value = true
  duplicateReport.value = null
  error.value = ''
  duplicateProgress.value = { message: '正在启动重复文件检测', percentage: 1, currentPath: path }
  try {
    duplicateReport.value = isTauri
      ? await invoke<DuplicateReport>('find_duplicates', { path, minSize: duplicateMinSize.value, exclusions: exclusionPaths.value })
      : buildPreviewDuplicates(path)
  } catch (value) {
    if (String(value).includes('已取消')) showNotice('重复文件检测已取消。')
    else handleError(value)
  } finally { duplicateScanning.value = false }
}

async function chooseDuplicateFolder() {
  if (!isTauri) { await scanDuplicates('E:\\Projects\\'); return }
  try {
    const selected = await open({ directory: true, multiple: false, title: '选择重复文件检测范围' })
    if (typeof selected === 'string') await scanDuplicates(selected)
  } catch (value) { handleError(value) }
}

async function cancelScan() {
  try {
    await invoke('cancel_scan')
    if (duplicateScanning.value) duplicateProgress.value = { ...duplicateProgress.value, message: '正在取消重复文件检测' }
    else if (folderAnalyzing.value) folderProgress.value = { ...folderProgress.value, message: '正在取消文件夹分析' }
    else progress.value = { ...progress.value, message: '正在取消扫描' }
  }
  catch (value) { handleError(value) }
}

async function openPath(path: string, selectFile = false) {
  if (!isTauri) return
  try { await invoke('open_in_explorer', { path, selectFile }) } catch (value) { handleError(value) }
}

function toggleFileSelection(path: string) {
  selectedFilePaths.value = selectedFilePaths.value.includes(path)
    ? selectedFilePaths.value.filter(item => item !== path)
    : [...selectedFilePaths.value, path]
}

function toggleAllFilteredFiles() {
  if (allFilteredFilesSelected.value) {
    const drop = new Set(filteredFiles.value.map(item => item.path))
    selectedFilePaths.value = selectedFilePaths.value.filter(path => !drop.has(path))
  } else {
    selectedFilePaths.value = [...new Set([...selectedFilePaths.value, ...filteredFiles.value.map(item => item.path)])]
  }
}

function toggleDuplicatePath(path: string) {
  selectedDuplicatePaths.value = selectedDuplicatePaths.value.includes(path)
    ? selectedDuplicatePaths.value.filter(item => item !== path)
    : [...selectedDuplicatePaths.value, path]
}

function selectDuplicateCopies() {
  if (!duplicateReport.value) return
  const copies: string[] = []
  for (const group of duplicateReport.value.groups) {
    // 保留每组第一个，其余作副本
    copies.push(...group.files.slice(1))
  }
  selectedDuplicatePaths.value = copies
  showNotice(copies.length
    ? `已选择 ${copies.length} 个重复副本（每组保留 1 个），可预览或移入回收站`
    : '没有可选择的重复副本')
}

function clearDuplicateSelection() {
  selectedDuplicatePaths.value = []
}

function openAgeBucket(id: string) {
  selectedAgeBucket.value = selectedAgeBucket.value === id ? null : id
}


function selectTypedFiles() {
  selectedFilePaths.value = [...new Set([...selectedFilePaths.value, ...typedLargeFiles.value.map(f => f.path)])]
}

function selectGroupCopies(group: DuplicateGroup) {
  selectedDuplicatePaths.value = [...new Set([...selectedDuplicatePaths.value, ...group.files.slice(1)])]
}
function goCleanupFromOverview() {
  page.value = 'cleanup'
}

function goLargeFilesReview() {
  page.value = 'files'
  fileTab.value = 'files'
  fileSizeFilter.value = '100mb'
}

function openRegionInFiles(regionId: string) {
  page.value = 'files'
  fileTab.value = 'directories'
  const sample = regionAttribution.value.find(item => item.id === regionId)?.samples[0]
  if (sample) {
    query.value = sample.split('\\').slice(0, 3).join('\\')
    showNotice(`已按区域筛选目录：${regionAttribution.value.find(item => item.id === regionId)?.label}`)
  } else {
    query.value = ''
  }
}

function openProject(root: string, name: string) {
  page.value = 'files'
  folderAnalysis.value = null
  void analyzeFolder(root)
  showNotice(`正在分析项目：${name}`)
}

function goAttribution() {
  page.value = 'insights'
  analysisTab.value = 'attribution'
}

function goMediaFromType(kind: string) {
  if (kind === '视频' || kind === '图片' || kind === '音频') {
    page.value = 'media'
    showNotice(`已打开媒体管理。可按「${kind}」相关范围继续分析。`)
    return
  }
  page.value = 'files'
  fileTab.value = 'types'
  fileTypeFilter.value = kind
}

function openDuplicateGroupInExplorer(group: DuplicateGroup) {
  const first = group.files[0]
  if (first) void openPath(first, true)
}

function requestRecycleFiles(source: 'files' | 'duplicates') {
  recycleSource.value = source
  const paths = source === 'duplicates' ? selectedDuplicatePaths.value : selectedFilePaths.value
  if (!paths.length) {
    showNotice('请先勾选要处理的文件')
    return
  }
  if (recyclePolicy.value === 'direct') {
    void runRecycleFiles()
    return
  }
  confirmRecycleFiles.value = true
}

async function runRecycleFiles() {
  const paths = [...recycleTargetPaths.value]
  if (!paths.length) return
  recyclingFiles.value = true
  error.value = ''
  try {
    if (!isTauri) {
      showNotice(`界面预览：将移入回收站 ${paths.length} 个文件（约 ${formatSize(recycleTargetBytes.value)}）`)
      confirmRecycleFiles.value = false
      return
    }
    const recycled = await invoke<{ recycledFiles: number; recycledBytes: number; failedItems: number }>('recycle_paths', { paths })
    confirmRecycleFiles.value = false
    const fail = recycled.failedItems ? `，${formatCount(recycled.failedItems)} 个失败` : ''
    const detail = detailedActivityLog.value
      ? formatDetailLines([
          `来源：${recycleSource.value === 'duplicates' ? '重复文件' : '文件审查'}`,
          `成功 ${recycled.recycledFiles} · 失败 ${recycled.failedItems} · ${formatSize(recycled.recycledBytes)}`,
          ...paths.slice(0, 40),
        ])
      : `来源：${recycleSource.value === 'duplicates' ? '重复文件' : '文件审查'}${fail}`
    showNotice(
      `已移入回收站 ${formatCount(recycled.recycledFiles)} 个文件（${formatSize(recycled.recycledBytes)}）${fail}`,
      4200,
      'recycle',
      detail,
      { count: recycled.recycledFiles, bytes: recycled.recycledBytes, failed: recycled.failedItems },
    )
    if (recycleSource.value === 'duplicates') {
      selectedDuplicatePaths.value = selectedDuplicatePaths.value.filter(path => !paths.includes(path))
      if (duplicateReport.value) {
        const removed = new Set(paths)
        const groups = duplicateReport.value.groups
          .map(group => ({ ...group, files: group.files.filter(file => !removed.has(file)) }))
          .filter(group => group.files.length > 1)
          .map(group => ({
            ...group,
            wastedBytes: group.size * Math.max(0, group.files.length - 1),
          }))
        duplicateReport.value = {
          ...duplicateReport.value,
          groups,
          duplicateFiles: groups.reduce((sum, group) => sum + group.files.length, 0),
          wastedBytes: groups.reduce((sum, group) => sum + group.wastedBytes, 0),
        }
      }
    } else {
      selectedFilePaths.value = selectedFilePaths.value.filter(path => !paths.includes(path))
      if (result.value) {
        const removed = new Set(paths)
        const next = {
          ...result.value,
          largeFiles: result.value.largeFiles.filter(file => !removed.has(file.path)),
        }
        result.value = next
        if (resultsByDrive.value[selectedDrive.value]) {
          resultsByDrive.value = {
            ...resultsByDrive.value,
            [selectedDrive.value]: next,
          }
        }
      }
    }
  } catch (value) {
    handleError(value)
  } finally {
    recyclingFiles.value = false
  }
}

async function previewRecycleFiles(source: 'files' | 'duplicates') {
  recycleSource.value = source
  const count = recycleTargetPaths.value.length
  if (!count) {
    showNotice('请先勾选文件')
    return
  }
  showNotice(`预览（不删除）：将移入回收站 ${formatCount(count)} 个文件，约 ${formatSize(recycleTargetBytes.value)}`)
}

async function exportReport() {
  if (!result.value) return
  try { showNotice(`报告已保存到 ${await invoke<string>('export_report', { result: result.value, outputDirectory: reportDirectory.value || null })}`) }
  catch (value) { handleError(value) }
}

async function openStorageSettings() {
  try { await invoke('open_storage_settings') } catch (value) { handleError(value) }
}

function samePath(a: string, b: string) {
  return a.replace(/[/\\]+$/g, '').toLowerCase() === b.replace(/[/\\]+$/g, '').toLowerCase()
}

/** 从文件夹分析跳到清理中心：有扫描结果则高亮/勾选对应项，否则提示先扫描 */
async function openCleanupFromFolder(item: FolderItem) {
  if (item.kind !== 'directory' || item.risk !== 'rebuildable') return
  highlightCleanupPath.value = item.path
  page.value = 'cleanup'
  if (!result.value && !resultsByDrive.value[selectedDrive.value]) {
    showNotice(`请先完整扫描 ${selectedDrive.value}，清理列表才会生成。当前路径：${item.path}`)
    return
  }
  if (!result.value && resultsByDrive.value[selectedDrive.value]) {
    result.value = resultsByDrive.value[selectedDrive.value]
  }
  if (!cleanup.value && cleanupByDrive.value[selectedDrive.value]) {
    cleanup.value = cleanupByDrive.value[selectedDrive.value]
  }
  if (!cleanup.value && !loadingCleanup.value) {
    await loadCleanup()
  }
  const match = cleanup.value?.items.find(entry => samePath(entry.path, item.path))
  if (match) {
    if (match.action === 'safe' && match.size > 0 && !selectedCleanup.value.includes(match.id)) {
      toggleCleanup(match.id)
    }
    showNotice(`已定位清理项：${match.name}`)
  } else {
    showNotice(`「${item.name}」已标为可重建，但未进入一键清理列表（低置信项如 build/dist 仅提示）。可在资源管理器中手动处理。`)
  }
}

function ensureCleanupTabVisible() {
  const opts = cleanupTabOptions.value
  if (!opts.some(t => t.id === cleanupTab.value)) {
    cleanupTab.value = (opts.find(t => t.id === 'fixed')?.id || opts[0]?.id || 'all') as CleanupTab
  }
}
function toggleAllSafe() {
  const ids = selectableInTab.value.map(item => item.id)
  if (allTabSelectableSelected.value) {
    selectedCleanup.value = selectedCleanup.value.filter(id => !ids.includes(id))
  } else {
    const set = new Set([...selectedCleanup.value, ...ids])
    selectedCleanup.value = [...set]
  }
  selectedCleanupByDrive.value = { ...selectedCleanupByDrive.value, [selectedDrive.value]: [...selectedCleanup.value] }
}

function toggleCleanup(id: string) {
  selectedCleanup.value = selectedCleanup.value.includes(id)
    ? selectedCleanup.value.filter(value => value !== id)
    : [...selectedCleanup.value, id]
  selectedCleanupByDrive.value = { ...selectedCleanupByDrive.value, [selectedDrive.value]: [...selectedCleanup.value] }
}

async function previewCleanup() {
  if (!selectedCleanup.value.length) return
  previewingCleanup.value = true
  error.value = ''
  try {
    const cleaned = await invoke<CleanupResult>('clean_items', {
      drive: selectedDrive.value,
      ids: selectedCleanup.value,
      dryRun: true,
      options: { blacklist: cleanupBlacklist.value, protectPrefixes: buildProtectPrefixes() },
    })
    const hot = cleaned.skippedHot ? `，热保护将跳过 ${formatCount(cleaned.skippedHot)} 项` : ''
    const fail = cleaned.failedItems ? `，约 ${formatCount(cleaned.failedItems)} 项可能失败` : ''
    showNotice(`预览（不删除）：将移入回收站约 ${formatSize(cleaned.freedBytes)}（约 ${formatCount(cleaned.deletedFiles)} 项）${hot}${fail}`)
  } catch (value) { handleError(value) }
  finally { previewingCleanup.value = false }
}

async function runCleanup() {
  if (!selectedCleanup.value.length) return
  if (selectedHasModelItems.value) {
    if (!modelStrongConfirm.value) {
      showNotice('请先勾选风险确认')
      return
    }
    if (modelConfirmPhrase.value.trim() !== '确认删除模型缓存') {
      showNotice('请输入确认词：确认删除模型缓存')
      return
    }
  }
  cleaning.value = true
  error.value = ''
  try {
    const cleaned = await invoke<CleanupResult>('clean_items', {
      drive: selectedDrive.value,
      ids: selectedCleanup.value,
      dryRun: false,
      options: {
        blacklist: cleanupBlacklist.value,
        protectPrefixes: buildProtectPrefixes(),
        strongConfirm: !selectedHasModelItems.value || modelStrongConfirm.value,
      },
    })
    confirmCleanup.value = false
    const hot = cleaned.skippedHot ? `，热保护跳过 ${formatCount(cleaned.skippedHot)} 项` : ''
    const fail = cleaned.failedItems ? `，跳过 ${formatCount(cleaned.failedItems)} 个占用或无权限项目` : ''
    const cleanedNames = selectedCleanupItems.value.map(item =>
      `${item.name}${item.category === 'developer' ? '（开发）' : item.category === 'app' ? '（应用）' : item.category === 'toolai' ? '（工具/AI）' : ''} · ${formatSize(item.size)} · ${item.path}`,
    )
    const cleanedDetail = detailedActivityLog.value
      ? formatDetailLines([
          `盘符 ${selectedDrive.value}`,
          `释放约 ${formatSize(cleaned.freedBytes)} · ${formatCount(cleaned.deletedFiles)} 项`,
          ...cleanedNames,
        ])
      : `盘符 ${selectedDrive.value} · 约 ${formatCount(cleaned.deletedFiles)} 项${hot}${fail}`
    showNotice(
      `已移入回收站 ${formatSize(cleaned.freedBytes)}（约 ${formatCount(cleaned.deletedFiles)} 项）${hot}${fail}。可在回收站还原。`,
      4200,
      'cleanup',
      cleanedDetail,
      { drive: selectedDrive.value, deletedFiles: cleaned.deletedFiles, freedBytes: cleaned.freedBytes },
    )
    await Promise.all([loadCleanup(), refreshUsage()])
  } catch (value) { handleError(value) }
  finally { cleaning.value = false }
}

async function loadRecycleBin() {
  if (!isTauri) return
  try {
    const result = await invoke<{ entries: RecycleEntry[]; totalBytes: number }>('list_recycle_items')
    recycleEntries.value = result.entries ?? []
    recycleTotalBytes.value = result.totalBytes ?? 0
  } catch (value) { handleError(value) }
}

async function openRecycleBin() {
  showRecycleBin.value = true
  await Promise.all([loadRecycleBin(), loadSystemRecycleBytes()])
}

async function loadSystemRecycleBytes() {
  if (!isTauri) return
  try { systemRecycleBytes.value = await invoke<number>('system_recycle_bin_bytes') } catch { /* 忽略 */ }
}

async function openSystemRecycleBin() {
  try {
    await invoke('open_system_recycle_bin')
  } catch (value) { handleError(value) }
}

async function emptySystemRecycleBin() {
  if (!confirmEmptySystemBin.value) {
    confirmEmptySystemBin.value = true
    showNotice('再次点击「清空系统回收站」才真正清空（影响所有磁盘）', 3500, 'notice')
    if (emptyBinConfirmTimer) clearTimeout(emptyBinConfirmTimer)
    emptyBinConfirmTimer = window.setTimeout(() => { confirmEmptySystemBin.value = false }, 3500)
    return
  }
  recycleBusy.value = 'empty'
  try {
    await invoke('empty_system_recycle_bin')
    showNotice('系统回收站已清空（文件不可恢复）', 4000, 'recycle')
    confirmEmptySystemBin.value = false
    await Promise.all([loadSystemRecycleBytes(), refreshUsage()])
  } catch (value) { handleError(value) }
  finally { recycleBusy.value = '' }
}

async function clearRecycleEntries() {
  recycleBusy.value = 'entries'
  try {
    await invoke('clear_recycle_entries')
    showNotice('已清空应用清理记录（不影响系统回收站）', 3000, 'recycle')
    await loadRecycleBin()
  } catch (value) { handleError(value) }
  finally { recycleBusy.value = '' }
}

async function openCleanupHistory() {
  showCleanupHistory.value = true
  cleanupHistoryBusy.value = 'loading'
  try {
    cleanupHistory.value = await invoke<CleanupSnapshot[]>('list_cleanup_snapshots')
  } catch (value) { handleError(value) }
  finally { cleanupHistoryBusy.value = '' }
}

async function deleteCleanupSnapshot(id: string) {
  cleanupHistoryBusy.value = id
  try {
    await invoke('delete_cleanup_snapshot', { id })
    cleanupHistory.value = cleanupHistory.value.filter(item => item.id !== id)
  } catch (value) { handleError(value) }
  finally { cleanupHistoryBusy.value = '' }
}

function snapshotTotalBytes(entry: CleanupSnapshotEntry): number {
  return entry.paths.reduce((sum, p) => sum + (p.size || 0), 0)
}

function buildPreviewFolder(path: string): FolderAnalysis {
  return {
    path,
    name: 'Projects',
    totalSize: 48_855_638_016,
    fileCount: 182_463,
    dirCount: 21_408,
    elapsedMs: 4380,
    skippedItems: 3,
    largeFiles: [{ path: `${path}\\archive.zip`, name: 'archive.zip', size: 4_831_838_208, modifiedDays: 430 }],
    children: [
      { path: `${path}\\node_modules`, name: 'node_modules', size: 21_796_126_720, fileCount: 124830, dirCount: 12884, kind: 'directory', risk: 'rebuildable', recommendation: 'Node.js 依赖；旁侧有 package.json 时建议可重建，关闭 dev server 后可移入回收站' },
      { path: `${path}\\assets`, name: 'assets', size: 12_347_883_520, fileCount: 4280, dirCount: 312, kind: 'directory', risk: 'review', recommendation: '可能包含个人或项目数据，请先打开检查内容和最近修改时间' },
      { path: `${path}\\target`, name: 'target', size: 9_985_835_008, fileCount: 51480, dirCount: 7920, kind: 'directory', risk: 'rebuildable', recommendation: 'Rust/Java 构建产物；旁侧有 Cargo.toml 或 pom.xml 时建议可重建' },
      { path: `${path}\\archive.zip`, name: 'archive.zip', size: 4_831_838_208, fileCount: 1, dirCount: 0, kind: 'file', risk: 'review', recommendation: '可能包含个人或项目数据，请先打开检查内容和最近修改时间' },
    ],
  }
}

async function analyzeFolder(path: string, rememberCurrent = true) {
  page.value = 'files'
  if (rememberCurrent && folderAnalysis.value && folderAnalysis.value.path !== path) folderHistory.value.push(folderAnalysis.value.path)
  folderAnalyzing.value = true
  error.value = ''
  folderProgress.value = { message: '正在启动文件夹分析', percentage: 1, currentPath: path }
  try {
    folderAnalysis.value = isTauri
      ? await invoke<FolderAnalysis>('analyze_folder', { path, options: scanOptions.value })
      : buildPreviewFolder(path)
  } catch (value) {
    if (String(value).includes('已取消')) showNotice('文件夹分析已取消。')
    else handleError(value)
  } finally { folderAnalyzing.value = false }
}

async function chooseFolder() {
  if (!isTauri) {
    folderHistory.value = []
    await analyzeFolder('C:\\Users\\User\\Projects', false)
    return
  }
  try {
    const selected = await open({ directory: true, multiple: false, title: '选择要分析的文件夹' })
    if (typeof selected === 'string') {
      folderHistory.value = []
      await analyzeFolder(selected, false)
    }
  } catch (value) { handleError(value) }
}

async function leaveFolderAnalysis() {
  const previous = folderHistory.value.pop()
  if (previous) await analyzeFolder(previous, false)
  else folderAnalysis.value = null
}

async function addExclusion() {
  if (!isTauri) {
    const preview = 'C:\\Users\\User\\AppData\\Local\\Temp'
    if (!exclusionPaths.value.includes(preview)) exclusionPaths.value = [...exclusionPaths.value, preview]
    return
  }
  try {
    const selected = await open({ directory: true, multiple: false, title: '选择扫描时排除的文件夹' })
    if (typeof selected === 'string' && !exclusionPaths.value.some(value => value.toLocaleLowerCase() === selected.toLocaleLowerCase())) {
      exclusionPaths.value = [...exclusionPaths.value, selected]
    }
  } catch (value) { handleError(value) }
}

function removeExclusion(path: string) {
  exclusionPaths.value = exclusionPaths.value.filter(value => value !== path)
}

async function addCleanupBlacklist() {
  if (!isTauri) {
    const preview = 'D:\\Projects\\keep-this'
    if (!cleanupBlacklist.value.includes(preview)) cleanupBlacklist.value = [...cleanupBlacklist.value, preview]
    return
  }
  try {
    const selected = await open({ directory: true, multiple: false, title: '选择永不进入清理列表的路径' })
    if (typeof selected === 'string' && !cleanupBlacklist.value.some(value => value.toLocaleLowerCase() === selected.toLocaleLowerCase())) {
      cleanupBlacklist.value = [...cleanupBlacklist.value, selected]
    }
  } catch (value) { handleError(value) }
}

function removeCleanupBlacklist(path: string) {
  cleanupBlacklist.value = cleanupBlacklist.value.filter(value => value !== path)
}

async function chooseReportDirectory() {
  if (!isTauri) { reportDirectory.value = 'C:\\Users\\User\\Desktop'; return }
  try {
    const selected = await open({ directory: true, multiple: false, title: '选择报告和诊断信息保存位置' })
    if (typeof selected === 'string') reportDirectory.value = selected
  } catch (value) { handleError(value) }
}

async function clearLocalHistory() {
  settingsBusy.value = 'history'
  try {
    const removed = isTauri ? await invoke<number>('clear_snapshots', { drive: null }) : snapshots.value.length
    snapshots.value = []
    confirmClearHistory.value = false
    showNotice(`已清除 ${formatCount(removed)} 条本地扫描快照。`)
  } catch (value) { handleError(value) }
  finally { settingsBusy.value = '' }
}

async function exportDiagnostics() {
  settingsBusy.value = 'diagnostics'
  try {
    if (!isTauri) { showNotice('界面预览：诊断信息将保存到所选报告目录。'); return }
    const path = await invoke<string>('export_diagnostics', {
      outputDirectory: reportDirectory.value || null,
      settings: advancedSettings(),
    })
    showNotice(`诊断信息已保存到 ${path}`)
  } catch (value) { handleError(value) }
  finally { settingsBusy.value = '' }
}

async function checkUpdates(silent = false) {
  settingsBusy.value = 'update'
  try {
    updateStatus.value = isTauri
      ? await invoke<UpdateStatus>('check_for_updates', { repository: 'sonemeng/disk-space-analyzer' })
      : { currentVersion: APP_VERSION, latestVersion: null, available: false, message: '当前为预览模式，无法检查更新' }
    if (!silent) showNotice(updateStatus.value.message)
  } catch (value) {
    updateStatus.value = { currentVersion: APP_VERSION, available: false, message: String(value) }
    if (!silent) handleError(value)
  } finally { settingsBusy.value = '' }
}

watch(
  [exclusionPaths, cleanupBlacklist, largeFileMb, scanThreads, snapshotLimit, reportDirectory, recyclePolicy, autoCheckUpdates],
  persistAdvancedSettings,
  { deep: true },
)

watch(showSettings, async (open) => { if (open) await loadSystemRecycleBytes() })

function onGlobalKeydown(event: KeyboardEvent) {
  if (event.key !== 'Escape') return
  if (selectedActivity.value) {
    selectedActivity.value = null
    event.preventDefault()
    return
  }
  if (showMessageLog.value) {
    showMessageLog.value = false
    event.preventDefault()
    return
  }
  if (confirmRecycleFiles.value) {
    confirmRecycleFiles.value = false
    event.preventDefault()
    return
  }
  if (confirmCleanup.value) {
    confirmCleanup.value = false
    event.preventDefault()
    return
  }
  if (confirmClearHistory.value) {
    confirmClearHistory.value = false
    event.preventDefault()
    return
  }
  if (showRecycleBin.value) {
    showRecycleBin.value = false
    event.preventDefault()
    return
  }
  if (showCleanupHistory.value) {
    showCleanupHistory.value = false
    event.preventDefault()
    return
  }
  if (showSettings.value) {
    showSettings.value = false
    event.preventDefault()
    return
  }
  if (statusPanel.value !== 'none' || pullDragging.value) {
    closeStatusPanel()
    event.preventDefault()
  }
}

onMounted(async () => {
  migrateLegacyMessageLog()
  loadUnreadCount()
  detailedActivityLog.value = isDetailedActivityLogEnabled()
  refreshActivityLog()
  loadAdvancedSettings()
  const savedTheme = localStorage.getItem('disk-analyzer-theme') as ThemeId | null
  applyTheme(themeOptions.some(theme => theme.id === savedTheme) ? savedTheme! : 'ocean')
  const savedFont = localStorage.getItem('disk-analyzer-font-scale') as FontScale | null
  const savedIcon = localStorage.getItem('disk-analyzer-icon-scale') as IconScale | null
  const savedDensity = localStorage.getItem('disk-analyzer-density') as UiDensity | null
  applyFontScale(savedFont && ['small', 'standard', 'large'].includes(savedFont) ? savedFont : 'standard')
  applyIconScale(savedIcon && ['compact', 'standard', 'large'].includes(savedIcon) ? savedIcon : 'standard')
  applyDensity(savedDensity && ['compact', 'comfortable'].includes(savedDensity) ? savedDensity : 'comfortable')
  const savedGlass = Number(localStorage.getItem('disk-analyzer-glass-strength') || '48')
  applyGlassStrength(Number.isFinite(savedGlass) ? savedGlass : 48)
  const savedBorder = Number(localStorage.getItem('disk-analyzer-border-strength') || '42')
  applyBorderStrength(Number.isFinite(savedBorder) ? savedBorder : 42)
  sidebarCollapsed.value = localStorage.getItem('disk-analyzer-sidebar-collapsed') === 'true'
  window.addEventListener('keydown', onGlobalKeydown)
  if (isTauri) {
    unlisten = await listen<ScanProgress>('scan-progress', event => { progress.value = event.payload })
    unlistenFolder = await listen<ScanProgress>('folder-progress', event => { folderProgress.value = event.payload })
    unlistenDuplicate = await listen<ScanProgress>('duplicate-progress', event => { duplicateProgress.value = event.payload })
    unlistenCleanup = await listen<{ message: string; percent: number }>('cleanup-progress', event => { cleanupProgress.value = event.payload })
  }
  await loadDrives()
  // 清理分析只在「完整扫描」完成后执行，启动/换盘不扫
  void loadSnapshots()
  void checkPendingResume()
  if (autoCheckUpdates.value) void checkUpdates(true)
})
onBeforeUnmount(() => {
  unlisten?.(); unlistenFolder?.(); unlistenDuplicate?.(); unlistenCleanup?.()
  window.removeEventListener('keydown', onGlobalKeydown)
  if (noticeTimer) clearTimeout(noticeTimer)
  if (noticeFadeTimer) clearTimeout(noticeFadeTimer)
  if (errorTimer) clearTimeout(errorTimer)
  if (errorFadeTimer) clearTimeout(errorFadeTimer)
})
</script>

<template>
  <div class="app-shell u1-shell" :class="{ collapsed: sidebarCollapsed }">
    <aside class="sidebar" :class="{ collapsed: sidebarCollapsed }">
      <div class="sidebar-head"><div class="brand"><span class="brand-mark"><HardDrive :size="20" /></span><div><strong>磁盘空间分析器</strong><small>空间诊断与安全清理</small></div></div><button class="collapse-button" :title="sidebarCollapsed ? '展开侧栏' : '折叠侧栏'" @click="toggleSidebar"><PanelLeftOpen v-if="sidebarCollapsed" :size="17" /><PanelLeftClose v-else :size="17" /></button></div>

      <nav class="main-nav" aria-label="主要功能">
        <button title="空间概览" :class="{ active: page === 'overview' }" @click="page = 'overview'"><LayoutDashboard :size="17" /><span>空间概览</span></button>
        <button title="清理中心" :class="{ active: page === 'cleanup' }" @click="page = 'cleanup'"><Trash2 :size="17" /><span>清理中心</span><b v-if="hasScanForDrive && cleanup?.safeBytes">{{ formatSize(cleanup.safeBytes) }}</b></button>
        <button title="文件审查" :class="{ active: page === 'files' }" @click="page = 'files'"><FileSearch :size="17" /><span>文件审查</span><b v-if="result">{{ result.largeFiles.length }}</b></button>
        <button title="深度分析" :class="{ active: page === 'insights' }" @click="page = 'insights'"><ChartNoAxesCombined :size="17" /><span>深度分析</span></button>
        <button title="媒体管理" :class="{ active: page === 'media' }" @click="openMediaCenter"><Library :size="17" /><span>媒体管理</span><b v-if="mediaNew">新</b></button>
        <button title="注册表检查" :class="{ active: page === 'registry' }" @click="page = 'registry'"><Database :size="17" /><span>注册表检查</span></button>
      </nav>

      <div class="sidebar-label sidebar-drive-title"><span>本机磁盘</span><button title="重新检测磁盘" :disabled="loadingDrives" @click="loadDrives"><RefreshCw :size="12" :class="{ spin: loadingDrives }" /></button></div>
      <div class="drive-list" :aria-busy="loadingDrives">
        <button v-for="drive in drives" :key="drive" class="drive-button" :title="`本地磁盘 ${drive}`" :class="{ active: selectedDrive === drive }" :disabled="scanning" @click="selectDrive(drive)">
          <HardDrive :size="16" /><span><b>本地磁盘 {{ drive }}</b></span><i v-if="selectedDrive === drive" />
        </button>
        <div v-if="loadingDrives" class="drive-loading"><RefreshCw :size="15" class="spin" /> 正在检测磁盘</div>
        <div v-else-if="!drives.length" class="drive-loading">未检测到磁盘</div>
      </div>

      <div class="sidebar-spacer" />
      <button class="settings-trigger" title="设置" @click="showSettings = true"><Settings :size="17" /><span><b>设置</b><small>{{ themeOptions.find(theme => theme.id === activeTheme)?.name }} · {{ fontScale === 'large' ? '大字号' : fontScale === 'small' ? '小字号' : '标准字号' }}</small></span><ChevronRight :size="15" /></button>
      <div class="safety-note"><ShieldCheck :size="17" /><div><b>默认只读</b><span>只有低风险白名单项目可在确认后清理</span></div></div>
      <div v-if="!isTauri" class="preview-badge"><Info :size="14" /> 界面预览</div>
      <div class="version">TAURI EDITION · {{ APP_VERSION }}</div>
    </aside>

    <main class="workspace">
      <div
        class="pull-edge"
        :class="{ hot: pullEdgeHot || pullDragging || statusPanel !== 'none' }"
        title="左半边下拉：通知 · 右半边下拉：快捷设置"
        @pointerenter="pullEdgeHot = true"
        @pointerleave="pullEdgeHot = false"
        @pointerdown="onPullStart($event)"
        @pointermove="onPullMove"
        @pointerup="onPullEnd"
        @pointercancel="onPullEnd"
      >
        <span class="pull-bar" :class="{ show: pullEdgeHot || pullDragging || statusPanel !== 'none' }" />
      </div>
      <header class="topbar">
        <div><div class="eyebrow">{{ page === 'media' ? '本地媒体' : page === 'registry' ? 'Windows 当前用户' : `${selectedDrive}\\` }} {{ pageTitle }}</div><h1>{{ pageTitle }}</h1></div>
        <div class="actions">
          <button v-if="pendingResume && !scanning" class="button secondary resume-banner" title="上次扫描中途停止，已保存进度" @click="resumeScan"><Play :size="16" /> 继续上次扫描<small v-if="pendingResume.completedFiles"><i>{{ pendingResume.drive }}</i></small></button>
          <button v-if="result && page === 'overview'" class="button secondary" @click="exportReport"><Download :size="17" /> 导出报告</button>
          <button v-if="page === 'files'" class="button secondary" :disabled="folderAnalyzing" @click="chooseFolder"><FolderSearch :size="17" /> 选择文件夹</button>
          <button v-if="scanning || folderAnalyzing || duplicateScanning" class="button danger" @click="cancelScan"><CircleStop :size="17" /> 取消分析</button>
          <button v-else-if="page !== 'media' && page !== 'registry'" class="button primary" :disabled="!selectedDrive" @click="startScan()"><Play :size="17" fill="currentColor" /> {{ result ? '重新扫描' : '完整扫描' }}</button>
        </div>
      </header>

      <div v-if="statusPanel !== 'none' || (pullDragging && pullDistance > 8)" class="status-scrim" :class="{ dim: statusPanel !== 'none' }" @click="closeStatusPanel" />
      <section
        class="status-shade"
        :class="{ open: shadeOpen || (pullDragging && pullDistance > 8), dragging: pullDragging, right: (statusPanel === 'quick' || (pullDragging && pullSide === 'right')) }"
        :style="shadeStyle"
        @click.stop
      >
        <div class="shade-handle" @pointerdown="onPullStart($event, statusPanel === 'quick' ? 'right' : 'left')" @pointermove="onPullMove" @pointerup="onPullEnd" @pointercancel="onPullEnd" />
        <template v-if="statusPanel !== 'quick' && !(pullDragging && pullSide === 'right' && statusPanel === 'none')">
          <header class="shade-head" @pointerdown="onPullStart($event, 'left')">
            <div>
              <b>通知中心</b>
              <small v-if="unreadBadge">未读 {{ unreadBadge }}</small>
            </div>
            <div class="shade-head-actions" @pointerdown.stop>
              <button type="button" class="text-button" :disabled="!shadeNotifyItems.length" @click="clearShadeNotifications">清除全部</button>
              <button type="button" class="text-button" @click="openActivityLog()">全部</button>
            </div>
          </header>
          <div class="status-list">
            <button v-for="item in shadeNotifyItems" :key="item.id" type="button" class="status-item" :class="item.kind" @click="openActivityDetail(item)">
              <div><b>{{ kindLabel(item.kind) }}</b><span>{{ new Date(item.at).toLocaleString('zh-CN', { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' }) }}</span></div>
              <p>{{ item.title }}</p>
              <small v-if="item.detail" class="status-item-detail">{{ item.detail.split('\n')[0] }}</small>
            </button>
          </div>
          <div class="shade-dismiss" @pointerdown="onPullStart($event, 'left')" />
          <footer class="shade-foot" @pointerdown="onPullStart($event, 'left')">
            <button class="button secondary compact" @pointerdown.stop @click="openActivityInSettings">设置中的活动日志</button>
          </footer>
        </template>
        <template v-else>
          <header class="shade-head shade-head-quick" @pointerdown="onPullStart($event, 'right')">
            <div>
              <b>快捷设置</b>
            </div>
            <div class="shade-head-actions" @pointerdown.stop>
              <button class="button secondary compact" @click="showSettings = true; statusPanel = 'none'">全部设置</button>
            </div>
          </header>
          <div class="quick-layout">
            <section class="quick-card">
              <div class="quick-card-title"><b>开关</b><small>常用行为</small></div>
              <div class="quick-toggles">
                <label class="quick-toggle"><span><b>启动检查更新</b><small>打开应用时查询版本</small></span><button type="button" class="toggle-switch" role="switch" :aria-checked="autoCheckUpdates" :class="{ active: autoCheckUpdates }" @click="autoCheckUpdates = !autoCheckUpdates"><i /></button></label>
                <label class="quick-toggle"><span><b>回收站每次确认</b><small>关闭则直接移入回收站</small></span><button type="button" class="toggle-switch" role="switch" :aria-checked="recyclePolicy === 'confirm'" :class="{ active: recyclePolicy === 'confirm' }" @click="recyclePolicy = recyclePolicy === 'confirm' ? 'direct' : 'confirm'"><i /></button></label>
                <label class="quick-toggle"><span><b>详细活动记录</b><small>清理/回收写入清单，更占本地存储</small></span><button type="button" class="toggle-switch" role="switch" :aria-checked="detailedActivityLog" :class="{ active: detailedActivityLog }" @click="toggleDetailedActivityLog"><i /></button></label>
              </div>
            </section>
            <section class="quick-card">
              <div class="quick-card-title"><b>外观</b><small>字号 · 密度 · 图标</small></div>
              <div class="quick-field"><span>字号</span><div class="chip-row"><button type="button" class="chip-btn" :class="{ active: fontScale === 'small' }" @click="applyFontScale('small')">小</button><button type="button" class="chip-btn" :class="{ active: fontScale === 'standard' }" @click="applyFontScale('standard')">中</button><button type="button" class="chip-btn" :class="{ active: fontScale === 'large' }" @click="applyFontScale('large')">大</button></div></div>
              <div class="quick-field"><span>密度</span><div class="chip-row"><button type="button" class="chip-btn" :class="{ active: uiDensity === 'compact' }" @click="applyDensity('compact')">紧凑</button><button type="button" class="chip-btn" :class="{ active: uiDensity === 'comfortable' }" @click="applyDensity('comfortable')">舒适</button></div></div>
              <div class="quick-field"><span>图标</span><div class="chip-row"><button type="button" class="chip-btn" :class="{ active: iconScale === 'compact' }" @click="applyIconScale('compact')">紧</button><button type="button" class="chip-btn" :class="{ active: iconScale === 'standard' }" @click="applyIconScale('standard')">标准</button><button type="button" class="chip-btn" :class="{ active: iconScale === 'large' }" @click="applyIconScale('large')">大</button></div></div>
            </section>
            <section class="quick-card">
              <div class="quick-card-title"><b>材质</b><small>通透度 · 边框清晰度</small></div>
              <div class="quick-slider">
                <div class="quick-slider-label"><span>通透度</span><em>{{ glassStrength < 34 ? '很透' : glassStrength < 67 ? '适中' : '更实' }} · {{ glassStrength }}</em></div>
                <input type="range" min="0" max="100" step="1" :value="glassStrength" @input="applyGlassStrength(Number(($event.target as HTMLInputElement).value))" />
              </div>
              <div class="quick-slider">
                <div class="quick-slider-label"><span>边框</span><em>{{ borderStrength < 28 ? '极淡' : borderStrength < 62 ? '适中' : '清晰' }} · {{ borderStrength }}</em></div>
                <input type="range" min="0" max="100" step="1" :value="borderStrength" @input="applyBorderStrength(Number(($event.target as HTMLInputElement).value))" />
              </div>
            </section>
            <section class="quick-card quick-card-wide">
              <div class="quick-card-title"><b>快捷入口</b><small>跳转到完整设置分区</small></div>
              <div class="quick-actions">
                <button type="button" class="quick-link" @click="settingsTab = 'appearance'; showSettings = true; statusPanel = 'none'"><Palette :size="18" /><span><b>外观</b><small>主题 · 通透 · 边框</small></span></button>
                <button type="button" class="quick-link" @click="settingsTab = 'scanning'; showSettings = true; statusPanel = 'none'"><SlidersHorizontal :size="18" /><span><b>扫描</b><small>排除 · 阈值 · 并发</small></span></button>
                <button type="button" class="quick-link" @click="openActivityInSettings"><History :size="18" /><span><b>活动</b><small>通知与操作记录</small></span></button>
                <button type="button" class="quick-link" @click="checkUpdates(); statusPanel = 'none'"><RefreshCw :size="18" /><span><b>检查更新</b><small>立即查询版本</small></span></button>
              </div>
            </section>
          </div>
          <div class="shade-dismiss" @pointerdown="onPullStart($event, 'right')" />
        </template>
      </section>

      <div v-if="error" class="alert error" :class="{ fading: errorFading }"><AlertTriangle :size="17" /><span class="alert-text" title="点击查看详情" @click="openLatestNoticeDetail">{{ error }}</span><button class="text-button log-link" title="活动日志" @click="openActivityLog('error')">记录</button><button aria-label="关闭" @click="dismissError(true)"><X :size="16" /></button></div>
      <div v-if="notice" class="alert notice" :class="{ fading: noticeFading }"><Check :size="17" /><span class="alert-text" title="点击查看详情" @click="openLatestNoticeDetail">{{ notice }}</span><button class="text-button log-link" title="活动日志" @click="openActivityLog()">记录</button><button aria-label="关闭" @click="dismissNotice(true)"><X :size="16" /></button></div>

      <section v-if="scanning" class="scan-strip">
        <div class="scan-line"><span class="pulse-dot" /><strong>{{ progress.message }}</strong><b>{{ progress.percentage }}%</b></div>
        <div class="progress-track"><div :style="{ width: `${progress.percentage}%` }" /></div>
        <div class="current-path" :title="progress.currentPath">{{ progress.currentPath || '正在准备文件系统读取…' }}</div>
      </section>
      <section v-if="folderAnalyzing" class="scan-strip folder-scan-strip">
        <div class="scan-line"><span class="pulse-dot" /><strong>{{ folderProgress.message }}</strong><b>{{ folderProgress.percentage }}%</b></div>
        <div class="progress-track"><div :style="{ width: `${folderProgress.percentage}%` }" /></div>
        <div class="current-path" :title="folderProgress.currentPath">{{ folderProgress.currentPath || '正在读取文件夹…' }}</div>
      </section>
      <section v-if="duplicateScanning" class="scan-strip duplicate-scan-strip">
        <div class="scan-line"><span class="pulse-dot" /><strong>{{ duplicateProgress.message }}</strong><b>{{ duplicateProgress.percentage }}%</b></div>
        <div class="progress-track"><div :style="{ width: `${duplicateProgress.percentage}%` }" /></div>
        <div class="current-path" :title="duplicateProgress.currentPath">{{ duplicateProgress.currentPath || '正在读取文件内容…' }}</div>
      </section>

      <template v-if="page === 'registry'">
        <RegistryCleaner />
      </template>

      <template v-else-if="page === 'media'">
        <MediaCenter :exclusions="exclusionPaths" :large-file-mb="largeFileMb" :scan-threads="scanThreads" :recycle-policy="recyclePolicy" :drives="drives" :selected-drive="selectedDrive" />
      </template>

      <template v-else-if="page === 'overview'">
        <section class="metrics" aria-label="磁盘容量">
          <div class="metric"><div class="metric-icon coral"><Gauge :size="19" /></div><div><span>磁盘使用率</span><strong>{{ usedPercent }}%</strong><small>{{ currentUsage ? `${formatSize(currentUsage.used)} 已使用` : '正在读取容量' }}</small></div></div>
          <div class="metric"><div class="metric-icon blue"><HardDrive :size="19" /></div><div><span>总容量</span><strong>{{ currentUsage ? formatSize(currentUsage.total) : '—' }}</strong><small>{{ selectedDrive }} 本地磁盘</small></div></div>
          <div class="metric"><div class="metric-icon green"><Sparkles :size="19" /></div><div><span>建议安全清理</span><strong>{{ !result ? '—' : loadingCleanup ? '分析中' : formatSize(cleanup?.safeBytes ?? 0) }}</strong><small>{{ result ? '临时文件 + 开发可重建' : '完整扫描后分析' }}</small></div></div>
          <div class="metric"><div class="metric-icon amber"><FileText :size="19" /></div><div><span>已扫描文件</span><strong>{{ result ? formatCount(result.scannedFiles) : '—' }}</strong><small>{{ result ? `${formatCount(result.scannedDirs)} 个目录` : '完整扫描后显示' }}</small></div></div>
        </section>

        <template v-if="result">
          <section v-if="loadingCleanup" class="reclaim-band">
            <div class="reclaim-icon"><LoaderCircle :size="22" class="spin" /></div>
            <div><span>扫描完成，正在分析 {{ selectedDrive }} 可清理项</span><strong>…</strong><small>临时文件、缓存与开发目录</small></div>
            <button class="button primary" disabled>分析中</button>
          </section>
          <section v-else-if="cleanup" class="reclaim-band">
            <div class="reclaim-icon"><Sparkles :size="22" /></div>
            <div>
              <span>{{ cleanup.safeBytes > 0 ? '发现可安全释放空间' : `${selectedDrive} 清理中心` }}</span>
              <strong>{{ formatSize(cleanup.safeBytes) }}</strong>
              <small>{{ cleanup.safeBytes > 0 ? '仅当前盘可清理项，移入回收站可还原' : '当前盘暂无命中项，可进入清理中心确认' }}</small>
            </div>
            <button class="button primary" @click="page = 'cleanup'">{{ cleanup.safeBytes > 0 ? '查看清理项' : '打开清理中心' }} <ChevronRight :size="16" /></button>
          </section>
          <div class="content-grid">
            <section class="panel distribution-panel">
              <div class="panel-heading"><div><span class="panel-kicker">空间结构</span><h2>{{ isSystemDrive ? '类别分布' : '文件类型分布' }}</h2></div><small>完整读取 {{ formatSize(categoryTotal) }}</small></div>
              <div class="distribution-body">
                <div class="donut" :style="{ background: `conic-gradient(${categoryGradient})` }"><div><strong>{{ usedPercent }}%</strong><span>磁盘已用</span></div></div>
                <div class="legend"><div v-for="item in displayCategories" :key="item.name"><i :style="{ background: item.color }" /><span>{{ item.name }}</span><b>{{ formatSize(item.size) }}</b></div></div>
              </div>
            </section>
            <section class="panel health-panel">
              <div class="panel-heading"><div><span class="panel-kicker">空间健康</span><h2>{{ usedPercent >= 90 ? '需要立即处理' : usedPercent >= 80 ? '空间偏紧张' : '当前状态良好' }}</h2></div><span class="health-score" :class="{ warn: usedPercent >= 80, critical: usedPercent >= 90 }">{{ 100 - usedPercent }}</span></div>
              <div class="health-track"><i :style="{ width: `${100 - usedPercent}%` }" /></div>
              <ul>
                <li><Check :size="14" /> 可用空间 {{ formatSize(currentUsage?.free) }}</li>
                <li><FileSearch :size="14" /> {{ result.largeFiles.length }} 个文件超过 100 MB</li>
                <li><ShieldCheck :size="14" /> {{ formatCount(result.skippedItems) }} 个无权限项目已安全跳过</li>
              </ul>
            </section>
          </div>

          <section class="panel attribution-panel">
            <div class="panel-heading">
              <div><span class="panel-kicker">空间归因</span><h2>空间花在哪</h2></div>
              <button class="text-button" @click="goAttribution">完整归因 <ChevronRight :size="15" /></button>
            </div>
            <div class="attr-region-bars">
              <button
                v-for="region in regionAttribution.slice(0, 5)"
                :key="region.id"
                type="button"
                class="attr-region-row"
                @click="openRegionInFiles(region.id)"
              >
                <i :style="{ background: region.color }" />
                <div class="attr-region-copy">
                  <b>{{ region.label }}</b>
                  <small>{{ region.count }} 个目录线索 · {{ (region.share * 100).toFixed(1) }}%</small>
                </div>
                <strong>{{ formatSize(region.size) }}</strong>
                <span class="attr-bar"><em :style="{ width: `${region.share * 100}%`, background: region.color }" /></span>
              </button>
              <div v-if="!regionAttribution.length" class="no-matches">扫描后显示区域归因</div>
            </div>
            <div v-if="projectAttribution.length" class="attr-project-mini">
              <div class="attr-mini-head"><b>项目 TOP</b><small>根据路径与开发目录启发式聚类</small></div>
              <div class="attr-project-chips">
                <button v-for="proj in projectAttribution.slice(0, 6)" :key="proj.key" type="button" class="attr-chip" :title="proj.root" @click="openProject(proj.root, proj.name)">
                  <span>{{ proj.name }}</span>
                  <em>{{ formatSize(proj.size) }}</em>
                </button>
              </div>
            </div>
          </section>

          <section class="panel insight-panel">
            <div class="panel-heading"><div><span class="panel-kicker">优先级建议</span><h2>下一步操作</h2></div></div>
            <div class="insight-grid">
              <button @click="goCleanupFromOverview"><span class="insight-icon green"><Trash2 :size="18" /></span><div><b>清理低风险缓存</b><small>预计释放 {{ formatSize(cleanup?.safeBytes ?? 0) }}（含开发目录）</small></div><ChevronRight :size="16" /></button>
              <button @click="goLargeFilesReview"><span class="insight-icon amber"><FileSearch :size="18" /></span><div><b>复核大文件</b><small>{{ result.largeFiles.length }} 个超过 100 MB 的文件 · 可筛选回收</small></div><ChevronRight :size="16" /></button>
              <button @click="goAttribution"><span class="insight-icon blue"><BarChart3 :size="18" /></span><div><b>空间归因</b><small>区域 / 项目占用分布</small></div><ChevronRight :size="16" /></button>
              <button @click="page = 'files'; fileTab = 'types'"><span class="insight-icon neutral"><Library :size="18" /></span><div><b>按类型浏览</b><small>安装包 / 镜像 / 压缩包 / 视频等</small></div><ChevronRight :size="16" /></button>
            </div>
          </section>
        </template>

        <section v-else-if="!scanning" class="empty-state">
          <div class="empty-visual"><HardDrive :size="42" /><span><Search :size="20" /></span></div>
          <h2>完整分析 {{ selectedDrive }} 的空间占用</h2>
          <p>逐文件读取真实大小，不再使用超时估算。扫描期间只读取元数据，不会修改文件。</p>
          <button class="button primary" @click="startScan()"><Play :size="17" fill="currentColor" /> 开始完整扫描</button>
        </section>
      </template>

      <template v-else-if="page === 'cleanup'">
        <template v-if="!hasScanForDrive">
          <section class="empty-state">
            <div class="empty-visual"><Trash2 :size="42" /><span><Search :size="20" /></span></div>
            <h2>请先完整扫描 {{ selectedDrive }}</h2>
            <p>清理建议在整盘扫描完成后生成。换盘会恢复该盘上次扫描的清理结果；重扫才会刷新。</p>
            <button class="button primary" @click="page = 'overview'; startScan()"><Play :size="17" fill="currentColor" /> 开始完整扫描</button>
          </section>
        </template>
        <template v-else>
          <section class="cleanup-hero">
            <div><span class="panel-kicker">{{ selectedDrive }} 可回收空间</span><strong>{{ loadingCleanup ? '正在分析…' : formatSize(cleanup?.safeBytes ?? 0) }}</strong></div>
            <div class="cleanup-breakdown"><div><span>可处理合计</span><b>{{ formatSize(cleanup?.safeBytes ?? 0) }}</b></div><div><span>开发</span><b>{{ formatSize(cleanup?.developerBytes ?? 0) }}</b></div><div><span>工具/AI</span><b>{{ formatSize(toolAiTotalBytes) }}</b></div><div><span>应用缓存</span><b>{{ formatSize(appCacheTotalBytes) }}</b></div><div><span>当前选择</span><b>{{ formatSize(selectedCleanupBytes) }}</b></div></div>
          </section>

          <section class="cleanup-list panel">
            <div class="cleanup-toolbar">
<div><h2>清理建议</h2><p title="切换上方分类后，「选择本类可处理」只作用于当前分类">强确认项需手动勾选，不会被本类全选带上</p></div>
              <div class="cleanup-actions">
                <button class="text-button" title="查看清理前自动生成的目录+大小快照，清空回收站后仍可对照" @click="openCleanupHistory">清理记录</button>
                <button class="text-button" :disabled="!selectableInTab.length" @click="toggleAllSafe">{{ allTabSelectableSelected ? '取消本类选择' : '选择本类可清理项' }}</button>
                <button class="button secondary" :disabled="!selectedCleanup.length || cleaning || previewingCleanup" @click="previewCleanup"><LoaderCircle v-if="previewingCleanup" :size="16" class="spin" /><Search v-else :size="16" /> 预览释放量</button>
                <button class="button primary" :disabled="!selectedCleanup.length || cleaning" @click="modelStrongConfirm = false; modelConfirmPhrase = ''; confirmCleanup = true"><Trash2 :size="16" /> 移入回收站 · {{ formatSize(selectedCleanupBytes) }}</button>
              </div>
            </div>

            <nav v-if="!loadingCleanup && cleanup?.items.length" class="cleanup-cat-tabs" aria-label="清理分类">
              <button
                v-for="tab in cleanupTabOptions"
                :key="tab.id"
                type="button"
                class="cleanup-cat-tab"
                :class="{ active: cleanupTab === tab.id }"
                @click="cleanupTab = tab.id"
              >
                <Check v-if="cleanupTab === tab.id" :size="14" class="cleanup-cat-check" />
                <b>{{ tab.label }}</b>
                <small v-if="tab.count">{{ tab.count }}</small>
                <small v-if="tab.bytes && tab.id !== 'all' && tab.id !== 'system'" class="bytes">{{ formatSize(tab.bytes) }}</small>
              </button>
            </nav>

            <div v-if="loadingCleanup" class="loading-state"><LoaderCircle :size="24" class="spin" /> 正在分析 {{ selectedDrive }} 可清理项…</div>
            <div v-else-if="!cleanup?.items.length" class="loading-state"><ShieldCheck :size="24" /><span style="margin-left:10px">{{ selectedDrive }} 未发现可清理项</span></div>
            <div v-else-if="!visibleCleanupItems.length" class="loading-state"><ShieldCheck :size="24" /><span style="margin-left:10px">当前分类暂无项目</span></div>
            <div v-else class="cleanup-groups">
              <div v-if="cleanupTab === 'all' || cleanupTab === 'fixed'" v-show="fixedSafeItems.length && (cleanupTab === 'fixed' || cleanupTab === 'all')" class="cleanup-group">
                <div class="cleanup-group-title"><b>固定白名单</b><small>临时文件 / 浏览器缓存 / 转储 · 默认勾选</small></div>
                <div class="cleanup-rows">
                  <div v-for="item in fixedSafeItems" :key="item.id" class="cleanup-row" :class="{ highlight: highlightCleanupPath && samePath(item.path, highlightCleanupPath) }">
                    <button class="check-button" :class="{ checked: selectedCleanup.includes(item.id) }" :disabled="item.size === 0" :aria-label="`选择${item.name}`" @click="toggleCleanup(item.id)"><Check v-if="selectedCleanup.includes(item.id)" :size="14" /></button>
                    <div class="cleanup-copy"><div><b>{{ item.name }}</b><span class="risk-badge safe">低风险</span></div><p>{{ item.description }}</p><small :title="item.path">{{ item.path }}</small></div>
                    <div class="cleanup-size"><b>{{ item.size ? formatSize(item.size) : '—' }}</b><span>{{ item.fileCount ? `${formatCount(item.fileCount)} 个文件` : '暂无可清理项' }}</span></div>
                    <div class="row-space" />
                  </div>
                </div>
              </div>
              <div v-if="cleanupTab === 'all' || cleanupTab === 'developer'" v-show="developerItems.length && (cleanupTab === 'developer' || cleanupTab === 'all')" class="cleanup-group">
                <div class="cleanup-group-title"><b>开发可重建</b><small>node_modules / target / 缓存 · 需手动勾选</small></div>
                <div class="cleanup-rows">
                  <div v-for="item in developerItems" :key="item.id" class="cleanup-row dev-row" :class="{ highlight: highlightCleanupPath && samePath(item.path, highlightCleanupPath) }">
                    <button class="check-button" :class="{ checked: selectedCleanup.includes(item.id) }" :disabled="item.size === 0" :aria-label="`选择${item.name}`" @click="toggleCleanup(item.id)"><Check v-if="selectedCleanup.includes(item.id)" :size="14" /></button>
                    <div class="cleanup-copy"><div><b>{{ item.name }}</b><span class="risk-badge developer">开发可重建</span></div><p>{{ item.description }}</p><small :title="item.path">{{ item.path }}</small></div>
                    <div class="cleanup-size"><b>{{ item.size ? formatSize(item.size) : '—' }}</b><span>{{ item.fileCount ? `${formatCount(item.fileCount)} 个文件` : '暂无可清理项' }}</span></div>
                    <button class="button secondary compact" @click="openPath(item.path)"><FolderOpen :size="15" /> 查看</button>
                  </div>
                </div>
              </div>
              <div v-if="cleanupTab === 'all' || cleanupTab === 'toolai'" v-show="toolAiItems.length && (cleanupTab === 'toolai' || cleanupTab === 'all')" class="cleanup-group">
                <div class="cleanup-group-title"><b>工具/AI 缓存</b><small>包缓存/编辑器缓存默认可清 · 模型需强确认</small></div>
                <div class="cleanup-rows">
                  <div v-for="item in toolAiNormalItems" :key="item.id" class="cleanup-row toolai-row toolai-cleanable" :class="{ highlight: highlightCleanupPath && samePath(item.path, highlightCleanupPath) }">
                    <button class="check-button" :class="{ checked: selectedCleanup.includes(item.id) }" :disabled="item.size === 0" :aria-label="`选择${item.name}`" @click="toggleCleanup(item.id)"><Check v-if="selectedCleanup.includes(item.id)" :size="14" /></button>
                    <div class="cleanup-copy">
                      <div>
                        <b>{{ item.name }}</b>
                        <span class="risk-badge cleanable">可清理</span>
                      </div>
                      <p>{{ item.description }}</p>
                      <small :title="item.path">{{ item.path }}</small>
                    </div>
                    <div class="cleanup-size"><b>{{ item.size ? formatSize(item.size) : '—' }}</b><span>{{ item.fileCount ? `${formatCount(item.fileCount)} 个文件` : '暂无' }}</span></div>
                    <button class="button secondary compact" @click="openPath(item.path)"><FolderOpen :size="15" /> 打开目录</button>
                  </div>
                  <div v-for="item in toolAiModelItems" :key="item.id" class="cleanup-row toolai-row toolai-model" :class="{ highlight: highlightCleanupPath && samePath(item.path, highlightCleanupPath) }">
                    <button class="check-button" :class="{ checked: selectedCleanup.includes(item.id) }" :disabled="item.size === 0" :aria-label="`选择${item.name}`" @click="toggleCleanup(item.id)"><Check v-if="selectedCleanup.includes(item.id)" :size="14" /></button>
                    <div class="cleanup-copy">
                      <div>
                        <b>{{ item.name }}</b>
                        <span class="risk-badge review">需强确认</span>
                      </div>
                      <p>{{ item.description }}</p>
                      <small :title="item.path">{{ item.path }}</small>
                    </div>
                    <div class="cleanup-size"><b>{{ item.size ? formatSize(item.size) : '—' }}</b><span>{{ item.fileCount ? `${formatCount(item.fileCount)} 个文件` : '暂无' }}</span></div>
                    <button class="button secondary compact" @click="openPath(item.path)"><FolderOpen :size="15" /> 打开目录</button>
                  </div>
                </div>
              </div>
              <div v-if="cleanupTab === 'all' || cleanupTab === 'app'" v-show="appCacheItems.length && (cleanupTab === 'app' || cleanupTab === 'all')" class="cleanup-group">
                <div class="cleanup-group-title"><b>应用缓存</b><small>通讯 / 视频 / 会议 / 办公桌面端 · 有客户端才显示 · 需强确认</small></div>
                <div class="cleanup-rows">
                  <div v-for="item in appCacheItems" :key="item.id" class="cleanup-row app-row" :class="{ highlight: highlightCleanupPath && samePath(item.path, highlightCleanupPath) }">
                    <button class="check-button" :class="{ checked: selectedCleanup.includes(item.id) }" :disabled="item.size === 0" :aria-label="`选择${item.name}`" @click="toggleCleanup(item.id)"><Check v-if="selectedCleanup.includes(item.id)" :size="14" /></button>
                    <div class="cleanup-copy">
                      <div>
                        <b>{{ item.name }}</b>
                        <span class="risk-badge review">需强确认</span>
                      </div>
                      <p>{{ item.description }}</p>
                      <small :title="item.path">{{ item.path }}</small>
                    </div>
                    <div class="cleanup-size"><b>{{ item.size ? formatSize(item.size) : '—' }}</b><span>{{ item.fileCount ? `${formatCount(item.fileCount)} 个文件` : '暂无' }}</span></div>
                    <button class="button secondary compact" @click="openPath(item.path)"><FolderOpen :size="15" /> 打开目录</button>
                  </div>
                </div>
              </div>
              <div v-if="cleanupTab === 'all' || cleanupTab === 'review'" v-show="reviewItems.length && (cleanupTab === 'review' || cleanupTab === 'all')" class="cleanup-group">
                <div class="cleanup-group-title"><b>需复核</b><small>不提供一键删除，请打开确认</small></div>
                <div class="cleanup-rows">
                  <div v-for="item in reviewItems" :key="item.id" class="cleanup-row">
                    <span class="action-symbol review"><AlertTriangle :size="17" /></span>
                    <div class="cleanup-copy"><div><b>{{ item.name }}</b><span class="risk-badge review">需复核</span></div><p>{{ item.description }}</p><small :title="item.path">{{ item.path }}</small></div>
                    <div class="cleanup-size"><b>{{ item.size ? formatSize(item.size) : '—' }}</b><span>{{ item.fileCount ? `${formatCount(item.fileCount)} 个文件` : '暂无' }}</span></div>
                    <button class="button secondary compact" @click="openPath(item.path)"><FolderOpen :size="15" /> 查看</button>
                  </div>
                </div>
              </div>
              <div v-if="cleanupTab === 'all' || cleanupTab === 'system'" v-show="systemItems.length && (cleanupTab === 'system' || cleanupTab === 'all')" class="cleanup-group">
                <div class="cleanup-group-title"><b>系统工具</b><small>交给 Windows 存储设置</small></div>
                <div class="cleanup-rows">
                  <div v-for="item in systemItems" :key="item.id" class="cleanup-row">
                    <span class="action-symbol system"><Settings :size="17" /></span>
                    <div class="cleanup-copy"><div><b>{{ item.name }}</b><span class="risk-badge system">系统工具</span></div><p>{{ item.description }}</p><small :title="item.path">{{ item.path }}</small></div>
                    <div class="cleanup-size"><b>—</b><span>由 Windows 评估</span></div>
                    <button class="button secondary compact" @click="openStorageSettings"><Settings :size="15" /> 打开设置</button>
                  </div>
                </div>
              </div>
            </div>
            <div class="cleanup-footnote"><ShieldCheck :size="16" /><span>按盘缓存上次扫描结果；重扫刷新。可先点「预览释放量」不删除。一律移入回收站。</span></div>
          </section>
        </template>
      </template>

      <template v-else-if="page === 'files'">
        <template v-if="folderAnalysis">
          <section class="folder-detail-head panel">
            <button class="icon-button back-button" title="返回上一层结果" @click="leaveFolderAnalysis"><ArrowLeft :size="18" /></button>
            <span class="folder-detail-icon"><FolderTree :size="22" /></span>
            <div><span class="panel-kicker">文件夹下钻分析</span><h2>{{ folderAnalysis.name }}</h2><p :title="folderAnalysis.path">{{ folderAnalysis.path }}</p></div>
            <button class="button secondary compact" @click="openPath(folderAnalysis.path)"><FolderOpen :size="15" /> 打开目录</button>
          </section>
          <section class="folder-metrics">
            <div><span>逻辑大小</span><b>{{ formatSize(folderAnalysis.totalSize) }}</b><small>递归统计全部子项</small></div>
            <div><span>直接子项</span><b>{{ folderAnalysis.children.length }}</b><small>{{ formatCount(folderAnalysis.dirCount) }} 个子目录</small></div>
            <div><span>文件数量</span><b>{{ formatCount(folderAnalysis.fileCount) }}</b><small>{{ folderAnalysis.largeFiles.length }} 个超过 100 MB</small></div>
            <div><span>分析用时</span><b>{{ (folderAnalysis.elapsedMs / 1000).toFixed(1) }} 秒</b><small>跳过 {{ formatCount(folderAnalysis.skippedItems) }} 项</small></div>
          </section>
          <section class="folder-contents panel">
            <div class="panel-heading folder-content-heading"><div><span class="panel-kicker">当前层级</span><h2>子项空间占用</h2></div><button class="button secondary compact" @click="chooseFolder"><FolderSearch :size="15" /> 分析其他文件夹</button></div>
            <div class="folder-analysis-rows">
              <div v-for="item in folderAnalysis.children" :key="item.path" class="folder-analysis-row">
                <span class="folder-kind" :class="item.kind"><FolderOpen v-if="item.kind === 'directory'" :size="18" /><FileText v-else :size="18" /></span>
                <div class="folder-item-copy"><div><b>{{ item.name }}</b><span class="folder-risk" :class="item.risk">{{ item.risk === 'rebuildable' ? '可重建内容' : item.risk === 'protected' ? '不建议删除' : '需要确认' }}</span></div><p>{{ item.recommendation }}</p><small>{{ formatCount(item.fileCount) }} 个文件 · {{ formatCount(item.dirCount) }} 个子目录</small></div>
                <div class="folder-size-bar"><div><b>{{ formatSize(item.size) }}</b><span>{{ folderAnalysis.totalSize ? (item.size / folderAnalysis.totalSize * 100).toFixed(1) : '0.0' }}%</span></div><i><em :class="item.risk" :style="{ width: `${item.size / maxFolderItemSize * 100}%` }" /></i></div>
                <div class="folder-row-actions">
                  <button v-if="item.kind === 'directory' && item.risk === 'rebuildable'" class="button secondary compact" title="跳转到清理中心并定位" @click="openCleanupFromFolder(item)"><Trash2 :size="15" /> 清理</button>
                  <button v-if="item.kind === 'directory'" class="button secondary compact" @click="analyzeFolder(item.path)"><FolderSearch :size="15" /> 分析</button>
                  <button class="icon-button" :title="item.kind === 'file' ? '在资源管理器中定位' : '在资源管理器中打开'" @click="openPath(item.path, item.kind === 'file')"><ExternalLink :size="16" /></button>
                </div>
              </div>
              <div v-if="!folderAnalysis.children.length" class="no-matches">此文件夹没有可读取的子项</div>
            </div>
            <div class="cleanup-footnote folder-note"><Info :size="16" /><span>可重建 = 邻居验证通过。node_modules/target 等可进清理中心；build/dist/out 仅提示。点「清理」跳转清理中心（需先完整扫描当前盘）。</span></div>
          </section>
        </template>

        <template v-else>
          <section class="folder-picker-band">
            <span><FolderSearch :size="21" /></span><div><b>分析指定文件夹</b><small>选择任意目录，查看直接子项的递归大小并继续逐层下钻</small></div><button class="button secondary" :disabled="folderAnalyzing" @click="chooseFolder"><FolderOpen :size="16" /> 选择文件夹</button>
          </section>
        <section v-if="result" class="results-section">
          <div class="result-toolbar">
            <div class="tabs">
              <button :class="{ active: fileTab === 'directories' }" @click="fileTab = 'directories'">目录排行 <span>{{ result.directories.length }}</span></button>
              <button :class="{ active: fileTab === 'files' }" @click="fileTab = 'files'">大文件 <span>{{ result.largeFiles.length }}</span></button>
              <button :class="{ active: fileTab === 'types' }" @click="fileTab = 'types'">按类型 <span>{{ fileTypeGroups.length }}</span></button>
            </div>
            <label class="search"><Search :size="16" /><input v-model="query" :placeholder="fileTab === 'directories' ? '筛选目录' : '筛选文件'" /></label>
          </div>
          <div v-if="fileTab === 'types'" class="type-browse panel">
            <div class="panel-heading"><div><span class="panel-kicker">大文件归因</span><h2>按文件类型汇总</h2></div><small>基于扫描大文件 TOP</small></div>
            <div class="type-cards">
              <button v-for="group in fileTypeGroups" :key="group.name" type="button" class="type-card" :class="{ active: fileTypeFilter === group.name }" @click="fileTypeFilter = fileTypeFilter === group.name ? 'all' : group.name">
                <b>{{ group.name }}</b>
                <strong>{{ formatSize(group.size) }}</strong>
                <small>{{ group.count }} 个文件</small>
                <span v-if="group.name === '视频' || group.name === '图片' || group.name === '音频'" class="type-jump" @click.stop="goMediaFromType(group.name)">媒体管理</span>
              </button>
            </div>
            <div class="file-bulk-actions type-actions">
              <button type="button" class="text-button" :disabled="!typedLargeFiles.length" @click="selectTypedFiles()">勾选下列表</button>
              <button type="button" class="button secondary compact" :disabled="!selectedFilePaths.length" @click="previewRecycleFiles('files')">预览</button>
              <button type="button" class="button primary compact" :disabled="!selectedFilePaths.length || recyclingFiles" @click="requestRecycleFiles('files')"><Recycle :size="14" /> 回收站</button>
              <button type="button" class="button secondary compact" @click="goCleanupFromOverview"><Trash2 :size="14" /> 清理中心</button>
            </div>
            <div class="table-wrap"><table><thead><tr><th class="check-col"></th><th>文件</th><th>类型</th><th>大小</th><th>位置</th><th></th></tr></thead><tbody><tr v-for="item in typedLargeFiles" :key="item.path" :class="{ selected: selectedFilePaths.includes(item.path) }"><td><button type="button" class="check-button" :class="{ checked: selectedFilePaths.includes(item.path) }" @click="toggleFileSelection(item.path)"><Check v-if="selectedFilePaths.includes(item.path)" :size="13" /></button></td><td><b>{{ item.name }}</b></td><td class="muted">{{ fileKindLabel(item.name) }}</td><td><strong>{{ formatSize(item.size) }}</strong></td><td class="muted path-text" :title="item.path">{{ item.path }}</td><td><button class="icon-button" @click="openPath(item.path, true)"><ExternalLink :size="15" /></button></td></tr></tbody></table><div v-if="!typedLargeFiles.length" class="no-matches">当前类型下没有大文件</div></div>
          </div>
          <div v-if="fileTab === 'files'" class="file-filter-bar">
            <div class="filter-group">
              <span>大小</span>
              <div class="chip-row">
                <button v-for="item in [{ id: 'all', label: '全部' }, { id: '100mb', label: '≥100MB' }, { id: '500mb', label: '≥500MB' }, { id: '1gb', label: '≥1GB' }]" :key="item.id" type="button" class="chip-btn" :class="{ active: fileSizeFilter === item.id }" @click="fileSizeFilter = item.id as typeof fileSizeFilter">{{ item.label }}</button>
              </div>
            </div>
            <div class="filter-group">
              <span>修改时间</span>
              <div class="chip-row">
                <button v-for="item in [{ id: 'all', label: '全部' }, { id: 'year', label: '≥90天' }, { id: 'old', label: '≥1年' }]" :key="item.id" type="button" class="chip-btn" :class="{ active: fileAgeFilter === item.id }" @click="fileAgeFilter = item.id as typeof fileAgeFilter">{{ item.label }}</button>
              </div>
            </div>
            <div class="filter-group">
              <span>排序</span>
              <div class="chip-row">
                <button v-for="item in [{ id: 'size', label: '大小' }, { id: 'age', label: '最久' }, { id: 'name', label: '名称' }]" :key="item.id" type="button" class="chip-btn" :class="{ active: fileSort === item.id }" @click="fileSort = item.id as typeof fileSort">{{ item.label }}</button>
              </div>
            </div>
            <div class="file-bulk-actions">
              <button type="button" class="text-button" :disabled="!filteredFiles.length" @click="toggleAllFilteredFiles">{{ allFilteredFilesSelected ? '取消全选' : '全选当前列表' }}</button>
              <button type="button" class="button secondary compact" :disabled="!selectedFilePaths.length || recyclingFiles" @click="previewRecycleFiles('files')"><Search :size="14" /> 预览</button>
              <button type="button" class="button primary compact" :disabled="!selectedFilePaths.length || recyclingFiles" @click="requestRecycleFiles('files')"><Recycle :size="14" /> 回收站 · {{ selectedFilePaths.length }}</button>
            </div>
          </div>
          <div v-if="fileTab === 'directories'" class="table-wrap"><table><thead><tr><th class="rank">#</th><th>目录</th><th>大小</th><th>已用占比</th><th>文件 / 子目录</th><th class="open-col">操作</th></tr></thead><tbody><tr v-for="(item, index) in filteredDirectories" :key="item.path" @dblclick="analyzeFolder(item.path)"><td class="rank">{{ index + 1 }}</td><td><div class="path-cell"><FolderOpen :size="18" /><div><b>{{ item.name }}</b><span :title="item.path">{{ item.path }}</span></div></div></td><td><strong>{{ formatSize(item.size) }}</strong></td><td><div class="share"><span>{{ currentUsage?.used ? (item.size / currentUsage.used * 100).toFixed(1) : '0.0' }}%</span><i><em :class="{ warm: currentUsage?.used && item.size / currentUsage.used > .05, hot: currentUsage?.used && item.size / currentUsage.used > .1 }" :style="{ width: `${item.size / maxDirectorySize * 100}%` }" /></i></div></td><td class="muted">{{ formatCount(item.fileCount) }} / {{ formatCount(item.dirCount) }}</td><td><div class="table-actions"><button class="icon-button" title="分析此文件夹" @click="analyzeFolder(item.path)"><FolderSearch :size="16" /></button><button class="icon-button" title="在资源管理器中打开" @click="openPath(item.path)"><ExternalLink :size="16" /></button></div></td></tr></tbody></table><div v-if="!filteredDirectories.length" class="no-matches">没有匹配的目录</div></div>
          <div v-else-if="fileTab === 'files'" class="table-wrap"><table><thead><tr><th class="check-col"></th><th class="rank">#</th><th>文件</th><th>大小</th><th>修改</th><th>位置</th><th class="open-col"></th></tr></thead><tbody><tr v-for="(item, index) in filteredFiles" :key="item.path" :class="{ selected: selectedFilePaths.includes(item.path) }" @dblclick="openPath(item.path, true)"><td><button type="button" class="check-button" :class="{ checked: selectedFilePaths.includes(item.path) }" @click.stop="toggleFileSelection(item.path)"><Check v-if="selectedFilePaths.includes(item.path)" :size="13" /></button></td><td class="rank">{{ index + 1 }}</td><td><div class="path-cell file"><FileText :size="18" /><div><b>{{ item.name }}</b></div></div></td><td><strong>{{ formatSize(item.size) }}</strong></td><td class="muted">{{ item.modifiedDays == null ? '—' : `${item.modifiedDays} 天` }}</td><td class="muted path-text" :title="item.path">{{ item.path }}</td><td><button class="icon-button" title="在资源管理器中定位" @click.stop="openPath(item.path, true)"><ExternalLink :size="16" /></button></td></tr></tbody></table><div v-if="!filteredFiles.length" class="no-matches">没有符合筛选条件的大文件</div></div>
        </section>
        <section v-else class="empty-state"><div class="empty-visual"><FolderSearch :size="42" /></div><h2>选择一个文件夹开始分析</h2><p>无需等待整盘扫描，可以直接查看指定目录内每个子项的真实占用。</p><div class="empty-actions"><button class="button primary" @click="chooseFolder"><FolderOpen :size="17" /> 选择文件夹</button><button class="button secondary" @click="page = 'overview'; startScan()"><Play :size="17" fill="currentColor" /> 扫描整个磁盘</button></div></section>
        </template>
      </template>

      <template v-else>
        <section class="analysis-tabs section-tabs">
          <button :class="{ active: analysisTab === 'duplicates' }" @click="analysisTab = 'duplicates'"><Fingerprint :size="16" /><span><b>重复文件</b><small>SHA-256</small></span></button>
          <button :class="{ active: analysisTab === 'history' }" @click="analysisTab = 'history'; loadSnapshots()"><History :size="16" /><span><b>空间趋势</b><small>快照 Diff</small></span></button>
          <button :class="{ active: analysisTab === 'age' }" @click="analysisTab = 'age'"><CalendarClock :size="16" /><span><b>文件年龄</b><small>修改热力</small></span></button>
          <button :class="{ active: analysisTab === 'attribution' }" @click="analysisTab = 'attribution'"><BarChart3 :size="16" /><span><b>空间归因</b><small>区域项目</small></span></button>
          <button :class="{ active: analysisTab === 'types' }" @click="analysisTab = 'types'"><Library :size="16" /><span><b>类型占用</b><small>大文件归类</small></span></button>
          <button :class="{ active: analysisTab === 'actions' }" @click="analysisTab = 'actions'"><Sparkles :size="16" /><span><b>行动清单</b><small>优先处理</small></span></button>
        </section>

        <template v-if="analysisTab === 'actions'">
          <template v-if="result">
            <section class="panel action-checklist">
              <div class="panel-heading"><div><span class="panel-kicker">优先处理</span><h2>行动清单</h2></div><small>按收益与风险排序的建议入口</small></div>
              <div class="action-list">
                <button v-for="item in actionChecklist" :key="item.id" type="button" class="action-item" :class="item.priority" @click="item.action()">
                  <span class="action-priority">{{ item.priority === 'high' ? '高' : item.priority === 'medium' ? '中' : '低' }}</span>
                  <div><b>{{ item.title }}</b><small>{{ item.detail }}</small></div>
                  <ChevronRight :size="16" />
                </button>
                <div v-if="!actionChecklist.length" class="no-matches">暂无特别建议，可先检测重复文件或打开清理中心</div>
              </div>
            </section>
          </template>
          <section v-else class="empty-state analysis-empty"><div class="empty-visual"><Sparkles :size="40" /></div><h2>扫描后生成行动清单</h2><p>综合清理项、大文件、重复与项目占用，给出下一步入口。</p><button class="button primary" @click="page = 'overview'; startScan()"><Play :size="17" /> 开始完整扫描</button></section>
        </template>

        <template v-else-if="analysisTab === 'types'">
          <template v-if="result">
            <section class="panel type-browse">
              <div class="panel-heading"><div><span class="panel-kicker">大文件归类</span><h2>类型占用</h2></div><button class="text-button" @click="page = 'files'; fileTab = 'types'">在文件审查中打开</button></div>
              <div class="type-cards">
                <button v-for="group in fileTypeGroups" :key="group.name" type="button" class="type-card" @click="page = 'files'; fileTab = 'types'; fileTypeFilter = group.name">
                  <b>{{ group.name }}</b>
                  <strong>{{ formatSize(group.size) }}</strong>
                  <small>{{ group.count }} 个文件</small>
                </button>
              </div>
              <div v-if="!fileTypeGroups.length" class="no-matches">当前扫描没有大文件类型数据</div>
              <div class="cleanup-footnote"><Info :size="16" /><span>与文件审查「按类型」同源，便于在深度分析中快速跳转处理。</span></div>
            </section>
          </template>
          <section v-else class="empty-state analysis-empty"><div class="empty-visual"><Library :size="40" /></div><h2>扫描后查看类型占用</h2><p>基于大文件 TOP 归类安装包、镜像、视频等。</p><button class="button primary" @click="page = 'overview'; startScan()"><Play :size="17" /> 开始完整扫描</button></section>
        </template>

        <template v-else-if="analysisTab === 'attribution'">
          <template v-if="result">
            <section class="section-head analysis-toolbar">
              <div><span class="panel-kicker">{{ selectedDrive }} 归因</span><h2>空间花在哪</h2><p>基于目录 TOP 与大文件路径的启发式聚类（非精确磁盘会计），用于决策「先处理哪一块」。</p></div>
              <div class="chip-row">
                <button type="button" class="chip-btn" :class="{ active: attributionFocus === 'regions' }" @click="attributionFocus = 'regions'">按区域</button>
                <button type="button" class="chip-btn" :class="{ active: attributionFocus === 'projects' }" @click="attributionFocus = 'projects'">按项目</button>
              </div>
            </section>
            <section v-if="attributionFocus === 'regions'" class="panel attribution-detail">
              <div class="panel-heading"><div><span class="panel-kicker">区域</span><h2>用户 / 系统 / 项目等</h2></div><small>合计线索 {{ formatSize(attributionTotal) }}</small></div>
              <div class="attr-region-bars detailed">
                <article v-for="region in regionAttribution" :key="region.id" class="attr-region-card">
                  <div class="attr-region-row static">
                    <i :style="{ background: region.color }" />
                    <div class="attr-region-copy">
                      <b>{{ region.label }}</b>
                      <small>{{ region.count }} 个路径线索 · {{ (region.share * 100).toFixed(1) }}%</small>
                    </div>
                    <strong>{{ formatSize(region.size) }}</strong>
                  </div>
                  <span class="attr-bar tall"><em :style="{ width: `${region.share * 100}%`, background: region.color }" /></span>
                  <div class="attr-actions">
                    <button type="button" class="button secondary compact" @click="openRegionInFiles(region.id)"><FolderSearch :size="14" /> 在审查中查看</button>
                    <button v-if="region.id === 'user-cache' || region.id === 'projects'" type="button" class="button primary compact" @click="goCleanupFromOverview"><Trash2 :size="14" /> 去清理中心</button>
                    <button v-if="region.id === 'user-files'" type="button" class="button secondary compact" @click="page = 'media'"><Library :size="14" /> 媒体管理</button>
                  </div>
                  <small class="attr-samples" v-if="region.samples.length">例如：{{ region.samples[0] }}</small>
                </article>
              </div>
            </section>
            <section v-else class="panel attribution-detail">
              <div class="panel-heading"><div><span class="panel-kicker">项目</span><h2>疑似工程占用 TOP</h2></div><small>{{ projectAttribution.length }} 个候选</small></div>
              <div class="attr-project-table">
                <div v-for="proj in projectAttribution" :key="proj.key" class="attr-project-row">
                  <div>
                    <b>{{ proj.name }}</b>
                    <small :title="proj.root">{{ proj.root }}</small>
                    <div class="attr-tags"><span v-for="tag in proj.tags" :key="tag">{{ tag }}</span></div>
                  </div>
                  <strong>{{ formatSize(proj.size) }}</strong>
                  <div class="attr-actions">
                    <button type="button" class="button secondary compact" @click="openProject(proj.root, proj.name)"><FolderSearch :size="14" /> 分析</button>
                    <button type="button" class="icon-button" title="打开" @click="openPath(proj.root)"><ExternalLink :size="15" /></button>
                    <button type="button" class="button primary compact" @click="goCleanupFromOverview"><Trash2 :size="14" /> 清理</button>
                  </div>
                </div>
                <div v-if="!projectAttribution.length" class="no-matches">未从当前目录 TOP 识别出明显项目簇。可在「文件审查」中选项目文件夹下钻。</div>
              </div>
            </section>
            <div class="cleanup-footnote"><Info :size="16" /><span>归因是启发式视图，不等于精确分区占用。处理前请在文件审查或清理中心确认。</span></div>
          </template>
          <section v-else class="empty-state analysis-empty"><div class="empty-visual"><BarChart3 :size="40" /></div><h2>扫描后生成空间归因</h2><p>完整扫描后，可从目录 TOP 与大文件路径归纳区域与项目占用。</p><button class="button primary" @click="page = 'overview'; startScan()"><Play :size="17" /> 开始完整扫描</button></section>
        </template>

        <template v-else-if="analysisTab === 'duplicates'">
            <section class="section-head analysis-toolbar">
              <div><span class="panel-kicker">检测范围</span><h2>{{ duplicateReport?.scope ?? `${selectedDrive}\\` }}</h2><p>先按大小预筛，再 SHA-256 校验。可选中重复副本移入回收站（每组建议保留 1 个）。</p></div>
              <div class="duplicate-controls"><div class="chip-row" aria-label="最小文件大小"><button v-for="size in [1, 10, 100]" :key="size" type="button" class="chip-btn" :class="{ active: duplicateMinSize === size * 1024 * 1024 }" @click="duplicateMinSize = size * 1024 * 1024">{{ size }} MB</button></div><button class="button secondary" :disabled="duplicateScanning" @click="chooseDuplicateFolder"><FolderSearch :size="16" /> 选择文件夹</button><button class="button primary" :disabled="duplicateScanning" @click="scanDuplicates()"><Fingerprint :size="16" /> 检测 {{ selectedDrive }}</button></div>
            </section>
          <template v-if="duplicateReport">
            <section class="analysis-metrics">
              <div><span>可避免占用</span><b>{{ formatSize(duplicateReport.wastedBytes) }}</b><small>保留每组一个副本</small></div><div><span>重复组</span><b>{{ duplicateReport.groups.length }}</b><small>{{ duplicateReport.duplicateFiles }} 个重复文件</small></div><div><span>哈希文件</span><b>{{ formatCount(duplicateReport.hashedFiles) }}</b><small>预筛 {{ formatCount(duplicateReport.scannedFiles) }} 个</small></div><div><span>已选副本</span><b>{{ selectedDuplicateCount }}</b><small>{{ formatSize(selectedDuplicateBytes) }}</small></div>
            </section>
            <section class="duplicate-results">
              <div class="duplicate-results-head">
                <div><span class="panel-kicker">内容完全一致</span><h2>重复文件组</h2></div>
                <div class="file-bulk-actions">
                  <button type="button" class="text-button" :disabled="!duplicateReport.groups.length" @click="selectDuplicateCopies">选择全部副本</button>
                  <button type="button" class="text-button" :disabled="!selectedDuplicatePaths.length" @click="clearDuplicateSelection">清空选择</button>
                  <button type="button" class="button secondary compact" :disabled="!selectedDuplicatePaths.length" @click="previewRecycleFiles('duplicates')"><Search :size="14" /> 预览</button>
                  <button type="button" class="button primary compact" :disabled="!selectedDuplicatePaths.length || recyclingFiles" @click="requestRecycleFiles('duplicates')"><Recycle :size="14" /> 回收站 · {{ selectedDuplicateCount }}</button>
                </div>
              </div>
              <div class="duplicate-groups">
                <article v-for="(group, index) in duplicateReport.groups" :key="group.hash" class="duplicate-group">
                  <div class="duplicate-group-head">
                    <span>{{ index + 1 }}</span>
                    <div>
                      <b>每个 {{ formatSize(group.size) }}</b>
                      <small>SHA-256 · {{ group.hash.slice(0, 20) }}…</small>
                    </div>
                    <strong>可释放 {{ formatSize(group.wastedBytes) }}</strong>
                  </div>
                  <div class="duplicate-paths">
                    <div v-for="(file, fileIndex) in group.files" :key="file" class="duplicate-path-row" :class="{ selected: selectedDuplicatePaths.includes(file), keep: fileIndex === 0 }">
                      <button type="button" class="check-button" :class="{ checked: selectedDuplicatePaths.includes(file) }" @click="toggleDuplicatePath(file)"><Check v-if="selectedDuplicatePaths.includes(file)" :size="13" /></button>
                      <FileText :size="15" class="dup-file-icon" />
                      <span class="dup-path" :title="file">{{ file }}</span>
                      <em v-if="fileIndex === 0" class="keep-tag">建议保留</em>
                      <span v-else class="keep-tag-space" />
                      <button type="button" class="icon-button" title="在资源管理器中定位" @click="openPath(file, true)"><ExternalLink :size="15" /></button>
                    </div>
                  </div>
                  <div class="duplicate-group-foot">
                    <button type="button" class="text-button" @click="openDuplicateGroupInExplorer(group)">打开首个位置</button>
                    <button type="button" class="text-button" @click="selectGroupCopies(group)">只选本组副本</button>
                  </div>
                </article>
                <div v-if="!duplicateReport.groups.length" class="no-matches">当前范围没有发现符合条件的重复文件</div>
              </div>
              <div class="cleanup-footnote"><ShieldCheck :size="16" /><span>默认可「选择全部副本」（每组去掉第一个）。移入回收站可还原；请确认无唯一备份或应用引用。</span></div>
            </section>
          </template>
          <section v-else-if="!duplicateScanning" class="empty-state analysis-empty"><div class="empty-visual"><Fingerprint :size="40" /></div><h2>查找内容完全相同的文件</h2><p>建议先用 100 MB 检测整盘，再对可疑文件夹使用 10 MB 或 1 MB。</p><button class="button primary" @click="scanDuplicates()"><Fingerprint :size="17" /> 检测当前磁盘</button></section>
        </template>

        <template v-else-if="analysisTab === 'history'">
          <section v-if="snapshots.length" class="history-summary panel">
            <div class="panel-heading"><div><span class="panel-kicker">{{ selectedDrive }} 本地快照</span><h2>已用空间趋势</h2></div><div class="history-delta" :class="{ down: snapshotDelta < 0 }"><span>较上次</span><b>{{ snapshotDelta >= 0 ? '+' : '−' }}{{ formatSize(Math.abs(snapshotDelta)) }}</b></div></div>
            <div class="trend-chart"><div v-for="snapshot in snapshots" :key="snapshot.id" class="trend-column"><div class="trend-value">{{ Math.round(snapshot.used / snapshot.total * 100) }}%</div><div class="trend-track"><i :style="{ height: `${snapshot.used / snapshot.total * 100}%` }" /></div><span>{{ new Date(snapshot.createdAt).toLocaleDateString('zh-CN', { month: 'numeric', day: 'numeric' }) }}</span></div></div>
            <div class="history-list"><div v-for="snapshot in [...snapshots].reverse().slice(0, 6)" :key="snapshot.id"><span>{{ new Date(snapshot.createdAt).toLocaleString('zh-CN', { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' }) }}</span><b>{{ formatSize(snapshot.used) }}</b><small>{{ formatCount(snapshot.scannedFiles) }} 个文件</small><em>{{ snapshot.directories[0]?.name ?? '—' }} · {{ formatSize(snapshot.directories[0]?.size ?? 0) }}</em></div></div>
            <section v-if="snapshotDiff" class="snapshot-diff">
              <div class="panel-heading compact"><div><span class="panel-kicker">最近两次对比</span><h2>快照 Diff</h2></div><small>文件数 {{ snapshotDiff.filesDelta >= 0 ? '+' : '' }}{{ formatCount(snapshotDiff.filesDelta) }}</small></div>
              <div class="diff-grid">
                <div>
                  <b>目录增长 TOP</b>
                  <div v-for="item in snapshotDiff.dirGrown" :key="'g'+item.path" class="diff-row grow">
                    <span :title="item.path">{{ item.name }}</span>
                    <em>+{{ formatSize(item.delta) }}</em>
                    <button class="icon-button" title="分析目录" @click="analyzeFolder(item.path)"><FolderSearch :size="14" /></button>
                    <button class="icon-button" title="打开" @click="openPath(item.path)"><ExternalLink :size="14" /></button>
                  </div>
                  <div v-if="!snapshotDiff.dirGrown.length" class="no-matches">无明显增长目录</div>
                </div>
                <div>
                  <b>目录下降 TOP</b>
                  <div v-for="item in snapshotDiff.dirShrunk" :key="'s'+item.path" class="diff-row shrink">
                    <span :title="item.path">{{ item.name }}</span>
                    <em>{{ formatSize(item.delta) }}</em>
                    <button class="icon-button" title="打开" @click="openPath(item.path)"><ExternalLink :size="14" /></button>
                  </div>
                  <div v-if="!snapshotDiff.dirShrunk.length" class="no-matches">无明显下降目录</div>
                </div>
                <div>
                  <b>类型变化</b>
                  <div v-for="item in snapshotDiff.typeGrown" :key="item.name" class="diff-row" :class="{ grow: item.delta > 0, shrink: item.delta < 0 }">
                    <span>{{ item.name }}</span>
                    <em>{{ item.delta >= 0 ? '+' : '' }}{{ formatSize(item.delta) }}</em>
                    <button class="text-button" @click="page = 'files'; fileTab = 'types'; fileTypeFilter = item.name">查看</button>
                  </div>
                  <div v-if="!snapshotDiff.typeGrown.length" class="no-matches">类型变化不大</div>
                </div>
              </div>
              <div class="file-bulk-actions" style="margin-top:12px">
                <button class="button secondary compact" @click="goLargeFilesReview"><FileSearch :size="14" /> 去大文件复核</button>
                <button class="button primary compact" @click="goCleanupFromOverview"><Trash2 :size="14" /> 去清理中心</button>
              </div>
            </section>
            <section v-else-if="snapshots.length === 1" class="snapshot-diff snapshot-diff-empty">
              <div class="panel-heading compact"><div><span class="panel-kicker">快照 Diff</span><h2>再扫一次即可对比</h2></div></div>
              <p class="diff-empty-copy">当前 {{ selectedDrive }} 只有 <b>1</b> 次完整扫描快照（{{ new Date(snapshots[0].createdAt).toLocaleString('zh-CN') }} · 已用 {{ formatSize(snapshots[0].used) }}）。再完成一次完整扫描后，这里会显示目录增长/下降与类型变化。</p>
              <div class="file-bulk-actions">
                <button class="button primary compact" @click="page = 'overview'; startScan()"><Play :size="14" /> 再扫一次 {{ selectedDrive }}</button>
                <button class="button secondary compact" @click="goLargeFilesReview"><FileSearch :size="14" /> 先看大文件</button>
              </div>
            </section>
            <div class="cleanup-footnote"><Info :size="16" /><span>每次完整扫描后自动保存快照；至少 2 次才有 Diff。每盘最多 {{ snapshotLimit }} 条，仅本机。</span></div>
          </section>
          <section v-else-if="!snapshotsLoading" class="empty-state analysis-empty"><div class="empty-visual"><History :size="40" /></div><h2>还没有 {{ selectedDrive }} 的历史快照</h2><p>完成一次整盘扫描后会自动记录；至少两次扫描才能看到空间变化。</p><button class="button primary" @click="page = 'overview'; startScan()"><Play :size="17" /> 开始完整扫描</button></section>
          <div v-else class="loading-state"><LoaderCircle :size="24" class="spin" /> 正在读取本地快照</div>
        </template>

        <template v-else>
          <template v-if="result">
            <section class="age-overview panel">
              <div class="panel-heading"><div><span class="panel-kicker">最近修改时间</span><h2>文件年龄热力图</h2></div><small>已统计 {{ formatSize(ageTotal) }}</small></div>
              <div class="age-stack"><i v-for="bucket in result.ageBuckets" :key="bucket.id" :style="{ width: `${ageTotal ? bucket.size / ageTotal * 100 : 0}%`, background: bucket.color }" :title="`${bucket.label}：${formatSize(bucket.size)}`" /></div>
              <div class="age-grid">
                <button
                  v-for="bucket in result.ageBuckets"
                  :key="bucket.id"
                  type="button"
                  class="age-bucket-card"
                  :class="[bucket.id, { active: selectedAgeBucket === bucket.id }]"
                  @click="openAgeBucket(bucket.id)"
                >
                  <span :style="{ background: bucket.color }" />
                  <b>{{ bucket.label }}</b>
                  <strong>{{ formatSize(bucket.size) }}</strong>
                  <small>{{ formatCount(bucket.fileCount) }} 个文件 · {{ ageTotal ? (bucket.size / ageTotal * 100).toFixed(1) : '0.0' }}% · 点击查看大文件</small>
                  <i><em :style="{ width: `${bucket.size / maxAgeSize * 100}%`, background: bucket.color }" /></i>
                </button>
              </div>
            </section>
            <section v-if="selectedAgeBucket" class="old-files panel">
              <div class="panel-heading">
                <div><span class="panel-kicker">年龄分段明细</span><h2>{{ result.ageBuckets.find(b => b.id === selectedAgeBucket)?.label || '所选分段' }} · 大文件</h2></div>
                <div class="file-bulk-actions">
                  <small>{{ ageBucketFiles.length }} 个（来自扫描大文件 TOP）</small>
                  <button type="button" class="text-button" @click="selectedAgeBucket = null">关闭</button>
                </div>
              </div>
              <div class="old-file-rows">
                <div v-for="file in ageBucketFiles" :key="file.path">
                  <CalendarClock :size="17" />
                  <div><b>{{ file.name }}</b><span :title="file.path">{{ file.path }}</span></div>
                  <strong>{{ formatSize(file.size) }}</strong>
                  <em>{{ file.modifiedDays == null ? '未知' : `${file.modifiedDays} 天` }}</em>
                  <button class="icon-button" title="在资源管理器中定位" @click="openPath(file.path, true)"><ExternalLink :size="15" /></button>
                </div>
                <div v-if="!ageBucketFiles.length" class="no-matches">该年龄段在大文件 TOP 中没有条目（完整年龄统计见上方占比）</div>
              </div>
            </section>
            <section class="old-files panel">
              <div class="panel-heading"><div><span class="panel-kicker">优先复核</span><h2>超过一年未修改的大文件</h2></div><small>{{ longUnusedFiles.length }} 个候选</small></div>
              <div class="old-file-rows"><div v-for="file in longUnusedFiles" :key="file.path"><CalendarClock :size="17" /><div><b>{{ file.name }}</b><span :title="file.path">{{ file.path }}</span></div><strong>{{ formatSize(file.size) }}</strong><em>{{ file.modifiedDays }} 天</em><button class="icon-button" title="在资源管理器中定位" @click="openPath(file.path, true)"><ExternalLink :size="15" /></button></div><div v-if="!longUnusedFiles.length" class="no-matches">TOP 大文件中没有超过一年未修改的项目</div></div>
              <div class="cleanup-footnote"><AlertTriangle :size="16" /><span>长期未修改不等于可以删除。可点上方年龄卡片查看分段内大文件。</span></div>
            </section>
          </template>
          <section v-else class="empty-state analysis-empty"><div class="empty-visual"><CalendarClock :size="40" /></div><h2>扫描后生成文件年龄热力图</h2><p>年龄统计复用完整扫描过程，不会额外遍历磁盘。</p><button class="button primary" @click="page = 'overview'; startScan()"><Play :size="17" /> 开始完整扫描</button></section>
        </template>
      </template>
    </main>

    <div v-if="showSettings" class="settings-backdrop" @click.self="showSettings = false">
      <aside class="settings-drawer" role="dialog" aria-modal="true" aria-labelledby="settings-title">
        <header class="settings-head"><div><span class="panel-kicker">应用偏好</span><h2 id="settings-title">设置</h2></div><button class="icon-button" title="关闭设置" @click="showSettings = false"><X :size="19" /></button></header>
        <nav class="settings-tabs" aria-label="设置分类"><button :class="{ active: settingsTab === 'appearance' }" @click="settingsTab = 'appearance'"><Palette :size="17" /> 外观</button><button :class="{ active: settingsTab === 'scanning' }" @click="settingsTab = 'scanning'"><SlidersHorizontal :size="17" /> 扫描</button><button :class="{ active: settingsTab === 'system' }" @click="settingsTab = 'system'"><Wrench :size="17" /> 系统</button><button :class="{ active: settingsTab === 'activity' }" @click="settingsTab = 'activity'; refreshActivityLog()"><History :size="17" /> 活动</button><button :class="{ active: settingsTab === 'about' }" @click="settingsTab = 'about'"><Info :size="17" /> 关于</button></nav>

        <div v-if="settingsTab === 'appearance'" class="settings-content">
          <section class="setting-section"><div class="setting-title"><div><b>界面配色</b><small>保留现有主题色板，U1 增强活力渐变与动效反馈</small></div><span>{{ themeOptions.find(theme => theme.id === activeTheme)?.name }}</span></div><div class="theme-options settings-theme-options"><button v-for="theme in themeOptions" :key="theme.id" :class="{ active: activeTheme === theme.id }" :aria-label="theme.name" @click="applyTheme(theme.id)"><i><em v-for="color in theme.colors" :key="color" :style="{ background: color }" /></i><span>{{ theme.name }}</span><Check v-if="activeTheme === theme.id" :size="14" /></button></div></section>
          <section class="setting-section">
            <div class="setting-title">
              <div><b>界面通透度</b><small>卡片/侧栏/状态栏毛玻璃：越低越透，越高越实</small></div>
              <span>{{ glassStrength < 34 ? '很透' : glassStrength < 67 ? '适中' : '更实' }} · {{ glassStrength }}</span>
            </div>
            <label class="glass-slider">
              <span>透</span>
              <input type="range" min="0" max="100" step="1" :value="glassStrength" @input="applyGlassStrength(Number(($event.target as HTMLInputElement).value))" />
              <span>实</span>
            </label>
            <small class="glass-slider-tip">同时影响状态栏下拉帘、顶栏与面板。本机自动保存。</small>
          </section>
          <section class="setting-section">
            <div class="setting-title">
              <div><b>边框清晰度</b><small>全局调节结构线深浅：面板、列表、分割线。输入框/弹窗会略实一点保证可用性</small></div>
              <span>{{ borderStrength < 28 ? '极淡' : borderStrength < 62 ? '适中' : '清晰' }} · {{ borderStrength }}</span>
            </div>
            <label class="glass-slider">
              <span>淡</span>
              <input type="range" min="0" max="100" step="1" :value="borderStrength" @input="applyBorderStrength(Number(($event.target as HTMLInputElement).value))" />
              <span>清</span>
            </label>
            <small class="glass-slider-tip">建议与通透度搭配：更透时边框略淡，更实时可稍清晰。本机自动保存。</small>
          </section>
          <section class="setting-section"><div class="setting-title"><div><b>字体大小</b><small>调整导航、表格和辅助说明文字</small></div></div><div class="chip-row"><button v-for="item in [{ id: 'small', label: '小' }, { id: 'standard', label: '标准' }, { id: 'large', label: '大' }]" :key="item.id" type="button" class="chip-btn" :class="{ active: fontScale === item.id }" @click="applyFontScale(item.id as FontScale)">{{ item.label }}</button></div></section>
          <section class="setting-section"><div class="setting-title"><div><b>图标大小</b><small>保持布局稳定，只调整图标视觉尺寸</small></div><span class="icon-size-preview"><HardDrive :size="18" /></span></div><div class="chip-row"><button v-for="item in [{ id: 'compact', label: '紧凑' }, { id: 'standard', label: '标准' }, { id: 'large', label: '大' }]" :key="item.id" type="button" class="chip-btn" :class="{ active: iconScale === item.id }" @click="applyIconScale(item.id as IconScale)">{{ item.label }}</button></div></section>
          <section class="setting-section"><div class="setting-title"><div><b>界面密度</b><small>控制导航、表格与结果行的垂直空间</small></div></div><div class="chip-row"><button type="button" class="chip-btn" :class="{ active: uiDensity === 'compact' }" @click="applyDensity('compact')">紧凑</button><button type="button" class="chip-btn" :class="{ active: uiDensity === 'comfortable' }" @click="applyDensity('comfortable')">舒适</button></div></section>
          <p class="settings-footnote"><Check :size="15" /> 外观设置会自动保存在本机。</p>
        </div>

        <div v-else-if="settingsTab === 'scanning'" class="settings-content">
          <section class="setting-section"><div class="setting-title"><div><b>扫描排除目录</b><small>整盘、文件夹、重复文件与媒体分析都会跳过</small></div><button class="button secondary compact" @click="addExclusion"><FolderCog :size="15" /> 添加</button></div><div v-if="exclusionPaths.length" class="exclusion-list"><div v-for="path in exclusionPaths" :key="path"><Ban :size="15" /><span :title="path">{{ path }}</span><button title="移除排除目录" @click="removeExclusion(path)"><X :size="15" /></button></div></div><p v-else class="setting-empty">尚未排除任何目录</p></section>
          <section class="setting-section"><div class="setting-title"><div><b>清理黑名单</b><small>这些路径及其子路径永不出现在清理中心（保护正在用的项目）</small></div><button class="button secondary compact" @click="addCleanupBlacklist"><FolderCog :size="15" /> 添加</button></div><div v-if="cleanupBlacklist.length" class="exclusion-list"><div v-for="path in cleanupBlacklist" :key="path"><ShieldCheck :size="15" /><span :title="path">{{ path }}</span><button title="移除清理黑名单" @click="removeCleanupBlacklist(path)"><X :size="15" /></button></div></div><p v-else class="setting-empty">尚未设置清理黑名单；可把常用 monorepo 根目录加进来</p></section>
          <section class="setting-section"><div class="setting-title"><div><b>默认大文件阈值</b><small>目录排行、媒体和文件审查使用同一标准</small></div><span>{{ largeFileMb >= 1024 ? '1 GB' : `${largeFileMb} MB` }}</span></div><div class="chip-row"><button v-for="value in [50, 100, 500, 1024]" :key="value" type="button" class="chip-btn" :class="{ active: largeFileMb === value }" @click="largeFileMb = value">{{ value === 1024 ? '1 GB' : `${value} MB` }}</button></div></section>
          <section class="setting-section"><div class="setting-title"><div><b>扫描并发数</b><small>媒体解码和哈希任务使用的工作线程</small></div><span>{{ scanThreads }} workers</span></div><div class="chip-row"><button v-for="value in [2, 4, 6, 8]" :key="value" type="button" class="chip-btn" :class="{ active: scanThreads === value }" @click="scanThreads = value">{{ value }}</button></div></section>
          <section class="setting-section"><div class="setting-title"><div><b>快照保留数量</b><small>每个磁盘独立保留，较旧记录自动淘汰</small></div><span>{{ snapshotLimit }} 条 / 盘</span></div><div class="chip-row"><button v-for="value in [10, 30, 60, 100]" :key="value" type="button" class="chip-btn" :class="{ active: snapshotLimit === value }" @click="snapshotLimit = value">{{ value }}</button></div></section>
          <p class="settings-footnote"><Check :size="15" /> 扫描设置将在下次任务启动时生效。</p>
        </div>

        <div v-else-if="settingsTab === 'system'" class="settings-content">
          <section class="setting-section"><div class="setting-title"><div><b>报告保存位置</b><small>HTML 报告和诊断信息默认写入此目录</small></div><button class="button secondary compact" @click="chooseReportDirectory"><FolderOpen :size="15" /> 选择</button></div><div class="setting-path"><span :title="reportDirectory">{{ reportDirectory || '桌面（系统默认）' }}</span><button v-if="reportDirectory" title="恢复默认位置" @click="reportDirectory = ''"><X :size="15" /></button></div></section>
          <section class="setting-section"><div class="setting-title"><div><b>回收站</b><small>删除的文件一律进 Windows 系统回收站（资源管理器可见、可右键还原），应用内只保留清理记录</small></div><Recycle :size="19" /></div><div class="chip-row"><button type="button" class="chip-btn" :class="{ active: recyclePolicy === 'confirm' }" @click="recyclePolicy = 'confirm'">每次确认</button><button type="button" class="chip-btn" :class="{ active: recyclePolicy === 'direct' }" @click="recyclePolicy = 'direct'">直接移入回收站</button></div><div class="setting-action-row recycle-settings-row"><span class="recycle-bin-usage"><Recycle :size="15" /> 系统回收站占用 <b>{{ systemRecycleBytes ? formatSize(systemRecycleBytes) : '—' }}</b></span><button class="button secondary compact" @click="openSystemRecycleBin"><FolderOpen :size="15" /> 打开系统回收站</button><button class="button danger compact" :class="{ confirm: confirmEmptySystemBin }" @click="emptySystemRecycleBin"><LoaderCircle v-if="recycleBusy === 'empty'" :size="15" class="spin" /><Trash2 v-else :size="15" /> {{ confirmEmptySystemBin ? '再次点击确认清空' : '清空系统回收站' }}</button><button class="text-button" @click="openRecycleBin">清理记录</button></div></section>
          <section class="setting-section"><div class="setting-title"><div><b>启动时检查更新</b><small>通过 GitHub Releases 检查公开发布版本</small></div><button class="toggle-switch" role="switch" :aria-checked="autoCheckUpdates" :class="{ active: autoCheckUpdates }" @click="autoCheckUpdates = !autoCheckUpdates"><i /></button></div><div class="setting-action-row"><span :class="{ success: updateStatus && !updateStatus.available, update: updateStatus?.available }">{{ updateStatus?.message || '尚未检查更新' }}</span><button class="button secondary compact" :disabled="settingsBusy === 'update'" @click="checkUpdates()"><LoaderCircle v-if="settingsBusy === 'update'" :size="15" class="spin" /><RefreshCw v-else :size="15" /> 立即检查</button></div></section>
          <section class="setting-section system-actions"><div><div><b>诊断信息</b><small>导出版本、平台、设置和快照状态，不包含文件内容</small></div><button class="button secondary compact" :disabled="settingsBusy === 'diagnostics'" @click="exportDiagnostics"><LoaderCircle v-if="settingsBusy === 'diagnostics'" :size="15" class="spin" /><Download v-else :size="15" /> 导出诊断</button></div><div><div><b>本地扫描历史</b><small>清除全部磁盘空间快照，不会删除文件</small></div><button class="button danger compact" @click="confirmClearHistory = true"><Trash2 :size="15" /> 清除历史</button></div></section>
        </div>

                <div v-else-if="settingsTab === 'activity'" class="settings-content">
          <section class="setting-section">
            <div class="setting-title">
              <div><b>活动日志</b><small>通知、错误、扫描/清理/回收/注册表等操作摘要（本机保存）</small></div>
              <span>{{ activityLog.length }} 条</span>
            </div>
            <div class="setting-title" style="margin-top:8px">
              <div><b>详细记录</b><small>开启后清理/回收等会写入项目清单，详情更完整；关闭则只记摘要更省空间</small></div>
              <button class="toggle-switch" role="switch" :aria-checked="detailedActivityLog" :class="{ active: detailedActivityLog }" @click="toggleDetailedActivityLog"><i /></button>
            </div>
            <div class="chip-row activity-filter">
              <button v-for="item in [{ id: 'all', label: '全部' }, { id: 'notice', label: '通知' }, { id: 'error', label: '错误' }, { id: 'recycle', label: '回收' }, { id: 'cleanup', label: '清理' }, { id: 'registry', label: '注册表' }]" :key="item.id" type="button" class="chip-btn" :class="{ active: activityFilter === item.id }" @click="activityFilter = item.id as any; refreshActivityLog()">{{ item.label }}</button>
            </div>
            <div class="activity-log-list">
              <article v-for="item in filteredActivityLog" :key="item.id" class="activity-log-row clickable" :class="item.kind" @click="openActivityDetail(item)">
                <div class="activity-log-head"><b>{{ kindLabel(item.kind) }}</b><span>{{ new Date(item.at).toLocaleString('zh-CN') }}</span></div>
                <p>{{ item.title }}</p>
                <small v-if="item.detail">{{ item.detail }}</small>
              </article>
              <p v-if="!filteredActivityLog.length" class="setting-empty">暂无活动记录</p>
            </div>
            <div class="setting-action-row" style="margin-top:12px">
              <button class="button secondary compact" @click="exportActivityLogFile"><Download :size="15" /> 导出</button>
              <button class="button danger compact" @click="clearAllActivity"><Trash2 :size="15" /> 清空</button>
            </div>
          </section>
          <p class="settings-footnote"><Info :size="15" /> 仅保存在本机浏览器存储，不会上传。完整操作审计可后续再加强。</p>
        </div>
<div v-else class="settings-content about-content">
          <div class="about-product"><span class="about-mark"><HardDrive :size="28" /></span><div><h3>磁盘空间分析器</h3><p>Windows 本地空间诊断、媒体管理与安全清理工具</p><b>版本 {{ APP_VERSION }}</b></div></div>
          <section class="about-section"><h4>系统架构</h4><dl><div><dt>桌面框架</dt><dd>Tauri 2</dd></div><div><dt>用户界面</dt><dd>Vue 3 + TypeScript</dd></div><div><dt>扫描引擎</dt><dd>Rust</dd></div><div><dt>运行平台</dt><dd>Windows 10 / 11 · 64 位</dd></div></dl></section>
          <section class="about-section"><h4>作者信息</h4><dl><div><dt>项目作者 / GitHub</dt><dd>songmeng@hotmail.com</dd></div><div><dt>软件许可</dt><dd>MIT License</dd></div></dl></section>
          <section class="about-safety"><ShieldCheck :size="20" /><div><b>本地优先，删除可恢复</b><p>扫描、哈希、缩略图和历史快照均在本机处理。媒体与清理中心统一移入应用回收桶，可在清理中心一键还原；开发缓存需邻居验证后才可勾选。</p></div></section>
        </div>
      </aside>
    </div>

    <div v-if="confirmClearHistory" class="modal-backdrop" @click.self="confirmClearHistory = false">
      <section class="confirm-dialog" role="dialog" aria-modal="true" aria-label="确认清除扫描历史">
        <button class="dialog-close" aria-label="关闭" @click="confirmClearHistory = false"><X :size="18" /></button>
        <span class="dialog-icon history"><Trash2 :size="24" /></span>
        <h2>清除全部本地扫描历史？</h2>
        <p>将删除所有磁盘的空间快照与增长趋势记录。此操作不会删除磁盘中的任何文件。</p>
        <div class="dialog-actions"><button class="button secondary" :disabled="settingsBusy === 'history'" @click="confirmClearHistory = false">取消</button><button class="button danger-solid" :disabled="settingsBusy === 'history'" @click="clearLocalHistory"><LoaderCircle v-if="settingsBusy === 'history'" :size="16" class="spin" /><Trash2 v-else :size="16" /> 确认清除</button></div>
      </section>
    </div>

    
    <div v-if="showMessageLog" class="modal-backdrop" @click.self="showMessageLog = false; selectedActivity = null">
      <section class="confirm-dialog message-log-dialog" role="dialog" aria-modal="true" :aria-label="selectedActivity ? '通知详情' : '活动日志'">
        <button class="dialog-close" aria-label="关闭" @click="showMessageLog = false; selectedActivity = null"><X :size="18" /></button>
        <template v-if="selectedActivity">
          <span class="dialog-icon history"><History :size="26" /></span>
          <h2>通知详情</h2>
          <div class="activity-detail">
            <div class="activity-detail-row"><span>类型</span><b>{{ kindLabel(selectedActivity.kind) }}</b></div>
            <div class="activity-detail-row"><span>时间</span><b>{{ new Date(selectedActivity.at).toLocaleString('zh-CN') }}</b></div>
            <div class="activity-detail-block"><span>摘要</span><p>{{ selectedActivity.title }}</p></div>
            <div v-if="selectedActivity.detail" class="activity-detail-block"><span>详情清单</span><pre class="activity-detail-pre">{{ selectedActivity.detail }}</pre></div>
            <div v-if="activityMetaLines(selectedActivity).length" class="activity-detail-block">
              <span>附加信息</span>
              <dl class="activity-meta">
                <div v-for="row in activityMetaLines(selectedActivity)" :key="row.key">
                  <dt>{{ row.key }}</dt>
                  <dd>
                    <button v-if="row.key.toLowerCase().includes('path') || row.key.toLowerCase().includes('directory') || row.key.toLowerCase().includes('backup')" type="button" class="text-button" @click="openPathFromMeta(row.value)">{{ row.value }}</button>
                    <template v-else>{{ row.value }}</template>
                  </dd>
                </div>
              </dl>
            </div>
          </div>
          <div class="dialog-actions">
            <button class="button secondary" @click="selectedActivity = null">返回列表</button>
            <button class="button primary" @click="showMessageLog = false; selectedActivity = null">关闭</button>
          </div>
        </template>
        <template v-else>
          <span class="dialog-icon history"><History :size="26" /></span>
          <h2>活动日志</h2>
          <p>点击任一条查看完整详情（本机保存）。</p>
          <div class="chip-row activity-filter modal-filter">
            <button type="button" class="chip-btn" :class="{ active: activityFilter === 'all' }" @click="activityFilter = 'all'">全部</button>
            <button type="button" class="chip-btn" :class="{ active: activityFilter === 'error' }" @click="activityFilter = 'error'">错误</button>
            <button type="button" class="chip-btn" :class="{ active: activityFilter === 'recycle' }" @click="activityFilter = 'recycle'">回收</button>
            <button type="button" class="chip-btn" :class="{ active: activityFilter === 'registry' }" @click="activityFilter = 'registry'">注册表</button>
          </div>
          <div class="message-log-list">
            <button v-for="item in filteredActivityLog" :key="item.id" type="button" class="message-log-row clickable" :class="item.kind" @click="selectedActivity = item">
              <b>{{ kindLabel(item.kind) }}</b>
              <span>{{ new Date(item.at).toLocaleString('zh-CN') }}</span>
              <p>{{ item.title }}</p>
              <small v-if="item.detail">{{ item.detail }}</small>
            </button>
            <div v-if="!filteredActivityLog.length" class="no-matches">暂无记录</div>
          </div>
          <div class="dialog-actions">
            <button class="button secondary" @click="openActivityInSettings">在设置中打开</button>
            <button class="button secondary" @click="exportActivityLogFile">导出</button>
            <button class="button secondary" @click="clearAllActivity">清空</button>
            <button class="button primary" @click="showMessageLog = false">关闭</button>
          </div>
        </template>
      </section>
    </div>
    <div v-if="confirmRecycleFiles" class="modal-backdrop" @click.self="confirmRecycleFiles = false">
      <section class="confirm-dialog" role="dialog" aria-modal="true" aria-label="确认移入回收站">
        <button class="dialog-close" aria-label="关闭" @click="confirmRecycleFiles = false"><X :size="18" /></button>
        <span class="dialog-icon"><Recycle :size="26" /></span>
        <h2>将 {{ recycleTargetPaths.length }} 个文件移入回收站？</h2>
        <p>合计约 {{ formatSize(recycleTargetBytes) }}。不会永久删除，将进入 Windows 系统回收站，可在资源管理器中还原。</p>
        <div class="dialog-actions">
          <button class="button secondary" :disabled="recyclingFiles" @click="confirmRecycleFiles = false">取消</button>
          <button class="button primary" :disabled="recyclingFiles" @click="runRecycleFiles">
            <LoaderCircle v-if="recyclingFiles" :size="16" class="spin" />
            <Recycle v-else :size="16" />
            确认移入回收站
          </button>
        </div>
      </section>
    </div>

    <div v-if="confirmCleanup" class="modal-backdrop" @click.self="confirmCleanup = false">
      <section class="confirm-dialog" role="dialog" aria-modal="true" aria-labelledby="confirm-title">
        <button class="dialog-close" aria-label="关闭" @click="confirmCleanup = false"><X :size="18" /></button>
        <span class="dialog-icon"><Trash2 :size="24" /></span>
        <h2 id="confirm-title">确认移入回收站 {{ formatSize(selectedCleanupBytes) }}？</h2>
        <p>磁盘 <b>{{ selectedDrive }}</b> · 共 {{ selectedCleanupItems.length }} 项 · 约 {{ formatSize(selectedCleanupBytes) }}。一律进 Windows 系统回收站（可还原），非永久删除。热目录/占用文件会自动跳过。</p>
        <div class="confirm-items"><div v-for="item in selectedCleanupItems" :key="item.id"><Check :size="14" /><span>{{ item.name }}{{ item.category === 'developer' ? ' · 开发' : item.category === 'app' ? ' · 应用缓存' : item.requiresStrongConfirm ? ' · 模型/强确认' : item.category === 'toolai' ? ' · 工具/AI' : '' }}</span><b>{{ formatSize(item.size) }}</b></div></div>
        <div v-if="selectedHasModelItems" class="model-strong-box">
          <div class="model-strong-head">
            <b>含需强确认项约 {{ formatSize(selectedModelBytes) }}</b>
            <span>模型/应用缓存 · 删除后可能难恢复</span>
          </div>
          <label class="model-strong-check">
            <input type="checkbox" v-model="modelStrongConfirm" />
            <span>我了解风险：删除后需重新下载或可能丢失本地缓存</span>
          </label>
          <div class="model-strong-input">
            <div class="model-strong-input-label">
              <span>输入确认词以继续</span>
              <em>确认删除模型缓存</em>
            </div>
            <input
              type="text"
              v-model="modelConfirmPhrase"
              placeholder="在此输入确认词"
              autocomplete="off"
              spellcheck="false"
              :class="{ ok: modelConfirmPhrase.trim() === '确认删除模型缓存', bad: modelConfirmPhrase.length > 0 && modelConfirmPhrase.trim() !== '确认删除模型缓存' }"
            />
          </div>
        </div>
        <div v-if="cleanupProgress" class="cleanup-progress"><div class="cleanup-progress-info"><LoaderCircle :size="14" class="spin" /><span>{{ cleanupProgress.message }}</span><b>{{ cleanupProgress.percent }}%</b></div><div class="cleanup-progress-bar"><i :style="{ width: cleanupProgress.percent + '%' }" /></div></div>
        <div class="dialog-actions"><button class="button secondary" :disabled="cleaning || previewingCleanup" @click="confirmCleanup = false">取消</button><button class="button secondary" :disabled="!selectedCleanup.length || cleaning || previewingCleanup" @click="previewCleanup"><LoaderCircle v-if="previewingCleanup" :size="16" class="spin" /><Search v-else :size="16" /> 先预览</button><button class="button danger-solid" :disabled="cleaning || (selectedHasModelItems && (!modelStrongConfirm || modelConfirmPhrase.trim() !== '确认删除模型缓存'))" @click="runCleanup"><LoaderCircle v-if="cleaning" :size="16" class="spin" /><Trash2 v-else :size="16" /> {{ cleaning ? '正在移入' : '确认移入回收站' }}</button></div>
      </section>
    </div>

    <div v-if="showRecycleBin" class="modal-backdrop" @click.self="showRecycleBin = false">
      <section class="confirm-dialog recycle-bin-dialog" role="dialog" aria-modal="true" aria-label="回收站与清理记录">
        <button class="dialog-close" aria-label="关闭" @click="showRecycleBin = false"><X :size="18" /></button>
        <span class="dialog-icon"><Recycle :size="26" /></span>
        <h2>回收站与清理记录</h2>
        <p>删除的文件都在 <b>Windows 系统回收站</b> 中，资源管理器里右键即可还原到原位置（不额外占空间、卸载无残留）。下方是应用内每次清理留下的记录。<b v-if="systemRecycleBytes"> 当前系统回收站占用 {{ formatSize(systemRecycleBytes) }}</b></p>
        <div class="recycle-actions-row">
          <button class="button secondary compact" @click="openSystemRecycleBin"><FolderOpen :size="15" /> 打开系统回收站</button>
          <button class="button danger compact" :class="{ confirm: confirmEmptySystemBin }" @click="emptySystemRecycleBin"><LoaderCircle v-if="recycleBusy === 'empty'" :size="15" class="spin" /><Trash2 v-else :size="15" /> {{ confirmEmptySystemBin ? '再次点击确认清空' : '清空系统回收站' }}</button>
        </div>
        <div class="recycle-bin-subtitle"><History :size="15" /> 应用清理记录（{{ recycleEntries.length }} 条）</div>
        <div v-if="recycleBusy === 'entries'" class="recycle-bin-tip"><LoaderCircle :size="15" class="spin" /> 正在清空记录…</div>
        <div v-else-if="!recycleEntries.length" class="recycle-bin-tip"><ShieldCheck :size="18" /> 暂无清理记录</div>
        <div v-else class="recycle-bin-list">
          <div v-for="entry in recycleEntries" :key="entry.id" class="recycle-bin-group">
            <div class="recycle-bin-head">
              <div class="recycle-bin-title">
                <b>{{ entry.label }}</b>
                <span class="source-tag">{{ entry.source }}</span>
                <small>{{ new Date(Number(entry.createdAt.split('.')[0]) * 1000).toLocaleString('zh-CN') }}</small>
              </div>
              <div class="recycle-bin-summary">
                <b>{{ formatSize(entry.totalBytes) }}</b>
                <small>{{ formatCount(entry.fileCount) }} 项</small>
              </div>
            </div>
            <details class="recycle-bin-detail">
              <summary>查看原路径清单（{{ entry.items.length }} 个顶层路径）</summary>
              <ul>
                <li v-for="item in entry.items" :key="item.original" :title="item.original">{{ item.original }}</li>
              </ul>
            </details>
          </div>
        </div>
        <div class="dialog-actions">
          <button class="button secondary" :disabled="!recycleEntries.length || recycleBusy === 'entries'" @click="clearRecycleEntries"><LoaderCircle v-if="recycleBusy === 'entries'" :size="16" class="spin" /><Trash2 v-else :size="16" /> 清空记录</button>
          <button class="button primary" @click="showRecycleBin = false">关闭</button>
        </div>
      </section>
    </div>

    <div v-if="showCleanupHistory" class="modal-backdrop" @click.self="showCleanupHistory = false">
      <section class="confirm-dialog cleanup-history-dialog" role="dialog" aria-modal="true" aria-label="清理记录">
        <button class="dialog-close" aria-label="关闭" @click="showCleanupHistory = false"><X :size="18" /></button>
        <span class="dialog-icon history"><History :size="26" /></span>
        <h2>清理记录</h2>
        <p>每次清理前自动保存目录与大小快照，即使回收站已清空也能对照当时清掉了什么（仅记录，不可恢复文件）。</p>
        <div v-if="cleanupHistoryBusy === 'loading'" class="recycle-bin-tip"><LoaderCircle :size="18" class="spin" /> 正在读取…</div>
        <div v-else-if="!cleanupHistory.length" class="recycle-bin-tip"><ShieldCheck :size="18" /> 暂无清理记录</div>
        <div v-else class="recycle-bin-list">
          <div v-for="snap in cleanupHistory" :key="snap.id" class="recycle-bin-group">
            <div class="recycle-bin-head">
              <div class="recycle-bin-title">
                <b>{{ snap.drive }} · {{ snap.entries.length }} 组</b>
                <small>{{ new Date(Number(snap.createdAt.split('.')[0]) * 1000).toLocaleString('zh-CN') }}</small>
              </div>
              <button class="button danger-solid compact" :disabled="cleanupHistoryBusy === snap.id" @click="deleteCleanupSnapshot(snap.id)"><LoaderCircle v-if="cleanupHistoryBusy === snap.id" :size="15" class="spin" /><Trash2 v-else :size="15" /> 删除记录</button>
            </div>
            <details v-for="(entry, idx) in snap.entries" :key="idx" class="recycle-bin-detail">
              <summary>{{ entry.label }} · {{ entry.paths.length }} 个路径 · 约 {{ formatSize(snapshotTotalBytes(entry)) }}</summary>
              <ul>
                <li v-for="p in entry.paths" :key="p.path" :title="p.path">{{ p.path }} <em>{{ formatSize(p.size) }}<template v-if="p.modifiedDays != null"> · {{ p.modifiedDays }} 天前修改</template></em></li>
              </ul>
            </details>
          </div>
        </div>
        <div class="dialog-actions"><button class="button primary" @click="showCleanupHistory = false">关闭</button></div>
      </section>
    </div>
  </div>
</template>

<style>
:root{font-family:"Segoe UI","Microsoft YaHei",sans-serif;color:#1d2939;background:#f3f5f7;font-synthesis:none}*{box-sizing:border-box}body{margin:0;min-width:760px;min-height:100vh}button,input{font:inherit;letter-spacing:0}button{cursor:pointer}.app-shell{min-height:100vh;display:grid;grid-template-columns:232px 1fr}.sidebar{position:fixed;inset:0 auto 0 0;width:232px;background:#18212b;color:#f8fafc;padding:20px 14px 16px;display:flex;flex-direction:column;border-right:1px solid #111820}.brand{display:flex;align-items:center;gap:11px;padding:2px 6px 22px}.brand-mark{width:36px;height:36px;display:grid;place-items:center;background:#e8583e;color:#fff;border-radius:6px}.brand strong{display:block;font-size:14px}.brand small{display:block;color:#8f9dac;font-size:10px;margin-top:2px}.main-nav{display:grid;gap:3px;margin-bottom:22px}.main-nav button{height:39px;border:0;background:transparent;color:#93a0ae;border-radius:5px;padding:0 9px;display:grid;grid-template-columns:19px 1fr auto;gap:9px;align-items:center;text-align:left;font-size:11px}.main-nav button:hover{background:#202c38;color:#fff}.main-nav button.active{background:#273543;color:#fff}.main-nav button b{font-size:8px;color:#e9a295;background:#442b2b;padding:2px 6px;border-radius:8px}.sidebar-label{font-size:9px;color:#718096;text-transform:uppercase;padding:0 8px 7px}.drive-list{display:grid;gap:4px}.drive-button{border:0;background:transparent;color:#9eabb9;border-radius:5px;padding:9px;display:grid;grid-template-columns:18px 1fr 7px;gap:8px;text-align:left;align-items:center}.drive-button:hover{background:#202c38;color:#fff}.drive-button.active{background:#273543;color:#fff}.drive-button span b,.drive-button span small{display:block}.drive-button span b{font-size:11px;font-weight:600}.drive-button span small{font-size:9px;color:#728192;margin-top:2px}.drive-button i{width:6px;height:6px;background:#e8583e;border-radius:50%}.drive-button:disabled{cursor:not-allowed;opacity:.6}.drive-loading{font-size:10px;color:#8f9dac;padding:12px;display:flex;gap:8px}.sidebar-spacer{flex:1}.safety-note{display:flex;gap:9px;align-items:flex-start;color:#82cbb2;background:#1c302d;border:1px solid #29443e;border-radius:6px;padding:11px}.safety-note b,.safety-note span{display:block}.safety-note b{font-size:10px}.safety-note span{font-size:9px;color:#78a699;margin-top:2px;line-height:1.45}.preview-badge{display:flex;align-items:center;gap:6px;margin-top:10px;padding:8px;color:#f0c36d;font-size:10px}.version{font-size:9px;color:#556575;text-align:center;margin-top:15px}.workspace{grid-column:2;padding:26px 30px 48px;max-width:1500px;width:100%;margin:0 auto}.topbar{display:flex;justify-content:space-between;align-items:center;margin-bottom:22px}.eyebrow{font-size:10px;color:#e8583e;font-weight:700}.topbar h1{font-size:24px;line-height:1.2;margin:4px 0 0;letter-spacing:0}.actions{display:flex;gap:8px}.button{height:38px;border-radius:5px;border:1px solid transparent;display:inline-flex;align-items:center;justify-content:center;gap:8px;padding:0 15px;font-size:11px;font-weight:650;white-space:nowrap}.button.primary{background:#e8583e;color:#fff;box-shadow:0 1px 2px #9f2e1e33}.button.primary:hover{background:#d94c34}.button.secondary{background:#fff;border-color:#d7dce2;color:#344054}.button.danger{background:#fff;border-color:#f2b4a9;color:#c43d28}.button.danger-solid{background:#c94331;color:#fff}.button.compact{height:32px;padding:0 10px}.button:disabled,.text-button:disabled,.check-button:disabled{opacity:.45;cursor:not-allowed}.alert{display:flex;align-items:center;gap:9px;padding:10px 12px;border-radius:5px;margin-bottom:12px;font-size:11px}.alert span{flex:1}.alert button{border:0;background:transparent;color:inherit;display:grid;place-items:center}.alert.error{background:#fef0ed;border:1px solid #f8c6bd;color:#9f2e1e}.alert.notice{background:#edf8f4;border:1px solid #b8e2d2;color:#176b50}.scan-strip{background:#fff;border:1px solid #dfe3e8;border-left:3px solid #e8583e;border-radius:5px;padding:13px 15px;margin-bottom:14px}.scan-line{display:flex;align-items:center;gap:8px;font-size:11px}.scan-line strong{flex:1}.scan-line b{color:#e8583e}.pulse-dot{width:7px;height:7px;border-radius:50%;background:#e8583e;box-shadow:0 0 0 4px #fae2dd}.progress-track{height:4px;background:#edf0f2;margin:11px 0 7px;border-radius:2px;overflow:hidden}.progress-track div{height:100%;background:#e8583e;transition:width .25s}.current-path{font-size:9px;color:#8a94a3;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.metrics{display:grid;grid-template-columns:repeat(4,minmax(150px,1fr));background:#fff;border:1px solid #dfe3e8;border-radius:6px;margin-bottom:14px}.metric{min-height:106px;padding:20px;display:flex;align-items:center;gap:13px;border-right:1px solid #e7eaee}.metric:last-child{border:0}.metric-icon{width:36px;height:36px;border-radius:5px;display:grid;place-items:center}.metric-icon.coral{background:#fce9e5;color:#d94c34}.metric-icon.blue{background:#e7f0fb;color:#3475b8}.metric-icon.green{background:#e5f5ef;color:#218b68}.metric-icon.amber{background:#fbf1dc;color:#a66c09}.metric span,.metric small,.metric strong{display:block}.metric span{font-size:9px;color:#7d8896}.metric strong{font-size:20px;margin:3px 0;color:#1d2939}.metric small{font-size:9px;color:#98a2b3}.reclaim-band{display:flex;align-items:center;gap:14px;padding:15px 17px;background:#eff8f4;border:1px solid #bfe2d5;border-radius:6px;margin-bottom:14px;color:#176b50}.reclaim-icon{width:40px;height:40px;border-radius:5px;background:#d7eee6;display:grid;place-items:center}.reclaim-band>div:nth-child(2){flex:1}.reclaim-band span,.reclaim-band strong,.reclaim-band small{display:block}.reclaim-band span{font-size:9px}.reclaim-band strong{font-size:18px;margin:1px 0}.reclaim-band small{font-size:9px;color:#609381}.content-grid{display:grid;grid-template-columns:minmax(430px,1.35fr) minmax(300px,.65fr);gap:14px;margin-bottom:14px}.panel,.results-section{background:#fff;border:1px solid #dfe3e8;border-radius:6px}.panel{padding:18px 20px}.panel-heading{display:flex;justify-content:space-between;align-items:flex-start}.panel-heading h2{font-size:14px;margin:3px 0}.panel-heading small,.panel-kicker{font-size:9px;color:#98a2b3}.panel-kicker{text-transform:uppercase;font-weight:700}.distribution-body{display:flex;align-items:center;gap:28px;margin-top:14px}.donut{width:126px;height:126px;flex:none;border-radius:50%;display:grid;place-items:center}.donut>div{width:78px;height:78px;border-radius:50%;background:#fff;display:grid;place-content:center;text-align:center}.donut strong,.donut span{display:block}.donut strong{font-size:19px}.donut span{font-size:9px;color:#8a94a3;margin-top:2px}.legend{flex:1;display:grid;grid-template-columns:repeat(2,minmax(130px,1fr));gap:10px 16px}.legend>div{display:grid;grid-template-columns:7px 1fr auto;align-items:center;gap:7px;font-size:9px}.legend i{width:7px;height:7px;border-radius:2px}.legend b{font-size:10px}.health-score{width:38px;height:38px;border-radius:50%;background:#e5f5ef;color:#218b68;display:grid;place-items:center;font-size:13px;font-weight:700}.health-score.warn{background:#fbf1dc;color:#a66c09}.health-score.critical{background:#fce9e5;color:#c94331}.health-track{height:6px;background:#edf0f2;border-radius:3px;margin:21px 0 17px;overflow:hidden}.health-track i{display:block;height:100%;background:#38a47c;border-radius:3px}.health-panel ul{list-style:none;padding:0;margin:0;display:grid;gap:11px}.health-panel li{display:flex;align-items:center;gap:8px;font-size:10px;color:#667085}.insight-panel{margin-bottom:14px}.insight-grid{display:grid;grid-template-columns:repeat(3,1fr);gap:10px;margin-top:14px}.insight-grid button{border:1px solid #e4e7eb;border-radius:5px;background:#fafbfc;padding:12px;display:grid;grid-template-columns:34px 1fr 16px;gap:9px;align-items:center;text-align:left;color:#667085}.insight-grid button:hover{background:#f4f6f8;border-color:#cfd5dc}.insight-grid b,.insight-grid small{display:block}.insight-grid b{font-size:10px;color:#273443}.insight-grid small{font-size:8px;margin-top:3px}.insight-icon{width:34px;height:34px;border-radius:5px;display:grid;place-items:center}.insight-icon.green{background:#e5f5ef;color:#218b68}.insight-icon.amber{background:#fbf1dc;color:#a66c09}.insight-icon.blue{background:#e7f0fb;color:#3475b8}.cleanup-hero{background:#fff;color:#1d2939;border-radius:6px;padding:22px 24px;margin-bottom:14px;display:grid;grid-template-columns:1fr 1.4fr;align-items:center;gap:30px}.cleanup-hero>div:first-child>strong{display:block;font-size:30px;margin:5px 0}.cleanup-hero p{font-size:9px;color:#667085;margin:0}.cleanup-breakdown{display:grid;grid-template-columns:repeat(5,minmax(0,1fr));border-left:1px solid #e4e7eb;gap:0}.cleanup-breakdown div{padding:7px 20px;border-right:1px solid #3a4652}.cleanup-breakdown span,.cleanup-breakdown b{display:block}.cleanup-breakdown span{font-size:9px;color:#94a2b0}.cleanup-breakdown b{font-size:15px;margin-top:5px}.cleanup-list{padding:0;overflow:hidden}.cleanup-toolbar{min-height:69px;padding:14px 17px;border-bottom:1px solid #e4e7eb;display:flex;align-items:center;justify-content:space-between;gap:20px}.cleanup-toolbar h2{font-size:14px;margin:0}.cleanup-toolbar p{font-size:9px;color:#8a94a3;margin:3px 0 0}.cleanup-actions{display:flex;align-items:center;gap:10px}.text-button{border:0;background:transparent;color:#3475b8;font-size:10px}.cleanup-rows{display:grid}.cleanup-row{min-height:82px;padding:13px 17px;border-bottom:1px solid #edf0f2;display:grid;grid-template-columns:24px minmax(260px,1fr) 120px 96px;gap:13px;align-items:center}.check-button{width:20px;height:20px;border:1px solid #b8c0ca;border-radius:4px;background:#fff;color:#fff;display:grid;place-items:center;padding:0}.check-button.checked{background:#e8583e;border-color:#e8583e}.action-symbol{width:24px;height:24px;display:grid;place-items:center}.action-symbol.review{color:#b27b18}.action-symbol.system{color:#477ead}.cleanup-copy{min-width:0}.cleanup-copy>div{display:flex;align-items:center;gap:8px}.cleanup-copy b{font-size:11px}.cleanup-copy p{font-size:9px;color:#667085;margin:4px 0}.cleanup-copy small{display:block;font-size:8px;color:#98a2b3;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.risk-badge{font-size:8px;padding:2px 6px;border-radius:3px;background:#e5f5ef;color:#218b68}.risk-badge.review{background:#fbf1dc;color:#94620d}.risk-badge.system{background:#e7f0fb;color:#3475b8}.risk-badge.developer{background:#e0f2fe;color:#0369a1}.cleanup-row.dev-row{background:#f8fbfd}.cleanup-row.highlight{outline:2px solid var(--accent);outline-offset:-2px;background:#f0f7ff}.cleanup-size{text-align:right}.cleanup-size b,.cleanup-size span{display:block}.cleanup-size b{font-size:13px}.cleanup-size span{font-size:8px;color:#98a2b3;margin-top:3px}.row-space{width:96px}.cleanup-footnote{display:flex;align-items:center;gap:8px;padding:12px 17px;background:#f7faf9;color:#4d7f6d;font-size:9px}.loading-state{height:300px;display:flex;flex-direction:column;gap:10px;align-items:center;justify-content:center;color:#8a94a3;font-size:10px}.results-section{overflow:hidden}.result-toolbar{height:58px;padding:0 16px;border-bottom:1px solid #e4e7eb;display:flex;align-items:center;justify-content:space-between}.tabs{align-self:stretch;display:flex;gap:20px}.tabs button{border:0;border-bottom:2px solid transparent;background:transparent;padding:2px 0 0;color:#7d8896;font-size:10px;font-weight:650}.tabs button.active{color:#1d2939;border-color:#e8583e}.tabs span{background:#eef0f2;border-radius:10px;padding:1px 6px;margin-left:4px;font-size:8px}.search{width:210px;height:32px;border:1px solid #d7dce2;border-radius:5px;display:flex;align-items:center;gap:7px;padding:0 9px;color:#98a2b3}.search:focus-within{border-color:#9ca8b4}.search input{border:0;outline:0;min-width:0;width:100%;font-size:10px;color:#344054}.table-wrap{overflow:auto}table{border-collapse:collapse;width:100%;font-size:10px}th{height:36px;text-align:left;color:#8a94a3;font-size:9px;font-weight:650;background:#fafbfc;border-bottom:1px solid #e7eaee;padding:0 12px}td{height:52px;border-bottom:1px solid #eef0f2;padding:7px 12px;color:#475467}tbody tr:last-child td{border-bottom:0}tbody tr:hover{background:#fafbfc}.rank{width:42px;text-align:center;color:#98a2b3}.open-col{width:48px}.path-cell{display:flex;align-items:center;gap:10px;color:#3475b8;min-width:180px}.path-cell>div{min-width:0}.path-cell b,.path-cell span{display:block}.path-cell b{font-size:10px;color:#273443;max-width:310px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.path-cell span{font-size:8px;color:#98a2b3;margin-top:2px;max-width:310px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.path-cell.file{color:#9b6b15}.muted{color:#8a94a3}.path-text{max-width:380px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.share{min-width:145px}.share span{font-size:8px}.share i{display:block;height:4px;background:#edf0f2;border-radius:2px;margin-top:5px;overflow:hidden}.share em{display:block;height:100%;background:#4b8dcc;border-radius:2px}.share em.warm{background:#e7a82b}.share em.hot{background:#e8583e}.icon-button{width:30px;height:30px;border:0;background:transparent;color:#84909d;border-radius:4px;display:grid;place-items:center}.icon-button:hover{background:#eef1f3;color:#273443}.no-matches{padding:32px;text-align:center;color:#98a2b3;font-size:10px}.empty-state{min-height:360px;border:1px dashed #ccd2d9;border-radius:6px;background:#f8f9fa;display:flex;flex-direction:column;align-items:center;justify-content:center;text-align:center;padding:32px}.empty-visual{width:88px;height:88px;border-radius:50%;background:#e9edf0;color:#617080;display:grid;place-items:center;position:relative}.empty-visual span{position:absolute;right:4px;bottom:4px;width:33px;height:33px;border-radius:50%;background:#e8583e;color:#fff;border:3px solid #f8f9fa;display:grid;place-items:center}.empty-state h2{font-size:16px;margin:16px 0 6px}.empty-state p{font-size:10px;color:#7d8896;max-width:440px;line-height:1.7;margin:0 0 18px}.modal-backdrop{position:fixed;inset:0;background:#10182099;z-index:20;display:grid;place-items:center;padding:20px}.confirm-dialog{width:min(440px,100%);background:#fff;border-radius:7px;padding:24px;position:relative;box-shadow:0 18px 55px #0e1726aa}.dialog-close{position:absolute;right:14px;top:14px;border:0;background:transparent;color:#7d8896;display:grid;place-items:center}.dialog-icon{width:46px;height:46px;border-radius:6px;background:#fce9e5;color:#c94331;display:grid;place-items:center}.confirm-dialog h2{font-size:17px;margin:15px 0 7px}.confirm-dialog>p{font-size:10px;line-height:1.6;color:#667085;margin:0 0 14px}.confirm-items{border:1px solid #e4e7eb;border-radius:5px;padding:5px 11px}.confirm-items div{height:33px;display:grid;grid-template-columns:16px 1fr auto;align-items:center;gap:7px;border-bottom:1px solid #edf0f2;font-size:10px;color:#218b68}.confirm-items div:last-child{border:0}.confirm-items b{color:#344054}.dialog-actions{display:flex;justify-content:flex-end;gap:8px;margin-top:18px}.spin{animation:spin 1s linear infinite}@keyframes spin{to{transform:rotate(360deg)}}@media(max-width:1050px){.workspace{padding:22px 20px}.metrics{grid-template-columns:1fr 1fr}.metric:nth-child(2){border-right:0}.metric:nth-child(-n+2){border-bottom:1px solid #e7eaee}.content-grid{grid-template-columns:1fr}.cleanup-hero{grid-template-columns:1fr}.cleanup-breakdown{border-left:0;border-top:1px solid #3a4652}.cleanup-breakdown div:first-child{padding-left:0}.insight-grid{grid-template-columns:1fr}.cleanup-row{grid-template-columns:24px minmax(220px,1fr) 100px 90px}}@media(max-width:800px){body{min-width:0}.app-shell{display:block}.sidebar{position:static;width:100%;height:auto;padding:12px 14px;display:grid;grid-template-columns:1fr auto;align-items:center}.brand{padding:0}.main-nav,.sidebar-label,.drive-list,.sidebar-spacer,.safety-note,.version{display:none}.preview-badge{margin:0}.workspace{display:block;padding:18px 14px}.topbar{align-items:flex-start}.topbar h1{font-size:20px}.actions .button.secondary{width:38px;padding:0;font-size:0}.metrics{grid-template-columns:1fr 1fr}.metric{padding:14px;min-height:90px}.metric-icon{display:none}.distribution-body{gap:16px}.legend{grid-template-columns:1fr}.reclaim-band{align-items:flex-start;flex-wrap:wrap}.reclaim-band .button{width:100%}.cleanup-breakdown div{padding:7px 10px}.cleanup-toolbar{align-items:flex-start;flex-direction:column}.cleanup-actions{width:100%;justify-content:space-between}.cleanup-row{grid-template-columns:24px 1fr 88px}.cleanup-row>.button,.row-space{grid-column:2 / 4;justify-self:start}.result-toolbar{height:auto;padding:10px;gap:10px;align-items:stretch;flex-direction:column}.tabs{height:36px}.search{width:100%}}
:root{--accent:#2f79c5;--accent-hover:#276bad;--accent-soft:#e7f0fb;--accent-ink:#235f99;--sidebar:#172331;--sidebar-hover:#1e2e3d;--sidebar-active:#24384a}:root[data-accent="forest"]{--accent:#23866b;--accent-hover:#1d735b;--accent-soft:#e2f2ec;--accent-ink:#176a53;--sidebar:#172824;--sidebar-hover:#1e342e;--sidebar-active:#29443b}:root[data-accent="coral"]{--accent:#e8583e;--accent-hover:#d94c34;--accent-soft:#fce9e5;--accent-ink:#c94331;--sidebar:#18212b;--sidebar-hover:#202c38;--sidebar-active:#273543}:root[data-accent="cherry"]{--accent:#c94c5f;--accent-hover:#b84052;--accent-soft:#f8e5e8;--accent-ink:#a93648;--sidebar:#281b22;--sidebar-hover:#36252e;--sidebar-active:#462e39}:root[data-accent="graphite"]{--accent:#52606f;--accent-hover:#43505e;--accent-soft:#e8ebee;--accent-ink:#3d4955;--sidebar:#161b22;--sidebar-hover:#20262e;--sidebar-active:#2a323c}.sidebar{background:var(--sidebar)}.main-nav button:hover,.drive-button:hover{background:var(--sidebar-hover)}.main-nav button.active,.drive-button.active{background:var(--sidebar-active)}.brand-mark,.button.primary,.check-button.checked,.empty-visual span{background:var(--accent)}.button.primary:hover{background:var(--accent-hover)}.eyebrow,.scan-line b{color:var(--accent)}.drive-button i,.pulse-dot{background:var(--accent)}.pulse-dot{box-shadow:0 0 0 4px var(--accent-soft)}.scan-strip{border-left-color:var(--accent)}.progress-track div{background:var(--accent)}.metric-icon.coral{background:var(--accent-soft);color:var(--accent-ink)}.tabs button.active{border-color:var(--accent)}.theme-trigger{width:100%;height:43px;border:0;background:transparent;color:#93a0ae;border-radius:5px;padding:0 9px;margin-bottom:8px;display:grid;grid-template-columns:19px 1fr 16px;gap:9px;align-items:center;text-align:left}.theme-trigger:hover{background:var(--sidebar-hover);color:#fff}.theme-trigger b,.theme-trigger small{display:block}.theme-trigger b{font-size:10px}.theme-trigger small{font-size:8px;color:#748494;margin-top:2px}.theme-popover{position:fixed;left:244px;bottom:18px;width:304px;background:#fff;color:#273443;border:1px solid #d7dce2;border-radius:7px;padding:16px;box-shadow:0 16px 42px #10182044;z-index:30}.theme-head{display:flex;justify-content:space-between;align-items:flex-start}.theme-head h2{font-size:14px;margin:3px 0 12px}.theme-head button{border:0;background:transparent;color:#84909d;display:grid;place-items:center}.theme-options{display:grid;grid-template-columns:1fr 1fr;gap:7px}.theme-options button{height:52px;border:1px solid #e1e5e9;background:#fafbfc;border-radius:5px;padding:7px;display:grid;grid-template-columns:42px 1fr 14px;align-items:center;gap:7px;text-align:left;color:#475467}.theme-options button:hover{background:#f3f5f7}.theme-options button.active{border-color:var(--accent);box-shadow:0 0 0 1px var(--accent)}.theme-options button>i{width:42px;height:26px;display:flex;border-radius:4px;overflow:hidden}.theme-options button>i em{flex:1}.theme-options button>span{font-size:9px}.theme-options button>svg{color:var(--accent)}.theme-popover>p{font-size:8px;color:#98a2b3;margin:11px 0 0}.insight-grid{grid-template-columns:repeat(auto-fit,minmax(210px,1fr))}.insight-icon.neutral{background:var(--accent-soft);color:var(--accent-ink)}.sidebar-drive-title{display:flex;align-items:center;justify-content:space-between}.sidebar-drive-title button{border:0;background:transparent;color:#718096;padding:2px;display:grid;place-items:center}.sidebar-drive-title button:hover{color:#fff}.folder-scan-strip{border-left-color:var(--accent)}.folder-picker-band{display:flex;align-items:center;gap:12px;padding:15px 17px;margin-bottom:14px;background:#eef5fc;border:1px solid #c7dcef;border-radius:6px;color:#2f6fae}.folder-picker-band>span{width:38px;height:38px;border-radius:5px;background:#dceaf7;display:grid;place-items:center}.folder-picker-band>div{flex:1}.folder-picker-band b,.folder-picker-band small{display:block}.folder-picker-band b{font-size:11px;color:#274b6d}.folder-picker-band small{font-size:9px;color:#66839e;margin-top:3px}.folder-detail-head{display:grid;grid-template-columns:32px 42px 1fr auto;gap:11px;align-items:center;margin-bottom:12px}.back-button{background:#f2f4f6}.folder-detail-icon{width:42px;height:42px;border-radius:5px;background:var(--accent-soft);color:var(--accent-ink);display:grid;place-items:center}.folder-detail-head h2{font-size:15px;margin:2px 0}.folder-detail-head p{font-size:9px;color:#8a94a3;margin:0;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.folder-metrics{display:grid;grid-template-columns:repeat(4,1fr);background:#202c37;color:#fff;border-radius:6px;margin-bottom:12px}.folder-metrics div{padding:17px 20px;border-right:1px solid #3a4652}.folder-metrics div:last-child{border:0}.folder-metrics span,.folder-metrics b,.folder-metrics small{display:block}.folder-metrics span{font-size:9px;color:#94a2b0}.folder-metrics b{font-size:17px;margin:4px 0}.folder-metrics small{font-size:8px;color:#82919f}.folder-contents{padding:0;overflow:hidden}.folder-content-heading{padding:15px 17px;border-bottom:1px solid #e4e7eb;align-items:center}.folder-analysis-rows{display:grid}.folder-analysis-row{min-height:86px;padding:13px 17px;border-bottom:1px solid #edf0f2;display:grid;grid-template-columns:26px minmax(280px,1fr) 210px 112px;gap:12px;align-items:center}.folder-kind{color:var(--accent)}.folder-kind.file{color:#a66c09}.folder-item-copy{min-width:0}.folder-item-copy>div{display:flex;align-items:center;gap:7px}.folder-item-copy b{font-size:11px;max-width:330px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.folder-item-copy p{font-size:9px;color:#667085;margin:4px 0;line-height:1.35}.folder-item-copy small{font-size:8px;color:#98a2b3}.folder-risk{font-size:8px;padding:2px 6px;border-radius:3px}.folder-risk.rebuildable{background:#e5f5ef;color:#218b68}.folder-risk.review{background:#fbf1dc;color:#94620d}.folder-risk.protected{background:#fce9e5;color:#c94331}.folder-size-bar>div{display:flex;justify-content:space-between;align-items:center}.folder-size-bar b{font-size:11px}.folder-size-bar span{font-size:8px;color:#8a94a3}.folder-size-bar i{height:5px;background:#edf0f2;border-radius:3px;display:block;margin-top:6px;overflow:hidden}.folder-size-bar em{display:block;height:100%;background:#e7a82b}.folder-size-bar em.rebuildable{background:#38a47c}.folder-size-bar em.protected{background:#e8583e}.folder-row-actions,.table-actions{display:flex;align-items:center;justify-content:flex-end;gap:3px}.folder-note{border-top:0}.empty-actions{display:flex;gap:8px}.open-col{width:92px}.table-actions .icon-button:first-child{color:var(--accent)}@media(max-width:1050px){.folder-analysis-row{grid-template-columns:26px minmax(220px,1fr) 170px 100px}.folder-metrics{grid-template-columns:1fr 1fr}.folder-metrics div:nth-child(2){border-right:0}.folder-metrics div:nth-child(-n+2){border-bottom:1px solid #3a4652}}
@media(max-width:800px){.main-nav{display:flex;grid-column:1 / -1;gap:4px;margin:10px 0 0;overflow:auto}.main-nav button{display:grid;flex:1;min-width:110px;grid-template-columns:18px 1fr auto}.folder-detail-head{grid-template-columns:32px 38px 1fr}.folder-detail-head>.button{grid-column:3}.folder-analysis-row{grid-template-columns:24px 1fr 90px}.folder-size-bar{grid-column:2 / 4}.folder-row-actions{grid-column:2 / 4;justify-content:flex-start}.folder-picker-band{align-items:flex-start;flex-wrap:wrap}.folder-picker-band .button{width:100%}.empty-actions{flex-direction:column;width:100%}}
:root{--accent-gradient:var(--accent);--accent-contrast:#fff}:root[data-accent="mintrose"]{--accent:#4c9386;--accent-hover:#3f7e72;--accent-soft:#e2f7f1;--accent-ink:#2b6f64;--accent-gradient:linear-gradient(135deg,#A9F1DF,#FFBBBB);--accent-contrast:#173b37;--sidebar:#10211f;--sidebar-hover:#19302c;--sidebar-active:#24443d}:root[data-accent="lavenderteal"]{--accent:#278b80;--accent-hover:#20766d;--accent-soft:#e3f3f1;--accent-ink:#1d6a62;--accent-gradient:linear-gradient(135deg,#D8B5FF,#1EAE98);--accent-contrast:#102e2a;--sidebar:#151c2a;--sidebar-hover:#202a3b;--sidebar-active:#29364b}.brand-mark,.button.primary{background:var(--accent-gradient);color:var(--accent-contrast)}.collapse-button{position:absolute;right:5px;top:29px;width:28px;height:28px;border:0;background:transparent;color:#778796;border-radius:4px;display:grid;place-items:center}.collapse-button:hover{background:var(--sidebar-hover);color:#fff}.app-shell.collapsed{grid-template-columns:72px 1fr}.sidebar.collapsed{width:72px;padding-left:10px;padding-right:10px}.sidebar.collapsed .brand{justify-content:center;padding-left:0;padding-right:0}.sidebar.collapsed .brand>div,.sidebar.collapsed .main-nav span,.sidebar.collapsed .main-nav b,.sidebar.collapsed .sidebar-label span,.sidebar.collapsed .sidebar-drive-title button,.sidebar.collapsed .drive-button span,.sidebar.collapsed .drive-button i,.sidebar.collapsed .theme-trigger span,.sidebar.collapsed .theme-trigger>svg:last-child,.sidebar.collapsed .safety-note,.sidebar.collapsed .preview-badge,.sidebar.collapsed .version{display:none}.sidebar.collapsed .collapse-button{right:-12px;top:67px;background:#fff;color:#52606f;border:1px solid #d7dce2;z-index:3}.sidebar.collapsed .main-nav button,.sidebar.collapsed .drive-button,.sidebar.collapsed .theme-trigger{display:grid;grid-template-columns:1fr;place-items:center;padding:0}.sidebar.collapsed .sidebar-label{height:10px}.sidebar.collapsed .theme-popover{left:84px}.analysis-tabs{display:grid;grid-template-columns:repeat(6,minmax(0,1fr));gap:8px;padding:10px}.analysis-tabs button{height:58px;border:1px solid transparent;background:transparent;border-radius:5px;padding:0 13px;display:flex;align-items:center;gap:10px;text-align:left;color:#7d8896}.analysis-tabs button:hover{background:#f4f6f8}.analysis-tabs button.active{background:var(--accent-soft);border-color:var(--accent);color:var(--accent-ink)}.analysis-tabs b,.analysis-tabs small{display:block}.analysis-tabs b{font-size:11px}.analysis-tabs small{font-size:8px;margin-top:3px;color:#8793a0}.analysis-toolbar{display:flex;align-items:center;justify-content:space-between;gap:24px;margin-bottom:14px}.analysis-toolbar h2{font-size:14px;margin:3px 0}.analysis-toolbar p{font-size:9px;color:#7d8896;margin:0}.duplicate-controls{display:flex;align-items:center;gap:8px}.size-segments{display:flex;background:#edf0f2;border-radius:5px;padding:2px}.size-segments button{height:30px;border:0;background:transparent;border-radius:4px;padding:0 9px;color:#667085;font-size:9px}.size-segments button.active{background:#fff;color:var(--accent-ink);box-shadow:0 1px 3px #10182022}.analysis-metrics{display:grid;grid-template-columns:repeat(4,1fr);background:#202c37;color:#fff;border-radius:6px;margin-bottom:14px}.analysis-metrics>div{padding:17px 20px;border-right:1px solid #3a4652}.analysis-metrics>div:last-child{border:0}.analysis-metrics span,.analysis-metrics b,.analysis-metrics small{display:block}.analysis-metrics span{font-size:9px;color:#94a2b0}.analysis-metrics b{font-size:18px;margin:4px 0}.analysis-metrics small{font-size:8px;color:#82919f}.duplicate-results{padding:0;overflow:hidden}.duplicate-results>.panel-heading{padding:16px 18px;border-bottom:1px solid #e4e7eb}.duplicate-groups{display:grid}.duplicate-group{padding:14px 18px;border-bottom:1px solid #e9ecef}.duplicate-group-head{display:grid;grid-template-columns:24px 1fr auto;gap:9px;align-items:center}.duplicate-group-head>span{width:22px;height:22px;border-radius:4px;background:var(--accent-soft);color:var(--accent-ink);display:grid;place-items:center;font-size:9px;font-weight:700}.duplicate-group-head b,.duplicate-group-head small{display:block}.duplicate-group-head b{font-size:11px}.duplicate-group-head small{font-size:8px;color:#98a2b3;margin-top:2px}.duplicate-group-head strong{font-size:11px;color:#c94331}.duplicate-paths{margin:9px 0 0 33px;background:#f8f9fa;border:1px solid #eceff2;border-radius:5px;padding:4px 9px}.duplicate-paths>div{height:34px;display:grid;grid-template-columns:17px 1fr 30px;gap:7px;align-items:center;border-bottom:1px solid #eceff2;color:#7d8896}.duplicate-paths>div:last-child{border:0}.duplicate-paths span{font-size:9px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.analysis-empty{min-height:300px}.history-summary{padding:0;overflow:hidden}.history-summary>.panel-heading{padding:17px 20px}.history-delta{text-align:right}.history-delta span,.history-delta b{display:block}.history-delta span{font-size:8px;color:#98a2b3}.history-delta b{font-size:14px;color:#c94331;margin-top:3px}.history-delta.down b{color:#218b68}.trend-chart{height:245px;padding:18px 24px 8px;display:flex;align-items:flex-end;gap:10px;border-top:1px solid #edf0f2;border-bottom:1px solid #edf0f2;overflow-x:auto}.trend-column{height:100%;min-width:44px;flex:1;display:grid;grid-template-rows:18px 1fr 22px;text-align:center}.trend-value{font-size:8px;color:#667085}.trend-track{height:100%;background:#f0f2f4;border-radius:4px 4px 0 0;display:flex;align-items:flex-end;overflow:hidden}.trend-track i{width:100%;min-height:2px;background:var(--accent-gradient);border-radius:4px 4px 0 0}.trend-column>span{font-size:8px;color:#8a94a3;padding-top:6px}.history-list{display:grid}.history-list>div{min-height:45px;padding:7px 20px;display:grid;grid-template-columns:130px 110px 120px 1fr;gap:10px;align-items:center;border-bottom:1px solid #edf0f2}.history-list span,.history-list small,.history-list em{font-size:9px;color:#7d8896;font-style:normal}.history-list b{font-size:10px}.history-list em{text-align:right;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.age-overview{margin-bottom:14px}.age-stack{height:16px;display:flex;border-radius:4px;overflow:hidden;margin:20px 0}.age-stack i{height:100%;min-width:0}.age-grid{display:grid;grid-template-columns:repeat(5,1fr);gap:9px}.age-grid>div{min-height:112px;border:1px solid #e4e7eb;border-radius:5px;padding:11px;position:relative}.age-grid>div>span{width:8px;height:8px;border-radius:2px;display:block;margin-bottom:9px}.age-grid b,.age-grid strong,.age-grid small{display:block}.age-grid b{font-size:9px;color:#667085}.age-grid strong{font-size:15px;margin:6px 0}.age-grid small{font-size:8px;color:#98a2b3}.age-grid>div>i{display:block;height:4px;background:#edf0f2;border-radius:2px;margin-top:10px;overflow:hidden}.age-grid>div>i em{display:block;height:100%}.old-files{padding:0;overflow:hidden}.old-files>.panel-heading{padding:16px 18px;border-bottom:1px solid #e4e7eb}.old-file-rows{display:grid}.old-file-rows>div:not(.no-matches){height:58px;padding:7px 18px;display:grid;grid-template-columns:22px minmax(260px,1fr) 100px 70px 30px;gap:9px;align-items:center;border-bottom:1px solid #edf0f2;color:#a66c09}.old-file-rows>div>div{min-width:0}.old-file-rows b,.old-file-rows span{display:block}.old-file-rows b{font-size:10px;color:#273443}.old-file-rows span{font-size:8px;color:#98a2b3;margin-top:3px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.old-file-rows strong{font-size:10px;color:#344054}.old-file-rows em{font-size:9px;color:#c94331;font-style:normal}@media(max-width:1050px){.analysis-toolbar{align-items:flex-start;flex-direction:column}.duplicate-controls{width:100%;flex-wrap:wrap}.analysis-metrics{grid-template-columns:1fr 1fr}.analysis-metrics>div:nth-child(2){border-right:0}.analysis-metrics>div:nth-child(-n+2){border-bottom:1px solid #3a4652}.age-grid{grid-template-columns:repeat(3,1fr)}}@media(max-width:800px){.app-shell.collapsed{display:block}.sidebar.collapsed{width:100%}.collapse-button{display:none}.theme-popover,.sidebar.collapsed .theme-popover{left:14px;right:14px;bottom:14px;width:auto}.analysis-tabs{display:grid;grid-template-columns:repeat(6,minmax(0,1fr));gap:8px;padding:10px}.analysis-toolbar{gap:12px}.duplicate-controls .button{flex:1}.age-grid{grid-template-columns:1fr 1fr}.history-list>div{grid-template-columns:1fr 1fr}.history-list em{grid-column:1 / -1;text-align:left}.old-file-rows>div:not(.no-matches){height:auto;grid-template-columns:20px 1fr 70px;padding:10px}.old-file-rows em{grid-column:2}.old-file-rows button{grid-column:3;grid-row:1}.duplicate-paths{margin-left:0}}
/* 5.1 appearance system: bright sidebar, stable collapse control, and persistent UI sizing. */
:root{--accent:#3182f6;--accent-hover:#1769d8;--accent-soft:#eaf4ff;--accent-ink:#175fad;--accent-gradient:linear-gradient(135deg,#3182f6,#70d6ff);--accent-contrast:#fff;--sidebar:#f2f8ff;--sidebar-hover:#fff;--sidebar-active:#fff;--sidebar-border:#d9e8f7;--sidebar-text:#344054;--sidebar-muted:#667085;--ui-body-font:12px;--ui-small-font:10px;--ui-title-font:15px;--ui-h1-font:26px;--ui-icon-scale:1.08}
:root[data-accent="forest"]{--accent:#12a47b;--accent-hover:#0d8664;--accent-soft:#e8fbf4;--accent-ink:#087355;--accent-gradient:linear-gradient(135deg,#12a47b,#8ee3c3);--accent-contrast:#fff;--sidebar:#effbf6;--sidebar-hover:#fff;--sidebar-active:#fff;--sidebar-border:#d1efe3}
:root[data-accent="coral"]{--accent:#ff6846;--accent-hover:#e84f2d;--accent-soft:#fff0eb;--accent-ink:#c53c20;--accent-gradient:linear-gradient(135deg,#ff6846,#ffb347);--accent-contrast:#fff;--sidebar:#fff6f1;--sidebar-hover:#fff;--sidebar-active:#fff;--sidebar-border:#f5ddd1}
:root[data-accent="cherry"]{--accent:#e94b72;--accent-hover:#ce3159;--accent-soft:#fff0f5;--accent-ink:#b52a4d;--accent-gradient:linear-gradient(135deg,#e94b72,#ff8fab);--accent-contrast:#fff;--sidebar:#fff5f8;--sidebar-hover:#fff;--sidebar-active:#fff;--sidebar-border:#f3dbe3}
:root[data-accent="graphite"]{--accent:#667085;--accent-hover:#50596b;--accent-soft:#f0f2f5;--accent-ink:#465063;--accent-gradient:linear-gradient(135deg,#667085,#b8c4ce);--accent-contrast:#fff;--sidebar:#f7f8fa;--sidebar-hover:#fff;--sidebar-active:#fff;--sidebar-border:#e1e5ea}
:root[data-accent="mintrose"]{--accent:#3b9c8a;--accent-hover:#2f8172;--accent-soft:#e9faf5;--accent-ink:#236f62;--accent-gradient:linear-gradient(135deg,#A9F1DF,#FFBBBB);--accent-contrast:#173b37;--sidebar:#f1fcf8;--sidebar-hover:#fff;--sidebar-active:#fff;--sidebar-border:#d6f0e8}
:root[data-accent="lavenderteal"]{--accent:#278b80;--accent-hover:#20766d;--accent-soft:#f0e9ff;--accent-ink:#5d3e8c;--accent-gradient:linear-gradient(135deg,#D8B5FF,#1EAE98);--accent-contrast:#15352f;--sidebar:#f7f3ff;--sidebar-hover:#fff;--sidebar-active:#fff;--sidebar-border:#e7dcf8}
:root[data-font-size="small"]{--ui-body-font:11px;--ui-small-font:9px;--ui-title-font:14px;--ui-h1-font:23px}
:root[data-font-size="large"]{--ui-body-font:14px;--ui-small-font:12px;--ui-title-font:17px;--ui-h1-font:29px}
:root[data-icon-size="compact"]{--ui-icon-scale:.94}
:root[data-icon-size="large"]{--ui-icon-scale:1.22}

.app-shell svg{transform:scale(var(--ui-icon-scale));transform-origin:center;transition:transform .16s ease}
.app-shell button,.app-shell input,.app-shell table,.app-shell p,.app-shell li{font-size:var(--ui-body-font)!important}
.app-shell small,.app-shell .panel-kicker,.app-shell .sidebar-label,.app-shell .version,.app-shell .risk-badge,.app-shell .current-path{font-size:var(--ui-small-font)!important}
.app-shell h1{font-size:var(--ui-h1-font)!important}.app-shell h2,.app-shell h3,.panel-heading h2,.cleanup-toolbar h2{font-size:var(--ui-title-font)!important}

.analysis-toolbar>div:first-child{min-width:0;flex:1}.duplicate-controls{flex:none;flex-wrap:wrap;justify-content:flex-end}.size-segments{flex:none}.size-segments button{height:auto;min-height:34px;white-space:nowrap}:root[data-font-size="large"] .size-segments button{min-height:38px;padding-left:10px;padding-right:10px}

.sidebar{background:var(--sidebar);color:var(--sidebar-text);border-right-color:var(--sidebar-border);box-shadow:3px 0 18px #3440540d;padding-top:16px}
.sidebar-head{display:grid;grid-template-columns:minmax(0,1fr) 30px;gap:6px;align-items:start;margin-bottom:18px}.brand{padding:0 4px;min-width:0}.brand strong{color:#1d2939}.brand small{color:var(--sidebar-muted)}
.brand-mark{box-shadow:0 7px 16px color-mix(in srgb,var(--accent) 28%,transparent)}
.collapse-button{position:static!important;right:auto!important;top:auto!important;width:30px;height:30px;border:1px solid var(--sidebar-border)!important;background:#fff!important;color:var(--accent-ink)!important;border-radius:5px;display:grid;place-items:center;box-shadow:0 2px 8px #34405412}.collapse-button:hover{border-color:var(--accent)!important;background:var(--accent-soft)!important;color:var(--accent-ink)!important}
.main-nav button,.drive-button{color:var(--sidebar-muted)}.main-nav button:hover,.drive-button:hover{background:var(--sidebar-hover);color:var(--sidebar-text);box-shadow:0 2px 9px #3440540d}.main-nav button.active,.drive-button.active{background:var(--sidebar-active);color:var(--accent-ink);box-shadow:inset 3px 0 0 var(--accent),0 3px 12px #34405414}.main-nav button b{background:#fff0f3;color:#d92d5b}.main-nav button:nth-child(1)>svg{color:#3182f6}.main-nav button:nth-child(2)>svg{color:#ff5d5d}.main-nav button:nth-child(3)>svg{color:#f59e0b}.main-nav button:nth-child(4)>svg{color:#8b5cf6}
.main-nav button:nth-child(5)>svg{color:#12a47b}.sidebar{overflow-y:auto}
.main-nav button:nth-child(6)>svg{color:#3182f6}
.sidebar-label,.sidebar-drive-title button{color:var(--sidebar-muted)}.sidebar-drive-title button:hover{color:var(--accent)}.drive-button span small,.drive-loading{color:#7d8896}.drive-button>svg{color:#12a47b}
.safety-note{color:#087355;background:#e8fbf4;border-color:#bdebdc}.safety-note span{color:#397b69}.version{color:#98a2b3}.preview-badge{color:#a15c00}
.settings-trigger{width:100%;min-height:48px;border:0;background:transparent;color:var(--sidebar-muted);border-radius:5px;padding:7px 9px;margin-bottom:8px;display:grid;grid-template-columns:19px 1fr 16px;gap:9px;align-items:center;text-align:left}.settings-trigger:hover{background:#fff;color:var(--sidebar-text);box-shadow:0 2px 9px #3440540d}.settings-trigger>svg:first-child{color:#f97316}.settings-trigger b,.settings-trigger small{display:block}.settings-trigger b{color:#344054}.settings-trigger small{color:#7d8896;margin-top:2px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}

.sidebar.collapsed .sidebar-head{display:flex;flex-direction:column;align-items:center;gap:11px;margin-bottom:17px}.sidebar.collapsed .brand{padding:0}.sidebar.collapsed .brand>div,.sidebar.collapsed .main-nav span,.sidebar.collapsed .main-nav b,.sidebar.collapsed .sidebar-label span,.sidebar.collapsed .sidebar-drive-title button,.sidebar.collapsed .drive-button span,.sidebar.collapsed .drive-button i,.sidebar.collapsed .settings-trigger span,.sidebar.collapsed .settings-trigger>svg:last-child,.sidebar.collapsed .safety-note,.sidebar.collapsed .preview-badge,.sidebar.collapsed .version{display:none}.sidebar.collapsed .collapse-button{position:static!important;width:34px;height:30px;z-index:auto}.sidebar.collapsed .main-nav button,.sidebar.collapsed .drive-button,.sidebar.collapsed .settings-trigger{display:grid;grid-template-columns:1fr;place-items:center;padding:0}.sidebar.collapsed .settings-trigger{height:42px;min-height:42px}

.settings-backdrop{position:fixed;inset:0;background:#10182852;z-index:40;display:flex;justify-content:flex-end;backdrop-filter:blur(2px)}.settings-drawer{width:min(460px,100%);height:100%;overflow:auto;background:#fff;color:#273443;border-left:1px solid #dfe3e8;box-shadow:-18px 0 50px #1018282b;padding:24px}.settings-head{display:flex;align-items:flex-start;justify-content:space-between}.settings-head h2{margin:4px 0 0}.settings-tabs{display:flex;flex-wrap:wrap;gap:8px;background:transparent;border-radius:0;padding:0;margin:18px 0 16px}.settings-tabs button{height:38px;border:0;border-radius:4px;background:transparent;color:#667085;display:flex;align-items:center;justify-content:center;gap:8px;font-weight:650}.settings-tabs button.active{background:#fff;color:var(--accent-ink);box-shadow:0 2px 8px #10182814}.settings-content{display:grid;gap:0}.setting-section{padding:18px 0;border-bottom:1px solid #eaecf0}.setting-section:first-child{padding-top:4px}.setting-title{display:flex;justify-content:space-between;align-items:center;gap:14px;margin-bottom:13px}.setting-title b,.setting-title small{display:block}.setting-title b{font-size:var(--ui-body-font)}.setting-title small{margin-top:4px;color:#7d8896}.setting-title>span{color:var(--accent-ink);font-size:var(--ui-small-font);white-space:nowrap}.settings-theme-options{grid-template-columns:1fr 1fr}.settings-theme-options button{height:56px}.preference-segments{display:flex;flex-wrap:wrap;gap:8px;background:transparent;border-radius:0;padding:0}.preference-segments button{height:36px;border:0;border-radius:4px;background:transparent;color:#667085;font-weight:600}.preference-segments button.active{background:#fff;color:var(--accent-ink);box-shadow:0 2px 7px #10182814}.icon-size-preview{width:36px;height:36px;border-radius:5px;background:var(--accent-soft);display:grid;place-items:center;color:var(--accent-ink)}.settings-footnote{display:flex;align-items:center;gap:8px;color:#4d7f6d;margin:16px 0 0!important}
.about-product{display:flex;align-items:center;gap:15px;padding:6px 0 20px;border-bottom:1px solid #eaecf0}.about-mark{width:58px;height:58px;flex:none;border-radius:8px;background:var(--accent-gradient);color:var(--accent-contrast);display:grid;place-items:center;box-shadow:0 8px 20px color-mix(in srgb,var(--accent) 25%,transparent)}.about-product h3{margin:0 0 4px}.about-product p{margin:0 0 8px!important;color:#667085}.about-product b{font-size:var(--ui-small-font);color:var(--accent-ink);background:var(--accent-soft);padding:3px 7px;border-radius:3px}.about-section{padding:19px 0;border-bottom:1px solid #eaecf0}.about-section h4{margin:0 0 10px;font-size:var(--ui-body-font)}.about-section dl{margin:0}.about-section dl>div{min-height:34px;display:flex;justify-content:space-between;gap:16px;align-items:center}.about-section dt{color:#667085}.about-section dd{margin:0;text-align:right;color:#273443;font-weight:600}.about-safety{display:flex;align-items:flex-start;gap:11px;margin-top:20px;padding:14px;background:#effaf6;border:1px solid #ccecdf;border-radius:6px;color:#087355}.about-safety p{margin:4px 0 0!important;line-height:1.55;color:#397b69}

.settings-drawer{width:min(540px,100%)}.settings-tabs{display:flex;flex-wrap:wrap;gap:8px}.exclusion-list{display:grid;border:1px solid #e4e7eb;border-radius:5px;overflow:hidden}.exclusion-list>div{min-height:38px;padding:6px 9px;display:grid;grid-template-columns:17px minmax(0,1fr) 28px;gap:8px;align-items:center;border-bottom:1px solid #edf0f2;color:#667085}.exclusion-list>div:last-child{border:0}.exclusion-list span{white-space:nowrap;overflow:hidden;text-overflow:ellipsis;color:#344054}.exclusion-list button,.setting-path button{border:0;background:transparent;color:#98a2b3;display:grid;place-items:center}.setting-empty{margin:0!important;padding:16px;border:1px dashed #d0d5dd;border-radius:5px;text-align:center;color:#98a2b3}.setting-path{min-height:40px;padding:7px 10px;border:1px solid #d7dce2;border-radius:5px;display:grid;grid-template-columns:minmax(0,1fr) 28px;align-items:center}.setting-path span{white-space:nowrap;overflow:hidden;text-overflow:ellipsis;color:#475467}.toggle-switch{width:42px;height:24px;border:0;border-radius:12px;background:#d0d5dd;padding:3px;transition:background .16s}.toggle-switch i{display:block;width:18px;height:18px;border-radius:50%;background:#fff;box-shadow:0 1px 3px #10182833;transition:transform .16s}.toggle-switch.active{background:var(--accent)}.toggle-switch.active i{transform:translateX(18px)}.setting-action-row{display:flex;align-items:center;justify-content:space-between;gap:12px}.setting-action-row>span{color:#7d8896}.setting-action-row>span.success{color:#218b68}.setting-action-row>span.update{color:#c94331;font-weight:650}.system-actions>div{min-height:65px;display:flex;align-items:center;justify-content:space-between;gap:15px;border-bottom:1px solid #edf0f2}.system-actions>div:last-child{border:0}.system-actions b,.system-actions small{display:block}.system-actions small{color:#7d8896;margin-top:4px}.dialog-icon.history{background:#fff0f0;color:#c94331}

:root[data-density="compact"] .main-nav button{height:36px}:root[data-density="compact"] .drive-button{padding-top:7px;padding-bottom:7px}:root[data-density="compact"] td{height:46px}:root[data-density="compact"] .result-toolbar{height:52px}:root[data-density="compact"] .cleanup-row{min-height:72px}:root[data-density="compact"] .folder-analysis-row{min-height:74px}:root[data-density="compact"] .old-file-rows>div:not(.no-matches){height:52px}
:root[data-density="comfortable"] .main-nav button{height:44px}:root[data-density="comfortable"] .drive-button{padding-top:11px;padding-bottom:11px}:root[data-density="comfortable"] td{height:58px}:root[data-density="comfortable"] .result-toolbar{height:64px}:root[data-density="comfortable"] .cleanup-row{min-height:92px}:root[data-density="comfortable"] .folder-analysis-row{min-height:96px}:root[data-density="comfortable"] .old-file-rows>div:not(.no-matches){height:64px}

@media(max-width:800px){.sidebar-head{grid-column:1;grid-row:1;margin:0}.sidebar-head .brand{padding:0}.collapse-button{display:none!important}.sidebar.collapsed .sidebar-head{display:block;margin:0}.sidebar.collapsed .brand>div,.sidebar.collapsed .main-nav span,.sidebar.collapsed .main-nav b{display:block}.sidebar.collapsed .main-nav button{display:grid;grid-template-columns:18px 1fr auto;padding:0 9px}.settings-trigger{grid-column:2;grid-row:1;width:42px;height:38px;min-height:38px;margin:0;padding:0;grid-template-columns:1fr;place-items:center}.settings-trigger span,.settings-trigger>svg:last-child,.sidebar.collapsed .settings-trigger span,.sidebar.collapsed .settings-trigger>svg:last-child{display:none}.settings-drawer{width:min(430px,100%);padding:19px}.settings-theme-options{grid-template-columns:1fr 1fr}}
.cleanup-groups{display:grid;gap:18px;padding:12px 14px 18px}.cleanup-group-title{display:flex;align-items:baseline;justify-content:space-between;gap:12px;padding:2px 2px 10px;border-bottom:1px solid #edf0f2;margin-bottom:6px}.cleanup-group-title b{font-size:13px;color:#273443}.cleanup-group-title small{color:#98a2b3;font-size:11px}.cleanup-actions{display:flex;flex-wrap:wrap;gap:8px;align-items:center}

.file-filter-bar{display:flex;flex-wrap:wrap;gap:10px 14px;align-items:center;padding:4px 2px 12px;margin:0 0 8px;background:transparent;border:0;border-radius:0}
.filter-group{display:flex;align-items:center;gap:6px;flex-wrap:wrap}
.filter-group>span{font-size:11px;color:#98a2b3;font-weight:650}
.filter-group button{height:30px;border:1px solid #d7dce2;border-radius:4px;background:#fff;color:#667085;padding:0 10px;font-size:12px;font-weight:600}
.filter-group button.active{border-color:var(--accent);background:var(--accent-soft);color:var(--accent-ink)}
.file-bulk-actions{display:flex;align-items:center;gap:8px;flex-wrap:wrap;margin-left:auto}
.check-col{width:36px}
.table-wrap tr.selected{background:#f3fbf8}
.duplicate-paths>div{display:grid;grid-template-columns:22px 16px minmax(0,1fr) auto auto;gap:8px;align-items:center;padding:4px 6px}
.duplicate-paths>div.selected{background:#f3fbf8;border-radius:4px}
.duplicate-paths .keep-tag{font-size:10px;color:#0f8f6b;background:#e8fbf4;padding:2px 6px;border-radius:999px;white-space:nowrap}
.age-bucket-card{border:1px solid #e4e7eb;border-radius:8px;background:#fff;padding:12px;text-align:left;cursor:pointer;display:grid;gap:4px;width:100%}
.age-bucket-card:hover,.age-bucket-card.active{border-color:var(--accent);box-shadow:0 4px 14px #10182812}
.age-bucket-card.active{background:var(--accent-soft)}
.age-bucket-card>span{width:10px;height:10px;border-radius:50%}
.button.compact{height:34px;padding:0 12px;font-size:12px}

.alert{transition:opacity .4s ease, transform .4s ease; opacity:1; transform:translateY(0)}
.alert.fading{opacity:0; transform:translateY(-6px); pointer-events:none}
.alert .log-link{margin-left:auto; color:inherit; opacity:.85; font-size:12px}
.message-log-dialog{width:min(520px,100%); text-align:left}
.recycle-bin-dialog,.cleanup-history-dialog{width:min(640px,100%); text-align:left}
.recycle-bin-list{display:grid;gap:10px;max-height:460px;overflow:auto;margin:4px 0 14px}
.recycle-bin-group{border:1px solid #e4e7eb;border-radius:8px;background:#fafbfc;padding:10px 12px}
.recycle-bin-head{display:flex;align-items:center;gap:10px;flex-wrap:wrap}
.recycle-bin-title{display:grid;gap:2px;min-width:200px;flex:1}
.recycle-bin-title b{font-size:13px;color:#101828}
.recycle-bin-title small{font-size:11px;color:#667085}
.recycle-bin-title .source-tag{display:inline-block;width:max-content;font-size:10px;color:#235f99;background:#e7f0fb;border-radius:10px;padding:1px 8px}
.recycle-bin-summary{display:grid;gap:2px;text-align:right;min-width:80px}
.recycle-bin-summary b{font-size:13px;color:#c94331}
.recycle-bin-summary small{font-size:10px;color:#667085}
.recycle-bin-detail{margin-top:8px;border-top:1px dashed #d7dce2;padding-top:6px}
.recycle-bin-detail summary{cursor:pointer;font-size:11px;color:#235f99;user-select:none}
.recycle-bin-detail ul{list-style:none;margin:6px 0 0;padding:0;max-height:180px;overflow:auto;display:grid;gap:2px}
.recycle-bin-detail li{font-size:11px;color:#475467;font-family:Consolas,"Courier New",monospace;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.recycle-bin-detail li em{font-style:normal;color:#98a2b3}
.recycle-actions-row{display:flex;gap:8px;flex-wrap:wrap;margin:2px 0 10px}
.recycle-bin-subtitle{display:flex;align-items:center;gap:6px;font-size:12px;color:#344054;font-weight:600;margin:2px 0 8px}
.cleanup-progress{margin:2px 0 12px;display:grid;gap:6px}
.cleanup-progress-info{display:flex;align-items:center;gap:6px;font-size:12px;color:#344054}
.cleanup-progress-info b{color:#c94331}
.cleanup-progress-bar{height:6px;border-radius:4px;background:#e4e7eb;overflow:hidden}
.cleanup-progress-bar i{display:block;height:100%;border-radius:4px;background:linear-gradient(90deg,#3b82f6,#2dd4bf);transition:width .3s ease}
.message-log-list{max-height:320px; overflow:auto; display:grid; gap:8px; margin:12px 0 16px}
.message-log-row{border:1px solid #e4e7eb; border-radius:6px; padding:10px 12px; background:#fafbfc}
.message-log-row.error{border-color:#ffcaca; background:#fff7f7}
.message-log-row b{margin-right:8px}
.message-log-row span{color:#98a2b3; font-size:11px}
.message-log-row p{margin:6px 0 0; color:#344054; line-height:1.45}

.type-browse{margin-bottom:12px;padding:14px}
.type-cards{display:grid;grid-template-columns:repeat(auto-fill,minmax(140px,1fr));gap:10px;margin:10px 0}
.type-card{border:1px solid #e4e7eb;border-radius:8px;background:#fff;padding:12px;text-align:left;cursor:pointer;display:grid;gap:4px;position:relative}
.type-card:hover,.type-card.active{border-color:var(--accent);background:var(--accent-soft)}
.type-card strong{font-size:16px}
.type-card small{color:#98a2b3}
.type-jump{position:absolute;right:8px;top:8px;font-size:10px;color:var(--accent-ink);background:#fff;border:1px solid #d7dce2;border-radius:999px;padding:2px 6px}
.type-actions{margin:8px 0 12px}
.snapshot-diff{margin-top:16px;padding-top:14px;border-top:1px solid #edf0f2}
.snapshot-diff .panel-heading.compact{margin-bottom:10px}
.diff-grid{display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:12px}
.diff-grid>div{border:1px solid #eef0f3;border-radius:8px;padding:10px;background:#fafbfc}
.diff-grid b{display:block;margin-bottom:8px;font-size:12px;color:#475467}
.diff-row{display:grid;grid-template-columns:minmax(0,1fr) auto auto auto;gap:6px;align-items:center;padding:6px 0;border-bottom:1px solid #f0f2f5;font-size:12px}
.diff-row:last-child{border:0}
.diff-row span{overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.diff-row.grow em{color:#c94331}
.diff-row.shrink em{color:#0f8f6b}
.duplicate-group-foot{grid-column:1/-1;display:flex;gap:10px;padding:4px 0 2px}
@media(max-width:980px){.diff-grid{grid-template-columns:1fr}}

.attribution-panel{margin-top:14px}
.attr-region-bars{display:grid;gap:8px}
.attr-region-row{display:grid;grid-template-columns:10px minmax(0,1fr) auto;gap:10px;align-items:center;width:100%;border:1px solid #eef0f3;background:#fff;border-radius:8px;padding:10px 12px;text-align:left;cursor:pointer}
.attr-region-row.static{cursor:default;border:0;padding:0;background:transparent}
.attr-region-row:hover{border-color:var(--accent);background:var(--accent-soft)}
.attr-region-row>i{width:10px;height:10px;border-radius:50%}
.attr-region-copy b,.attr-region-copy small{display:block}
.attr-region-copy small{color:#98a2b3;margin-top:2px}
.attr-bar{grid-column:1/-1;display:block;height:6px;background:#edf0f2;border-radius:99px;overflow:hidden}
.attr-bar.tall{height:8px;margin:10px 0}
.attr-bar em{display:block;height:100%}
.attr-project-mini{margin-top:14px;padding-top:12px;border-top:1px solid #edf0f2}
.attr-mini-head{display:flex;justify-content:space-between;gap:10px;margin-bottom:8px}
.attr-mini-head small{color:#98a2b3}
.attr-project-chips{display:flex;flex-wrap:wrap;gap:8px}
.attr-chip{border:1px solid #e4e7eb;background:#fff;border-radius:999px;padding:6px 10px;display:inline-flex;gap:8px;align-items:center}
.attr-chip em{color:var(--accent-ink);font-style:normal;font-weight:700}
.attribution-detail{display:grid;gap:12px}
.attr-region-card{border:1px solid #eef0f3;border-radius:10px;padding:14px;background:#fff}
.attr-actions{display:flex;flex-wrap:wrap;gap:8px;margin-top:10px}
.attr-samples{display:block;margin-top:8px;color:#98a2b3;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.attr-project-table{display:grid;gap:8px}
.attr-project-row{display:grid;grid-template-columns:minmax(0,1fr) auto auto;gap:12px;align-items:center;padding:12px;border:1px solid #eef0f3;border-radius:8px;background:#fff}
.attr-project-row b,.attr-project-row small{display:block}
.attr-project-row small{color:#98a2b3;margin-top:3px}
.attr-tags{display:flex;flex-wrap:wrap;gap:6px;margin-top:6px}
.attr-tags span{font-size:11px;background:#f2f4f7;color:#667085;padding:2px 7px;border-radius:999px}
@media(max-width:900px){.attr-project-row{grid-template-columns:1fr}}

.analysis-tabs{display:grid;grid-template-columns:repeat(6,minmax(0,1fr));gap:8px;padding:10px !important}
.analysis-tabs>button{min-width:0;padding:10px 8px;display:grid;grid-template-columns:18px 1fr;gap:8px;align-items:center;text-align:left}
.analysis-tabs>button b{display:block;font-size:12px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.analysis-tabs>button small{display:block;font-size:10px;color:#98a2b3;margin-top:2px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.action-checklist .action-list{display:grid;gap:8px}
.action-item{display:grid;grid-template-columns:36px minmax(0,1fr) 18px;gap:12px;align-items:center;width:100%;border:1px solid #eef0f3;background:#fff;border-radius:8px;padding:12px 14px;text-align:left}
.action-item:hover{border-color:var(--accent);background:var(--accent-soft)}
.action-item b,.action-item small{display:block}
.action-item small{margin-top:3px;color:#98a2b3}
.action-priority{width:36px;height:36px;border-radius:8px;display:grid;place-items:center;font-size:12px;font-weight:700}
.action-item.high .action-priority{background:#fff0f0;color:#c94331}
.action-item.medium .action-priority{background:#fff5e6;color:#c27803}
.action-item.low .action-priority{background:#e8fbf4;color:#0f8f6b}
@media(max-width:1100px){.analysis-tabs{grid-template-columns:repeat(3,minmax(0,1fr))}}
@media(max-width:800px){.analysis-tabs{grid-template-columns:repeat(2,minmax(0,1fr))}}

.settings-tabs{display:flex!important;flex-wrap:wrap!important;grid-template-columns:none!important;background:transparent!important}
.activity-filter{grid-auto-flow:column;grid-auto-columns:1fr;margin-bottom:12px}
.activity-filter.modal-filter{margin:10px 0 12px}
.activity-log-list{display:grid;gap:8px;max-height:360px;overflow:auto}
.activity-log-row{border:1px solid #eef0f2;border-radius:8px;padding:10px 12px;background:#fafbfc}
.activity-log-row.error{border-color:#ffcaca;background:#fff7f7}
.activity-log-row.recycle,.activity-log-row.cleanup{border-color:#ccecdf;background:#f3fbf8}
.activity-log-row.registry{border-color:#ddd6fe;background:#f5f3ff}
.activity-log-head{display:flex;justify-content:space-between;gap:8px;align-items:center}
.activity-log-head span{color:#98a2b3;font-size:11px}
.activity-log-row p{margin:6px 0 0;color:#344054;line-height:1.45}
.activity-log-row small{display:block;margin-top:4px;color:#667085}
.message-log-row small{display:block;margin-top:4px;color:#667085}

.status-chip{position:relative;width:36px;height:36px;border:1px solid #d7dce2;border-radius:8px;background:#fff}
.status-chip.active{border-color:var(--accent);background:var(--accent-soft);color:var(--accent-ink)}
.status-chip em{position:absolute;top:-4px;right:-4px;min-width:16px;height:16px;padding:0 4px;border-radius:999px;background:#c94331;color:#fff;font-size:10px;font-style:normal;display:grid;place-items:center}
.status-scrim{position:fixed;inset:0;z-index:25;background:transparent}
.status-dropdown{position:absolute;top:72px;right:24px;z-index:30;width:min(380px,calc(100vw - 48px));background:#fff;border:1px solid #e4e7eb;border-radius:12px;box-shadow:0 18px 50px #1018282e;overflow:hidden}
.status-dropdown header{display:flex;justify-content:space-between;align-items:center;gap:10px;padding:12px 14px;border-bottom:1px solid #edf0f2}
.status-dropdown header b,.status-dropdown header small{display:block}
.status-dropdown header small{color:#98a2b3;margin-top:2px;font-size:11px}
.status-list{max-height:360px;overflow:auto;padding:8px}
.status-item{width:100%;border:0;background:#fafbfc;border:1px solid #eef0f2;border-radius:8px;padding:10px;text-align:left;margin-bottom:8px;cursor:pointer}
.status-item:hover{border-color:var(--accent);background:var(--accent-soft)}
.status-item div{display:flex;justify-content:space-between;gap:8px}
.status-item span{color:#98a2b3;font-size:11px}
.status-item p{margin:6px 0 0;color:#344054;line-height:1.4;display:-webkit-box;-webkit-line-clamp:2;-webkit-box-orient:vertical;overflow:hidden}
.status-dropdown footer{padding:10px 14px;border-top:1px solid #edf0f2}
.quick-grid{padding:12px 14px 14px;display:grid;gap:12px}
.quick-row{display:flex;align-items:center;justify-content:space-between;gap:12px}
.quick-row>span{color:#475467;font-size:13px}
.preference-segments.mini{max-width:180px}
.quick-actions{display:grid;grid-template-columns:1fr 1fr;gap:8px}
.alert-text{flex:1;cursor:pointer}
.alert-text:hover{text-decoration:underline}
.activity-log-row.clickable,.message-log-row.clickable{cursor:pointer;width:100%;text-align:left}
.activity-log-row.clickable:hover,.message-log-row.clickable:hover{border-color:var(--accent)}
.activity-detail{text-align:left;margin:12px 0 16px;display:grid;gap:10px}
.activity-detail-row,.activity-detail-block{border:1px solid #eef0f2;border-radius:8px;padding:10px 12px;background:#fafbfc}
.activity-detail-row{display:flex;justify-content:space-between;gap:12px}
.activity-detail-row span,.activity-detail-block>span{color:#98a2b3;font-size:12px}
.activity-detail-block p{margin:6px 0 0;color:#344054;line-height:1.5;word-break:break-word}
.activity-meta{margin:8px 0 0;display:grid;gap:8px}
.activity-meta>div{display:grid;gap:2px}
.activity-meta dt{color:#98a2b3;font-size:11px}
.activity-meta dd{margin:0;word-break:break-all}
.workspace{position:relative}

.pull-edge{position:absolute;left:0;right:0;top:0;height:22px;z-index:22;cursor:default}
.status-scrim{position:fixed;inset:0;z-index:25;background:transparent}
.status-scrim.dim{background:#10182833;backdrop-filter:blur(1px)}
.status-shade{position:fixed;left:12px;top:0;transform:translateY(-105%);width:min(440px,calc(100vw - 24px));max-height:min(72vh,640px);z-index:30;background:#fff;border:1px solid #e4e7eb;border-top:0;border-radius:0 0 16px 16px;box-shadow:0 20px 50px #10182833;display:flex;flex-direction:column;overflow:hidden;transition:transform .22s ease}
.status-shade.dragging{transition:none}
.status-shade.open{/* transform controlled inline */}
.shade-handle{width:44px;height:5px;border-radius:99px;background:#d0d5dd;margin:10px auto 6px;flex:none}
.shade-tabs{display:grid;grid-template-columns:1fr 1fr;gap:6px;padding:0 12px 8px}
.shade-tabs button{height:34px;border:1px solid #d7dce2;border-radius:999px;background:color-mix(in srgb,#fff 58%,transparent);color:#667085;font-weight:650}
.shade-tabs button.active{background:var(--accent-soft);color:var(--accent-ink)}
.shade-head{display:flex;justify-content:space-between;align-items:center;gap:10px;padding:4px 14px 10px;border-bottom:1px solid #edf0f2}
.shade-head b,.shade-head small{display:block}
.shade-head small{color:#98a2b3;margin-top:2px;font-size:11px}
.shade-foot{padding:10px 14px;border-top:1px solid #edf0f2}
.status-chip em{position:absolute;top:-4px;right:-4px;min-width:16px;height:16px;padding:0 4px;border-radius:999px;background:#c94331;color:#fff;font-size:10px;font-style:normal;display:grid;place-items:center}

.pull-edge{position:absolute;left:0;right:0;top:0;height:22px;z-index:22;cursor:default}
.pull-bar{position:absolute;left:50%;top:8px;width:46px;height:4px;margin-left:-23px;border-radius:99px;background:#c5cad3;opacity:0;transform:translateY(-2px);transition:opacity .18s ease, transform .18s ease;pointer-events:none}
.pull-bar.show{opacity:.95;transform:translateY(0)}




.status-shade.right{left:auto;right:12px;transform:translateY(-105%)}
.status-shade.right.open,.status-shade.right.dragging{/* transform via inline for Y only - handled below */}
.status-item-detail{display:block;margin-top:4px;color:#98a2b3;font-size:11px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.activity-detail-pre{margin:6px 0 0;padding:10px;border-radius:6px;background:#fff;border:1px solid #eef0f2;color:#344054;font:12px/1.45 Consolas,"Microsoft YaHei",monospace;white-space:pre-wrap;word-break:break-word;max-height:240px;overflow:auto}

/* ========== U1 v1: youth-facing shell on existing theme palettes ========== */
:root{
  --u1-radius:14px;
  --u1-radius-sm:10px;
  --u1-shadow:0 10px 30px #10182812;
  --u1-shadow-hover:0 14px 36px #10182818;
  --u1-ease:cubic-bezier(.2,.8,.2,1);
  --u1-dur:.18s;
}
body{background:
  radial-gradient(1200px 500px at 12% -10%, color-mix(in srgb, var(--accent-soft) 88%, #fff) 0%, transparent 55%),
  radial-gradient(900px 420px at 100% 0%, color-mix(in srgb, var(--accent) 10%, #fff) 0%, transparent 45%),
  #f4f6f9}
.app-shell{transition:grid-template-columns var(--u1-dur) var(--u1-ease)}
.workspace{position:relative;padding-bottom:20px}
.topbar{position:sticky;top:0;z-index:15;backdrop-filter:blur(12px);background:color-mix(in srgb,#fff 78%, transparent);border-bottom:1px solid color-mix(in srgb,var(--sidebar-border) 70%, transparent);margin:0 -2px 14px;padding:10px 2px 12px}
.topbar h1{letter-spacing:-.02em}
.button{transition:transform var(--u1-dur) var(--u1-ease), box-shadow var(--u1-dur) var(--u1-ease), background var(--u1-dur) ease, border-color var(--u1-dur) ease, color var(--u1-dur) ease;border-radius:999px}
.button:active{transform:scale(.98)}
.button.primary{box-shadow:0 8px 20px color-mix(in srgb,var(--accent) 28%, transparent)}
.button.primary:hover{box-shadow:0 10px 26px color-mix(in srgb,var(--accent) 34%, transparent);transform:translateY(-1px)}
.button.secondary{background:#fff;border:1px solid color-mix(in srgb,var(--sidebar-border) 90%, #d0d5dd);box-shadow:0 1px 2px #10182808}
.button.secondary:hover{border-color:var(--accent);background:var(--accent-soft);color:var(--accent-ink)}
.panel,.results-section .table-wrap,.media-metrics,.registry-options.panel,.backup-panel.panel,.history-summary.panel,.age-overview.panel,.old-files.panel,.duplicate-results.panel,.attribution-panel,.insight-panel,.type-browse.panel,.action-checklist.panel,.analysis-toolbar.panel,.analysis-tabs.panel,.cleanup-list.panel,.folder-contents.panel,.folder-detail-head.panel{border-radius:var(--u1-radius)!important;border-color:color-mix(in srgb,var(--sidebar-border) 85%, #e4e7eb)!important;box-shadow:var(--u1-shadow);transition:transform var(--u1-dur) var(--u1-ease), box-shadow var(--u1-dur) var(--u1-ease), border-color var(--u1-dur) ease}
.panel:hover,.analysis-tabs.panel:hover{box-shadow:var(--u1-shadow-hover)}
.metrics,.media-metrics,.registry-metrics,.analysis-metrics,.folder-metrics{border-radius:var(--u1-radius);overflow:hidden;box-shadow:var(--u1-shadow);border:1px solid color-mix(in srgb,var(--sidebar-border) 85%, #e4e7eb);background:#fff}
.metric,.registry-metrics>div,.analysis-metrics>div,.folder-metrics>div,.media-metrics>div{transition:background var(--u1-dur) ease}
.metric:hover,.registry-metrics>div:hover,.analysis-metrics>div:hover{background:color-mix(in srgb,var(--accent-soft) 55%, #fff)}
.metric-btn,.category-card,.type-card,.age-bucket-card,.attr-region-row,.attr-chip,.action-item,.insight-grid>button,.status-item{transition:transform var(--u1-dur) var(--u1-ease), box-shadow var(--u1-dur) var(--u1-ease), border-color var(--u1-dur) ease, background var(--u1-dur) ease}
.metric-btn:hover,.category-card:hover,.type-card:hover,.age-bucket-card:hover,.attr-region-row:hover,.attr-chip:hover,.action-item:hover,.insight-grid>button:hover{transform:translateY(-2px);box-shadow:0 10px 22px #10182814}
.metric-btn:active,.category-card:active,.type-card:active,.action-item:active,.insight-grid>button:active{transform:translateY(0) scale(.99)}
.main-nav button,.drive-button,.settings-trigger{transition:background var(--u1-dur) var(--u1-ease), color var(--u1-dur) ease, box-shadow var(--u1-dur) ease, transform var(--u1-dur) var(--u1-ease);border-radius:12px!important}
.main-nav button:hover,.drive-button:hover,.settings-trigger:hover{transform:translateX(2px)}
.main-nav button.active{transform:none}
.sidebar{backdrop-filter:blur(18px)}
.brand-mark{border-radius:12px!important}
.progress-track,.registry-progress>i,.media-progress>i,.health-track{border-radius:999px;overflow:hidden}
.progress-track div,.registry-progress>i em,.media-progress>i em,.health-track>i{transition:width .28s var(--u1-ease)}
.tabs button,.preference-segments button,.size-segments button,.filter-group button,.shade-tabs button{transition:background var(--u1-dur) ease, color var(--u1-dur) ease, border-color var(--u1-dur) ease, transform var(--u1-dur) var(--u1-ease);border-radius:999px}
.tabs button.active,.preference-segments button.active,.filter-group button.active{box-shadow:0 2px 10px color-mix(in srgb,var(--accent) 18%, transparent)}
.alert{border-radius:12px;box-shadow:0 8px 24px #10182810}
.table-wrap{border-radius:var(--u1-radius);overflow:hidden}
.table-wrap tr{transition:background .14s ease}
.table-wrap tr:hover{background:color-mix(in srgb,var(--accent-soft) 45%, #fff)}
.check-button,.registry-check{transition:transform .14s var(--u1-ease), background .14s ease, border-color .14s ease}
.check-button:active,.registry-check:active{transform:scale(.92)}
.empty-state,.analysis-empty,.registry-welcome{border-radius:var(--u1-radius);background:linear-gradient(180deg,#fff 0%, color-mix(in srgb,var(--accent-soft) 35%, #fff) 100%);border:1px solid color-mix(in srgb,var(--sidebar-border) 80%, #e4e7eb);box-shadow:var(--u1-shadow)}
.empty-visual{animation:u1-float 3.6s var(--u1-ease) infinite}
@keyframes u1-float{0%,100%{transform:translateY(0)}50%{transform:translateY(-4px)}}
.status-shade{border-radius:0 0 18px 18px!important;box-shadow:0 22px 50px #10182828!important;backdrop-filter:blur(12px)}
.pull-bar{background:color-mix(in srgb,var(--accent) 45%, #c5cad3)!important}
.reclaim-band{border-radius:var(--u1-radius);box-shadow:var(--u1-shadow);border:1px solid color-mix(in srgb,var(--sidebar-border) 80%, #e4e7eb);background:linear-gradient(120deg,#fff 0%, color-mix(in srgb,var(--accent-soft) 70%, #fff) 100%)}
.cleanup-hero{border-radius:var(--u1-radius);box-shadow:var(--u1-shadow)}
.theme-options button{border-radius:12px;transition:transform var(--u1-dur) var(--u1-ease), box-shadow var(--u1-dur) ease, border-color var(--u1-dur) ease}
.theme-options button:hover{transform:translateY(-2px);box-shadow:0 8px 18px #10182814}
.theme-options button.active{box-shadow:0 0 0 2px var(--accent), 0 8px 18px color-mix(in srgb,var(--accent) 20%, transparent)}
.settings-drawer{border-radius:16px 0 0 16px}
.settings-tabs{border-radius:0!important;background:transparent!important}
.icon-button{border-radius:10px;transition:background var(--u1-dur) ease, color var(--u1-dur) ease, transform var(--u1-dur) var(--u1-ease)}
.icon-button:hover{transform:translateY(-1px)}
.text-button{transition:color var(--u1-dur) ease, opacity var(--u1-dur) ease}
.u1-shell{}
/* ========== U1 glass / translucency ========== */
:root{
  --u1-glass: color-mix(in srgb, #fff 52%, transparent);
  --u1-glass-strong: color-mix(in srgb, #fff 58%, transparent);
  --u1-border-a: 38; --u1-border-hair-a: 20; --u1-border: color-mix(in srgb, var(--sidebar-border) calc(var(--u1-border-a) * 1%), transparent); --u1-border-hair: color-mix(in srgb, var(--sidebar-border) calc(var(--u1-border-hair-a) * 1%), transparent); --u1-glass-border: color-mix(in srgb, #fff calc(var(--u1-border-a) * .55%), var(--u1-border));
  --u1-blur: 20px;
}
.sidebar{
  background: color-mix(in srgb, var(--sidebar) 68%, transparent) !important;
  backdrop-filter: blur(22px) saturate(1.15);
  -webkit-backdrop-filter: blur(22px) saturate(1.15);
  border-right: 1px solid var(--u1-glass-border) !important;
}
.topbar{
  background: color-mix(in srgb, #fff 48%, transparent) !important;
  backdrop-filter: blur(22px) saturate(1.2);
  -webkit-backdrop-filter: blur(22px) saturate(1.2);
  border-bottom-color: var(--u1-glass-border) !important;
  box-shadow: 0 8px 24px #10182808;
}
.panel,
.results-section .table-wrap,
.media-metrics,
.registry-options.panel,
.backup-panel.panel,
.history-summary.panel,
.age-overview.panel,
.old-files.panel,
.duplicate-results.panel,
.attribution-panel,
.insight-panel,
.type-browse.panel,
.action-checklist.panel,
.analysis-toolbar.panel,
.analysis-tabs.panel,
.cleanup-list.panel,
.folder-contents.panel,
.folder-detail-head.panel,
.metrics,
.registry-metrics,
.analysis-metrics,
.folder-metrics,
.reclaim-band,
.cleanup-hero,
.media-scope.panel,
.registry-hero{
  background: var(--u1-glass) !important;
  backdrop-filter: blur(20px) saturate(1.12);
  -webkit-backdrop-filter: blur(20px) saturate(1.12);
  border-color: var(--u1-border) !important;
  box-shadow: 0 10px 28px #1018280f, inset 0 1px 0 #ffffffa6 !important;
}
.metric:hover,
.registry-metrics>div:hover,
.analysis-metrics>div:hover,
.table-wrap tr:hover{
  background: color-mix(in srgb, var(--accent-soft) 40%, #ffffffcc) !important;
}
.button.secondary,
.icon-button,
.check-button,
.registry-check,
.dropdown-trigger,
.field select,
.field input,
.exclusion-add input,
.search input{
  background: color-mix(in srgb, #fff 56%, transparent) !important;
  backdrop-filter: blur(12px);
  -webkit-backdrop-filter: blur(12px);
}
.button.primary{
  backdrop-filter: blur(6px);
}
.main-nav button.active,
.drive-button.active,
.settings-trigger:hover,
.main-nav button:hover,
.drive-button:hover{
  background: color-mix(in srgb, #fff 68%, transparent) !important;
  backdrop-filter: blur(18px);
  -webkit-backdrop-filter: blur(18px);
}
.status-shade{
  background: color-mix(in srgb, #fff 56%, transparent) !important;
  backdrop-filter: blur(22px) saturate(1.2) !important;
  -webkit-backdrop-filter: blur(22px) saturate(1.2) !important;
  border-color: var(--u1-border) !important;
  box-shadow: 0 24px 60px #1018282a, inset 0 1px 0 #ffffffc8 !important;
}
.status-item,
.activity-log-row,
.message-log-row,
.category-card,
.type-card,
.age-bucket-card,
.attr-region-card,
.attr-project-row,
.action-item,
.duplicate-group,
.cleanup-row,
.folder-analysis-row{
  background: color-mix(in srgb, #fff 52%, transparent) !important;
  backdrop-filter: blur(18px);
  -webkit-backdrop-filter: blur(18px);
  border-color: var(--u1-border) !important;
}
.settings-drawer{
  background: color-mix(in srgb, #fff 68%, transparent) !important;
  backdrop-filter: blur(22px) saturate(1.15);
  -webkit-backdrop-filter: blur(22px) saturate(1.15);
}
.settings-backdrop{background:#10182840!important;backdrop-filter:blur(6px)}
.status-scrim.dim{background:#1018282a!important;backdrop-filter:blur(4px)}
.alert{
  background: color-mix(in srgb, #fff 58%, transparent) !important;
  backdrop-filter: blur(18px);
  -webkit-backdrop-filter: blur(18px);
}
.alert.notice{background:color-mix(in srgb,#effaf6 62%, transparent)!important}
.alert.error{background:color-mix(in srgb,#fff0f0 64%, transparent)!important}
.empty-state,.analysis-empty,.registry-welcome{
  background: linear-gradient(180deg, color-mix(in srgb,#fff 80%, transparent), color-mix(in srgb,var(--accent-soft) 50%, transparent)) !important;
  backdrop-filter: blur(18px);
  -webkit-backdrop-filter: blur(18px);
}
.confirm-dialog,.repair-dialog,.message-log-dialog{
  background: color-mix(in srgb, #fff 76%, transparent) !important;
  backdrop-filter: blur(20px) saturate(1.1);
  -webkit-backdrop-filter: blur(20px) saturate(1.1);
  border: 1px solid var(--u1-glass-border);
  box-shadow: 0 24px 60px #10182830, inset 0 1px 0 #ffffffd0;
}
.preference-segments,.size-segments{
  background: transparent !important;
  backdrop-filter: none;
}
.settings-tabs{
  background: transparent !important;
  backdrop-filter: none !important;
  -webkit-backdrop-filter: none !important;
}
.preference-segments button.active,.settings-tabs button.active,.tabs button.active{
  background: color-mix(in srgb, #fff 48%, transparent) !important;
}
/* keep tables readable */
.table-wrap{background: color-mix(in srgb, #fff 48%, transparent) !important}
.table-wrap thead th{background: color-mix(in srgb, #f8fafc 80%, transparent)}

/* U1 round 2: deeper glass + list polish */
:root{--u1-glass: color-mix(in srgb, #fff calc(var(--u1-glass-pct, 50) * 1%), transparent); --u1-glass-strong: color-mix(in srgb, #fff calc(var(--u1-glass-pct-strong, 64) * 1%), transparent); --u1-blur: var(--u1-blur-px, 22px)}
.workspace{background:transparent}
.panel-kicker{letter-spacing:.08em;text-transform:uppercase;opacity:.85}
.panel>header,.registry-results>header,.panel-heading{border-bottom-color:color-mix(in srgb,#fff 40%, #edf0f2)!important}
.registry-filter-bar{gap:12px!important;padding:14px 16px!important;align-items:end}
.registry-filter-bar .field.grow input,
.registry-filter-bar input[type="search"]{min-height:40px;border-radius:12px!important;padding:0 14px!important;background:color-mix(in srgb,#fff 55%,transparent)!important}
.registry-filter-bar .field.compact{min-width:0}
.registry-filter-bar select{min-height:40px;border-radius:12px!important;background:color-mix(in srgb,#fff 55%,transparent)!important}
.file-filter-bar{padding:12px 14px!important;border-radius:14px!important;gap:12px!important}
.filter-group button,.size-segments button{backdrop-filter:blur(10px)}
.cleanup-row,.folder-analysis-row,.duplicate-group,.registry-row{border:1px solid color-mix(in srgb,#fff 45%, #eef0f2);margin-bottom:8px;border-radius:12px;padding:10px 12px}
.cleanup-rows,.registry-rows,.folder-analysis-rows,.duplicate-groups{gap:8px;display:grid}
.scan-strip,.media-progress{background:color-mix(in srgb,#fff 48%,transparent)!important;backdrop-filter:blur(16px);border:1px solid color-mix(in srgb,#fff 45%, #e4e7eb);border-radius:14px;box-shadow:0 8px 24px #1018280c}
.donut{box-shadow:inset 0 0 0 1px color-mix(in srgb,#fff 50%, transparent), 0 12px 28px #10182810}
.health-panel,.distribution-panel{overflow:hidden}
.button.repair-button,.button.danger-solid{box-shadow:0 8px 20px color-mix(in srgb,var(--accent) 22%, transparent)}

/* chip-row: no outer long trough frame */
.chip-row{display:flex;flex-wrap:wrap;gap:8px;align-items:center;background:transparent!important;padding:0!important;border:0!important;box-shadow:none!important}
.chip-row.mini{gap:6px}
.chip-btn{height:34px;border:1px solid color-mix(in srgb,var(--sidebar-border) 80%, #d7dce2);border-radius:999px;background:color-mix(in srgb,#fff 58%, transparent);color:#52606d;padding:0 12px;font-size:12px;font-weight:650;backdrop-filter:blur(10px);transition:transform .16s var(--u1-ease), background .16s ease, border-color .16s ease, color .16s ease, box-shadow .16s ease}
.chip-btn:hover{transform:translateY(-1px);border-color:var(--accent);color:var(--accent-ink)}
.chip-btn.active{background:var(--accent-soft);border-color:var(--accent);color:var(--accent-ink);box-shadow:0 4px 12px color-mix(in srgb,var(--accent) 18%, transparent)}
.chip-btn:active{transform:scale(.98)}
.preference-segments,.size-segments,.settings-tabs{background:transparent!important;padding:0!important;gap:8px!important;box-shadow:none!important;border:0!important}
.preference-segments{display:flex!important;flex-wrap:wrap!important;grid-auto-flow:unset!important;grid-auto-columns:unset!important}
.preference-segments button,.size-segments button,.settings-tabs button,.filter-group>button,.tabs button,.shade-tabs button{border:1px solid color-mix(in srgb,var(--sidebar-border) 80%, #d7dce2)!important;border-radius:999px!important;background:color-mix(in srgb,#fff 58%, transparent)!important;box-shadow:none!important}
.preference-segments button.active,.size-segments button.active,.settings-tabs button.active,.filter-group>button.active,.tabs button.active,.shade-tabs button.active{background:var(--accent-soft)!important;border-color:var(--accent)!important;color:var(--accent-ink)!important;box-shadow:0 4px 12px color-mix(in srgb,var(--accent) 16%, transparent)!important}
.glass-slider{display:grid;grid-template-columns:20px 1fr 20px;gap:10px;align-items:center;margin-top:6px}
.glass-slider input[type="range"]{width:100%;accent-color:var(--accent)}
.glass-slider-tip{display:block;margin-top:8px;color:#98a2b3;font-size:11px}
.glass-quick input[type="range"]{width:min(160px,42vw);accent-color:var(--accent)}
.loading-state{display:flex;align-items:center;justify-content:center;gap:10px;min-height:120px;border-radius:var(--u1-radius);background:var(--u1-glass);backdrop-filter:blur(var(--u1-blur-px,18px));border:1px solid var(--u1-glass-border);color:#667085}
.empty-state,.analysis-empty,.registry-welcome{animation:u1-rise .35s var(--u1-ease) both}
@keyframes u1-rise{from{opacity:0;transform:translateY(8px)}to{opacity:1;transform:translateY(0)}}
.scan-strip{animation:u1-rise .25s var(--u1-ease) both}
.alert.notice{animation:u1-pop .28s var(--u1-ease) both}
@keyframes u1-pop{from{opacity:0;transform:translateY(-6px) scale(.98)}to{opacity:1;transform:translateY(0) scale(1)}}
.status-list .no-matches{padding:28px 12px;text-align:center;color:#98a2b3}
:root{--u1-glass-pct:50;--u1-glass-pct-strong:64;--u1-blur-px:22px;--u1-glass:color-mix(in srgb,#fff calc(var(--u1-glass-pct)*1%),transparent);--u1-glass-strong:color-mix(in srgb,#fff calc(var(--u1-glass-pct-strong)*1%),transparent);--u1-blur:var(--u1-blur-px)}

/* A mild de-box */
.section-head,.analysis-toolbar.section-head{background:transparent!important;border:0!important;box-shadow:none!important;backdrop-filter:none!important;-webkit-backdrop-filter:none!important;padding:4px 2px 14px!important;margin:0 0 8px!important;border-radius:0!important}
.section-head h2,.analysis-toolbar h2{margin:4px 0 6px;letter-spacing:-.02em}
.section-head p{margin:0;color:#667085;line-height:1.55;max-width:62ch}
.analysis-tabs.section-tabs,.analysis-tabs.panel{background:transparent!important;border:0!important;box-shadow:none!important;backdrop-filter:none!important;padding:4px 0 10px!important}
.analysis-tabs.section-tabs>button{background:color-mix(in srgb,#fff 52%, transparent)!important;border:1px solid color-mix(in srgb,var(--sidebar-border) 75%, #e4e7eb)!important;border-radius:14px!important;box-shadow:none!important}
.analysis-tabs.section-tabs>button.active{background:var(--accent-soft)!important;border-color:var(--accent)!important;box-shadow:0 6px 16px color-mix(in srgb,var(--accent) 16%, transparent)!important}
.panel-heading,.registry-results>header,.media-results>header,.duplicate-results .panel-heading,.cleanup-toolbar{background:transparent!important;border-bottom:0!important}
.file-filter-bar,.registry-filter-bar.panel,.registry-filter-bar{background:transparent!important;border:0!important;box-shadow:none!important;backdrop-filter:none!important}
.folder-picker-band{background:transparent!important;border:0!important;box-shadow:none!important;padding:8px 2px 14px!important}
.duplicate-results.panel,.media-results.panel,.registry-results.panel,.cleanup-list.panel,.history-summary.panel,.age-overview.panel,.old-files.panel,.action-checklist.panel,.type-browse.panel,.attribution-detail.panel,.folder-contents.panel{box-shadow:0 8px 24px #1018280a!important}
.cleanup-row,.folder-analysis-row,.duplicate-group,.registry-row,.media-row,.attr-project-row,.attr-region-card,.action-item{border-color:color-mix(in srgb,#fff 35%, #edf0f2)!important;box-shadow:none!important}

/* page title: no outer frame (空间概览/清理中心/文件审查...) */
.workspace>.topbar,
.topbar{
  position:static!important;
  top:auto!important;
  z-index:5!important;
  background:transparent!important;
  backdrop-filter:none!important;
  -webkit-backdrop-filter:none!important;
  border:0!important;
  border-bottom:0!important;
  box-shadow:none!important;
  margin:0 0 10px!important;
  padding:2px 2px 8px!important;
}
.topbar h1{margin:2px 0 0!important;letter-spacing:-.03em}
.topbar .eyebrow{opacity:.9}
.topbar .actions{gap:8px}
/* keep analysis tabs as separate chips, no shared outer trough */
.analysis-tabs,
.analysis-tabs.section-tabs,
.analysis-tabs.panel{
  background:transparent!important;
  border:0!important;
  box-shadow:none!important;
  backdrop-filter:none!important;
  -webkit-backdrop-filter:none!important;
  padding:2px 0 12px!important;
  margin:0 0 4px!important;
}

/* border slider must affect tabs/chips strongly */
.chip-btn,
.chip-row .chip-btn,
.filter-group .chip-btn,
.preference-segments button,
.size-segments button,
.settings-tabs button,
.tabs button,
.shade-tabs button,
.analysis-tabs>button,
.analysis-tabs.section-tabs>button,
.media-kind-tabs button,
.media-kind-tabs .chip-btn{
  border-color: var(--u1-border-chip, var(--u1-border)) !important;
}
.chip-btn.active,
.preference-segments button.active,
.size-segments button.active,
.settings-tabs button.active,
.tabs button.active,
.shade-tabs button.active,
.analysis-tabs>button.active,
.analysis-tabs.section-tabs>button.active,
.media-kind-tabs button.active{
  border-color: var(--accent) !important;
}
.panel,
.metrics,
.media-metrics,
.registry-metrics,
.analysis-metrics,
.folder-metrics,
.table-wrap,
.cleanup-list.panel,
.duplicate-results.panel,
.history-summary.panel,
.age-overview.panel,
.old-files.panel,
.action-checklist.panel,
.type-browse.panel,
.attribution-panel,
.insight-panel,
.folder-contents.panel,
.media-results.panel,
.backup-panel.panel,
.reclaim-band,
.cleanup-hero,
.scan-strip,
.loading-state,
.empty-state,
.analysis-empty,
.registry-welcome,
.category-card,
.type-card,
.age-bucket-card,
.duplicate-group,
.cleanup-row,
.folder-analysis-row,
.registry-row,
.media-row,
.attr-project-row,
.attr-region-card,
.action-item,
.status-item,
.activity-log-row,
.message-log-row,
.status-shade{
  border-color: var(--u1-border) !important;
}
/* zero end: hide structure borders completely when slider near 0 */
html[style*="--u1-border-a: 0"] .panel,
html[style*="--u1-border-a: 0"] .metrics,
html[style*="--u1-border-a: 0"] .table-wrap,
html[style*="--u1-border-a: 0"] .cleanup-row,
html[style*="--u1-border-a: 0"] .registry-row,
html[style*="--u1-border-a: 0"] .media-row,
html[style*="--u1-border-a: 0"] .duplicate-group,
html[style*="--u1-border-a: 0"] .category-card,
html[style*="--u1-border-a: 0"] .type-card,
html[style*="--u1-border-a: 0"] .age-bucket-card,
html[style*="--u1-border-a: 0"] .action-item,
html[style*="--u1-border-a: 0"] .attr-region-card,
html[style*="--u1-border-a: 0"] .attr-project-row,
html[style*="--u1-border-a: 0"] .status-item,
html[style*="--u1-border-a: 0"] .activity-log-row{
  border-color: transparent !important;
}

/* settings tabs: independent chips only — no gray trough */
.settings-drawer .settings-tabs,
nav.settings-tabs,
.settings-tabs{
  display:flex!important;
  flex-wrap:wrap!important;
  align-items:center!important;
  justify-content:flex-start!important;
  gap:8px!important;
  margin:18px 0 16px!important;
  padding:0!important;
  border:0!important;
  border-radius:0!important;
  background:transparent!important;
  background-color:transparent!important;
  background-image:none!important;
  box-shadow:none!important;
  backdrop-filter:none!important;
  -webkit-backdrop-filter:none!important;
  grid-template-columns:none!important;
  grid-auto-flow:unset!important;
}
.settings-drawer .settings-tabs button,
nav.settings-tabs button,
.settings-tabs button{
  flex:0 0 auto!important;
  width:auto!important;
  min-width:0!important;
  height:36px!important;
  padding:0 14px!important;
  border:1px solid var(--u1-border-chip, var(--u1-border, #d7dce2))!important;
  border-radius:999px!important;
  background:color-mix(in srgb,#fff 70%, transparent)!important;
  box-shadow:none!important;
  color:#667085!important;
  gap:6px!important;
}
.settings-drawer .settings-tabs button.active,
nav.settings-tabs button.active,
.settings-tabs button.active{
  background:var(--accent-soft)!important;
  border-color:var(--accent)!important;
  color:var(--accent-ink)!important;
  box-shadow:0 4px 12px color-mix(in srgb,var(--accent) 16%, transparent)!important;
}

/* full-bleed notification shade */
.pull-edge{
  height:28px!important;
  z-index:28!important;
}
.status-shade,
.status-shade.right{
  left:var(--shade-left, 232px)!important;
  right:0!important;
  width:auto!important;
  max-width:none!important;
  max-height:100vh!important;
  border-radius:0 0 18px 0!important;
  border-left:1px solid var(--u1-border)!important;
  border-right:0!important;
  border-bottom:1px solid var(--u1-border)!important;
}
.status-shade .status-list{
  flex:1;
  max-height:none!important;
  min-height:0;
  overflow:auto;
  padding:10px 16px 16px!important;
  display:grid;
  grid-template-columns:repeat(auto-fill,minmax(280px,1fr));
  gap:10px;
  align-content:start;
}
.status-shade .status-item{
  margin:0!important;
  min-height:88px;
}
/* old quick-grid overridden by quick-layout */
.status-shade .shade-head,
.status-shade .shade-foot{
  padding-left:18px!important;
  padding-right:18px!important;
}
.status-shade .shade-tabs{
  padding:0 16px 10px!important;
  max-width:420px;
}

/* border width + stronger extremes */
.panel,.metrics,.media-metrics,.registry-metrics,.analysis-metrics,.folder-metrics,.table-wrap,
.cleanup-list.panel,.duplicate-results.panel,.history-summary.panel,.age-overview.panel,.old-files.panel,
.action-checklist.panel,.type-browse.panel,.attribution-panel,.insight-panel,.folder-contents.panel,
.media-results.panel,.backup-panel.panel,.reclaim-band,.cleanup-hero,.scan-strip,.loading-state,
.empty-state,.analysis-empty,.category-card,.type-card,.age-bucket-card,.duplicate-group,.cleanup-row,
.folder-analysis-row,.registry-row,.media-row,.attr-project-row,.attr-region-card,.action-item,
.status-item,.activity-log-row,.message-log-row,.chip-btn,.analysis-tabs>button,.settings-tabs button{
  border-width: var(--u1-border-w, 1px) !important;
  border-style: solid !important;
}
html[data-border-level="none"] .panel,
html[data-border-level="none"] .metrics,
html[data-border-level="none"] .table-wrap,
html[data-border-level="none"] .cleanup-row,
html[data-border-level="none"] .registry-row,
html[data-border-level="none"] .media-row,
html[data-border-level="none"] .duplicate-group,
html[data-border-level="none"] .category-card,
html[data-border-level="none"] .type-card,
html[data-border-level="none"] .age-bucket-card,
html[data-border-level="none"] .action-item,
html[data-border-level="none"] .chip-btn,
html[data-border-level="none"] .analysis-tabs>button,
html[data-border-level="none"] .settings-tabs button,
html[data-border-level="none"] .status-item,
html[data-border-level="none"] .activity-log-row{
  border-color: transparent !important;
  box-shadow: none !important;
}
html[data-border-level="hard"] .panel,
html[data-border-level="hard"] .metrics,
html[data-border-level="hard"] .table-wrap,
html[data-border-level="hard"] .cleanup-list.panel,
html[data-border-level="hard"] .duplicate-results.panel{
  border-color: #52606f !important;
  box-shadow: 0 1px 0 #52606f22 !important;
}

/* quick settings: full-width adaptive layout */
.status-shade .quick-layout{
  flex:0 0 auto;
  min-height:0;
  overflow:visible;
  padding:12px 18px 24px;
  display:grid;
  grid-template-columns:repeat(3,minmax(0,1fr));
  gap:14px;
  align-content:start;
}
.status-shade .quick-card{
  background:color-mix(in srgb,#fff 58%, transparent);
  border:1px solid var(--u1-border);
  border-radius:16px;
  padding:14px 16px 16px;
  backdrop-filter:blur(14px);
  box-shadow:0 8px 24px #1018280c;
  display:grid;
  gap:12px;
  min-width:0;
}
.status-shade .quick-card-wide{grid-column:1 / -1}
.status-shade .quick-card-title{display:flex;align-items:baseline;justify-content:space-between;gap:10px}
.status-shade .quick-card-title b{font-size:14px;letter-spacing:-.02em}
.status-shade .quick-card-title small{color:#98a2b3;font-size:11px}
.status-shade .quick-toggles{display:grid;gap:10px}
.status-shade .quick-toggle{
  display:grid;
  grid-template-columns:minmax(0,1fr) auto;
  gap:12px;
  align-items:center;
  padding:10px 12px;
  border-radius:12px;
  background:color-mix(in srgb,#fff 42%, transparent);
  border:1px solid var(--u1-border-hair, var(--u1-border));
}
.status-shade .quick-toggle span{display:grid;gap:2px;min-width:0}
.status-shade .quick-toggle b{font-size:13px;color:#344054}
.status-shade .quick-toggle small{color:#98a2b3;font-size:11px;line-height:1.35}
.status-shade .quick-field{
  display:grid;
  grid-template-columns:52px minmax(0,1fr);
  gap:10px;
  align-items:center;
}
.status-shade .quick-field>span{color:#667085;font-size:12px;font-weight:650}
.status-shade .quick-slider{display:grid;gap:8px}
.status-shade .quick-slider-label{display:flex;justify-content:space-between;gap:10px;align-items:center}
.status-shade .quick-slider-label span{color:#667085;font-size:12px;font-weight:650}
.status-shade .quick-slider-label em{font-style:normal;color:#98a2b3;font-size:11px}
.status-shade .quick-slider input[type="range"]{width:100%;accent-color:var(--accent)}
.status-shade .quick-actions{
  display:grid;
  grid-template-columns:repeat(4,minmax(0,1fr));
  gap:10px;
}
.status-shade .quick-link{
  display:grid;
  grid-template-columns:28px minmax(0,1fr);
  gap:10px;
  align-items:center;
  text-align:left;
  border:1px solid var(--u1-border);
  border-radius:14px;
  background:color-mix(in srgb,#fff 50%, transparent);
  padding:12px 12px;
  color:inherit;
  transition:transform .16s var(--u1-ease), border-color .16s ease, background .16s ease, box-shadow .16s ease;
}
.status-shade .quick-link:hover{
  transform:translateY(-2px);
  border-color:var(--accent);
  background:var(--accent-soft);
  box-shadow:0 10px 22px #10182812;
}
.status-shade .quick-link span{display:grid;gap:2px;min-width:0}
.status-shade .quick-link b{font-size:13px}
.status-shade .quick-link small{color:#98a2b3;font-size:11px}
.status-shade .shade-head-quick{align-items:flex-start}
.status-shade .shade-head-actions{display:flex;gap:8px;flex-wrap:wrap}
/* windowed / narrow: stack cards */
@media (max-width: 1200px){
  .status-shade .quick-layout{grid-template-columns:repeat(2,minmax(0,1fr))}
  .status-shade .quick-actions{grid-template-columns:repeat(2,minmax(0,1fr))}
}
@media (max-width: 820px){
  .status-shade .quick-layout{grid-template-columns:1fr}
  .status-shade .quick-actions{grid-template-columns:1fr}
  .status-shade .quick-field{grid-template-columns:1fr}
}
/* notification list also benefits from full height */
.status-shade{display:flex;flex-direction:column}
.status-shade .status-list{flex:1}
.status-shade .shade-foot{margin-top:auto}

/* U1.7: empty / loading / no-matches consistency */
.no-matches{
  display:grid;
  place-items:center;
  gap:6px;
  min-height:96px;
  padding:22px 16px!important;
  text-align:center;
  color:#98a2b3!important;
  font-size:13px;
  border:1px dashed var(--u1-border-hair, var(--u1-border))!important;
  border-radius:14px;
  background:color-mix(in srgb,#fff 36%, transparent);
}
.status-list .no-matches,
.table-wrap .no-matches{
  grid-column:1 / -1;
}
.loading-state{
  min-height:140px!important;
  border-color:var(--u1-border)!important;
  border-width:var(--u1-border-w,1px)!important;
  color:#667085!important;
  letter-spacing:-.01em;
}
.empty-state,.analysis-empty,.registry-welcome{
  border-color:var(--u1-border)!important;
  border-width:var(--u1-border-w,1px)!important;
  background:
    radial-gradient(120% 80% at 12% 0%, color-mix(in srgb,var(--accent-soft) 70%, transparent), transparent 55%),
    linear-gradient(180deg, color-mix(in srgb,#fff 88%, transparent), color-mix(in srgb,var(--accent-soft) 28%, #fff))!important;
  padding:36px 28px!important;
}
.empty-state .empty-visual,
.analysis-empty .empty-visual{
  width:72px;height:72px;border-radius:20px;
  display:grid;place-items:center;
  margin:0 auto 12px;
  background:color-mix(in srgb,var(--accent-soft) 80%, #fff);
  color:var(--accent-ink);
  box-shadow:0 10px 28px color-mix(in srgb,var(--accent) 14%, transparent);
}
.empty-state h2,.analysis-empty h2{letter-spacing:-.03em;margin:0 0 8px}
.empty-state p,.analysis-empty p{color:#667085;line-height:1.55;max-width:46ch;margin:0 auto 18px}
.empty-actions{display:flex;flex-wrap:wrap;gap:10px;justify-content:center}
/* quick panel chips denser */
.status-shade .chip-row{gap:6px!important}
.status-shade .chip-btn{
  height:30px!important;
  padding:0 10px!important;
  font-size:11px!important;
  border-color:var(--u1-border-chip, var(--u1-border))!important;
}
/* shade glass follows material slider */
.status-shade{
  background:var(--u1-glass, color-mix(in srgb,#fff 82%, transparent))!important;
  backdrop-filter:blur(var(--u1-blur-px, 16px)) saturate(1.15)!important;
  -webkit-backdrop-filter:blur(var(--u1-blur-px, 16px)) saturate(1.15)!important;
}
.status-scrim.dim{background:#10182838!important;backdrop-filter:blur(2px)}
/* avoid nested box feel on result toolbar */
.result-toolbar{
  background:transparent!important;
  border:0!important;
  box-shadow:none!important;
  backdrop-filter:none!important;
  padding:4px 2px 12px!important;
}

/* U1.8 sweep: no double chrome / filter troughs */
.file-filter-bar,
.registry-filter-bar,
.registry-filter-bar.panel,
.result-toolbar,
.cleanup-toolbar,
.folder-picker-band,
.duplicate-controls,
.analysis-toolbar,
.analysis-toolbar.panel,
.analysis-toolbar.section-head{
  background:transparent!important;
  border:0!important;
  box-shadow:none!important;
  backdrop-filter:none!important;
  -webkit-backdrop-filter:none!important;
}
.file-filter-bar{
  display:flex!important;
  flex-wrap:wrap!important;
  gap:12px 16px!important;
  align-items:center!important;
  padding:4px 2px 12px!important;
  margin:0 0 8px!important;
}
.filter-group{
  display:flex!important;
  align-items:center!important;
  gap:8px!important;
  flex-wrap:wrap!important;
  background:transparent!important;
  border:0!important;
  padding:0!important;
}
.filter-group>span{color:#98a2b3!important;font-size:11px!important;font-weight:650}
.filter-group button,
.filter-group .chip-btn{
  height:32px!important;
  border:1px solid var(--u1-border-chip, var(--u1-border, #d7dce2))!important;
  border-radius:999px!important;
  background:color-mix(in srgb,#fff 62%, transparent)!important;
  color:#52606d!important;
  padding:0 12px!important;
  box-shadow:none!important;
}
.filter-group button.active,
.filter-group .chip-btn.active{
  background:var(--accent-soft)!important;
  border-color:var(--accent)!important;
  color:var(--accent-ink)!important;
}
/* kill nested box: rows inside lists shouldn't look like cards-in-cards when list already framed */
.cleanup-list.panel .cleanup-row,
.registry-results.panel .registry-row,
.media-results.panel .media-row,
.duplicate-results.panel .duplicate-group,
.folder-contents.panel .folder-analysis-row{
  border-color:color-mix(in srgb, var(--u1-border, #edf0f2) 70%, transparent)!important;
  box-shadow:none!important;
  background:color-mix(in srgb,#fff 42%, transparent)!important;
}
/* panel headers: hairline only, no second box */
.panel>header,
.panel-heading,
.registry-results>header,
.media-results>header,
.backup-panel>header{
  background:transparent!important;
  border-bottom:1px solid var(--u1-border-hair, var(--u1-border))!important;
  box-shadow:none!important;
}
/* tabs row: chips only */
.tabs,
.size-segments,
.preference-segments{
  background:transparent!important;
  border:0!important;
  box-shadow:none!important;
  padding:0!important;
  gap:8px!important;
}
.tabs button,
.size-segments button{
  border:1px solid var(--u1-border-chip, var(--u1-border))!important;
  border-radius:999px!important;
  background:color-mix(in srgb,#fff 62%, transparent)!important;
}
.tabs button.active,
.size-segments button.active{
  background:var(--accent-soft)!important;
  border-color:var(--accent)!important;
  color:var(--accent-ink)!important;
}
/* result toolbar tabs area */
.result-toolbar{
  display:flex!important;
  flex-wrap:wrap!important;
  align-items:center!important;
  gap:10px!important;
  min-height:0!important;
  height:auto!important;
  padding:2px 2px 12px!important;
}

.risk-badge.toolai{background:#eef2ff;color:#4338ca;border:1px solid #c7d2fe}
.cleanup-row.toolai-row{border-style:dashed!important}

/* U1 readability fix: glass must not inherit dark-card white text */
.workspace,
.workspace .panel,
.workspace .cleanup-list,
.workspace .cleanup-row,
.workspace .duplicate-group,
.workspace .duplicate-results,
.workspace .analysis-metrics,
.workspace .metrics,
.workspace .table-wrap{
  color:#1d2939!important;
}
.cleanup-hero{
  background:linear-gradient(135deg, color-mix(in srgb,var(--accent-soft) 55%, #fff), #fff)!important;
  color:#1d2939!important;
  border:1px solid var(--u1-border, #e4e7eb)!important;
}
.cleanup-hero strong,
.cleanup-hero b,
.cleanup-hero h2,
.cleanup-hero .panel-kicker{
  color:#1d2939!important;
}
.cleanup-hero p,
.cleanup-hero small,
.cleanup-hero span{
  color:#667085!important;
}
.cleanup-breakdown{
  grid-template-columns:repeat(5,minmax(0,1fr))!important;
  border-left:1px solid #e4e7eb!important;
}
.cleanup-breakdown>div{
  border-right:1px solid #eef0f2!important;
  padding:8px 12px!important;
}
.cleanup-breakdown>div:last-child{border-right:0!important}
.cleanup-breakdown span{color:#667085!important;font-size:11px!important}
.cleanup-breakdown b{color:#1d2939!important;font-size:14px!important}
.cleanup-copy,
.cleanup-copy b,
.cleanup-copy p,
.cleanup-size b{
  color:#1d2939!important;
}
.cleanup-copy p,
.cleanup-copy small,
.cleanup-size span{
  color:#667085!important;
}
.cleanup-list.panel .cleanup-row,
.duplicate-results.panel .duplicate-group{
  background:color-mix(in srgb,#fff 88%, transparent)!important;
  color:#1d2939!important;
  border:1px solid var(--u1-border, #e4e7eb)!important;
}
.duplicate-group-head b,
.duplicate-group-head strong,
.duplicate-paths,
.duplicate-paths span,
.duplicate-paths b{
  color:#1d2939!important;
}
.duplicate-group-head small,
.duplicate-paths small,
.muted{
  color:#667085!important;
}
.duplicate-groups{
  display:grid!important;
  gap:10px!important;
}
.duplicate-group{
  display:grid!important;
  gap:10px!important;
  border-radius:14px!important;
  padding:14px 16px!important;
  border-bottom:0!important;
}
.duplicate-group-head{
  display:grid!important;
  grid-template-columns:28px minmax(0,1fr) auto!important;
  gap:10px!important;
  align-items:center!important;
}
.duplicate-paths>div{
  display:grid!important;
  grid-template-columns:22px 16px minmax(0,1fr) auto auto!important;
  gap:8px!important;
  align-items:center!important;
  color:#1d2939!important;
}
.analysis-metrics,
.folder-metrics,
.metrics,
.media-metrics,
.registry-metrics{
  color:#1d2939!important;
  background:color-mix(in srgb,#fff 90%, transparent)!important;
}
.analysis-metrics span,
.folder-metrics span,
.metrics span,
.media-metrics small{
  color:#667085!important;
}
.analysis-metrics b,
.folder-metrics b,
.metrics strong,
.media-metrics b{
  color:#1d2939!important;
}
/* glass panels: keep body text dark on light glass */
.panel,
.cleanup-list.panel,
.duplicate-results.panel,
.history-summary.panel,
.age-overview.panel,
.old-files.panel,
.type-browse.panel,
.action-checklist.panel,
.attribution-panel,
.insight-panel{
  color:#1d2939!important;
}
.panel p,
.panel small,
.panel .muted{
  color:#667085!important;
}
.panel h2,
.panel b,
.panel strong{
  color:#1d2939!important;
}
/* primary buttons keep contrast text */
.button.primary,
.brand-mark{
  color:var(--accent-contrast, #fff)!important;
}
/* toolai badge keep */
.risk-badge.toolai{background:#eef2ff!important;color:#4338ca!important;border:1px solid #c7d2fe!important}
.cleanup-row.toolai-row{border-style:dashed!important}

/* UI polish: duplicate cards + file tabs radius + registry input */
.duplicate-groups{
  display:grid!important;
  gap:14px!important;
  padding:4px 2px 12px!important;
}
.duplicate-group{
  display:grid!important;
  grid-template-columns:1fr!important;
  gap:12px!important;
  margin:0!important;
  padding:14px 16px!important;
  border:1px solid var(--u1-border, #e4e7eb)!important;
  border-radius:12px!important;
  border-bottom:1px solid var(--u1-border, #e4e7eb)!important;
  background:#fff!important;
  color:#1d2939!important;
  box-shadow:0 4px 14px #10182808!important;
}
.duplicate-group-head{
  display:grid!important;
  grid-template-columns:28px minmax(0,1fr) auto!important;
  gap:12px!important;
  align-items:center!important;
  width:100%!important;
}
.duplicate-group-head>span{
  width:28px!important;height:28px!important;
  border-radius:8px!important;
  background:var(--accent-soft)!important;
  color:var(--accent-ink)!important;
  display:grid!important;place-items:center!important;
  font-size:12px!important;font-weight:700!important;
}
.duplicate-group-head b{display:block!important;color:#1d2939!important;font-size:13px!important}
.duplicate-group-head small{display:block!important;color:#667085!important;font-size:11px!important;margin-top:2px!important}
.duplicate-group-head strong{color:var(--accent-ink)!important;font-size:13px!important;white-space:nowrap!important}
.duplicate-paths{
  display:grid!important;
  gap:6px!important;
  width:100%!important;
}
.duplicate-path-row,
.duplicate-paths>div.duplicate-path-row{
  display:grid!important;
  grid-template-columns:22px 18px minmax(0,1fr) auto 32px!important;
  gap:8px!important;
  align-items:center!important;
  padding:8px 10px!important;
  border-radius:8px!important;
  color:#1d2939!important;
  min-width:0!important;
}
.duplicate-path-row.selected{background:#f3fbf8!important}
.duplicate-path-row .dup-path,
.duplicate-path-row>span.dup-path{
  min-width:0!important;
  overflow:hidden!important;
  text-overflow:ellipsis!important;
  white-space:nowrap!important;
  color:#344054!important;
  font-size:12px!important;
}
.duplicate-path-row .dup-file-icon{color:#98a2b3!important;flex:none}
.keep-tag{
  font-size:10px!important;
  color:#0f8f6b!important;
  background:#e8fbf4!important;
  padding:2px 8px!important;
  border-radius:6px!important;
  white-space:nowrap!important;
  font-style:normal!important;
}
.keep-tag-space{width:1px;height:1px}
.duplicate-group-foot{
  display:flex!important;
  flex-wrap:wrap!important;
  gap:8px 14px!important;
  align-items:center!important;
  width:100%!important;
  padding:4px 2px 0!important;
  margin:0!important;
  border-top:1px solid #eef0f2!important;
  grid-column:auto!important;
}
.duplicate-group-foot .text-button{
  height:32px!important;
  padding:0 10px!important;
  color:var(--accent-ink)!important;
  font-size:12px!important;
  font-weight:650!important;
}
/* 文件审查 tabs：圆角收一点，别用胶囊 999 */
.results-section .tabs,
.result-toolbar .tabs{
  display:flex!important;
  flex-wrap:wrap!important;
  gap:8px!important;
  background:transparent!important;
  border:0!important;
  padding:0!important;
}
.results-section .tabs button,
.result-toolbar .tabs button{
  height:36px!important;
  border-radius:10px!important;
  border:1px solid var(--u1-border-chip, #d7dce2)!important;
  background:color-mix(in srgb,#fff 88%, transparent)!important;
  color:#52606d!important;
  padding:0 14px!important;
  font-size:12px!important;
  font-weight:650!important;
  box-shadow:none!important;
}
.results-section .tabs button.active,
.result-toolbar .tabs button.active{
  background:var(--accent-soft)!important;
  border-color:var(--accent)!important;
  color:var(--accent-ink)!important;
  box-shadow:none!important;
}
.results-section .tabs button span,
.result-toolbar .tabs button span{
  margin-left:6px;
  color:#98a2b3;
  font-weight:600;
}
.results-section .tabs button.active span{
  color:var(--accent-ink);
  opacity:.75;
}
.result-toolbar{
  display:flex!important;
  flex-wrap:wrap!important;
  align-items:center!important;
  justify-content:space-between!important;
  gap:12px!important;
  padding:4px 2px 12px!important;
}
.result-toolbar .search{
  min-width:200px;
  flex:0 1 280px;
}
/* 筛选 chip 圆角同步略收 */
.file-filter-bar .chip-btn,
.results-section .chip-btn{
  border-radius:10px!important;
}
/* 注册表关键字输入：边框与全局一致，不要过深 */
.registry-filter-bar .field input,
.registry-filter-bar input[type="search"],
.registry-filter-bar input[type="text"],
.registry-filter-bar .field.grow input,
.registry-page .field input,
.registry-page input[type="search"]{
  border:1px solid var(--u1-border, #e4e7eb)!important;
  border-color:var(--u1-border, #e4e7eb)!important;
  box-shadow:none!important;
  background:color-mix(in srgb,#fff 92%, transparent)!important;
  border-radius:10px!important;
  color:#344054!important;
}
.registry-filter-bar .field input:focus,
.registry-filter-bar input:focus,
.registry-page .field input:focus{
  border-color:color-mix(in srgb, var(--accent) 55%, #d7dce2)!important;
  outline:none!important;
  box-shadow:0 0 0 3px color-mix(in srgb, var(--accent-soft) 80%, transparent)!important;
}
.registry-filter-bar select,
.registry-page .field select{
  border:1px solid var(--u1-border, #e4e7eb)!important;
  border-radius:10px!important;
  background:#fff!important;
}

.cleanup-row.toolai-cleanable{border-style:solid!important}
.cleanup-row.toolai-readonly{border-style:dashed!important;opacity:.96}
.risk-badge.developer{background:#e8fbf4;color:#0f8f6b;border:1px solid #bdebdc}
.model-strong-box{
  margin:14px 0 6px;
  padding:14px 16px;
  border-radius:12px;
  border:1px solid #f0d9a8;
  background:linear-gradient(180deg,#fffdf8,#fff8eb);
  text-align:left;
  display:grid;
  gap:12px;
}
.model-strong-head{display:flex;align-items:baseline;justify-content:space-between;gap:10px;flex-wrap:wrap}
.model-strong-head b{color:#8a5a00;font-size:13px}
.model-strong-head span{color:#b45309;font-size:11px;font-weight:650}
.model-strong-check{
  display:flex;align-items:flex-start;gap:10px;
  color:#5c3d00;font-size:13px;font-weight:650;line-height:1.4;cursor:pointer;
}
.model-strong-check input{
  width:16px;height:16px;margin-top:2px;accent-color:#d97706;flex:none;
}
.model-strong-input{display:grid;gap:8px}
.model-strong-input-label{
  display:flex;align-items:center;justify-content:space-between;gap:10px;flex-wrap:wrap;
  font-size:12px;color:#667085;
}
.model-strong-input-label em{
  font-style:normal;font-weight:700;color:#92400e;
  background:#fff7ed;border:1px dashed #f0d9a8;border-radius:6px;padding:2px 8px;
}
.model-strong-input input{
  height:40px;width:100%;box-sizing:border-box;
  border:1px solid #e4e7eb;border-radius:10px;
  padding:0 12px;color:#1d2939;background:#fff;
  font-size:13px;outline:none;
  transition:border-color .15s ease, box-shadow .15s ease;
}
.model-strong-input input:focus{
  border-color:#f59e0b;box-shadow:0 0 0 3px #fef3c7;
}
.model-strong-input input.ok{
  border-color:#12a47b;box-shadow:0 0 0 3px #d1fae5;background:#f0fdf9;
}
.model-strong-input input.bad{
  border-color:#ef4444;box-shadow:0 0 0 3px #fee2e2;
}

/* cleanup category chips */
.cleanup-list.panel .cleanup-cat-tabs,
.cleanup-cat-tabs{
  display:flex!important;
  flex-wrap:wrap!important;
  justify-content:flex-start!important;
  align-items:center!important;
  gap:10px!important;
  width:100%!important;
  margin:2px 0 12px!important;
  padding:2px 0 10px!important;
  box-sizing:border-box!important;
}
.cleanup-cat-tab{
  height:36px!important;
  min-height:36px!important;
  border-radius:10px!important;
  padding:0 14px!important;
  border:1px solid var(--u1-border-chip, #d7dce2)!important;
  background:color-mix(in srgb,#fff 92%, transparent)!important;
  color:#475467!important;
  display:inline-flex!important;
  align-items:center!important;
  justify-content:center!important;
  gap:6px!important;
  font-size:12px!important;
  font-weight:650!important;
  box-shadow:none!important;
  flex:0 0 auto!important;
  margin:0!important;
  line-height:1!important;
  transition:background .15s ease, border-color .15s ease, color .15s ease, box-shadow .15s ease!important;
}
.cleanup-cat-tab:hover{
  border-color:color-mix(in srgb, var(--accent) 45%, #d7dce2)!important;
  color:var(--accent-ink)!important;
}
.cleanup-cat-tab b{font-weight:650!important;letter-spacing:-.01em}
.cleanup-cat-tab small{
  color:#98a2b3!important;font-weight:650!important;font-size:11px!important;
  min-width:1.1em;text-align:center;
}
.cleanup-cat-check{
  flex:none!important;
  color:var(--accent)!important;
  stroke-width:2.5!important;
}
.cleanup-cat-tab.active{
  background:var(--accent-soft)!important;
  border-color:var(--accent)!important;
  color:var(--accent-ink)!important;
  box-shadow:0 0 0 1px color-mix(in srgb, var(--accent) 25%, transparent), 0 4px 12px color-mix(in srgb, var(--accent) 12%, transparent)!important;
  font-weight:700!important;
}
.cleanup-cat-tab.active b{color:var(--accent-ink)!important}
.cleanup-cat-tab.active small{color:var(--accent-ink)!important;opacity:.8!important}
.cleanup-cat-tab.active .cleanup-cat-check{color:var(--accent)!important}
.cleanup-row.app-row{border-color:color-mix(in srgb,#6366f1 35%, var(--u1-border, #e4e7eb))!important}
/* checkbox selected: clearer check */
.cleanup-row .check-button{
  width:22px!important;height:22px!important;
  border-radius:6px!important;
  border:1.5px solid #d0d5dd!important;
  background:#fff!important;
  display:grid!important;place-items:center!important;
  flex:none!important;
}
.cleanup-row .check-button.checked{
  border-color:var(--accent)!important;
  background:var(--accent)!important;
  color:#fff!important;
  box-shadow:0 2px 8px color-mix(in srgb, var(--accent) 28%, transparent)!important;
}
.cleanup-row .check-button.checked svg{color:#fff!important;stroke:#fff!important}

/* cleanup content shared left inset: match 清理建议 / 固定白名单 */
.cleanup-list.panel{
  --cleanup-inset-x: 14px;
}
.cleanup-list.panel .cleanup-toolbar{
  padding-left: var(--cleanup-inset-x) !important;
  padding-right: var(--cleanup-inset-x) !important;
  box-sizing: border-box !important;
}
.cleanup-list.panel .cleanup-cat-tabs,
.cleanup-cat-tabs{
  padding-left: var(--cleanup-inset-x) !important;
  padding-right: var(--cleanup-inset-x) !important;
  margin-left: 0 !important;
  margin-right: 0 !important;
  width: 100% !important;
  box-sizing: border-box !important;
  justify-content: flex-start !important;
}
.cleanup-list.panel .cleanup-groups{
  padding-left: var(--cleanup-inset-x) !important;
  padding-right: var(--cleanup-inset-x) !important;
}
.cleanup-list.panel .cleanup-footnote{
  padding-left: var(--cleanup-inset-x) !important;
  padding-right: var(--cleanup-inset-x) !important;
}
.cleanup-cat-tab small.bytes{
  color:#98a2b3!important;
  font-size:10px!important;
  font-weight:600!important;
  margin-left:2px;
}
.cleanup-cat-tab.active small.bytes{color:var(--accent-ink)!important;opacity:.75!important}

.cleanup-list.panel{
  padding-left:0!important;
  padding-right:0!important;
}
.cleanup-list.panel .cleanup-toolbar,
.cleanup-list.panel .cleanup-cat-tabs,
.cleanup-list.panel .cleanup-groups,
.cleanup-list.panel .cleanup-footnote,
.cleanup-list.panel .loading-state{
  padding-left:16px!important;
  padding-right:16px!important;
  box-sizing:border-box!important;
}
.cleanup-list.panel .cleanup-cat-tabs{
  padding-top:4px!important;
  padding-bottom:12px!important;
}
.cleanup-list.panel .cleanup-groups{
  padding-top:4px!important;
  padding-bottom:16px!important;
}

/* duplicate groups: no outer panel box; prevent right overflow */
.duplicate-results{
  background:transparent!important;
  border:0!important;
  box-shadow:none!important;
  backdrop-filter:none!important;
  padding:0!important;
  overflow:visible!important;
  max-width:100%!important;
  min-width:0!important;
}
.duplicate-results-head{
  display:flex;
  flex-wrap:wrap;
  align-items:flex-start;
  justify-content:space-between;
  gap:12px;
  padding:4px 2px 14px;
  margin:0 0 4px;
  background:transparent!important;
  border:0!important;
  box-shadow:none!important;
}
.duplicate-results-head h2{margin:4px 0 0;letter-spacing:-.02em}
.duplicate-results-head .file-bulk-actions{
  display:flex;flex-wrap:wrap;gap:8px;align-items:center;justify-content:flex-end;
}
.duplicate-groups{
  display:grid!important;
  gap:14px!important;
  width:100%!important;
  max-width:100%!important;
  min-width:0!important;
  padding:0 2px 12px!important;
  box-sizing:border-box!important;
  overflow:hidden!important;
}
.duplicate-group{
  display:grid!important;
  grid-template-columns:minmax(0,1fr)!important;
  gap:12px!important;
  width:100%!important;
  max-width:100%!important;
  min-width:0!important;
  margin:0!important;
  padding:14px 18px 14px 16px!important;
  border:1px solid var(--u1-border, #e4e7eb)!important;
  border-radius:12px!important;
  background:#fff!important;
  color:#1d2939!important;
  box-shadow:0 4px 14px #10182808!important;
  box-sizing:border-box!important;
  overflow:hidden!important;
}
.duplicate-group-head{
  display:grid!important;
  grid-template-columns:28px minmax(0,1fr) max-content!important;
  gap:10px 12px!important;
  align-items:center!important;
  width:100%!important;
  max-width:100%!important;
  min-width:0!important;
}
.duplicate-group-head>div{min-width:0!important;overflow:hidden!important}
.duplicate-group-head b{
  display:block!important;
  overflow:hidden!important;
  text-overflow:ellipsis!important;
  white-space:nowrap!important;
}
.duplicate-group-head small{
  display:block!important;
  overflow:hidden!important;
  text-overflow:ellipsis!important;
  white-space:nowrap!important;
}
/* 完整显示「可释放 xxx」，禁止截成「可...」 */
.duplicate-group-head strong{
  justify-self:end!important;
  white-space:nowrap!important;
  overflow:visible!important;
  text-overflow:clip!important;
  max-width:none!important;
  font-size:12px!important;
  font-weight:700!important;
  color:var(--accent-ink)!important;
  flex:none!important;
  padding-left:8px!important;
}
.duplicate-paths{
  display:grid!important;
  gap:6px!important;
  width:100%!important;
  max-width:100%!important;
  min-width:0!important;
  overflow:hidden!important;
}
.duplicate-path-row,
.duplicate-paths>div.duplicate-path-row{
  display:grid!important;
  grid-template-columns:22px 18px minmax(0,1fr) max-content 34px!important;
  gap:8px!important;
  align-items:center!important;
  width:100%!important;
  max-width:100%!important;
  min-width:0!important;
  box-sizing:border-box!important;
  padding:8px 6px 8px 8px!important;
  overflow:hidden!important;
  border-radius:8px!important;
}
.duplicate-path-row .dup-path,
.duplicate-path-row>span.dup-path{
  min-width:0!important;
  max-width:100%!important;
  overflow:hidden!important;
  text-overflow:ellipsis!important;
  white-space:nowrap!important;
}
.duplicate-path-row .icon-button{
  width:32px!important;
  height:32px!important;
  margin:0!important;
  justify-self:end!important;
}
.duplicate-path-row .keep-tag{
  justify-self:end!important;
  margin-right:2px!important;
}
.duplicate-path-row .keep-tag-space{
  width:0!important;
  min-width:0!important;
  padding:0!important;
  margin:0!important;
  overflow:hidden!important;
}
.duplicate-group-foot{
  display:flex!important;
  flex-wrap:wrap!important;
  gap:8px 12px!important;
  width:100%!important;
  max-width:100%!important;
  min-width:0!important;
  box-sizing:border-box!important;
  border-top:1px solid #eef0f2!important;
  padding-top:10px!important;
  margin:0!important;
}

.risk-badge.cleanable{background:#e8fbf4;color:#0f8f6b;border:1px solid #bdebdc}

.status-shade .shade-foot{
  display:flex!important;
  flex-wrap:wrap!important;
  gap:8px!important;
  align-items:center!important;
  justify-content:flex-start!important;
}
.status-shade .shade-head-actions .text-button:disabled{
  opacity:.4;cursor:not-allowed;
}
/* 上推时帘跟手更顺 */
.status-shade.dragging{
  transition:none!important;
}

.status-shade .shade-handle{
  height:18px!important;
  
  flex:none!important;
  touch-action:none;
}



.status-shade .shade-head-actions{cursor:default;user-select:auto}

/* shade: android-like sheet, no grab cursor, soft motion */
.pull-edge{
  cursor:default!important;
  touch-action:none;
}
.pull-bar{
  transition:opacity .22s ease, transform .22s cubic-bezier(.22,.9,.3,1)!important;
}
.status-scrim{
  transition:background .28s ease, backdrop-filter .28s ease, opacity .28s ease!important;
}
.status-scrim.dim{
  background:#10182840!important;
  backdrop-filter:blur(3px);
}
.status-shade{
  will-change:transform,opacity;
  transition:transform .32s cubic-bezier(.22,.9,.3,1), opacity .28s ease!important;
}
.status-shade.dragging{
  transition:none!important;
}
.status-shade .shade-handle{
  width:40px!important;
  height:4px!important;
  margin:10px auto 4px!important;
  border-radius:99px!important;
  background:#c5cad3!important;
  cursor:default!important;
  flex:none!important;
  touch-action:none;
  opacity:.9;
}
.status-shade .shade-head,
.status-shade .shade-foot{
  cursor:default!important;
  touch-action:none;
  user-select:none;
}
.status-shade .shade-head-actions,
.status-shade .shade-head-actions *{
  cursor:pointer!important;
  user-select:auto;
  touch-action:auto;
}
/* 底部上滑区：类似安卓通知栏底部空白 */
.status-shade .shade-dismiss{
  flex:1 1 auto;
  min-height:48px;
  touch-action:none;
  cursor:default;
}
.status-shade .status-list{
  flex:0 1 auto;
  max-height:none!important;
}
.status-shade .status-list:empty,
.status-shade .status-list:not(:has(*)){
  display:none;
}
/* 通知空态：不显示虚线框文案 */
.status-shade .status-list .no-matches{
  display:none!important;
}
.status-shade .shade-foot{
  margin-top:0!important;
  border-top:1px solid color-mix(in srgb, var(--u1-border, #edf0f2) 80%, transparent)!important;
}
</style>
