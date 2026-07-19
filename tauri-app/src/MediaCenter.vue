<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { open } from '@tauri-apps/plugin-dialog'
import {
  AlertTriangle,
  Check,
  CircleStop,
  ExternalLink,
  Eye,
  FileAudio,
  FileVideo,
  Files,
  FolderOpen,
  HardDrive,
  Image as ImageIcon,
  Images,
  LoaderCircle,
  Music2,
  Play,
  Recycle,
  ScanSearch,
  ShieldCheck,
  Video,
  X,
} from '@lucide/vue'
import { appendActivity } from './activityLog'

interface MediaItem {
  path: string
  name: string
  kind: 'image' | 'video' | 'audio'
  format: string
  size: number
  modifiedDays?: number | null
  width?: number | null
  height?: number | null
  durationMs?: number | null
  codec?: string | null
  bitrate?: number | null
  sampleRate?: number | null
  lossless: boolean
  screenshot: boolean
  blurry: boolean
  blurScore?: number | null
  oversized: boolean
  exactGroup?: number | null
  similarGroup?: number | null
  thumbnail?: string | null
}

interface MediaScanResult {
  scope: string
  items: MediaItem[]
  imageCount: number
  videoCount: number
  audioCount: number
  imageBytes: number
  videoBytes: number
  audioBytes: number
  exactGroups: number
  similarGroups: number
  duplicateBytes: number
  screenshotCount: number
  blurryCount: number
  oversizedCount: number
  elapsedMs: number
  skippedItems: number
  ffprobeAvailable: boolean
  truncated: boolean
}

interface RecycleResult { recycledFiles: number; recycledBytes: number; failedItems: number }
interface Progress { message: string; percentage: number; currentPath?: string }

const props = defineProps<{
  exclusions: string[]
  largeFileMb: number
  scanThreads: number
  recyclePolicy: 'confirm' | 'direct'
  drives: string[]
  selectedDrive: string
}>()

const isTauri = '__TAURI_INTERNALS__' in window
const result = ref<MediaScanResult | null>(null)
const scanning = ref(false)
const recycling = ref(false)
const scope = ref('')
const driveChoice = ref(props.selectedDrive || props.drives[0] || 'C:')
const progress = ref<Progress>({ message: '等待选择媒体文件夹', percentage: 0 })
const error = ref('')
const notice = ref('')
/** 扫描阶段类型：只收集/分析所选类型，可显著加速整盘 */
const scanKinds = ref<'all' | 'image' | 'video' | 'audio'>('all')
const selectedKind = ref<'all' | 'image' | 'video' | 'audio'>('all')
const issueFilter = ref<'all' | 'exact' | 'similar' | 'screenshot' | 'blurry' | 'oversized' | 'lossless'>('all')
const selectedPaths = ref<string[]>([])
const previewItem = ref<MediaItem | null>(null)
const confirmRecycle = ref(false)
let unlisten: UnlistenFn | undefined

const selectedItems = computed(() => result.value?.items.filter(item => selectedPaths.value.includes(item.path)) ?? [])
const selectedBytes = computed(() => selectedItems.value.reduce((sum, item) => sum + item.size, 0))
const filteredItems = computed(() => result.value?.items.filter(item => {
  if (selectedKind.value !== 'all' && item.kind !== selectedKind.value) return false
  return issueFilter.value === 'all'
    || (issueFilter.value === 'exact' && !!item.exactGroup)
    || (issueFilter.value === 'similar' && !!item.similarGroup)
    || (issueFilter.value === 'screenshot' && item.screenshot)
    || (issueFilter.value === 'blurry' && item.blurry)
    || (issueFilter.value === 'oversized' && item.oversized)
    || (issueFilter.value === 'lossless' && item.lossless)
}) ?? [])

function formatSize(bytes = 0) {
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  let value = bytes
  let unit = 0
  while (value >= 1024 && unit < units.length - 1) { value /= 1024; unit += 1 }
  return `${value.toFixed(unit < 2 ? 0 : 1)} ${units[unit]}`
}

function formatDuration(milliseconds?: number | null) {
  if (!milliseconds) return '时长未知'
  const seconds = Math.round(milliseconds / 1000)
  const hours = Math.floor(seconds / 3600)
  const minutes = Math.floor(seconds % 3600 / 60)
  const remainder = seconds % 60
  return hours ? `${hours}:${String(minutes).padStart(2, '0')}:${String(remainder).padStart(2, '0')}` : `${minutes}:${String(remainder).padStart(2, '0')}`
}

function formatBitrate(value?: number | null) {
  return value ? `${Math.round(value / 1000)} kbps` : '码率未知'
}

function itemMetadata(item: MediaItem) {
  if (item.kind === 'image') return item.width && item.height ? `${item.width} x ${item.height}` : '尺寸未知'
  if (item.kind === 'video') return `${formatDuration(item.durationMs)} · ${item.width && item.height ? `${item.width} x ${item.height}` : '分辨率未知'} · ${item.codec || item.format} · ${formatBitrate(item.bitrate)}`
  return `${formatDuration(item.durationMs)} · ${item.codec || item.format} · ${formatBitrate(item.bitrate)}${item.sampleRate ? ` · ${Math.round(item.sampleRate / 1000)} kHz` : ''}`
}

function buildPreview(path: string): MediaScanResult {
  const items: MediaItem[] = [
    { path: `${path}\\Photos\\IMG_2048.jpg`, name: 'IMG_2048.jpg', kind: 'image', format: 'JPG', size: 18_245_632, width: 6048, height: 4024, lossless: false, screenshot: false, blurry: false, blurScore: 186, oversized: true, exactGroup: 1, similarGroup: 1 },
    { path: `${path}\\Backups\\IMG_2048-copy.jpg`, name: 'IMG_2048-copy.jpg', kind: 'image', format: 'JPG', size: 18_245_632, width: 6048, height: 4024, lossless: false, screenshot: false, blurry: false, blurScore: 186, oversized: true, exactGroup: 1, similarGroup: 1 },
    { path: `${path}\\Screenshots\\Screenshot 2026-07-15.png`, name: 'Screenshot 2026-07-15.png', kind: 'image', format: 'PNG', size: 4_512_930, width: 2560, height: 1440, lossless: false, screenshot: true, blurry: false, blurScore: 140, oversized: false },
    { path: `${path}\\Photos\\soft-focus.jpg`, name: 'soft-focus.jpg', kind: 'image', format: 'JPG', size: 3_145_728, width: 4000, height: 3000, lossless: false, screenshot: false, blurry: true, blurScore: 24, oversized: false, similarGroup: 2 },
    { path: `${path}\\Videos\\holiday-4k.mp4`, name: 'holiday-4k.mp4', kind: 'video', format: 'MP4', size: 4_831_838_208, width: 3840, height: 2160, durationMs: 742000, codec: 'H264', bitrate: 48_000_000, lossless: false, screenshot: false, blurry: false, oversized: true },
    { path: `${path}\\Music\\album-master.flac`, name: 'album-master.flac', kind: 'audio', format: 'FLAC', size: 184_549_376, durationMs: 312000, codec: 'FLAC', bitrate: 4_200_000, sampleRate: 96000, lossless: true, screenshot: false, blurry: false, oversized: true },
  ]
  return {
    scope: path, items, imageCount: 4, videoCount: 1, audioCount: 1,
    imageBytes: items.filter(item => item.kind === 'image').reduce((sum, item) => sum + item.size, 0),
    videoBytes: items.filter(item => item.kind === 'video').reduce((sum, item) => sum + item.size, 0),
    audioBytes: items.filter(item => item.kind === 'audio').reduce((sum, item) => sum + item.size, 0),
    exactGroups: 1, similarGroups: 2, duplicateBytes: 18_245_632, screenshotCount: 1,
    blurryCount: 1, oversizedCount: 4, elapsedMs: 4820, skippedItems: 2,
    ffprobeAvailable: false, truncated: false,
  }
}

async function scanMedia(path = scope.value) {
  if (!path) return
  scope.value = path
  scanning.value = true
  selectedPaths.value = []
  error.value = ''
  notice.value = ''
  progress.value = { message: '正在启动媒体分析', percentage: 1, currentPath: path }
  try {
    result.value = isTauri
      ? await invoke<MediaScanResult>('scan_media', {
          path,
          options: {
            exclusions: props.exclusions,
            largeFileBytes: props.largeFileMb * 1024 * 1024,
            threads: props.scanThreads,
            kinds: scanKinds.value,
          },
        })
      : buildPreview(path)
    // 扫描后默认筛选对齐扫描类型，避免「只扫了视频却显示全部为空」的困惑
    if (scanKinds.value !== 'all') selectedKind.value = scanKinds.value
  } catch (value) {
    if (String(value).includes('已取消')) notice.value = '媒体分析已取消。'
    else error.value = String(value)
  } finally { scanning.value = false }
}

async function chooseFolder() {
  if (!isTauri) { await scanMedia('D:\\Media'); return }
  try {
    const selected = await open({ directory: true, multiple: false, title: '选择图片、视频和音频所在文件夹' })
    if (typeof selected === 'string') await scanMedia(selected)
  } catch (value) { error.value = String(value) }
}

async function scanDrive() {
  if (!driveChoice.value) return
  const root = driveChoice.value.endsWith('\\') ? driveChoice.value : `${driveChoice.value}\\`
  await scanMedia(root)
}

async function cancel() {
  try { await invoke('cancel_scan') } catch (value) { error.value = String(value) }
}

function toggleSelection(path: string) {
  selectedPaths.value = selectedPaths.value.includes(path)
    ? selectedPaths.value.filter(value => value !== path)
    : [...selectedPaths.value, path]
}

function selectDuplicateCopies() {
  if (!result.value) return
  const groups = new Map<number, MediaItem[]>()
  result.value.items.filter(item => item.exactGroup).forEach(item => {
    const group = groups.get(item.exactGroup!) ?? []
    group.push(item)
    groups.set(item.exactGroup!, group)
  })
  selectedPaths.value = [...groups.values()].flatMap(items => items.slice(1).map(item => item.path))
  issueFilter.value = 'exact'
}

async function requestRecycle() {
  if (!selectedPaths.value.length) return
  if (props.recyclePolicy === 'direct') await runRecycle()
  else confirmRecycle.value = true
}

async function runRecycle() {
  if (!selectedPaths.value.length) return
  recycling.value = true
  error.value = ''
  try {
    if (!isTauri) {
      notice.value = `界面预览：将有 ${selectedPaths.value.length} 个文件移入 Windows 回收站。`
      confirmRecycle.value = false
      return
    }
    const paths = [...selectedPaths.value]
    const recycled = await invoke<RecycleResult>('recycle_media', { paths })
    if (result.value) result.value.items = result.value.items.filter(item => !paths.includes(item.path))
    selectedPaths.value = []
    confirmRecycle.value = false
    notice.value = `已将 ${recycled.recycledFiles} 个文件（${formatSize(recycled.recycledBytes)}）移入回收站${recycled.failedItems ? `，${recycled.failedItems} 项未处理` : ''}。建议重新扫描以刷新汇总。`
    appendActivity(
      'recycle',
      `媒体回收：${recycled.recycledFiles} 个文件（${formatSize(recycled.recycledBytes)}）`,
      recycled.failedItems ? `${recycled.failedItems} 项未处理` : undefined,
    )
  } catch (value) { error.value = String(value) }
  finally { recycling.value = false }
}

async function openInExplorer(path: string) {
  if (!isTauri) return
  try { await invoke('open_in_explorer', { path, selectFile: true }) } catch (value) { error.value = String(value) }
}

async function openMedia(path: string) {
  if (!isTauri) return
  try { await invoke('open_media_file', { path }) } catch (value) { error.value = String(value) }
}

onMounted(async () => {
  if (isTauri) unlisten = await listen<Progress>('media-progress', event => { progress.value = event.payload })
})
watch(() => props.selectedDrive, value => {
  if (value && props.drives.includes(value)) driveChoice.value = value
})
onBeforeUnmount(() => { unlisten?.() })
</script>

<template>
  <div class="media-center">
    <div v-if="error" class="media-alert error"><AlertTriangle :size="17" /><span>{{ error }}</span><button title="关闭" @click="error = ''"><X :size="16" /></button></div>
    <div v-if="notice" class="media-alert notice"><Check :size="17" /><span>{{ notice }}</span><button title="关闭" @click="notice = ''"><X :size="16" /></button></div>

    <section class="section-head media-scope">
      <span class="scope-icon"><Images :size="23" /></span>
      <div><span class="panel-kicker">媒体分析范围</span><h2>{{ scope || '尚未选择文件夹或磁盘' }}</h2><p>分析整个磁盘或指定文件夹，图片相似度、音视频属性和重复校验均在本机完成。</p></div>
      <div class="scope-actions">
        <label class="drive-scope" title="扫描媒体类型"><select v-model="scanKinds" :disabled="scanning" aria-label="扫描类型"><option value="all">全部类型</option><option value="image">仅图片</option><option value="video">仅视频</option><option value="audio">仅音频</option></select></label>
        <label class="drive-scope" title="选择要分析的磁盘"><HardDrive :size="15" /><select v-model="driveChoice" :disabled="scanning"><option v-for="drive in drives" :key="drive" :value="drive">{{ drive }} 整盘</option></select></label>
        <button class="button secondary" :disabled="scanning || !driveChoice" @click="scanDrive"><HardDrive :size="16" /> 分析整盘</button>
        <button class="button secondary" :disabled="scanning" @click="chooseFolder"><FolderOpen :size="16" /> 选择文件夹</button>
        <button v-if="scanning" class="button danger" @click="cancel"><CircleStop :size="16" /> 取消</button>
        <button v-else-if="scope" class="button primary" @click="scanMedia()"><Play :size="16" /> 重新分析</button>
      </div>
    </section>

    <section v-if="scanning" class="media-progress scan-strip"><div><LoaderCircle :size="17" class="spin" /><strong>{{ progress.message }}</strong><b>{{ progress.percentage }}%</b></div><i><em :style="{ width: `${progress.percentage}%` }" /></i><small :title="progress.currentPath">{{ progress.currentPath }}</small></section>

    <template v-if="result">
      <section class="media-metrics">
        <div><span class="media-metric-icon image"><ImageIcon :size="20" /></span><div><small>图片</small><b>{{ result.imageCount }}</b><span>{{ formatSize(result.imageBytes) }}</span></div></div>
        <div><span class="media-metric-icon video"><Video :size="20" /></span><div><small>视频</small><b>{{ result.videoCount }}</b><span>{{ formatSize(result.videoBytes) }}</span></div></div>
        <div><span class="media-metric-icon audio"><Music2 :size="20" /></span><div><small>音频</small><b>{{ result.audioCount }}</b><span>{{ formatSize(result.audioBytes) }}</span></div></div>
        <div><span class="media-metric-icon duplicate"><Files :size="20" /></span><div><small>完全重复</small><b>{{ result.exactGroups }} 组</b><span>可避免 {{ formatSize(result.duplicateBytes) }}</span></div></div>
      </section>

      <section class="section-head media-toolbar">
        <div class="chip-row media-kind-tabs"><button v-for="item in [{ id: 'all', label: '全部' }, { id: 'image', label: '图片' }, { id: 'video', label: '视频' }, { id: 'audio', label: '音频' }]" :key="item.id" type="button" class="chip-btn" :class="{ active: selectedKind === item.id }" @click="selectedKind = item.id as typeof selectedKind">{{ item.label }}</button></div>
        <label class="media-filter"><ScanSearch :size="16" /><select v-model="issueFilter"><option value="all">全部状态</option><option value="exact">完全重复</option><option value="similar">相似图片</option><option value="screenshot">截图</option><option value="blurry">可能模糊</option><option value="oversized">超大媒体</option><option value="lossless">无损音频</option></select></label>
        <button class="text-button" :disabled="!result.exactGroups" @click="selectDuplicateCopies">选择重复副本</button>
        <button class="button recycle-button" :disabled="!selectedPaths.length || recycling" @click="requestRecycle"><Recycle :size="16" /> 移入回收站 · {{ selectedPaths.length }}</button>
      </section>

      <section class="media-results panel">
        <header><div><span class="panel-kicker">媒体审查</span><h2>{{ filteredItems.length }} 个结果</h2></div><div><span>{{ result.ffprobeAvailable ? '完整视频元数据' : '基础视频元数据' }}</span><span v-if="result.truncated">仅显示最大的 2000 项</span></div></header>
        <div class="media-rows">
          <article v-for="item in filteredItems" :key="item.path" class="media-row" :class="{ selected: selectedPaths.includes(item.path) }">
            <button class="media-check" :class="{ checked: selectedPaths.includes(item.path) }" :title="selectedPaths.includes(item.path) ? '取消选择' : '选择文件'" @click="toggleSelection(item.path)"><Check v-if="selectedPaths.includes(item.path)" :size="14" /></button>
            <div class="media-thumb" :class="item.kind"><img v-if="item.thumbnail" :src="item.thumbnail" :alt="item.name"><ImageIcon v-else-if="item.kind === 'image'" :size="25" /><FileVideo v-else-if="item.kind === 'video'" :size="25" /><FileAudio v-else :size="25" /></div>
            <div class="media-copy"><div><b :title="item.name">{{ item.name }}</b><span class="format-badge">{{ item.format }}</span></div><p>{{ itemMetadata(item) }}</p><small :title="item.path">{{ item.path }}</small><div class="media-flags"><span v-if="item.exactGroup" class="critical">重复组 {{ item.exactGroup }}</span><span v-if="item.similarGroup" class="similar">相似组 {{ item.similarGroup }}</span><span v-if="item.screenshot">截图</span><span v-if="item.blurry" class="warning">可能模糊</span><span v-if="item.oversized" class="warning">超大</span><span v-if="item.lossless">无损</span></div></div>
            <div class="media-size"><b>{{ formatSize(item.size) }}</b><span>{{ item.modifiedDays == null ? '时间未知' : `${item.modifiedDays} 天前修改` }}</span></div>
            <div class="media-actions"><button title="预览详情" @click="previewItem = item"><Eye :size="16" /></button><button title="用系统默认程序打开" @click="openMedia(item.path)"><Play :size="15" /></button><button title="在资源管理器中定位" @click="openInExplorer(item.path)"><ExternalLink :size="16" /></button></div>
          </article>
          <div v-if="!filteredItems.length" class="media-empty">当前筛选条件下没有媒体文件</div>
        </div>
        <footer><ShieldCheck :size="16" /><span>相似、模糊和长期未修改都只是复核线索。应用不会自动选择或永久删除个人媒体。</span></footer>
      </section>
    </template>

    <section v-else-if="!scanning" class="media-welcome panel"><div><Images :size="45" /></div><h2>整理图片、视频和音频</h2><p>分析整个磁盘或指定媒体文件夹，查找完全重复与相似图片，统计音视频属性，并将确认不要的内容移入回收站。</p><div class="welcome-actions"><button class="button primary" :disabled="!driveChoice" @click="scanDrive"><HardDrive :size="17" /> 分析 {{ driveChoice }} 整盘</button><button class="button secondary" @click="chooseFolder"><FolderOpen :size="17" /> 选择媒体文件夹</button></div></section>

    <div v-if="previewItem" class="media-modal" @click.self="previewItem = null"><section class="media-preview" role="dialog" aria-modal="true" aria-label="媒体预览"><button class="preview-close" title="关闭预览" @click="previewItem = null"><X :size="18" /></button><div class="preview-visual" :class="previewItem.kind"><img v-if="previewItem.thumbnail" :src="previewItem.thumbnail" :alt="previewItem.name"><ImageIcon v-else-if="previewItem.kind === 'image'" :size="54" /><FileVideo v-else-if="previewItem.kind === 'video'" :size="54" /><FileAudio v-else :size="54" /></div><div class="preview-copy"><span class="panel-kicker">媒体详情</span><h2>{{ previewItem.name }}</h2><p>{{ itemMetadata(previewItem) }}</p><dl><div><dt>大小</dt><dd>{{ formatSize(previewItem.size) }}</dd></div><div><dt>格式</dt><dd>{{ previewItem.format }}</dd></div><div v-if="previewItem.blurScore != null"><dt>清晰度分数</dt><dd>{{ previewItem.blurScore.toFixed(1) }}</dd></div><div><dt>路径</dt><dd :title="previewItem.path">{{ previewItem.path }}</dd></div></dl><div class="preview-actions"><button class="button secondary" @click="openInExplorer(previewItem.path)"><ExternalLink :size="16" /> 定位文件</button><button class="button primary" @click="openMedia(previewItem.path)"><Play :size="16" /> {{ previewItem.kind === 'image' ? '系统查看' : '系统播放器预览' }}</button></div></div></section></div>

    <div v-if="confirmRecycle" class="media-modal" @click.self="confirmRecycle = false"><section class="recycle-dialog" role="dialog" aria-modal="true" aria-label="确认移入回收站"><span><Recycle :size="26" /></span><h2>将 {{ selectedPaths.length }} 个文件移入回收站？</h2><p>合计 {{ formatSize(selectedBytes) }}。文件不会永久删除，可在 Windows 回收站中恢复。</p><div><button class="button secondary" :disabled="recycling" @click="confirmRecycle = false">取消</button><button class="button recycle-button" :disabled="recycling" @click="runRecycle"><LoaderCircle v-if="recycling" :size="16" class="spin" /><Recycle v-else :size="16" /> 确认移入回收站</button></div></section></div>
  </div>
</template>

<style scoped>
.media-center{display:grid;gap:14px}.media-alert{min-height:42px;padding:9px 12px;border-radius:5px;display:flex;align-items:center;gap:9px}.media-alert span{flex:1}.media-alert button{border:0;background:transparent;color:inherit;display:grid;place-items:center}.media-alert.error{background:#fff0f0;border:1px solid #ffcaca;color:#a52b2b}.media-alert.notice{background:#effaf6;border:1px solid #ccecdf;color:#087355}.media-scope{display:grid;grid-template-columns:44px minmax(0,1fr) auto;align-items:center;gap:13px}.scope-icon{width:44px;height:44px;border-radius:6px;background:var(--accent-soft);color:var(--accent-ink);display:grid;place-items:center}.media-scope h2{margin:3px 0;font-size:15px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.media-scope p{margin:0;color:#667085}.scope-actions{display:flex;gap:8px}.media-progress{padding-top:13px;padding-bottom:13px}.media-progress>div{display:flex;align-items:center;gap:8px}.media-progress strong{flex:1}.media-progress b{color:var(--accent)}.media-progress>i{display:block;height:5px;background:#edf0f2;border-radius:3px;overflow:hidden;margin:10px 0 6px}.media-progress>i em{display:block;height:100%;background:var(--accent-gradient)}.media-progress small{display:block;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;color:#8a94a3}.media-metrics{display:grid;grid-template-columns:repeat(4,1fr);background:#fff;border:1px solid #dfe3e8;border-radius:6px}.media-metrics>div{min-height:92px;padding:16px 18px;display:flex;align-items:center;gap:11px;border-right:1px solid #e7eaee}.media-metrics>div:last-child{border:0}.media-metric-icon{width:38px;height:38px;border-radius:6px;display:grid;place-items:center}.media-metric-icon.image{background:#eaf4ff;color:#3182f6}.media-metric-icon.video{background:#fff0f5;color:#e94b72}.media-metric-icon.audio{background:#e8fbf4;color:#12a47b}.media-metric-icon.duplicate{background:#fff5e6;color:#e18a00}.media-metrics small,.media-metrics b,.media-metrics span{display:block}.media-metrics b{font-size:18px;margin:2px 0}.media-metrics small,.media-metrics>div>div>span{color:#7d8896}.media-toolbar{padding:9px;display:flex;align-items:center;gap:9px}.media-kind-tabs{display:flex;align-self:stretch;background:#f2f4f7;border-radius:5px;padding:2px}.media-kind-tabs button{min-width:56px;border:0;background:transparent;color:#667085;border-radius:4px;padding:0 10px}.media-kind-tabs button.active{background:#fff;color:var(--accent-ink);box-shadow:0 1px 4px #1018281a}.media-filter{height:36px;min-width:150px;border:1px solid #d7dce2;border-radius:5px;display:flex;align-items:center;gap:7px;padding:0 9px;color:#667085}.media-filter select{border:0;background:transparent;outline:0;flex:1;color:#344054}.media-toolbar>.text-button{margin-left:auto}.recycle-button{height:38px;border:0;border-radius:5px;background:#eaf8f3;color:#087355;display:inline-flex;align-items:center;justify-content:center;gap:7px;padding:0 13px;font-weight:650;white-space:nowrap}.recycle-button:hover{background:#d9f2e8}.recycle-button:disabled{opacity:.45;cursor:not-allowed}.media-results{padding:0;overflow:hidden}.media-results>header{min-height:66px;padding:13px 17px;display:flex;align-items:center;justify-content:space-between;border-bottom:1px solid #e4e7eb}.media-results h2{margin:3px 0}.media-results>header>div:last-child{display:flex;gap:7px}.media-results>header>div:last-child span{padding:3px 7px;border-radius:3px;background:#f2f4f7;color:#667085}.media-row{min-height:94px;padding:11px 15px;display:grid;grid-template-columns:22px 78px minmax(260px,1fr) 112px 102px;gap:12px;align-items:center;border-bottom:1px solid #edf0f2}.media-row.selected{background:var(--accent-soft)}.media-check{width:20px;height:20px;padding:0;border:1px solid #b8c0ca;background:#fff;border-radius:4px;color:#fff;display:grid;place-items:center}.media-check.checked{background:var(--accent);border-color:var(--accent)}.media-thumb{width:78px;height:64px;border-radius:5px;background:#eef2f6;color:#667085;display:grid;place-items:center;overflow:hidden}.media-thumb.image{background:#eaf4ff;color:#3182f6}.media-thumb.video{background:#fff0f5;color:#e94b72}.media-thumb.audio{background:#e8fbf4;color:#12a47b}.media-thumb img{width:100%;height:100%;object-fit:cover}.media-copy{min-width:0}.media-copy>div:first-child{display:flex;align-items:center;gap:7px}.media-copy b{max-width:420px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.format-badge{padding:2px 5px;border-radius:3px;background:#f2f4f7;color:#667085}.media-copy p{margin:4px 0;color:#475467;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.media-copy small{display:block;color:#98a2b3;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.media-flags{display:flex;gap:5px;margin-top:5px;flex-wrap:wrap}.media-flags span{padding:2px 5px;border-radius:3px;background:#eef2f6;color:#667085}.media-flags .critical{background:#fff0f0;color:#c43232}.media-flags .similar{background:#f0e9ff;color:#6842a1}.media-flags .warning{background:#fff5e6;color:#9a6500}.media-size{text-align:right}.media-size b,.media-size span{display:block}.media-size span{color:#98a2b3;margin-top:4px}.media-actions{display:flex;justify-content:flex-end;gap:3px}.media-actions button{width:30px;height:30px;border:0;background:transparent;color:#7d8896;border-radius:4px;display:grid;place-items:center}.media-actions button:hover{background:#fff;color:var(--accent-ink)}.media-empty{padding:40px;text-align:center;color:#98a2b3}.media-results>footer{min-height:42px;padding:10px 16px;background:#f7faf9;color:#4d7f6d;display:flex;align-items:center;gap:8px}.media-welcome{min-height:430px;display:flex;flex-direction:column;align-items:center;justify-content:center;text-align:center}.media-welcome>div{width:88px;height:88px;border-radius:50%;background:var(--accent-soft);color:var(--accent-ink);display:grid;place-items:center}.media-welcome h2{margin:16px 0 6px}.media-welcome p{max-width:580px;color:#667085;line-height:1.7;margin:0 0 18px}.media-modal{position:fixed;inset:0;background:#10182880;z-index:60;display:grid;place-items:center;padding:20px}.media-preview{width:min(780px,100%);max-height:calc(100vh - 40px);overflow:auto;background:#fff;border-radius:7px;box-shadow:0 22px 60px #10182855;display:grid;grid-template-columns:minmax(280px,1fr) minmax(300px,.9fr);position:relative}.preview-close{position:absolute;right:13px;top:13px;z-index:2;width:32px;height:32px;border:0;background:#ffffffdd;color:#667085;border-radius:4px;display:grid;place-items:center}.preview-visual{min-height:390px;background:#eef2f6;color:#667085;display:grid;place-items:center;overflow:hidden}.preview-visual.image{background:#eaf4ff;color:#3182f6}.preview-visual.video{background:#fff0f5;color:#e94b72}.preview-visual.audio{background:#e8fbf4;color:#12a47b}.preview-visual img{width:100%;height:100%;max-height:520px;object-fit:contain}.preview-copy{padding:32px 28px}.preview-copy h2{margin:5px 0 8px;word-break:break-word}.preview-copy>p{color:#667085;margin:0 0 18px}.preview-copy dl{margin:0}.preview-copy dl>div{min-height:38px;display:grid;grid-template-columns:100px minmax(0,1fr);gap:12px;align-items:center;border-bottom:1px solid #edf0f2}.preview-copy dt{color:#7d8896}.preview-copy dd{margin:0;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.preview-actions{display:flex;gap:8px;margin-top:22px}.recycle-dialog{width:min(430px,100%);background:#fff;border-radius:7px;padding:25px;box-shadow:0 22px 60px #10182855;text-align:center}.recycle-dialog>span{width:52px;height:52px;border-radius:7px;background:#eaf8f3;color:#087355;display:grid;place-items:center;margin:auto}.recycle-dialog h2{margin:15px 0 7px}.recycle-dialog p{color:#667085;line-height:1.6;margin:0}.recycle-dialog>div{display:flex;justify-content:center;gap:8px;margin-top:20px}.spin{animation:spin 1s linear infinite}@keyframes spin{to{transform:rotate(360deg)}}
@media(max-width:1050px){.media-metrics{grid-template-columns:1fr 1fr}.media-metrics>div:nth-child(2){border-right:0}.media-metrics>div:nth-child(-n+2){border-bottom:1px solid #e7eaee}.media-toolbar{flex-wrap:wrap}.media-toolbar>.text-button{margin-left:0}.media-row{grid-template-columns:22px 64px minmax(220px,1fr) 100px}.media-thumb{width:64px;height:58px}.media-actions{grid-column:3 / 5;justify-content:flex-start}.media-preview{grid-template-columns:1fr}.preview-visual{min-height:280px}}
@media(max-width:800px){.media-scope{grid-template-columns:40px 1fr}.scope-actions{grid-column:1 / -1;flex-wrap:wrap}.scope-actions .button{flex:1}.media-metrics{grid-template-columns:1fr 1fr}.media-kind-tabs{width:100%}.media-kind-tabs button{flex:1}.media-filter{flex:1}.media-toolbar>.text-button{margin-left:auto}.media-row{grid-template-columns:20px 54px minmax(0,1fr)}.media-thumb{width:54px;height:50px}.media-size{grid-column:3;text-align:left}.media-actions{grid-column:3;justify-content:flex-start}.preview-copy{padding:24px 20px}}
.scope-actions{align-items:center;justify-content:flex-end;flex-wrap:wrap;max-width:570px}.drive-scope{height:38px;border:1px solid #d7dce2;border-radius:5px;background:#fff;color:#667085;display:flex;align-items:center;gap:6px;padding:0 8px}.drive-scope select{border:0;background:transparent;color:#344054;outline:0}.media-welcome>.welcome-actions{width:auto;height:auto;border-radius:0;background:transparent;color:inherit;display:flex;gap:8px;flex-wrap:wrap;justify-content:center}
@media(max-width:1150px){.media-scope{grid-template-columns:44px minmax(0,1fr)}.scope-actions{grid-column:1 / -1;max-width:none;justify-content:flex-start}}

.section-head{background:transparent!important;border:0!important;box-shadow:none!important;padding:4px 2px 12px!important}
.media-scope.section-head{display:grid;grid-template-columns:44px minmax(0,1fr) auto;align-items:center;gap:13px}
.media-toolbar.section-head{display:flex;flex-wrap:wrap;gap:10px;align-items:center}
.chip-row{display:flex;flex-wrap:wrap;gap:8px;background:transparent!important;padding:0!important;border:0!important}
.chip-btn{height:34px;border:1px solid var(--u1-border-chip, var(--u1-border, #d7dce2));border-radius:999px;background:color-mix(in srgb,#fff 58%, transparent);color:#52606d;padding:0 12px;font-size:12px;font-weight:650}
.chip-btn.active{background:var(--accent-soft);border-color:var(--accent);color:var(--accent-ink)}
.media-kind-tabs{background:transparent!important;padding:0!important;border:0!important;gap:8px!important}
.media-results.panel{box-shadow:0 8px 24px #1018280a}
.media-row{border:1px solid color-mix(in srgb,#fff 35%, #edf0f2);border-radius:12px;margin-bottom:8px}

/* U1.8: media chrome sweep */
.section-head,.media-scope.section-head,.media-toolbar.section-head{
  background:transparent!important;
  border:0!important;
  box-shadow:none!important;
  backdrop-filter:none!important;
}
.chip-row,.media-kind-tabs{
  background:transparent!important;
  border:0!important;
  box-shadow:none!important;
  padding:0!important;
  gap:8px!important;
}
.media-results.panel .media-row{
  border-color:color-mix(in srgb, var(--u1-border, #edf0f2) 70%, transparent)!important;
  box-shadow:none!important;
}
.media-results>header{
  background:transparent!important;
  border-bottom:1px solid var(--u1-border-hair, var(--u1-border, #edf0f2))!important;
}
</style>
