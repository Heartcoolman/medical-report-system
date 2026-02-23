import type { Component } from 'solid-js'
import type { BadgeProps } from './Badge'
import { Badge } from './Badge'

export type TestItemStatus = 'normal' | 'high' | 'low'

type DisplayStatus = 'normal' | 'high' | 'critical_high' | 'low' | 'critical_low' | 'positive' | 'negative'

const DISPLAY_VARIANT_MAP: Record<DisplayStatus, NonNullable<BadgeProps['variant']>> = {
  normal: 'success',
  high: 'warning',
  critical_high: 'error',
  low: 'info',
  critical_low: 'error',
  positive: 'warning',
  negative: 'success',
}

const DISPLAY_LABEL_MAP: Record<DisplayStatus, string> = {
  normal: '正常',
  high: '偏高',
  critical_high: '严重偏高',
  low: '偏低',
  critical_low: '严重偏低',
  positive: '阳性',
  negative: '阴性',
}

function normalizeStatus(status: string): TestItemStatus | null {
  if (status === 'normal' || status === 'high' || status === 'low') return status
  return null
}

export interface TestItemStatusBadgeProps {
  status: string
  value?: string
  referenceRange?: string
  dot?: boolean
  class?: string
}

function isNumericValue(value: string | undefined): boolean {
  if (!value) return true
  const stripped = value.replace(/[<>≤≥＜＞↑↓★☆*]/g, '').trim()
  if (stripped === '') return true
  return !isNaN(parseFloat(stripped))
}

function parseRange(range: string | undefined): { low?: number; high?: number } | null {
  if (!range) return null
  const r = range.trim()
  const m = r.match(/^([\d.]+)\s*[-~～—]+\s*([\d.]+)$/)
  if (m) return { low: parseFloat(m[1]), high: parseFloat(m[2]) }
  const upper = r.match(/^[<＜≤]\s*([\d.]+)$/)
  if (upper) return { high: parseFloat(upper[1]) }
  const lower = r.match(/^[>＞≥]\s*([\d.]+)$/)
  if (lower) return { low: parseFloat(lower[1]) }
  return null
}

function isCritical(value: string | undefined, range: string | undefined, status: 'high' | 'low'): boolean {
  if (!value || !range) return false
  const numVal = parseFloat(value.replace(/[<>≤≥＜＞↑↓]/g, '').trim())
  if (isNaN(numVal)) return false
  const parsed = parseRange(range)
  if (!parsed) return false
  if (status === 'high' && parsed.high != null) {
    const span = (parsed.low != null) ? parsed.high - parsed.low : parsed.high
    return span > 0 && (numVal - parsed.high) / span > 0.5
  }
  if (status === 'low' && parsed.low != null) {
    const span = (parsed.high != null) ? parsed.high - parsed.low : parsed.low
    return span > 0 && (parsed.low - numVal) / span > 0.5
  }
  return false
}

function isQualitativePositive(value: string | undefined): boolean {
  if (!value) return false
  const v = value.trim()
  return /阳性|弱阳性|\+/.test(v)
}

function isQualitativeNegative(value: string | undefined): boolean {
  if (!value) return false
  const v = value.trim()
  return /^阴性$|^-$|^（-）$|^\(-\)$/.test(v)
}

function resolveDisplayStatus(status: TestItemStatus, value?: string, referenceRange?: string): DisplayStatus {
  const nonNumeric = !isNumericValue(value)

  if (status === 'normal') {
    if (nonNumeric && isQualitativeNegative(value)) return 'negative'
    return 'normal'
  }

  if (nonNumeric) {
    if (isQualitativePositive(value)) return 'positive'
    return 'positive'
  }

  if (status === 'high') {
    return isCritical(value, referenceRange, 'high') ? 'critical_high' : 'high'
  }

  if (status === 'low') {
    return isCritical(value, referenceRange, 'low') ? 'critical_low' : 'low'
  }

  return 'normal'
}

export const TestItemStatusBadge: Component<TestItemStatusBadgeProps> = (props) => {
  const normalized = normalizeStatus(props.status)

  if (!normalized) {
    return (
      <Badge variant="warning" dot={props.dot} class={props.class}>
        {props.status || '未知'}
      </Badge>
    )
  }

  const display = resolveDisplayStatus(normalized, props.value, props.referenceRange)

  return (
    <Badge variant={DISPLAY_VARIANT_MAP[display]} dot={props.dot} class={props.class}>
      {DISPLAY_LABEL_MAP[display]}
    </Badge>
  )
}
