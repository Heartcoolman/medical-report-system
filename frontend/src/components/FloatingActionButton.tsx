import type { Component, JSX } from 'solid-js'
import { Show, splitProps } from 'solid-js'
import { cn } from '@/lib/utils'

export interface FloatingActionButtonProps extends JSX.ButtonHTMLAttributes<HTMLButtonElement> {
  icon?: JSX.Element
  label?: string
  variant?: 'primary' | 'secondary'
  size?: 'md' | 'lg'
  extended?: boolean
  position?: 'bottom-right' | 'bottom-center'
}

const variantStyles: Record<NonNullable<FloatingActionButtonProps['variant']>, string> = {
  primary: 'bg-accent text-accent-content hover:bg-accent-hover shadow-lg hover:shadow-xl',
  secondary: 'bg-surface-elevated text-content border border-border/40 hover:bg-surface-secondary shadow-lg hover:shadow-xl',
}

const sizeStyles: Record<NonNullable<FloatingActionButtonProps['size']>, string> = {
  md: 'h-12 min-w-12',
  lg: 'h-14 min-w-14',
}

const positionStyles: Record<NonNullable<FloatingActionButtonProps['position']>, string> = {
  'bottom-right': 'fixed bottom-6 right-6',
  'bottom-center': 'fixed bottom-6 left-1/2 -translate-x-1/2',
}

export const FloatingActionButton: Component<FloatingActionButtonProps> = (props) => {
  const [local, rest] = splitProps(props, [
    'icon',
    'label',
    'variant',
    'size',
    'extended',
    'position',
    'class',
    'children',
  ])

  return (
    <button
      {...rest}
      class={cn(
        'z-40 inline-flex items-center justify-center rounded-2xl',
        'transition-all duration-200 ease-[cubic-bezier(0.34,1.56,0.64,1)] cursor-pointer',
        'active:scale-[0.92]',
        variantStyles[local.variant ?? 'primary'],
        sizeStyles[local.size ?? 'md'],
        positionStyles[local.position ?? 'bottom-right'],
        local.extended ? 'px-5 gap-2' : 'px-0',
        local.class,
      )}
    >
      <Show when={local.icon}>
        <span class="inline-flex shrink-0">{local.icon}</span>
      </Show>
      <Show when={local.extended && local.label}>
        <span class="text-sm font-medium whitespace-nowrap">{local.label}</span>
      </Show>
      {local.children}
    </button>
  )
}
