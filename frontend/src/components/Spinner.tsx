import type { Component } from 'solid-js'
import { Show } from 'solid-js'
import { cn } from '@/lib/utils'

export interface SpinnerProps {
  size?: 'xs' | 'sm' | 'md' | 'lg' | 'xl'
  color?: 'accent' | 'content' | 'inverse' | 'success' | 'warning' | 'error'
  variant?: 'spin' | 'orbital'
  class?: string
}

const sizeMap: Record<NonNullable<SpinnerProps['size']>, string> = {
  xs: 'h-3 w-3 border-[1.5px]',
  sm: 'h-4 w-4 border-2',
  md: 'h-5 w-5 border-2',
  lg: 'h-6 w-6 border-2',
  xl: 'h-8 w-8 border-[3px]',
}

const colorMap: Record<NonNullable<SpinnerProps['color']>, string> = {
  accent: 'border-accent/30 border-t-accent',
  content: 'border-content/30 border-t-content',
  inverse: 'border-content-inverse/30 border-t-content-inverse',
  success: 'border-success/30 border-t-success',
  warning: 'border-warning/30 border-t-warning',
  error: 'border-error/30 border-t-error',
}

const orbitalSizeMap: Record<NonNullable<SpinnerProps['size']>, { size: number; stroke: number; dot: number }> = {
  xs: { size: 12, stroke: 1.5, dot: 1.5 },
  sm: { size: 16, stroke: 2, dot: 2 },
  md: { size: 20, stroke: 2, dot: 2.5 },
  lg: { size: 24, stroke: 2, dot: 3 },
  xl: { size: 32, stroke: 3, dot: 3.5 },
}

const orbitalColorMap: Record<NonNullable<SpinnerProps['color']>, string> = {
  accent: 'var(--accent)',
  content: 'var(--content)',
  inverse: 'var(--content-inverse)',
  success: 'var(--success)',
  warning: 'var(--warning)',
  error: 'var(--error)',
}

export const Spinner: Component<SpinnerProps> = (props) => {
  const variant = () => props.variant ?? 'spin'

  return (
    <Show
      when={variant() === 'orbital'}
      fallback={
        <span
          role="status"
          aria-label="加载中"
          class={cn(
            'inline-block rounded-full animate-spin',
            sizeMap[props.size ?? 'md'],
            colorMap[props.color ?? 'accent'],
            props.class,
          )}
        />
      }
    >
      {(() => {
        const cfg = orbitalSizeMap[props.size ?? 'md']
        const color = orbitalColorMap[props.color ?? 'accent']
        const r = (cfg.size - cfg.stroke) / 2
        const center = cfg.size / 2
        return (
          <svg
            role="status"
            aria-label="加载中"
            width={cfg.size}
            height={cfg.size}
            viewBox={`0 0 ${cfg.size} ${cfg.size}`}
            class={cn('inline-block', props.class)}
          >
            <circle
              cx={center}
              cy={center}
              r={r}
              fill="none"
              stroke={color}
              stroke-opacity="0.2"
              stroke-width={cfg.stroke}
            />
            <circle
              cx={center}
              cy={cfg.stroke / 2}
              r={cfg.dot}
              fill={color}
            >
              <animateTransform
                attributeName="transform"
                type="rotate"
                from={`0 ${center} ${center}`}
                to={`360 ${center} ${center}`}
                dur="1s"
                repeatCount="indefinite"
              />
            </circle>
          </svg>
        )
      })()}
    </Show>
  )
}
