// --- Trend analysis helper functions ---

export function parseReferenceRange(ref: string): { min: number; max: number } | null {
  const m = ref.match(/^([\d.]+)\s*[-~]\s*([\d.]+)$/)
  if (!m) return null
  const min = parseFloat(m[1])
  const max = parseFloat(m[2])
  if (isNaN(min) || isNaN(max)) return null
  return { min, max }
}

export function niceRange(min: number, max: number): { lo: number; hi: number; step: number } {
  if (min === max) {
    const offset = Math.abs(min) * 0.1 || 1
    min -= offset
    max += offset
  }
  const range = max - min
  const rawStep = range / 5
  const magnitude = Math.pow(10, Math.floor(Math.log10(rawStep)))
  const normalized = rawStep / magnitude
  let step: number
  if (normalized <= 1) step = magnitude
  else if (normalized <= 2) step = 2 * magnitude
  else if (normalized <= 5) step = 5 * magnitude
  else step = 10 * magnitude

  const lo = Math.floor(min / step) * step
  const hi = Math.ceil(max / step) * step
  return { lo, hi, step }
}

export function formatDate(dateStr: string): string {
  const d = new Date(dateStr)
  const mm = String(d.getMonth() + 1).padStart(2, '0')
  const dd = String(d.getDate()).padStart(2, '0')
  return `${mm}-${dd}`
}

export function statusColor(status: string): string {
  if (status === 'critical_high') return 'var(--error)'
  if (status === 'high') return 'var(--error)'
  if (status === 'critical_low') return 'var(--error)'
  if (status === 'low') return 'var(--info)'
  return 'var(--success)'
}

import type { TrendItemInfo } from '@/api/types'

export function groupItems(items: TrendItemInfo[]): Map<string, TrendItemInfo[]> {
  const groups = new Map<string, TrendItemInfo[]>()
  for (const item of items) {
    const key = item.report_type
    const list = groups.get(key)
    if (list) list.push(item)
    else groups.set(key, [item])
  }
  return groups
}
