import type { Component } from 'solid-js'
import type { BadgeProps } from './Badge'
import { Badge } from './Badge'

export type TestItemStatus = 'critical_high' | 'high' | 'normal' | 'low' | 'critical_low'

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
  if (
    status === 'critical_high'
    || status === 'high'
    || status === 'normal'
    || status === 'low'
    || status === 'critical_low'
  ) {
    return status
  }
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

function resolveDisplayStatus(status: TestItemStatus, value?: string): DisplayStatus {
  const nonNumeric = !isNumericValue(value)

  if (status === 'critical_high' || status === 'critical_low') {
    return status
  }

  if (status === 'normal') {
    if (nonNumeric && isQualitativeNegative(value)) return 'negative'
    return 'normal'
  }

  if (nonNumeric) {
    if (isQualitativePositive(value)) return 'positive'
    return 'positive'
  }

  if (status === 'high') {
    return 'high'
  }

  if (status === 'low') {
    return 'low'
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

  const display = resolveDisplayStatus(normalized, props.value)

  return (
    <Badge variant={DISPLAY_VARIANT_MAP[display]} dot={props.dot} class={props.class}>
      {DISPLAY_LABEL_MAP[display]}
    </Badge>
  )
}
