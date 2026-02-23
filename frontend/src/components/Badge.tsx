import type { Component, JSX } from 'solid-js'
import { Show } from 'solid-js'
import { cn } from '@/lib/utils'

export interface BadgeProps extends JSX.HTMLAttributes<HTMLSpanElement> {
  variant?: 'default' | 'accent' | 'success' | 'warning' | 'error' | 'info'
  dot?: boolean
}

const variantStyles: Record<NonNullable<BadgeProps['variant']>, string> = {
  default: 'bg-surface-tertiary text-content-secondary',
  accent: 'bg-accent-light text-accent',
  success: 'bg-success-light text-success',
  warning: 'bg-warning-light text-warning',
  error: 'bg-error-light text-error',
  info: 'bg-info-light text-info',
}

const dotColorStyles: Record<NonNullable<BadgeProps['variant']>, string> = {
  default: 'bg-content-tertiary',
  accent: 'bg-accent',
  success: 'bg-success',
  warning: 'bg-warning',
  error: 'bg-error',
  info: 'bg-info',
}

export const Badge: Component<BadgeProps> = (props) => {
  return (
    <span
      {...props}
      class={cn(
        'inline-flex items-center gap-1.5 rounded-full px-2.5 py-0.5 text-xs font-medium',
        variantStyles[props.variant ?? 'default'],
        props.class,
      )}
    >
      <Show when={props.dot}>
        <span class={cn('h-1.5 w-1.5 rounded-full', dotColorStyles[props.variant ?? 'default'])} />
      </Show>
      {props.children}
    </span>
  )
}
