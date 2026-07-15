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
  Wrench,
  X,
} from '@lucide/vue'
import MediaCenter from './MediaCenter.vue'
import RegistryCleaner from './RegistryCleaner.vue'

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
}
interface CleanupReport { items: CleanupItem[]; safeBytes: number; reviewBytes: number }
interface CleanupResult { freedBytes: number; deletedFiles: number; failedItems: number }
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
  largeFileMb: number
  scanThreads: number
  snapshotLimit: number
  reportDirectory: string
  recyclePolicy: 'confirm' | 'direct'
  autoCheckUpdates: boolean
}

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
const settingsTab = ref<'appearance' | 'scanning' | 'system' | 'about'>('appearance')
const fontScale = ref<FontScale>('standard')
const iconScale = ref<IconScale>('standard')
const uiDensity = ref<UiDensity>('comfortable')
const exclusionPaths = ref<string[]>([])
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
const scanning = ref(false)
const folderAnalyzing = ref(false)
const cleaning = ref(false)
const loadingDrives = ref(true)
const loadingCleanup = ref(false)
const page = ref<'overview' | 'cleanup' | 'files' | 'insights' | 'media' | 'registry'>('overview')
const analysisTab = ref<'duplicates' | 'history' | 'age'>('duplicates')
const fileTab = ref<'directories' | 'files'>('directories')
const query = ref('')
const error = ref('')
const notice = ref('')
const selectedCleanup = ref<string[]>([])
const confirmCleanup = ref(false)
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
let unlisten: UnlistenFn | undefined
let unlistenFolder: UnlistenFn | undefined
let unlistenDuplicate: UnlistenFn | undefined

const currentUsage = computed(() => result.value?.usage ?? usage.value)
const isSystemDrive = computed(() => selectedDrive.value.toUpperCase() === 'C:')
const largestDirectory = computed(() => result.value?.directories[0] ?? null)
const displayCategories = computed(() => isSystemDrive.value ? result.value?.categories ?? [] : result.value?.fileTypes ?? [])
const usedPercent = computed(() => currentUsage.value?.total ? Math.round(currentUsage.value.used / currentUsage.value.total * 100) : 0)
const selectedCleanupItems = computed(() => cleanup.value?.items.filter(item => selectedCleanup.value.includes(item.id)) ?? [])
const selectedCleanupBytes = computed(() => selectedCleanupItems.value.reduce((sum, item) => sum + item.size, 0))
const safeItems = computed(() => cleanup.value?.items.filter(item => item.action === 'safe' && item.size > 0) ?? [])
const allSafeSelected = computed(() => safeItems.value.length > 0 && safeItems.value.every(item => selectedCleanup.value.includes(item.id)))
const filteredDirectories = computed(() => {
  const needle = query.value.trim().toLocaleLowerCase()
  return result.value?.directories.filter(item => !needle || `${item.name} ${item.path}`.toLocaleLowerCase().includes(needle)) ?? []
})
const filteredFiles = computed(() => {
  const needle = query.value.trim().toLocaleLowerCase()
  return result.value?.largeFiles.filter(item => !needle || `${item.name} ${item.path}`.toLocaleLowerCase().includes(needle)) ?? []
})
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
const latestSnapshot = computed(() => snapshots.value[snapshots.value.length - 1] ?? null)
const previousSnapshot = computed(() => snapshots.value[snapshots.value.length - 2] ?? null)
const snapshotDelta = computed(() => latestSnapshot.value && previousSnapshot.value ? latestSnapshot.value.used - previousSnapshot.value.used : 0)
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
function handleError(value: unknown) { error.value = typeof value === 'string' ? value : value instanceof Error ? value.message : '操作未能完成' }
function applyTheme(theme: ThemeId) {
  activeTheme.value = theme
  document.documentElement.dataset.accent = theme
  localStorage.setItem('disk-analyzer-theme', theme)
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
      if (!detected.length) notice.value = '盘符检测响应超时，已使用系统盘 C:，不影响扫描。'
    } else drives.value = ['C:', 'D:', 'E:']
    if (!drives.value.includes(selectedDrive.value)) selectedDrive.value = drives.value[0]
  } catch (value) { handleError(value) }
  finally { loadingDrives.value = false }
  await refreshUsage()
}

async function loadCleanup() {
  if (!isTauri) {
    if (!isSystemDrive.value) {
      cleanup.value = null
      selectedCleanup.value = []
      return
    }
    cleanup.value = {
      safeBytes: 4_563_402_752,
      reviewBytes: 12_884_901_888,
      items: [
        { id: 'user-temp', name: '用户临时文件', description: '应用安装、解压和运行产生的过期临时文件', path: 'C:\\Users\\User\\AppData\\Local\\Temp', size: 1_827_160_064, fileCount: 2841, action: 'safe', risk: 'low' },
        { id: 'browser-cache', name: '浏览器缓存', description: 'Chrome 与 Edge 可重新下载的网页缓存，清理前建议关闭浏览器', path: 'C:\\Users\\User\\AppData\\Local\\Google\\Chrome\\User Data\\Default\\Cache', size: 2_521_653_248, fileCount: 1743, action: 'safe', risk: 'low' },
        { id: 'crash-dumps', name: '程序崩溃转储', description: '用于故障诊断的旧转储文件，不影响程序正常运行', path: 'C:\\Users\\User\\AppData\\Local\\CrashDumps', size: 214_589_440, fileCount: 8, action: 'safe', risk: 'low' },
        { id: 'large-downloads', name: '下载目录大文件', description: '下载目录中超过 100 MB 的内容，需要确认用途后手动处理', path: 'C:\\Users\\User\\Downloads', size: 12_884_901_888, fileCount: 11, action: 'review', risk: 'medium' },
        { id: 'windows-storage', name: 'Windows 系统清理', description: '更新缓存、旧系统文件和回收站应交给 Windows 存储设置处理', path: 'Windows 设置 > 系统 > 存储 > 临时文件', size: 0, fileCount: 0, action: 'system', risk: 'medium' },
      ],
    }
    selectedCleanup.value = cleanup.value.items.filter(item => item.action === 'safe').map(item => item.id)
    return
  }
  if (selectedDrive.value[0].toUpperCase() !== 'C') return
  loadingCleanup.value = true
  try {
    cleanup.value = await invoke<CleanupReport>('analyze_cleanup')
    selectedCleanup.value = cleanup.value.items.filter(item => item.action === 'safe' && item.size > 0).map(item => item.id)
  } catch (value) { handleError(value) }
  finally { loadingCleanup.value = false }
}

async function selectDrive(drive: string) {
  selectedDrive.value = drive
  result.value = null
  cleanup.value = null
  selectedCleanup.value = []
  folderAnalysis.value = null
  folderHistory.value = []
  page.value = 'overview'
  await refreshUsage()
  await loadCleanup()
  await loadSnapshots()
}

async function startScan() {
  if (!isTauri) {
    result.value = buildPreviewScan(selectedDrive.value)
    usage.value = result.value.usage
    await loadSnapshots()
    notice.value = '当前显示界面预览数据；Tauri 程序会读取真实磁盘。'
    return
  }
  scanning.value = true
  result.value = null
  error.value = ''
  notice.value = ''
  query.value = ''
  progress.value = { message: '正在启动完整扫描', percentage: 1 }
  try {
    result.value = await invoke<ScanResult>('start_scan', { drive: selectedDrive.value, options: scanOptions.value })
    usage.value = result.value.usage
    try {
      await invoke<ScanSnapshot>('save_snapshot', { result: result.value, limit: snapshotLimit.value })
      await loadSnapshots()
    } catch (snapshotError) {
      notice.value = `扫描完成，但历史快照保存失败：${String(snapshotError)}`
    }
    await loadCleanup()
  } catch (value) {
    if (String(value).includes('扫描已取消')) notice.value = '扫描已取消，没有改动任何文件。'
    else handleError(value)
  } finally { scanning.value = false }
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
    if (String(value).includes('已取消')) notice.value = '重复文件检测已取消。'
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

async function exportReport() {
  if (!result.value) return
  try { notice.value = `报告已保存到 ${await invoke<string>('export_report', { result: result.value, outputDirectory: reportDirectory.value || null })}` }
  catch (value) { handleError(value) }
}

async function openStorageSettings() {
  try { await invoke('open_storage_settings') } catch (value) { handleError(value) }
}

function toggleCleanup(id: string) {
  selectedCleanup.value = selectedCleanup.value.includes(id)
    ? selectedCleanup.value.filter(value => value !== id)
    : [...selectedCleanup.value, id]
}

function toggleAllSafe() {
  selectedCleanup.value = allSafeSelected.value ? [] : safeItems.value.map(item => item.id)
}

async function runCleanup() {
  if (!selectedCleanup.value.length) return
  cleaning.value = true
  error.value = ''
  try {
    const cleaned = await invoke<CleanupResult>('clean_items', { ids: selectedCleanup.value })
    confirmCleanup.value = false
    notice.value = `已释放 ${formatSize(cleaned.freedBytes)}，删除 ${formatCount(cleaned.deletedFiles)} 个文件${cleaned.failedItems ? `，跳过 ${formatCount(cleaned.failedItems)} 个占用或无权限项目` : ''}。`
    await Promise.all([loadCleanup(), refreshUsage()])
  } catch (value) { handleError(value) }
  finally { cleaning.value = false }
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
      { path: `${path}\\node_modules`, name: 'node_modules', size: 21_796_126_720, fileCount: 124830, dirCount: 12884, kind: 'directory', risk: 'rebuildable', recommendation: '通常可以重新生成；关闭相关应用并确认项目不在使用后再处理' },
      { path: `${path}\\assets`, name: 'assets', size: 12_347_883_520, fileCount: 4280, dirCount: 312, kind: 'directory', risk: 'review', recommendation: '可能包含个人或项目数据，请先打开检查内容和最近修改时间' },
      { path: `${path}\\target`, name: 'target', size: 9_985_835_008, fileCount: 51480, dirCount: 7920, kind: 'directory', risk: 'rebuildable', recommendation: '通常可以重新生成；关闭相关应用并确认项目不在使用后再处理' },
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
    if (String(value).includes('已取消')) notice.value = '文件夹分析已取消。'
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
    notice.value = `已清除 ${formatCount(removed)} 条本地扫描快照。`
  } catch (value) { handleError(value) }
  finally { settingsBusy.value = '' }
}

async function exportDiagnostics() {
  settingsBusy.value = 'diagnostics'
  try {
    if (!isTauri) { notice.value = '界面预览：诊断信息将保存到所选报告目录。'; return }
    const path = await invoke<string>('export_diagnostics', {
      outputDirectory: reportDirectory.value || null,
      settings: advancedSettings(),
    })
    notice.value = `诊断信息已保存到 ${path}`
  } catch (value) { handleError(value) }
  finally { settingsBusy.value = '' }
}

async function checkUpdates(silent = false) {
  settingsBusy.value = 'update'
  try {
    updateStatus.value = isTauri
      ? await invoke<UpdateStatus>('check_for_updates', { repository: 'sonemeng/disk-space-analyzer' })
      : { currentVersion: '6.1.0', latestVersion: null, available: false, message: '仓库发布后即可检查更新' }
    if (!silent) notice.value = updateStatus.value.message
  } catch (value) {
    updateStatus.value = { currentVersion: '6.1.0', available: false, message: String(value) }
    if (!silent) handleError(value)
  } finally { settingsBusy.value = '' }
}

watch(
  [exclusionPaths, largeFileMb, scanThreads, snapshotLimit, reportDirectory, recyclePolicy, autoCheckUpdates],
  persistAdvancedSettings,
  { deep: true },
)

onMounted(async () => {
  loadAdvancedSettings()
  const savedTheme = localStorage.getItem('disk-analyzer-theme') as ThemeId | null
  applyTheme(themeOptions.some(theme => theme.id === savedTheme) ? savedTheme! : 'ocean')
  const savedFont = localStorage.getItem('disk-analyzer-font-scale') as FontScale | null
  const savedIcon = localStorage.getItem('disk-analyzer-icon-scale') as IconScale | null
  const savedDensity = localStorage.getItem('disk-analyzer-density') as UiDensity | null
  applyFontScale(savedFont && ['small', 'standard', 'large'].includes(savedFont) ? savedFont : 'standard')
  applyIconScale(savedIcon && ['compact', 'standard', 'large'].includes(savedIcon) ? savedIcon : 'standard')
  applyDensity(savedDensity && ['compact', 'comfortable'].includes(savedDensity) ? savedDensity : 'comfortable')
  sidebarCollapsed.value = localStorage.getItem('disk-analyzer-sidebar-collapsed') === 'true'
  if (isTauri) {
    unlisten = await listen<ScanProgress>('scan-progress', event => { progress.value = event.payload })
    unlistenFolder = await listen<ScanProgress>('folder-progress', event => { folderProgress.value = event.payload })
    unlistenDuplicate = await listen<ScanProgress>('duplicate-progress', event => { duplicateProgress.value = event.payload })
  }
  await loadDrives()
  await loadCleanup()
  await loadSnapshots()
  if (autoCheckUpdates.value) await checkUpdates(true)
})
onBeforeUnmount(() => { unlisten?.(); unlistenFolder?.(); unlistenDuplicate?.() })
</script>

<template>
  <div class="app-shell" :class="{ collapsed: sidebarCollapsed }">
    <aside class="sidebar" :class="{ collapsed: sidebarCollapsed }">
      <div class="sidebar-head"><div class="brand"><span class="brand-mark"><HardDrive :size="20" /></span><div><strong>磁盘空间分析器</strong><small>空间诊断与安全清理</small></div></div><button class="collapse-button" :title="sidebarCollapsed ? '展开侧栏' : '折叠侧栏'" @click="toggleSidebar"><PanelLeftOpen v-if="sidebarCollapsed" :size="17" /><PanelLeftClose v-else :size="17" /></button></div>

      <nav class="main-nav" aria-label="主要功能">
        <button title="空间概览" :class="{ active: page === 'overview' }" @click="page = 'overview'"><LayoutDashboard :size="17" /><span>空间概览</span></button>
        <button title="清理中心" :class="{ active: page === 'cleanup' }" @click="page = 'cleanup'"><Trash2 :size="17" /><span>清理中心</span><b v-if="cleanup?.safeBytes">{{ formatSize(cleanup.safeBytes) }}</b></button>
        <button title="文件审查" :class="{ active: page === 'files' }" @click="page = 'files'"><FileSearch :size="17" /><span>文件审查</span><b v-if="result">{{ result.largeFiles.length }}</b></button>
        <button title="深度分析" :class="{ active: page === 'insights' }" @click="page = 'insights'"><ChartNoAxesCombined :size="17" /><span>深度分析</span></button>
        <button title="媒体管理" :class="{ active: page === 'media' }" @click="openMediaCenter"><Library :size="17" /><span>媒体管理</span><b v-if="mediaNew">NEW</b></button>
        <button title="注册表检查" :class="{ active: page === 'registry' }" @click="page = 'registry'"><Database :size="17" /><span>注册表检查</span></button>
      </nav>

      <div class="sidebar-label sidebar-drive-title"><span>本机磁盘</span><button title="重新检测磁盘" :disabled="loadingDrives" @click="loadDrives"><RefreshCw :size="12" :class="{ spin: loadingDrives }" /></button></div>
      <div class="drive-list" :aria-busy="loadingDrives">
        <button v-for="drive in drives" :key="drive" class="drive-button" :title="`本地磁盘 ${drive}`" :class="{ active: selectedDrive === drive }" :disabled="scanning" @click="selectDrive(drive)">
          <HardDrive :size="16" /><span><b>本地磁盘 {{ drive }}</b><small>{{ drive }}\</small></span><i v-if="selectedDrive === drive" />
        </button>
        <div v-if="loadingDrives" class="drive-loading"><RefreshCw :size="15" class="spin" /> 正在检测磁盘</div>
        <div v-else-if="!drives.length" class="drive-loading">未检测到磁盘</div>
      </div>

      <div class="sidebar-spacer" />
      <button class="settings-trigger" title="设置" @click="showSettings = true"><Settings :size="17" /><span><b>设置</b><small>{{ themeOptions.find(theme => theme.id === activeTheme)?.name }} · {{ fontScale === 'large' ? '大字号' : fontScale === 'small' ? '小字号' : '标准字号' }}</small></span><ChevronRight :size="15" /></button>
      <div class="safety-note"><ShieldCheck :size="17" /><div><b>默认只读</b><span>只有低风险白名单项目可在确认后清理</span></div></div>
      <div v-if="!isTauri" class="preview-badge"><Info :size="14" /> 界面预览</div>
      <div class="version">TAURI EDITION · 6.1</div>
    </aside>

    <main class="workspace">
      <header class="topbar">
        <div><div class="eyebrow">{{ page === 'media' ? '本地媒体' : page === 'registry' ? 'Windows 当前用户' : `${selectedDrive}\\` }} {{ pageTitle }}</div><h1>{{ pageTitle }}</h1></div>
        <div class="actions">
          <button v-if="result && page === 'overview'" class="button secondary" @click="exportReport"><Download :size="17" /> 导出报告</button>
          <button v-if="page === 'files'" class="button secondary" :disabled="folderAnalyzing" @click="chooseFolder"><FolderSearch :size="17" /> 选择文件夹</button>
          <button v-if="scanning || folderAnalyzing || duplicateScanning" class="button danger" @click="cancelScan"><CircleStop :size="17" /> 取消分析</button>
          <button v-else-if="page !== 'media' && page !== 'registry'" class="button primary" :disabled="!selectedDrive" @click="startScan"><Play :size="17" fill="currentColor" /> {{ result ? '重新扫描' : '完整扫描' }}</button>
        </div>
      </header>

      <div v-if="error" class="alert error"><AlertTriangle :size="17" /><span>{{ error }}</span><button aria-label="关闭" @click="error = ''"><X :size="16" /></button></div>
      <div v-if="notice" class="alert notice"><Check :size="17" /><span>{{ notice }}</span><button aria-label="关闭" @click="notice = ''"><X :size="16" /></button></div>

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
          <div v-if="isSystemDrive" class="metric"><div class="metric-icon green"><Sparkles :size="19" /></div><div><span>建议安全清理</span><strong>{{ loadingCleanup ? '分析中' : formatSize(cleanup?.safeBytes ?? 0) }}</strong><small>仅临时文件与可重建缓存</small></div></div>
          <div v-else class="metric"><div class="metric-icon green"><FolderTree :size="19" /></div><div><span>最大目录</span><strong>{{ largestDirectory ? formatSize(largestDirectory.size) : '—' }}</strong><small>{{ largestDirectory?.name ?? '完成扫描后显示' }}</small></div></div>
          <div class="metric"><div class="metric-icon amber"><FileText :size="19" /></div><div><span>已扫描文件</span><strong>{{ result ? formatCount(result.scannedFiles) : '—' }}</strong><small>{{ result ? `${formatCount(result.scannedDirs)} 个目录` : '完整扫描后显示' }}</small></div></div>
        </section>

        <section v-if="isSystemDrive && cleanup?.safeBytes" class="reclaim-band">
          <div class="reclaim-icon"><Sparkles :size="22" /></div>
          <div><span>发现可安全释放空间</span><strong>{{ formatSize(cleanup.safeBytes) }}</strong><small>已排除个人文档和系统关键文件</small></div>
          <button class="button primary" @click="page = 'cleanup'">查看清理项 <ChevronRight :size="16" /></button>
        </section>

        <template v-if="result">
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

          <section class="panel insight-panel">
            <div class="panel-heading"><div><span class="panel-kicker">优先级建议</span><h2>下一步操作</h2></div></div>
            <div v-if="isSystemDrive" class="insight-grid">
              <button @click="page = 'cleanup'"><span class="insight-icon green"><Trash2 :size="18" /></span><div><b>清理低风险缓存</b><small>预计释放 {{ formatSize(cleanup?.safeBytes ?? 0) }}</small></div><ChevronRight :size="16" /></button>
              <button @click="page = 'files'; fileTab = 'files'"><span class="insight-icon amber"><FileSearch :size="18" /></span><div><b>复核大文件</b><small>{{ result.largeFiles.length }} 个超过 100 MB 的文件</small></div><ChevronRight :size="16" /></button>
              <button @click="openStorageSettings"><span class="insight-icon blue"><Settings :size="18" /></span><div><b>运行系统清理</b><small>更新缓存与旧系统文件交给 Windows</small></div><ChevronRight :size="16" /></button>
              <button @click="chooseFolder"><span class="insight-icon neutral"><FolderSearch :size="18" /></span><div><b>分析指定文件夹</b><small>直接查看任意目录的子项占用</small></div><ChevronRight :size="16" /></button>
            </div>
            <div v-else class="insight-grid data-drive-actions">
              <button :disabled="!largestDirectory" @click="largestDirectory && analyzeFolder(largestDirectory.path)"><span class="insight-icon green"><FolderTree :size="18" /></span><div><b>深入最大目录</b><small>{{ largestDirectory ? `${largestDirectory.name} · ${formatSize(largestDirectory.size)}` : '等待扫描结果' }}</small></div><ChevronRight :size="16" /></button>
              <button @click="page = 'files'; fileTab = 'files'"><span class="insight-icon amber"><FileSearch :size="18" /></span><div><b>复核大文件</b><small>{{ result.largeFiles.length }} 个超过 100 MB 的文件</small></div><ChevronRight :size="16" /></button>
              <button @click="chooseFolder"><span class="insight-icon blue"><FolderSearch :size="18" /></span><div><b>分析指定文件夹</b><small>选择目录并逐层查看空间占用</small></div><ChevronRight :size="16" /></button>
              <button @click="openPath(`${selectedDrive}\\`)"><span class="insight-icon neutral"><FolderOpen :size="18" /></span><div><b>浏览磁盘根目录</b><small>在资源管理器中打开 {{ selectedDrive }}\</small></div><ChevronRight :size="16" /></button>
            </div>
          </section>
        </template>

        <section v-else-if="!scanning" class="empty-state">
          <div class="empty-visual"><HardDrive :size="42" /><span><Search :size="20" /></span></div>
          <h2>完整分析 {{ selectedDrive }} 的空间占用</h2>
          <p>逐文件读取真实大小，不再使用超时估算。扫描期间只读取元数据，不会修改文件。</p>
          <button class="button primary" @click="startScan"><Play :size="17" fill="currentColor" /> 开始完整扫描</button>
        </section>
      </template>

      <template v-else-if="page === 'cleanup'">
        <section class="cleanup-hero">
          <div><span class="panel-kicker">可回收空间</span><strong>{{ loadingCleanup ? '正在分析…' : formatSize(cleanup?.safeBytes ?? 0) }}</strong><p>仅统计超过 24 小时的临时文件和可重新生成的缓存。</p></div>
          <div class="cleanup-breakdown"><div><span>可自动处理</span><b>{{ formatSize(cleanup?.safeBytes ?? 0) }}</b></div><div><span>建议人工复核</span><b>{{ formatSize(cleanup?.reviewBytes ?? 0) }}</b></div><div><span>当前选择</span><b>{{ formatSize(selectedCleanupBytes) }}</b></div></div>
        </section>

        <section class="cleanup-list panel">
          <div class="cleanup-toolbar">
            <div><h2>清理建议</h2><p>系统关键内容不会提供直接删除按钮</p></div>
            <div class="cleanup-actions"><button class="text-button" :disabled="!safeItems.length" @click="toggleAllSafe">{{ allSafeSelected ? '取消全选' : '选择全部安全项' }}</button><button class="button primary" :disabled="!selectedCleanup.length || cleaning" @click="confirmCleanup = true"><Trash2 :size="16" /> 清理所选 · {{ formatSize(selectedCleanupBytes) }}</button></div>
          </div>

          <div v-if="loadingCleanup" class="loading-state"><LoaderCircle :size="24" class="spin" /> 正在计算可释放空间</div>
          <div v-else class="cleanup-rows">
            <div v-for="item in cleanup?.items" :key="item.id" class="cleanup-row">
              <button v-if="item.action === 'safe'" class="check-button" :class="{ checked: selectedCleanup.includes(item.id) }" :disabled="item.size === 0" :aria-label="`选择${item.name}`" @click="toggleCleanup(item.id)"><Check v-if="selectedCleanup.includes(item.id)" :size="14" /></button>
              <span v-else class="action-symbol" :class="item.action"><AlertTriangle v-if="item.action === 'review'" :size="17" /><Settings v-else :size="17" /></span>
              <div class="cleanup-copy"><div><b>{{ item.name }}</b><span class="risk-badge" :class="item.action">{{ item.action === 'safe' ? '低风险' : item.action === 'review' ? '需复核' : '系统工具' }}</span></div><p>{{ item.description }}</p><small :title="item.path">{{ item.path }}</small></div>
              <div class="cleanup-size"><b>{{ item.size ? formatSize(item.size) : '—' }}</b><span>{{ item.fileCount ? `${formatCount(item.fileCount)} 个文件` : item.action === 'system' ? '由 Windows 评估' : '暂无可清理项' }}</span></div>
              <button v-if="item.action === 'review'" class="button secondary compact" @click="openPath(item.path)"><FolderOpen :size="15" /> 查看</button>
              <button v-else-if="item.action === 'system'" class="button secondary compact" @click="openStorageSettings"><Settings :size="15" /> 打开设置</button>
              <div v-else class="row-space" />
            </div>
          </div>
          <div class="cleanup-footnote"><ShieldCheck :size="16" /><span>安全清理采用固定路径白名单。正在使用、24 小时内更新或权限不足的文件会被保留。</span></div>
        </section>
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
                <div class="folder-row-actions"><button v-if="item.kind === 'directory'" class="button secondary compact" @click="analyzeFolder(item.path)"><FolderSearch :size="15" /> 分析</button><button class="icon-button" :title="item.kind === 'file' ? '在资源管理器中定位' : '在资源管理器中打开'" @click="openPath(item.path, item.kind === 'file')"><ExternalLink :size="16" /></button></div>
              </div>
              <div v-if="!folderAnalysis.children.length" class="no-matches">此文件夹没有可读取的子项</div>
            </div>
            <div class="cleanup-footnote folder-note"><Info :size="16" /><span>“可重建内容”是保守建议，不会自动删除。项目依赖、构建产物和缓存应在确认不再使用后，通过对应工具清理。</span></div>
          </section>
        </template>

        <template v-else>
          <section class="folder-picker-band">
            <span><FolderSearch :size="21" /></span><div><b>分析指定文件夹</b><small>选择任意目录，查看直接子项的递归大小并继续逐层下钻</small></div><button class="button secondary" :disabled="folderAnalyzing" @click="chooseFolder"><FolderOpen :size="16" /> 选择文件夹</button>
          </section>
        <section v-if="result" class="results-section">
          <div class="result-toolbar">
            <div class="tabs"><button :class="{ active: fileTab === 'directories' }" @click="fileTab = 'directories'">目录排行 <span>{{ result.directories.length }}</span></button><button :class="{ active: fileTab === 'files' }" @click="fileTab = 'files'">大文件 <span>{{ result.largeFiles.length }}</span></button></div>
            <label class="search"><Search :size="16" /><input v-model="query" :placeholder="fileTab === 'directories' ? '筛选目录' : '筛选文件'" /></label>
          </div>
          <div v-if="fileTab === 'directories'" class="table-wrap"><table><thead><tr><th class="rank">#</th><th>目录</th><th>大小</th><th>已用占比</th><th>文件 / 子目录</th><th class="open-col">操作</th></tr></thead><tbody><tr v-for="(item, index) in filteredDirectories" :key="item.path" @dblclick="analyzeFolder(item.path)"><td class="rank">{{ index + 1 }}</td><td><div class="path-cell"><FolderOpen :size="18" /><div><b>{{ item.name }}</b><span :title="item.path">{{ item.path }}</span></div></div></td><td><strong>{{ formatSize(item.size) }}</strong></td><td><div class="share"><span>{{ currentUsage?.used ? (item.size / currentUsage.used * 100).toFixed(1) : '0.0' }}%</span><i><em :class="{ warm: currentUsage?.used && item.size / currentUsage.used > .05, hot: currentUsage?.used && item.size / currentUsage.used > .1 }" :style="{ width: `${item.size / maxDirectorySize * 100}%` }" /></i></div></td><td class="muted">{{ formatCount(item.fileCount) }} / {{ formatCount(item.dirCount) }}</td><td><div class="table-actions"><button class="icon-button" title="分析此文件夹" @click="analyzeFolder(item.path)"><FolderSearch :size="16" /></button><button class="icon-button" title="在资源管理器中打开" @click="openPath(item.path)"><ExternalLink :size="16" /></button></div></td></tr></tbody></table><div v-if="!filteredDirectories.length" class="no-matches">没有匹配的目录</div></div>
          <div v-else class="table-wrap"><table><thead><tr><th class="rank">#</th><th>文件</th><th>大小</th><th>位置</th><th class="open-col"></th></tr></thead><tbody><tr v-for="(item, index) in filteredFiles" :key="item.path" @dblclick="openPath(item.path, true)"><td class="rank">{{ index + 1 }}</td><td><div class="path-cell file"><FileText :size="18" /><div><b>{{ item.name }}</b></div></div></td><td><strong>{{ formatSize(item.size) }}</strong></td><td class="muted path-text" :title="item.path">{{ item.path }}</td><td><button class="icon-button" title="在资源管理器中定位" @click="openPath(item.path, true)"><ExternalLink :size="16" /></button></td></tr></tbody></table><div v-if="!filteredFiles.length" class="no-matches">没有找到超过 100 MB 的大文件</div></div>
        </section>
        <section v-else class="empty-state"><div class="empty-visual"><FolderSearch :size="42" /></div><h2>选择一个文件夹开始分析</h2><p>无需等待整盘扫描，可以直接查看指定目录内每个子项的真实占用。</p><div class="empty-actions"><button class="button primary" @click="chooseFolder"><FolderOpen :size="17" /> 选择文件夹</button><button class="button secondary" @click="page = 'overview'; startScan()"><Play :size="17" fill="currentColor" /> 扫描整个磁盘</button></div></section>
        </template>
      </template>

      <template v-else>
        <section class="analysis-tabs panel">
          <button :class="{ active: analysisTab === 'duplicates' }" @click="analysisTab = 'duplicates'"><Fingerprint :size="17" /><span><b>重复文件</b><small>SHA-256 内容校验</small></span></button>
          <button :class="{ active: analysisTab === 'history' }" @click="analysisTab = 'history'; loadSnapshots()"><History :size="17" /><span><b>空间趋势</b><small>本地扫描快照</small></span></button>
          <button :class="{ active: analysisTab === 'age' }" @click="analysisTab = 'age'"><CalendarClock :size="17" /><span><b>文件年龄</b><small>最近修改热力图</small></span></button>
        </section>

        <template v-if="analysisTab === 'duplicates'">
          <section class="analysis-toolbar panel">
            <div><span class="panel-kicker">检测范围</span><h2>{{ duplicateReport?.scope ?? `${selectedDrive}\\` }}</h2><p>先按大小预筛，再读取候选文件计算 SHA-256；结果只用于定位，不会自动删除。</p></div>
            <div class="duplicate-controls"><div class="size-segments" aria-label="最小文件大小"><button v-for="size in [1, 10, 100]" :key="size" :class="{ active: duplicateMinSize === size * 1024 * 1024 }" @click="duplicateMinSize = size * 1024 * 1024">{{ size }} MB</button></div><button class="button secondary" :disabled="duplicateScanning" @click="chooseDuplicateFolder"><FolderSearch :size="16" /> 选择文件夹</button><button class="button primary" :disabled="duplicateScanning" @click="scanDuplicates()"><Fingerprint :size="16" /> 检测 {{ selectedDrive }}</button></div>
          </section>
          <template v-if="duplicateReport">
            <section class="analysis-metrics">
              <div><span>可避免占用</span><b>{{ formatSize(duplicateReport.wastedBytes) }}</b><small>保留每组一个副本</small></div><div><span>重复组</span><b>{{ duplicateReport.groups.length }}</b><small>{{ duplicateReport.duplicateFiles }} 个重复文件</small></div><div><span>哈希文件</span><b>{{ formatCount(duplicateReport.hashedFiles) }}</b><small>预筛 {{ formatCount(duplicateReport.scannedFiles) }} 个</small></div><div><span>检测用时</span><b>{{ (duplicateReport.elapsedMs / 1000).toFixed(1) }} 秒</b><small>跳过 {{ formatCount(duplicateReport.skippedItems) }} 项</small></div>
            </section>
            <section class="duplicate-results panel">
              <div class="panel-heading"><div><span class="panel-kicker">内容完全一致</span><h2>重复文件组</h2></div><small>最多显示 200 组</small></div>
              <div class="duplicate-groups"><article v-for="(group, index) in duplicateReport.groups" :key="group.hash" class="duplicate-group"><div class="duplicate-group-head"><span>{{ index + 1 }}</span><div><b>每个 {{ formatSize(group.size) }}</b><small>SHA-256 · {{ group.hash.slice(0, 20) }}…</small></div><strong>可释放 {{ formatSize(group.wastedBytes) }}</strong></div><div class="duplicate-paths"><div v-for="file in group.files" :key="file"><FileText :size="15" /><span :title="file">{{ file }}</span><button class="icon-button" title="在资源管理器中定位" @click="openPath(file, true)"><ExternalLink :size="15" /></button></div></div></article><div v-if="!duplicateReport.groups.length" class="no-matches">当前范围没有发现符合条件的重复文件</div></div>
              <div class="cleanup-footnote"><ShieldCheck :size="16" /><span>哈希相同只代表内容一致。删除前仍需确认文件用途、备份策略和应用引用关系。</span></div>
            </section>
          </template>
          <section v-else-if="!duplicateScanning" class="empty-state analysis-empty"><div class="empty-visual"><Fingerprint :size="40" /></div><h2>查找内容完全相同的文件</h2><p>建议先用 100 MB 检测整盘，再对可疑文件夹使用 10 MB 或 1 MB。</p><button class="button primary" @click="scanDuplicates()"><Fingerprint :size="17" /> 检测当前磁盘</button></section>
        </template>

        <template v-else-if="analysisTab === 'history'">
          <section v-if="snapshots.length" class="history-summary panel">
            <div class="panel-heading"><div><span class="panel-kicker">{{ selectedDrive }} 本地快照</span><h2>已用空间趋势</h2></div><div class="history-delta" :class="{ down: snapshotDelta < 0 }"><span>较上次</span><b>{{ snapshotDelta >= 0 ? '+' : '−' }}{{ formatSize(Math.abs(snapshotDelta)) }}</b></div></div>
            <div class="trend-chart"><div v-for="snapshot in snapshots" :key="snapshot.id" class="trend-column"><div class="trend-value">{{ Math.round(snapshot.used / snapshot.total * 100) }}%</div><div class="trend-track"><i :style="{ height: `${snapshot.used / snapshot.total * 100}%` }" /></div><span>{{ new Date(snapshot.createdAt).toLocaleDateString('zh-CN', { month: 'numeric', day: 'numeric' }) }}</span></div></div>
            <div class="history-list"><div v-for="snapshot in [...snapshots].reverse().slice(0, 6)" :key="snapshot.id"><span>{{ new Date(snapshot.createdAt).toLocaleString('zh-CN', { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' }) }}</span><b>{{ formatSize(snapshot.used) }}</b><small>{{ formatCount(snapshot.scannedFiles) }} 个文件</small><em>{{ snapshot.directories[0]?.name ?? '—' }} · {{ formatSize(snapshot.directories[0]?.size ?? 0) }}</em></div></div>
            <div class="cleanup-footnote"><Info :size="16" /><span>每次完整扫描后自动保存一个快照，每个盘最多保留 {{ snapshotLimit }} 次，数据仅保存在本机。</span></div>
          </section>
          <section v-else-if="!snapshotsLoading" class="empty-state analysis-empty"><div class="empty-visual"><History :size="40" /></div><h2>还没有 {{ selectedDrive }} 的历史快照</h2><p>完成一次整盘扫描后会自动记录；至少两次扫描才能看到空间变化。</p><button class="button primary" @click="page = 'overview'; startScan()"><Play :size="17" /> 开始完整扫描</button></section>
          <div v-else class="loading-state"><LoaderCircle :size="24" class="spin" /> 正在读取本地快照</div>
        </template>

        <template v-else>
          <template v-if="result">
            <section class="age-overview panel">
              <div class="panel-heading"><div><span class="panel-kicker">最近修改时间</span><h2>文件年龄热力图</h2></div><small>已统计 {{ formatSize(ageTotal) }}</small></div>
              <div class="age-stack"><i v-for="bucket in result.ageBuckets" :key="bucket.id" :style="{ width: `${ageTotal ? bucket.size / ageTotal * 100 : 0}%`, background: bucket.color }" :title="`${bucket.label}：${formatSize(bucket.size)}`" /></div>
              <div class="age-grid"><div v-for="bucket in result.ageBuckets" :key="bucket.id" :class="bucket.id"><span :style="{ background: bucket.color }" /><b>{{ bucket.label }}</b><strong>{{ formatSize(bucket.size) }}</strong><small>{{ formatCount(bucket.fileCount) }} 个文件 · {{ ageTotal ? (bucket.size / ageTotal * 100).toFixed(1) : '0.0' }}%</small><i><em :style="{ width: `${bucket.size / maxAgeSize * 100}%`, background: bucket.color }" /></i></div></div>
            </section>
            <section class="old-files panel">
              <div class="panel-heading"><div><span class="panel-kicker">优先复核</span><h2>超过一年未修改的大文件</h2></div><small>{{ longUnusedFiles.length }} 个候选</small></div>
              <div class="old-file-rows"><div v-for="file in longUnusedFiles" :key="file.path"><CalendarClock :size="17" /><div><b>{{ file.name }}</b><span :title="file.path">{{ file.path }}</span></div><strong>{{ formatSize(file.size) }}</strong><em>{{ file.modifiedDays }} 天</em><button class="icon-button" title="在资源管理器中定位" @click="openPath(file.path, true)"><ExternalLink :size="15" /></button></div><div v-if="!longUnusedFiles.length" class="no-matches">TOP 大文件中没有超过一年未修改的项目</div></div>
              <div class="cleanup-footnote"><AlertTriangle :size="16" /><span>长期未修改不等于可以删除。归档、备份和项目资源可能多年不变，请结合用途判断。</span></div>
            </section>
          </template>
          <section v-else class="empty-state analysis-empty"><div class="empty-visual"><CalendarClock :size="40" /></div><h2>扫描后生成文件年龄热力图</h2><p>年龄统计复用完整扫描过程，不会额外遍历磁盘。</p><button class="button primary" @click="page = 'overview'; startScan()"><Play :size="17" /> 开始完整扫描</button></section>
        </template>
      </template>
    </main>

    <div v-if="showSettings" class="settings-backdrop" @click.self="showSettings = false">
      <aside class="settings-drawer" role="dialog" aria-modal="true" aria-labelledby="settings-title">
        <header class="settings-head"><div><span class="panel-kicker">应用偏好</span><h2 id="settings-title">设置</h2></div><button class="icon-button" title="关闭设置" @click="showSettings = false"><X :size="19" /></button></header>
        <nav class="settings-tabs" aria-label="设置分类"><button :class="{ active: settingsTab === 'appearance' }" @click="settingsTab = 'appearance'"><Palette :size="17" /> 外观</button><button :class="{ active: settingsTab === 'scanning' }" @click="settingsTab = 'scanning'"><SlidersHorizontal :size="17" /> 扫描</button><button :class="{ active: settingsTab === 'system' }" @click="settingsTab = 'system'"><Wrench :size="17" /> 系统</button><button :class="{ active: settingsTab === 'about' }" @click="settingsTab = 'about'"><Info :size="17" /> 关于</button></nav>

        <div v-if="settingsTab === 'appearance'" class="settings-content">
          <section class="setting-section"><div class="setting-title"><div><b>界面配色</b><small>浅色侧栏搭配高活力强调色</small></div><span>{{ themeOptions.find(theme => theme.id === activeTheme)?.name }}</span></div><div class="theme-options settings-theme-options"><button v-for="theme in themeOptions" :key="theme.id" :class="{ active: activeTheme === theme.id }" :aria-label="theme.name" @click="applyTheme(theme.id)"><i><em v-for="color in theme.colors" :key="color" :style="{ background: color }" /></i><span>{{ theme.name }}</span><Check v-if="activeTheme === theme.id" :size="14" /></button></div></section>
          <section class="setting-section"><div class="setting-title"><div><b>字体大小</b><small>调整导航、表格和辅助说明文字</small></div></div><div class="preference-segments"><button v-for="item in [{ id: 'small', label: '小' }, { id: 'standard', label: '标准' }, { id: 'large', label: '大' }]" :key="item.id" :class="{ active: fontScale === item.id }" @click="applyFontScale(item.id as FontScale)">{{ item.label }}</button></div></section>
          <section class="setting-section"><div class="setting-title"><div><b>图标大小</b><small>保持布局稳定，只调整图标视觉尺寸</small></div><span class="icon-size-preview"><HardDrive :size="18" /></span></div><div class="preference-segments"><button v-for="item in [{ id: 'compact', label: '紧凑' }, { id: 'standard', label: '标准' }, { id: 'large', label: '大' }]" :key="item.id" :class="{ active: iconScale === item.id }" @click="applyIconScale(item.id as IconScale)">{{ item.label }}</button></div></section>
          <section class="setting-section"><div class="setting-title"><div><b>界面密度</b><small>控制导航、表格与结果行的垂直空间</small></div></div><div class="preference-segments"><button :class="{ active: uiDensity === 'compact' }" @click="applyDensity('compact')">紧凑</button><button :class="{ active: uiDensity === 'comfortable' }" @click="applyDensity('comfortable')">舒适</button></div></section>
          <p class="settings-footnote"><Check :size="15" /> 外观设置会自动保存在本机。</p>
        </div>

        <div v-else-if="settingsTab === 'scanning'" class="settings-content">
          <section class="setting-section"><div class="setting-title"><div><b>扫描排除目录</b><small>整盘、文件夹、重复文件与媒体分析都会跳过</small></div><button class="button secondary compact" @click="addExclusion"><FolderCog :size="15" /> 添加</button></div><div v-if="exclusionPaths.length" class="exclusion-list"><div v-for="path in exclusionPaths" :key="path"><Ban :size="15" /><span :title="path">{{ path }}</span><button title="移除排除目录" @click="removeExclusion(path)"><X :size="15" /></button></div></div><p v-else class="setting-empty">尚未排除任何目录</p></section>
          <section class="setting-section"><div class="setting-title"><div><b>默认大文件阈值</b><small>目录排行、媒体和文件审查使用同一标准</small></div><span>{{ largeFileMb >= 1024 ? '1 GB' : `${largeFileMb} MB` }}</span></div><div class="preference-segments"><button v-for="value in [50, 100, 500, 1024]" :key="value" :class="{ active: largeFileMb === value }" @click="largeFileMb = value">{{ value === 1024 ? '1 GB' : `${value} MB` }}</button></div></section>
          <section class="setting-section"><div class="setting-title"><div><b>扫描并发数</b><small>媒体解码和哈希任务使用的工作线程</small></div><span>{{ scanThreads }} workers</span></div><div class="preference-segments"><button v-for="value in [2, 4, 6, 8]" :key="value" :class="{ active: scanThreads === value }" @click="scanThreads = value">{{ value }}</button></div></section>
          <section class="setting-section"><div class="setting-title"><div><b>快照保留数量</b><small>每个磁盘独立保留，较旧记录自动淘汰</small></div><span>{{ snapshotLimit }} 条 / 盘</span></div><div class="preference-segments"><button v-for="value in [10, 30, 60, 100]" :key="value" :class="{ active: snapshotLimit === value }" @click="snapshotLimit = value">{{ value }}</button></div></section>
          <p class="settings-footnote"><Check :size="15" /> 扫描设置将在下次任务启动时生效。</p>
        </div>

        <div v-else-if="settingsTab === 'system'" class="settings-content">
          <section class="setting-section"><div class="setting-title"><div><b>报告保存位置</b><small>HTML 报告和诊断信息默认写入此目录</small></div><button class="button secondary compact" @click="chooseReportDirectory"><FolderOpen :size="15" /> 选择</button></div><div class="setting-path"><span :title="reportDirectory">{{ reportDirectory || '桌面（系统默认）' }}</span><button v-if="reportDirectory" title="恢复默认位置" @click="reportDirectory = ''"><X :size="15" /></button></div></section>
          <section class="setting-section"><div class="setting-title"><div><b>回收站策略</b><small>媒体文件始终进入 Windows 回收站，不会永久删除</small></div><Recycle :size="19" /></div><div class="preference-segments"><button :class="{ active: recyclePolicy === 'confirm' }" @click="recyclePolicy = 'confirm'">每次确认</button><button :class="{ active: recyclePolicy === 'direct' }" @click="recyclePolicy = 'direct'">直接移入回收站</button></div></section>
          <section class="setting-section"><div class="setting-title"><div><b>启动时检查更新</b><small>通过 GitHub Releases 检查公开发布版本</small></div><button class="toggle-switch" role="switch" :aria-checked="autoCheckUpdates" :class="{ active: autoCheckUpdates }" @click="autoCheckUpdates = !autoCheckUpdates"><i /></button></div><div class="setting-action-row"><span :class="{ success: updateStatus && !updateStatus.available, update: updateStatus?.available }">{{ updateStatus?.message || '尚未检查更新' }}</span><button class="button secondary compact" :disabled="settingsBusy === 'update'" @click="checkUpdates()"><LoaderCircle v-if="settingsBusy === 'update'" :size="15" class="spin" /><RefreshCw v-else :size="15" /> 立即检查</button></div></section>
          <section class="setting-section system-actions"><div><div><b>诊断信息</b><small>导出版本、平台、设置和快照状态，不包含文件内容</small></div><button class="button secondary compact" :disabled="settingsBusy === 'diagnostics'" @click="exportDiagnostics"><LoaderCircle v-if="settingsBusy === 'diagnostics'" :size="15" class="spin" /><Download v-else :size="15" /> 导出诊断</button></div><div><div><b>本地扫描历史</b><small>清除全部磁盘空间快照，不会删除文件</small></div><button class="button danger compact" @click="confirmClearHistory = true"><Trash2 :size="15" /> 清除历史</button></div></section>
        </div>

        <div v-else class="settings-content about-content">
          <div class="about-product"><span class="about-mark"><HardDrive :size="28" /></span><div><h3>磁盘空间分析器</h3><p>Windows 本地空间诊断、媒体管理与安全清理工具</p><b>版本 6.1.0</b></div></div>
          <section class="about-section"><h4>系统架构</h4><dl><div><dt>桌面框架</dt><dd>Tauri 2</dd></div><div><dt>用户界面</dt><dd>Vue 3 + TypeScript</dd></div><div><dt>扫描引擎</dt><dd>Rust</dd></div><div><dt>运行平台</dt><dd>Windows 10 / 11 · 64 位</dd></div></dl></section>
          <section class="about-section"><h4>作者信息</h4><dl><div><dt>项目作者 / GitHub</dt><dd>songmeng@hotmail.com</dd></div><div><dt>软件许可</dt><dd>MIT License</dd></div></dl></section>
          <section class="about-safety"><ShieldCheck :size="20" /><div><b>本地优先，删除可恢复</b><p>扫描、哈希、缩略图和历史快照均在本机处理。媒体整理统一使用 Windows 回收站。</p></div></section>
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

    <div v-if="confirmCleanup" class="modal-backdrop" @click.self="confirmCleanup = false">
      <section class="confirm-dialog" role="dialog" aria-modal="true" aria-labelledby="confirm-title">
        <button class="dialog-close" aria-label="关闭" @click="confirmCleanup = false"><X :size="18" /></button>
        <span class="dialog-icon"><Trash2 :size="24" /></span>
        <h2 id="confirm-title">确认清理 {{ formatSize(selectedCleanupBytes) }}？</h2>
        <p>将永久删除所选临时文件和缓存。个人文档、下载内容与系统关键文件不在本次操作范围内。</p>
        <div class="confirm-items"><div v-for="item in selectedCleanupItems" :key="item.id"><Check :size="14" /><span>{{ item.name }}</span><b>{{ formatSize(item.size) }}</b></div></div>
        <div class="dialog-actions"><button class="button secondary" :disabled="cleaning" @click="confirmCleanup = false">取消</button><button class="button danger-solid" :disabled="cleaning" @click="runCleanup"><LoaderCircle v-if="cleaning" :size="16" class="spin" /><Trash2 v-else :size="16" /> {{ cleaning ? '正在清理' : '确认清理' }}</button></div>
      </section>
    </div>
  </div>
</template>

<style>
:root{font-family:"Segoe UI","Microsoft YaHei",sans-serif;color:#1d2939;background:#f3f5f7;font-synthesis:none}*{box-sizing:border-box}body{margin:0;min-width:760px;min-height:100vh}button,input{font:inherit;letter-spacing:0}button{cursor:pointer}.app-shell{min-height:100vh;display:grid;grid-template-columns:232px 1fr}.sidebar{position:fixed;inset:0 auto 0 0;width:232px;background:#18212b;color:#f8fafc;padding:20px 14px 16px;display:flex;flex-direction:column;border-right:1px solid #111820}.brand{display:flex;align-items:center;gap:11px;padding:2px 6px 22px}.brand-mark{width:36px;height:36px;display:grid;place-items:center;background:#e8583e;color:#fff;border-radius:6px}.brand strong{display:block;font-size:14px}.brand small{display:block;color:#8f9dac;font-size:10px;margin-top:2px}.main-nav{display:grid;gap:3px;margin-bottom:22px}.main-nav button{height:39px;border:0;background:transparent;color:#93a0ae;border-radius:5px;padding:0 9px;display:grid;grid-template-columns:19px 1fr auto;gap:9px;align-items:center;text-align:left;font-size:11px}.main-nav button:hover{background:#202c38;color:#fff}.main-nav button.active{background:#273543;color:#fff}.main-nav button b{font-size:8px;color:#e9a295;background:#442b2b;padding:2px 6px;border-radius:8px}.sidebar-label{font-size:9px;color:#718096;text-transform:uppercase;padding:0 8px 7px}.drive-list{display:grid;gap:4px}.drive-button{border:0;background:transparent;color:#9eabb9;border-radius:5px;padding:9px;display:grid;grid-template-columns:18px 1fr 7px;gap:8px;text-align:left;align-items:center}.drive-button:hover{background:#202c38;color:#fff}.drive-button.active{background:#273543;color:#fff}.drive-button span b,.drive-button span small{display:block}.drive-button span b{font-size:11px;font-weight:600}.drive-button span small{font-size:9px;color:#728192;margin-top:2px}.drive-button i{width:6px;height:6px;background:#e8583e;border-radius:50%}.drive-button:disabled{cursor:not-allowed;opacity:.6}.drive-loading{font-size:10px;color:#8f9dac;padding:12px;display:flex;gap:8px}.sidebar-spacer{flex:1}.safety-note{display:flex;gap:9px;align-items:flex-start;color:#82cbb2;background:#1c302d;border:1px solid #29443e;border-radius:6px;padding:11px}.safety-note b,.safety-note span{display:block}.safety-note b{font-size:10px}.safety-note span{font-size:9px;color:#78a699;margin-top:2px;line-height:1.45}.preview-badge{display:flex;align-items:center;gap:6px;margin-top:10px;padding:8px;color:#f0c36d;font-size:10px}.version{font-size:9px;color:#556575;text-align:center;margin-top:15px}.workspace{grid-column:2;padding:26px 30px 48px;max-width:1500px;width:100%;margin:0 auto}.topbar{display:flex;justify-content:space-between;align-items:center;margin-bottom:22px}.eyebrow{font-size:10px;color:#e8583e;font-weight:700}.topbar h1{font-size:24px;line-height:1.2;margin:4px 0 0;letter-spacing:0}.actions{display:flex;gap:8px}.button{height:38px;border-radius:5px;border:1px solid transparent;display:inline-flex;align-items:center;justify-content:center;gap:8px;padding:0 15px;font-size:11px;font-weight:650;white-space:nowrap}.button.primary{background:#e8583e;color:#fff;box-shadow:0 1px 2px #9f2e1e33}.button.primary:hover{background:#d94c34}.button.secondary{background:#fff;border-color:#d7dce2;color:#344054}.button.danger{background:#fff;border-color:#f2b4a9;color:#c43d28}.button.danger-solid{background:#c94331;color:#fff}.button.compact{height:32px;padding:0 10px}.button:disabled,.text-button:disabled,.check-button:disabled{opacity:.45;cursor:not-allowed}.alert{display:flex;align-items:center;gap:9px;padding:10px 12px;border-radius:5px;margin-bottom:12px;font-size:11px}.alert span{flex:1}.alert button{border:0;background:transparent;color:inherit;display:grid;place-items:center}.alert.error{background:#fef0ed;border:1px solid #f8c6bd;color:#9f2e1e}.alert.notice{background:#edf8f4;border:1px solid #b8e2d2;color:#176b50}.scan-strip{background:#fff;border:1px solid #dfe3e8;border-left:3px solid #e8583e;border-radius:5px;padding:13px 15px;margin-bottom:14px}.scan-line{display:flex;align-items:center;gap:8px;font-size:11px}.scan-line strong{flex:1}.scan-line b{color:#e8583e}.pulse-dot{width:7px;height:7px;border-radius:50%;background:#e8583e;box-shadow:0 0 0 4px #fae2dd}.progress-track{height:4px;background:#edf0f2;margin:11px 0 7px;border-radius:2px;overflow:hidden}.progress-track div{height:100%;background:#e8583e;transition:width .25s}.current-path{font-size:9px;color:#8a94a3;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.metrics{display:grid;grid-template-columns:repeat(4,minmax(150px,1fr));background:#fff;border:1px solid #dfe3e8;border-radius:6px;margin-bottom:14px}.metric{min-height:106px;padding:20px;display:flex;align-items:center;gap:13px;border-right:1px solid #e7eaee}.metric:last-child{border:0}.metric-icon{width:36px;height:36px;border-radius:5px;display:grid;place-items:center}.metric-icon.coral{background:#fce9e5;color:#d94c34}.metric-icon.blue{background:#e7f0fb;color:#3475b8}.metric-icon.green{background:#e5f5ef;color:#218b68}.metric-icon.amber{background:#fbf1dc;color:#a66c09}.metric span,.metric small,.metric strong{display:block}.metric span{font-size:9px;color:#7d8896}.metric strong{font-size:20px;margin:3px 0;color:#1d2939}.metric small{font-size:9px;color:#98a2b3}.reclaim-band{display:flex;align-items:center;gap:14px;padding:15px 17px;background:#eff8f4;border:1px solid #bfe2d5;border-radius:6px;margin-bottom:14px;color:#176b50}.reclaim-icon{width:40px;height:40px;border-radius:5px;background:#d7eee6;display:grid;place-items:center}.reclaim-band>div:nth-child(2){flex:1}.reclaim-band span,.reclaim-band strong,.reclaim-band small{display:block}.reclaim-band span{font-size:9px}.reclaim-band strong{font-size:18px;margin:1px 0}.reclaim-band small{font-size:9px;color:#609381}.content-grid{display:grid;grid-template-columns:minmax(430px,1.35fr) minmax(300px,.65fr);gap:14px;margin-bottom:14px}.panel,.results-section{background:#fff;border:1px solid #dfe3e8;border-radius:6px}.panel{padding:18px 20px}.panel-heading{display:flex;justify-content:space-between;align-items:flex-start}.panel-heading h2{font-size:14px;margin:3px 0}.panel-heading small,.panel-kicker{font-size:9px;color:#98a2b3}.panel-kicker{text-transform:uppercase;font-weight:700}.distribution-body{display:flex;align-items:center;gap:28px;margin-top:14px}.donut{width:126px;height:126px;flex:none;border-radius:50%;display:grid;place-items:center}.donut>div{width:78px;height:78px;border-radius:50%;background:#fff;display:grid;place-content:center;text-align:center}.donut strong,.donut span{display:block}.donut strong{font-size:19px}.donut span{font-size:9px;color:#8a94a3;margin-top:2px}.legend{flex:1;display:grid;grid-template-columns:repeat(2,minmax(130px,1fr));gap:10px 16px}.legend>div{display:grid;grid-template-columns:7px 1fr auto;align-items:center;gap:7px;font-size:9px}.legend i{width:7px;height:7px;border-radius:2px}.legend b{font-size:10px}.health-score{width:38px;height:38px;border-radius:50%;background:#e5f5ef;color:#218b68;display:grid;place-items:center;font-size:13px;font-weight:700}.health-score.warn{background:#fbf1dc;color:#a66c09}.health-score.critical{background:#fce9e5;color:#c94331}.health-track{height:6px;background:#edf0f2;border-radius:3px;margin:21px 0 17px;overflow:hidden}.health-track i{display:block;height:100%;background:#38a47c;border-radius:3px}.health-panel ul{list-style:none;padding:0;margin:0;display:grid;gap:11px}.health-panel li{display:flex;align-items:center;gap:8px;font-size:10px;color:#667085}.insight-panel{margin-bottom:14px}.insight-grid{display:grid;grid-template-columns:repeat(3,1fr);gap:10px;margin-top:14px}.insight-grid button{border:1px solid #e4e7eb;border-radius:5px;background:#fafbfc;padding:12px;display:grid;grid-template-columns:34px 1fr 16px;gap:9px;align-items:center;text-align:left;color:#667085}.insight-grid button:hover{background:#f4f6f8;border-color:#cfd5dc}.insight-grid b,.insight-grid small{display:block}.insight-grid b{font-size:10px;color:#273443}.insight-grid small{font-size:8px;margin-top:3px}.insight-icon{width:34px;height:34px;border-radius:5px;display:grid;place-items:center}.insight-icon.green{background:#e5f5ef;color:#218b68}.insight-icon.amber{background:#fbf1dc;color:#a66c09}.insight-icon.blue{background:#e7f0fb;color:#3475b8}.cleanup-hero{background:#202c37;color:#fff;border-radius:6px;padding:22px 24px;margin-bottom:14px;display:grid;grid-template-columns:1fr 1.4fr;align-items:center;gap:30px}.cleanup-hero>div:first-child>strong{display:block;font-size:30px;margin:5px 0}.cleanup-hero p{font-size:9px;color:#9dabb8;margin:0}.cleanup-breakdown{display:grid;grid-template-columns:repeat(3,1fr);border-left:1px solid #3a4652}.cleanup-breakdown div{padding:7px 20px;border-right:1px solid #3a4652}.cleanup-breakdown span,.cleanup-breakdown b{display:block}.cleanup-breakdown span{font-size:9px;color:#94a2b0}.cleanup-breakdown b{font-size:15px;margin-top:5px}.cleanup-list{padding:0;overflow:hidden}.cleanup-toolbar{min-height:69px;padding:14px 17px;border-bottom:1px solid #e4e7eb;display:flex;align-items:center;justify-content:space-between;gap:20px}.cleanup-toolbar h2{font-size:14px;margin:0}.cleanup-toolbar p{font-size:9px;color:#8a94a3;margin:3px 0 0}.cleanup-actions{display:flex;align-items:center;gap:10px}.text-button{border:0;background:transparent;color:#3475b8;font-size:10px}.cleanup-rows{display:grid}.cleanup-row{min-height:82px;padding:13px 17px;border-bottom:1px solid #edf0f2;display:grid;grid-template-columns:24px minmax(260px,1fr) 120px 96px;gap:13px;align-items:center}.check-button{width:20px;height:20px;border:1px solid #b8c0ca;border-radius:4px;background:#fff;color:#fff;display:grid;place-items:center;padding:0}.check-button.checked{background:#e8583e;border-color:#e8583e}.action-symbol{width:24px;height:24px;display:grid;place-items:center}.action-symbol.review{color:#b27b18}.action-symbol.system{color:#477ead}.cleanup-copy{min-width:0}.cleanup-copy>div{display:flex;align-items:center;gap:8px}.cleanup-copy b{font-size:11px}.cleanup-copy p{font-size:9px;color:#667085;margin:4px 0}.cleanup-copy small{display:block;font-size:8px;color:#98a2b3;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.risk-badge{font-size:8px;padding:2px 6px;border-radius:3px;background:#e5f5ef;color:#218b68}.risk-badge.review{background:#fbf1dc;color:#94620d}.risk-badge.system{background:#e7f0fb;color:#3475b8}.cleanup-size{text-align:right}.cleanup-size b,.cleanup-size span{display:block}.cleanup-size b{font-size:13px}.cleanup-size span{font-size:8px;color:#98a2b3;margin-top:3px}.row-space{width:96px}.cleanup-footnote{display:flex;align-items:center;gap:8px;padding:12px 17px;background:#f7faf9;color:#4d7f6d;font-size:9px}.loading-state{height:300px;display:flex;flex-direction:column;gap:10px;align-items:center;justify-content:center;color:#8a94a3;font-size:10px}.results-section{overflow:hidden}.result-toolbar{height:58px;padding:0 16px;border-bottom:1px solid #e4e7eb;display:flex;align-items:center;justify-content:space-between}.tabs{align-self:stretch;display:flex;gap:20px}.tabs button{border:0;border-bottom:2px solid transparent;background:transparent;padding:2px 0 0;color:#7d8896;font-size:10px;font-weight:650}.tabs button.active{color:#1d2939;border-color:#e8583e}.tabs span{background:#eef0f2;border-radius:10px;padding:1px 6px;margin-left:4px;font-size:8px}.search{width:210px;height:32px;border:1px solid #d7dce2;border-radius:5px;display:flex;align-items:center;gap:7px;padding:0 9px;color:#98a2b3}.search:focus-within{border-color:#9ca8b4}.search input{border:0;outline:0;min-width:0;width:100%;font-size:10px;color:#344054}.table-wrap{overflow:auto}table{border-collapse:collapse;width:100%;font-size:10px}th{height:36px;text-align:left;color:#8a94a3;font-size:9px;font-weight:650;background:#fafbfc;border-bottom:1px solid #e7eaee;padding:0 12px}td{height:52px;border-bottom:1px solid #eef0f2;padding:7px 12px;color:#475467}tbody tr:last-child td{border-bottom:0}tbody tr:hover{background:#fafbfc}.rank{width:42px;text-align:center;color:#98a2b3}.open-col{width:48px}.path-cell{display:flex;align-items:center;gap:10px;color:#3475b8;min-width:180px}.path-cell>div{min-width:0}.path-cell b,.path-cell span{display:block}.path-cell b{font-size:10px;color:#273443;max-width:310px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.path-cell span{font-size:8px;color:#98a2b3;margin-top:2px;max-width:310px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.path-cell.file{color:#9b6b15}.muted{color:#8a94a3}.path-text{max-width:380px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.share{min-width:145px}.share span{font-size:8px}.share i{display:block;height:4px;background:#edf0f2;border-radius:2px;margin-top:5px;overflow:hidden}.share em{display:block;height:100%;background:#4b8dcc;border-radius:2px}.share em.warm{background:#e7a82b}.share em.hot{background:#e8583e}.icon-button{width:30px;height:30px;border:0;background:transparent;color:#84909d;border-radius:4px;display:grid;place-items:center}.icon-button:hover{background:#eef1f3;color:#273443}.no-matches{padding:32px;text-align:center;color:#98a2b3;font-size:10px}.empty-state{min-height:360px;border:1px dashed #ccd2d9;border-radius:6px;background:#f8f9fa;display:flex;flex-direction:column;align-items:center;justify-content:center;text-align:center;padding:32px}.empty-visual{width:88px;height:88px;border-radius:50%;background:#e9edf0;color:#617080;display:grid;place-items:center;position:relative}.empty-visual span{position:absolute;right:4px;bottom:4px;width:33px;height:33px;border-radius:50%;background:#e8583e;color:#fff;border:3px solid #f8f9fa;display:grid;place-items:center}.empty-state h2{font-size:16px;margin:16px 0 6px}.empty-state p{font-size:10px;color:#7d8896;max-width:440px;line-height:1.7;margin:0 0 18px}.modal-backdrop{position:fixed;inset:0;background:#10182099;z-index:20;display:grid;place-items:center;padding:20px}.confirm-dialog{width:min(440px,100%);background:#fff;border-radius:7px;padding:24px;position:relative;box-shadow:0 18px 55px #0e1726aa}.dialog-close{position:absolute;right:14px;top:14px;border:0;background:transparent;color:#7d8896;display:grid;place-items:center}.dialog-icon{width:46px;height:46px;border-radius:6px;background:#fce9e5;color:#c94331;display:grid;place-items:center}.confirm-dialog h2{font-size:17px;margin:15px 0 7px}.confirm-dialog>p{font-size:10px;line-height:1.6;color:#667085;margin:0 0 14px}.confirm-items{border:1px solid #e4e7eb;border-radius:5px;padding:5px 11px}.confirm-items div{height:33px;display:grid;grid-template-columns:16px 1fr auto;align-items:center;gap:7px;border-bottom:1px solid #edf0f2;font-size:10px;color:#218b68}.confirm-items div:last-child{border:0}.confirm-items b{color:#344054}.dialog-actions{display:flex;justify-content:flex-end;gap:8px;margin-top:18px}.spin{animation:spin 1s linear infinite}@keyframes spin{to{transform:rotate(360deg)}}@media(max-width:1050px){.workspace{padding:22px 20px}.metrics{grid-template-columns:1fr 1fr}.metric:nth-child(2){border-right:0}.metric:nth-child(-n+2){border-bottom:1px solid #e7eaee}.content-grid{grid-template-columns:1fr}.cleanup-hero{grid-template-columns:1fr}.cleanup-breakdown{border-left:0;border-top:1px solid #3a4652}.cleanup-breakdown div:first-child{padding-left:0}.insight-grid{grid-template-columns:1fr}.cleanup-row{grid-template-columns:24px minmax(220px,1fr) 100px 90px}}@media(max-width:800px){body{min-width:0}.app-shell{display:block}.sidebar{position:static;width:100%;height:auto;padding:12px 14px;display:grid;grid-template-columns:1fr auto;align-items:center}.brand{padding:0}.main-nav,.sidebar-label,.drive-list,.sidebar-spacer,.safety-note,.version{display:none}.preview-badge{margin:0}.workspace{display:block;padding:18px 14px}.topbar{align-items:flex-start}.topbar h1{font-size:20px}.actions .button.secondary{width:38px;padding:0;font-size:0}.metrics{grid-template-columns:1fr 1fr}.metric{padding:14px;min-height:90px}.metric-icon{display:none}.distribution-body{gap:16px}.legend{grid-template-columns:1fr}.reclaim-band{align-items:flex-start;flex-wrap:wrap}.reclaim-band .button{width:100%}.cleanup-breakdown div{padding:7px 10px}.cleanup-toolbar{align-items:flex-start;flex-direction:column}.cleanup-actions{width:100%;justify-content:space-between}.cleanup-row{grid-template-columns:24px 1fr 88px}.cleanup-row>.button,.row-space{grid-column:2 / 4;justify-self:start}.result-toolbar{height:auto;padding:10px;gap:10px;align-items:stretch;flex-direction:column}.tabs{height:36px}.search{width:100%}}
:root{--accent:#2f79c5;--accent-hover:#276bad;--accent-soft:#e7f0fb;--accent-ink:#235f99;--sidebar:#172331;--sidebar-hover:#1e2e3d;--sidebar-active:#24384a}:root[data-accent="forest"]{--accent:#23866b;--accent-hover:#1d735b;--accent-soft:#e2f2ec;--accent-ink:#176a53;--sidebar:#172824;--sidebar-hover:#1e342e;--sidebar-active:#29443b}:root[data-accent="coral"]{--accent:#e8583e;--accent-hover:#d94c34;--accent-soft:#fce9e5;--accent-ink:#c94331;--sidebar:#18212b;--sidebar-hover:#202c38;--sidebar-active:#273543}:root[data-accent="cherry"]{--accent:#c94c5f;--accent-hover:#b84052;--accent-soft:#f8e5e8;--accent-ink:#a93648;--sidebar:#281b22;--sidebar-hover:#36252e;--sidebar-active:#462e39}:root[data-accent="graphite"]{--accent:#52606f;--accent-hover:#43505e;--accent-soft:#e8ebee;--accent-ink:#3d4955;--sidebar:#161b22;--sidebar-hover:#20262e;--sidebar-active:#2a323c}.sidebar{background:var(--sidebar)}.main-nav button:hover,.drive-button:hover{background:var(--sidebar-hover)}.main-nav button.active,.drive-button.active{background:var(--sidebar-active)}.brand-mark,.button.primary,.check-button.checked,.empty-visual span{background:var(--accent)}.button.primary:hover{background:var(--accent-hover)}.eyebrow,.scan-line b{color:var(--accent)}.drive-button i,.pulse-dot{background:var(--accent)}.pulse-dot{box-shadow:0 0 0 4px var(--accent-soft)}.scan-strip{border-left-color:var(--accent)}.progress-track div{background:var(--accent)}.metric-icon.coral{background:var(--accent-soft);color:var(--accent-ink)}.tabs button.active{border-color:var(--accent)}.theme-trigger{width:100%;height:43px;border:0;background:transparent;color:#93a0ae;border-radius:5px;padding:0 9px;margin-bottom:8px;display:grid;grid-template-columns:19px 1fr 16px;gap:9px;align-items:center;text-align:left}.theme-trigger:hover{background:var(--sidebar-hover);color:#fff}.theme-trigger b,.theme-trigger small{display:block}.theme-trigger b{font-size:10px}.theme-trigger small{font-size:8px;color:#748494;margin-top:2px}.theme-popover{position:fixed;left:244px;bottom:18px;width:304px;background:#fff;color:#273443;border:1px solid #d7dce2;border-radius:7px;padding:16px;box-shadow:0 16px 42px #10182044;z-index:30}.theme-head{display:flex;justify-content:space-between;align-items:flex-start}.theme-head h2{font-size:14px;margin:3px 0 12px}.theme-head button{border:0;background:transparent;color:#84909d;display:grid;place-items:center}.theme-options{display:grid;grid-template-columns:1fr 1fr;gap:7px}.theme-options button{height:52px;border:1px solid #e1e5e9;background:#fafbfc;border-radius:5px;padding:7px;display:grid;grid-template-columns:42px 1fr 14px;align-items:center;gap:7px;text-align:left;color:#475467}.theme-options button:hover{background:#f3f5f7}.theme-options button.active{border-color:var(--accent);box-shadow:0 0 0 1px var(--accent)}.theme-options button>i{width:42px;height:26px;display:flex;border-radius:4px;overflow:hidden}.theme-options button>i em{flex:1}.theme-options button>span{font-size:9px}.theme-options button>svg{color:var(--accent)}.theme-popover>p{font-size:8px;color:#98a2b3;margin:11px 0 0}.insight-grid{grid-template-columns:repeat(auto-fit,minmax(210px,1fr))}.insight-icon.neutral{background:var(--accent-soft);color:var(--accent-ink)}.sidebar-drive-title{display:flex;align-items:center;justify-content:space-between}.sidebar-drive-title button{border:0;background:transparent;color:#718096;padding:2px;display:grid;place-items:center}.sidebar-drive-title button:hover{color:#fff}.folder-scan-strip{border-left-color:var(--accent)}.folder-picker-band{display:flex;align-items:center;gap:12px;padding:15px 17px;margin-bottom:14px;background:#eef5fc;border:1px solid #c7dcef;border-radius:6px;color:#2f6fae}.folder-picker-band>span{width:38px;height:38px;border-radius:5px;background:#dceaf7;display:grid;place-items:center}.folder-picker-band>div{flex:1}.folder-picker-band b,.folder-picker-band small{display:block}.folder-picker-band b{font-size:11px;color:#274b6d}.folder-picker-band small{font-size:9px;color:#66839e;margin-top:3px}.folder-detail-head{display:grid;grid-template-columns:32px 42px 1fr auto;gap:11px;align-items:center;margin-bottom:12px}.back-button{background:#f2f4f6}.folder-detail-icon{width:42px;height:42px;border-radius:5px;background:var(--accent-soft);color:var(--accent-ink);display:grid;place-items:center}.folder-detail-head h2{font-size:15px;margin:2px 0}.folder-detail-head p{font-size:9px;color:#8a94a3;margin:0;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.folder-metrics{display:grid;grid-template-columns:repeat(4,1fr);background:#202c37;color:#fff;border-radius:6px;margin-bottom:12px}.folder-metrics div{padding:17px 20px;border-right:1px solid #3a4652}.folder-metrics div:last-child{border:0}.folder-metrics span,.folder-metrics b,.folder-metrics small{display:block}.folder-metrics span{font-size:9px;color:#94a2b0}.folder-metrics b{font-size:17px;margin:4px 0}.folder-metrics small{font-size:8px;color:#82919f}.folder-contents{padding:0;overflow:hidden}.folder-content-heading{padding:15px 17px;border-bottom:1px solid #e4e7eb;align-items:center}.folder-analysis-rows{display:grid}.folder-analysis-row{min-height:86px;padding:13px 17px;border-bottom:1px solid #edf0f2;display:grid;grid-template-columns:26px minmax(280px,1fr) 210px 112px;gap:12px;align-items:center}.folder-kind{color:var(--accent)}.folder-kind.file{color:#a66c09}.folder-item-copy{min-width:0}.folder-item-copy>div{display:flex;align-items:center;gap:7px}.folder-item-copy b{font-size:11px;max-width:330px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.folder-item-copy p{font-size:9px;color:#667085;margin:4px 0;line-height:1.35}.folder-item-copy small{font-size:8px;color:#98a2b3}.folder-risk{font-size:8px;padding:2px 6px;border-radius:3px}.folder-risk.rebuildable{background:#e5f5ef;color:#218b68}.folder-risk.review{background:#fbf1dc;color:#94620d}.folder-risk.protected{background:#fce9e5;color:#c94331}.folder-size-bar>div{display:flex;justify-content:space-between;align-items:center}.folder-size-bar b{font-size:11px}.folder-size-bar span{font-size:8px;color:#8a94a3}.folder-size-bar i{height:5px;background:#edf0f2;border-radius:3px;display:block;margin-top:6px;overflow:hidden}.folder-size-bar em{display:block;height:100%;background:#e7a82b}.folder-size-bar em.rebuildable{background:#38a47c}.folder-size-bar em.protected{background:#e8583e}.folder-row-actions,.table-actions{display:flex;align-items:center;justify-content:flex-end;gap:3px}.folder-note{border-top:0}.empty-actions{display:flex;gap:8px}.open-col{width:92px}.table-actions .icon-button:first-child{color:var(--accent)}@media(max-width:1050px){.folder-analysis-row{grid-template-columns:26px minmax(220px,1fr) 170px 100px}.folder-metrics{grid-template-columns:1fr 1fr}.folder-metrics div:nth-child(2){border-right:0}.folder-metrics div:nth-child(-n+2){border-bottom:1px solid #3a4652}}
@media(max-width:800px){.main-nav{display:flex;grid-column:1 / -1;gap:4px;margin:10px 0 0;overflow:auto}.main-nav button{display:grid;flex:1;min-width:110px;grid-template-columns:18px 1fr auto}.folder-detail-head{grid-template-columns:32px 38px 1fr}.folder-detail-head>.button{grid-column:3}.folder-analysis-row{grid-template-columns:24px 1fr 90px}.folder-size-bar{grid-column:2 / 4}.folder-row-actions{grid-column:2 / 4;justify-content:flex-start}.folder-picker-band{align-items:flex-start;flex-wrap:wrap}.folder-picker-band .button{width:100%}.empty-actions{flex-direction:column;width:100%}}
:root{--accent-gradient:var(--accent);--accent-contrast:#fff}:root[data-accent="mintrose"]{--accent:#4c9386;--accent-hover:#3f7e72;--accent-soft:#e2f7f1;--accent-ink:#2b6f64;--accent-gradient:linear-gradient(135deg,#A9F1DF,#FFBBBB);--accent-contrast:#173b37;--sidebar:#10211f;--sidebar-hover:#19302c;--sidebar-active:#24443d}:root[data-accent="lavenderteal"]{--accent:#278b80;--accent-hover:#20766d;--accent-soft:#e3f3f1;--accent-ink:#1d6a62;--accent-gradient:linear-gradient(135deg,#D8B5FF,#1EAE98);--accent-contrast:#102e2a;--sidebar:#151c2a;--sidebar-hover:#202a3b;--sidebar-active:#29364b}.brand-mark,.button.primary{background:var(--accent-gradient);color:var(--accent-contrast)}.collapse-button{position:absolute;right:5px;top:29px;width:28px;height:28px;border:0;background:transparent;color:#778796;border-radius:4px;display:grid;place-items:center}.collapse-button:hover{background:var(--sidebar-hover);color:#fff}.app-shell.collapsed{grid-template-columns:72px 1fr}.sidebar.collapsed{width:72px;padding-left:10px;padding-right:10px}.sidebar.collapsed .brand{justify-content:center;padding-left:0;padding-right:0}.sidebar.collapsed .brand>div,.sidebar.collapsed .main-nav span,.sidebar.collapsed .main-nav b,.sidebar.collapsed .sidebar-label span,.sidebar.collapsed .sidebar-drive-title button,.sidebar.collapsed .drive-button span,.sidebar.collapsed .drive-button i,.sidebar.collapsed .theme-trigger span,.sidebar.collapsed .theme-trigger>svg:last-child,.sidebar.collapsed .safety-note,.sidebar.collapsed .preview-badge,.sidebar.collapsed .version{display:none}.sidebar.collapsed .collapse-button{right:-12px;top:67px;background:#fff;color:#52606f;border:1px solid #d7dce2;z-index:3}.sidebar.collapsed .main-nav button,.sidebar.collapsed .drive-button,.sidebar.collapsed .theme-trigger{display:grid;grid-template-columns:1fr;place-items:center;padding:0}.sidebar.collapsed .sidebar-label{height:10px}.sidebar.collapsed .theme-popover{left:84px}.analysis-tabs{display:grid;grid-template-columns:repeat(3,1fr);gap:8px;padding:8px;margin-bottom:14px}.analysis-tabs button{height:58px;border:1px solid transparent;background:transparent;border-radius:5px;padding:0 13px;display:flex;align-items:center;gap:10px;text-align:left;color:#7d8896}.analysis-tabs button:hover{background:#f4f6f8}.analysis-tabs button.active{background:var(--accent-soft);border-color:var(--accent);color:var(--accent-ink)}.analysis-tabs b,.analysis-tabs small{display:block}.analysis-tabs b{font-size:11px}.analysis-tabs small{font-size:8px;margin-top:3px;color:#8793a0}.analysis-toolbar{display:flex;align-items:center;justify-content:space-between;gap:24px;margin-bottom:14px}.analysis-toolbar h2{font-size:14px;margin:3px 0}.analysis-toolbar p{font-size:9px;color:#7d8896;margin:0}.duplicate-controls{display:flex;align-items:center;gap:8px}.size-segments{display:flex;background:#edf0f2;border-radius:5px;padding:2px}.size-segments button{height:30px;border:0;background:transparent;border-radius:4px;padding:0 9px;color:#667085;font-size:9px}.size-segments button.active{background:#fff;color:var(--accent-ink);box-shadow:0 1px 3px #10182022}.analysis-metrics{display:grid;grid-template-columns:repeat(4,1fr);background:#202c37;color:#fff;border-radius:6px;margin-bottom:14px}.analysis-metrics>div{padding:17px 20px;border-right:1px solid #3a4652}.analysis-metrics>div:last-child{border:0}.analysis-metrics span,.analysis-metrics b,.analysis-metrics small{display:block}.analysis-metrics span{font-size:9px;color:#94a2b0}.analysis-metrics b{font-size:18px;margin:4px 0}.analysis-metrics small{font-size:8px;color:#82919f}.duplicate-results{padding:0;overflow:hidden}.duplicate-results>.panel-heading{padding:16px 18px;border-bottom:1px solid #e4e7eb}.duplicate-groups{display:grid}.duplicate-group{padding:14px 18px;border-bottom:1px solid #e9ecef}.duplicate-group-head{display:grid;grid-template-columns:24px 1fr auto;gap:9px;align-items:center}.duplicate-group-head>span{width:22px;height:22px;border-radius:4px;background:var(--accent-soft);color:var(--accent-ink);display:grid;place-items:center;font-size:9px;font-weight:700}.duplicate-group-head b,.duplicate-group-head small{display:block}.duplicate-group-head b{font-size:11px}.duplicate-group-head small{font-size:8px;color:#98a2b3;margin-top:2px}.duplicate-group-head strong{font-size:11px;color:#c94331}.duplicate-paths{margin:9px 0 0 33px;background:#f8f9fa;border:1px solid #eceff2;border-radius:5px;padding:4px 9px}.duplicate-paths>div{height:34px;display:grid;grid-template-columns:17px 1fr 30px;gap:7px;align-items:center;border-bottom:1px solid #eceff2;color:#7d8896}.duplicate-paths>div:last-child{border:0}.duplicate-paths span{font-size:9px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.analysis-empty{min-height:300px}.history-summary{padding:0;overflow:hidden}.history-summary>.panel-heading{padding:17px 20px}.history-delta{text-align:right}.history-delta span,.history-delta b{display:block}.history-delta span{font-size:8px;color:#98a2b3}.history-delta b{font-size:14px;color:#c94331;margin-top:3px}.history-delta.down b{color:#218b68}.trend-chart{height:245px;padding:18px 24px 8px;display:flex;align-items:flex-end;gap:10px;border-top:1px solid #edf0f2;border-bottom:1px solid #edf0f2;overflow-x:auto}.trend-column{height:100%;min-width:44px;flex:1;display:grid;grid-template-rows:18px 1fr 22px;text-align:center}.trend-value{font-size:8px;color:#667085}.trend-track{height:100%;background:#f0f2f4;border-radius:4px 4px 0 0;display:flex;align-items:flex-end;overflow:hidden}.trend-track i{width:100%;min-height:2px;background:var(--accent-gradient);border-radius:4px 4px 0 0}.trend-column>span{font-size:8px;color:#8a94a3;padding-top:6px}.history-list{display:grid}.history-list>div{min-height:45px;padding:7px 20px;display:grid;grid-template-columns:130px 110px 120px 1fr;gap:10px;align-items:center;border-bottom:1px solid #edf0f2}.history-list span,.history-list small,.history-list em{font-size:9px;color:#7d8896;font-style:normal}.history-list b{font-size:10px}.history-list em{text-align:right;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.age-overview{margin-bottom:14px}.age-stack{height:16px;display:flex;border-radius:4px;overflow:hidden;margin:20px 0}.age-stack i{height:100%;min-width:0}.age-grid{display:grid;grid-template-columns:repeat(5,1fr);gap:9px}.age-grid>div{min-height:112px;border:1px solid #e4e7eb;border-radius:5px;padding:11px;position:relative}.age-grid>div>span{width:8px;height:8px;border-radius:2px;display:block;margin-bottom:9px}.age-grid b,.age-grid strong,.age-grid small{display:block}.age-grid b{font-size:9px;color:#667085}.age-grid strong{font-size:15px;margin:6px 0}.age-grid small{font-size:8px;color:#98a2b3}.age-grid>div>i{display:block;height:4px;background:#edf0f2;border-radius:2px;margin-top:10px;overflow:hidden}.age-grid>div>i em{display:block;height:100%}.old-files{padding:0;overflow:hidden}.old-files>.panel-heading{padding:16px 18px;border-bottom:1px solid #e4e7eb}.old-file-rows{display:grid}.old-file-rows>div:not(.no-matches){height:58px;padding:7px 18px;display:grid;grid-template-columns:22px minmax(260px,1fr) 100px 70px 30px;gap:9px;align-items:center;border-bottom:1px solid #edf0f2;color:#a66c09}.old-file-rows>div>div{min-width:0}.old-file-rows b,.old-file-rows span{display:block}.old-file-rows b{font-size:10px;color:#273443}.old-file-rows span{font-size:8px;color:#98a2b3;margin-top:3px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.old-file-rows strong{font-size:10px;color:#344054}.old-file-rows em{font-size:9px;color:#c94331;font-style:normal}@media(max-width:1050px){.analysis-toolbar{align-items:flex-start;flex-direction:column}.duplicate-controls{width:100%;flex-wrap:wrap}.analysis-metrics{grid-template-columns:1fr 1fr}.analysis-metrics>div:nth-child(2){border-right:0}.analysis-metrics>div:nth-child(-n+2){border-bottom:1px solid #3a4652}.age-grid{grid-template-columns:repeat(3,1fr)}}@media(max-width:800px){.app-shell.collapsed{display:block}.sidebar.collapsed{width:100%}.collapse-button{display:none}.theme-popover,.sidebar.collapsed .theme-popover{left:14px;right:14px;bottom:14px;width:auto}.analysis-tabs{grid-template-columns:1fr}.analysis-toolbar{gap:12px}.duplicate-controls .button{flex:1}.age-grid{grid-template-columns:1fr 1fr}.history-list>div{grid-template-columns:1fr 1fr}.history-list em{grid-column:1 / -1;text-align:left}.old-file-rows>div:not(.no-matches){height:auto;grid-template-columns:20px 1fr 70px;padding:10px}.old-file-rows em{grid-column:2}.old-file-rows button{grid-column:3;grid-row:1}.duplicate-paths{margin-left:0}}
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

.settings-backdrop{position:fixed;inset:0;background:#10182852;z-index:40;display:flex;justify-content:flex-end;backdrop-filter:blur(2px)}.settings-drawer{width:min(460px,100%);height:100%;overflow:auto;background:#fff;color:#273443;border-left:1px solid #dfe3e8;box-shadow:-18px 0 50px #1018282b;padding:24px}.settings-head{display:flex;align-items:flex-start;justify-content:space-between}.settings-head h2{margin:4px 0 0}.settings-tabs{display:grid;grid-template-columns:1fr 1fr;background:#f2f4f7;border-radius:6px;padding:3px;margin:22px 0}.settings-tabs button{height:38px;border:0;border-radius:4px;background:transparent;color:#667085;display:flex;align-items:center;justify-content:center;gap:8px;font-weight:650}.settings-tabs button.active{background:#fff;color:var(--accent-ink);box-shadow:0 2px 8px #10182814}.settings-content{display:grid;gap:0}.setting-section{padding:18px 0;border-bottom:1px solid #eaecf0}.setting-section:first-child{padding-top:4px}.setting-title{display:flex;justify-content:space-between;align-items:center;gap:14px;margin-bottom:13px}.setting-title b,.setting-title small{display:block}.setting-title b{font-size:var(--ui-body-font)}.setting-title small{margin-top:4px;color:#7d8896}.setting-title>span{color:var(--accent-ink);font-size:var(--ui-small-font);white-space:nowrap}.settings-theme-options{grid-template-columns:1fr 1fr}.settings-theme-options button{height:56px}.preference-segments{display:grid;grid-auto-flow:column;grid-auto-columns:1fr;background:#f2f4f7;border-radius:6px;padding:3px}.preference-segments button{height:36px;border:0;border-radius:4px;background:transparent;color:#667085;font-weight:600}.preference-segments button.active{background:#fff;color:var(--accent-ink);box-shadow:0 2px 7px #10182814}.icon-size-preview{width:36px;height:36px;border-radius:5px;background:var(--accent-soft);display:grid;place-items:center;color:var(--accent-ink)}.settings-footnote{display:flex;align-items:center;gap:8px;color:#4d7f6d;margin:16px 0 0!important}
.about-product{display:flex;align-items:center;gap:15px;padding:6px 0 20px;border-bottom:1px solid #eaecf0}.about-mark{width:58px;height:58px;flex:none;border-radius:8px;background:var(--accent-gradient);color:var(--accent-contrast);display:grid;place-items:center;box-shadow:0 8px 20px color-mix(in srgb,var(--accent) 25%,transparent)}.about-product h3{margin:0 0 4px}.about-product p{margin:0 0 8px!important;color:#667085}.about-product b{font-size:var(--ui-small-font);color:var(--accent-ink);background:var(--accent-soft);padding:3px 7px;border-radius:3px}.about-section{padding:19px 0;border-bottom:1px solid #eaecf0}.about-section h4{margin:0 0 10px;font-size:var(--ui-body-font)}.about-section dl{margin:0}.about-section dl>div{min-height:34px;display:flex;justify-content:space-between;gap:16px;align-items:center}.about-section dt{color:#667085}.about-section dd{margin:0;text-align:right;color:#273443;font-weight:600}.about-safety{display:flex;align-items:flex-start;gap:11px;margin-top:20px;padding:14px;background:#effaf6;border:1px solid #ccecdf;border-radius:6px;color:#087355}.about-safety p{margin:4px 0 0!important;line-height:1.55;color:#397b69}

.settings-drawer{width:min(540px,100%)}.settings-tabs{grid-template-columns:repeat(4,1fr)}.exclusion-list{display:grid;border:1px solid #e4e7eb;border-radius:5px;overflow:hidden}.exclusion-list>div{min-height:38px;padding:6px 9px;display:grid;grid-template-columns:17px minmax(0,1fr) 28px;gap:8px;align-items:center;border-bottom:1px solid #edf0f2;color:#667085}.exclusion-list>div:last-child{border:0}.exclusion-list span{white-space:nowrap;overflow:hidden;text-overflow:ellipsis;color:#344054}.exclusion-list button,.setting-path button{border:0;background:transparent;color:#98a2b3;display:grid;place-items:center}.setting-empty{margin:0!important;padding:16px;border:1px dashed #d0d5dd;border-radius:5px;text-align:center;color:#98a2b3}.setting-path{min-height:40px;padding:7px 10px;border:1px solid #d7dce2;border-radius:5px;display:grid;grid-template-columns:minmax(0,1fr) 28px;align-items:center}.setting-path span{white-space:nowrap;overflow:hidden;text-overflow:ellipsis;color:#475467}.toggle-switch{width:42px;height:24px;border:0;border-radius:12px;background:#d0d5dd;padding:3px;transition:background .16s}.toggle-switch i{display:block;width:18px;height:18px;border-radius:50%;background:#fff;box-shadow:0 1px 3px #10182833;transition:transform .16s}.toggle-switch.active{background:var(--accent)}.toggle-switch.active i{transform:translateX(18px)}.setting-action-row{display:flex;align-items:center;justify-content:space-between;gap:12px}.setting-action-row>span{color:#7d8896}.setting-action-row>span.success{color:#218b68}.setting-action-row>span.update{color:#c94331;font-weight:650}.system-actions>div{min-height:65px;display:flex;align-items:center;justify-content:space-between;gap:15px;border-bottom:1px solid #edf0f2}.system-actions>div:last-child{border:0}.system-actions b,.system-actions small{display:block}.system-actions small{color:#7d8896;margin-top:4px}.dialog-icon.history{background:#fff0f0;color:#c94331}

:root[data-density="compact"] .main-nav button{height:36px}:root[data-density="compact"] .drive-button{padding-top:7px;padding-bottom:7px}:root[data-density="compact"] td{height:46px}:root[data-density="compact"] .result-toolbar{height:52px}:root[data-density="compact"] .cleanup-row{min-height:72px}:root[data-density="compact"] .folder-analysis-row{min-height:74px}:root[data-density="compact"] .old-file-rows>div:not(.no-matches){height:52px}
:root[data-density="comfortable"] .main-nav button{height:44px}:root[data-density="comfortable"] .drive-button{padding-top:11px;padding-bottom:11px}:root[data-density="comfortable"] td{height:58px}:root[data-density="comfortable"] .result-toolbar{height:64px}:root[data-density="comfortable"] .cleanup-row{min-height:92px}:root[data-density="comfortable"] .folder-analysis-row{min-height:96px}:root[data-density="comfortable"] .old-file-rows>div:not(.no-matches){height:64px}

@media(max-width:800px){.sidebar-head{grid-column:1;grid-row:1;margin:0}.sidebar-head .brand{padding:0}.collapse-button{display:none!important}.sidebar.collapsed .sidebar-head{display:block;margin:0}.sidebar.collapsed .brand>div,.sidebar.collapsed .main-nav span,.sidebar.collapsed .main-nav b{display:block}.sidebar.collapsed .main-nav button{display:grid;grid-template-columns:18px 1fr auto;padding:0 9px}.settings-trigger{grid-column:2;grid-row:1;width:42px;height:38px;min-height:38px;margin:0;padding:0;grid-template-columns:1fr;place-items:center}.settings-trigger span,.settings-trigger>svg:last-child,.sidebar.collapsed .settings-trigger span,.sidebar.collapsed .settings-trigger>svg:last-child{display:none}.settings-drawer{width:min(430px,100%);padding:19px}.settings-theme-options{grid-template-columns:1fr 1fr}}
</style>
