import type { Component } from 'solid-js'
import { Show } from 'solid-js'
import { cn } from '@/lib/utils'

export interface ProgressProps {
  value: number
  max?: number
  variant?: 'linear' | 'circular'
  color?: 'accent' | 'success' | 'warning' | 'error'
  size?: 'sm' | 'md' | 'lg'
  showPercentage?: boolean
  class?: string
}

const colorStylesBar: Record<NonNullable<ProgressProps['color']>, string> = {
  accent: 'bg-accent',
  success: 'bg-success',
  warning: 'bg-warning',
  error: 'bg-error',
}

const barSizeStyles: Record<NonNullable<ProgressProps['size']>, string> = {
  sm: 'h-1.5',
  md: 'h-2.5',
  lg: 'h-4',
}

const circleSizeMap: Record<NonNullable<ProgressProps['size']>, number> = {
  sm: 32,
  md: 48,
  lg: 64,
}

const strokeWidthMap: Record<NonNullable<ProgressProps['size']>, number> = {
  sm: 3,
  md: 4,
  lg: 5,
}

const colorStroke: Record<NonNullable<ProgressProps['color']>, string> = {
  accent: 'var(--accent)',
  success: 'var(--success)',
  warning: 'var(--warning)',
  error: 'var(--error)',
}

const textSizeMap: Record<NonNullable<ProgressProps['size']>, string> = {
  sm: 'text-[8px]',
  md: 'text-xs',
  lg: 'text-sm',
}

export const Progress: Component<ProgressProps> = (props) => {
  const max = () => props.max ?? 100
  const percentage = () => Math.min(100, Math.max(0, (props.value / max()) * 100))
  const color = () => props.color ?? 'accent'
  const size = () => props.size ?? 'md'

  return (
    <Show
      when={props.variant !== 'circular'}
      fallback={<CircularProgress percentage={percentage()} color={color()} size={size()} showPercentage={props.showPercentage} class={props.class} />}
    >
      <div class={cn('flex items-center gap-3', props.class)}>
        <div
          class={cn(
            'flex-1 rounded-full bg-surface-tertiary overflow-hidden',
            barSizeStyles[size()],
          )}
          role="progressbar"
          aria-valuenow={props.value}
          aria-valuemin={0}
          aria-valuemax={max()}
        >
          <div
            class={cn(
              'h-full rounded-full transition-all duration-[var(--transition-normal)]',
              colorStylesBar[color()],
            )}
            style={{ width: `${percentage()}%` }}
          />
        </div>
        <Show when={props.showPercentage}>
          <span class="text-xs font-medium text-content-secondary tabular-nums min-w-[3ch]">
            {Math.round(percentage())}%
          </span>
        </Show>
      </div>
    </Show>
  )
}

interface CircularProgressInternalProps {
  percentage: number
  color: NonNullable<ProgressProps['color']>
  size: NonNullable<ProgressProps['size']>
  showPercentage?: boolean
  class?: string
}

const CircularProgress: Component<CircularProgressInternalProps> = (props) => {
  const dim = () => circleSizeMap[props.size]
  const strokeWidth = () => strokeWidthMap[props.size]
  const radius = () => (dim() - strokeWidth()) / 2
  const circumference = () => 2 * Math.PI * radius()
  const offset = () => circumference() - (props.percentage / 100) * circumference()

  return (
    <div
      class={cn('relative inline-flex items-center justify-center', props.class)}
      role="progressbar"
      aria-valuenow={Math.round(props.percentage)}
      aria-valuemin={0}
      aria-valuemax={100}
    >
      <svg width={dim()} height={dim()} class="-rotate-90">
        <circle
          cx={dim() / 2}
          cy={dim() / 2}
          r={radius()}
          fill="none"
          stroke="var(--surface-tertiary)"
          stroke-width={strokeWidth()}
        />
        <circle
          cx={dim() / 2}
          cy={dim() / 2}
          r={radius()}
          fill="none"
          stroke={colorStroke[props.color]}
          stroke-width={strokeWidth()}
          stroke-linecap="round"
          stroke-dasharray={String(circumference())}
          stroke-dashoffset={String(offset())}
          class="transition-all duration-[var(--transition-normal)]"
        />
      </svg>
      <Show when={props.showPercentage}>
        <span class={cn('absolute font-medium text-content tabular-nums', textSizeMap[props.size])}>
          {Math.round(props.percentage)}%
        </span>
      </Show>
    </div>
  )
}
