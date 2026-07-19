/** 本机活动日志：通知 / 错误 / 清理 / 注册表修复等，便于溯源 */

export type ActivityKind =
  | 'notice'
  | 'error'
  | 'scan'
  | 'cleanup'
  | 'recycle'
  | 'registry'
  | 'backup'
  | 'system'

export interface ActivityEntry {
  id: string
  kind: ActivityKind
  title: string
  detail?: string
  at: string
  meta?: Record<string, string | number | boolean | null | undefined>
}

const STORAGE_KEY = 'disk-analyzer-activity-log'
const DETAIL_PREF_KEY = 'disk-analyzer-activity-detail'
const MAX_ENTRIES = 120
/** 详细模式下每条最多保留的列表项，控制 localStorage 体积 */
const MAX_DETAIL_LINES = 40

const listeners = new Set<() => void>()

export function isDetailedActivityLogEnabled() {
  return localStorage.getItem(DETAIL_PREF_KEY) === '1'
}

export function setDetailedActivityLogEnabled(on: boolean) {
  localStorage.setItem(DETAIL_PREF_KEY, on ? '1' : '0')
  notify()
}

export function formatDetailLines(lines: string[], limit = MAX_DETAIL_LINES) {
  const clean = lines.map(line => line.trim()).filter(Boolean)
  if (!clean.length) return undefined
  if (clean.length <= limit) return clean.join('\n')
  return `${clean.slice(0, limit).join('\n')}\n…另有 ${clean.length - limit} 项未列出`
}

function notify() {
  listeners.forEach(fn => {
    try {
      fn()
    } catch {
      /* ignore */
    }
  })
}

export function subscribeActivityLog(fn: () => void) {
  listeners.add(fn)
  return () => listeners.delete(fn)
}

export function loadActivityLog(): ActivityEntry[] {
  try {
    const raw = JSON.parse(localStorage.getItem(STORAGE_KEY) || '[]') as ActivityEntry[]
    if (!Array.isArray(raw)) return []
    return raw
      .filter(item => item && typeof item.title === 'string' && item.at)
      .slice(0, MAX_ENTRIES)
  } catch {
    return []
  }
}

function saveActivityLog(entries: ActivityEntry[]) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(entries.slice(0, MAX_ENTRIES)))
  } catch {
    /* quota */
  }
  notify()
}

export function appendActivity(
  kind: ActivityKind,
  title: string,
  detail?: string,
  meta?: ActivityEntry['meta'],
) {
  const entry: ActivityEntry = {
    id: `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
    kind,
    title,
    detail,
    at: new Date().toISOString(),
    meta,
  }
  const next = [entry, ...loadActivityLog()].slice(0, MAX_ENTRIES)
  saveActivityLog(next)
  return entry
}

export function clearActivityLog() {
  localStorage.removeItem(STORAGE_KEY)
  // migrate-away old keys
  localStorage.removeItem('disk-analyzer-message-log')
  notify()
}

export function exportActivityLogText(entries: ActivityEntry[] = loadActivityLog()) {
  const lines = entries.flatMap(item => {
    const time = new Date(item.at).toLocaleString('zh-CN')
    const head = `[${time}] [${item.kind}] ${item.title}`
    if (!item.detail) return [head]
    return [head, ...item.detail.split('\n').map(line => `  ${line}`)]
  })
  return lines.join('\n')
}

export function kindLabel(kind: ActivityKind) {
  switch (kind) {
    case 'notice':
      return '通知'
    case 'error':
      return '错误'
    case 'scan':
      return '扫描'
    case 'cleanup':
      return '清理'
    case 'recycle':
      return '回收站'
    case 'registry':
      return '注册表'
    case 'backup':
      return '备份'
    case 'system':
      return '系统'
    default:
      return kind
  }
}

/** 合并旧消息记录到活动日志（一次性） */
export function migrateLegacyMessageLog() {
  try {
    const raw = JSON.parse(localStorage.getItem('disk-analyzer-message-log') || '[]') as Array<{
      id?: string
      type?: string
      text?: string
      at?: string
    }>
    if (!Array.isArray(raw) || !raw.length) return
    const existing = loadActivityLog()
    if (existing.length) {
      localStorage.removeItem('disk-analyzer-message-log')
      return
    }
    const migrated: ActivityEntry[] = raw
      .filter(item => item?.text)
      .map(item => ({
        id: item.id || `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
        kind: item.type === 'error' ? 'error' : 'notice',
        title: item.text!,
        at: item.at || new Date().toISOString(),
      }))
    saveActivityLog(migrated)
    localStorage.removeItem('disk-analyzer-message-log')
  } catch {
    /* ignore */
  }
}
