import type { Component, JSX } from 'solid-js'
import { Show, splitProps } from 'solid-js'
import { cn } from '@/lib/utils'
import { Spinner } from './Spinner'

export interface ButtonProps extends JSX.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: 'primary' | 'secondary' | 'outline' | 'ghost' | 'danger' | 'success' | 'warning'
  size?: 'xs' | 'sm' | 'md' | 'lg' | 'xl'
  loading?: boolean
  fullWidth?: boolean
  leftIcon?: JSX.Element
  rightIcon?: JSX.Element
}

const variantStyles: Record<NonNullable<ButtonProps['variant']>, string> = {
  primary: 'bg-accent text-accent-content hover:bg-accent-hover shadow-sm',
  secondary: 'bg-surface-tertiary text-content hover:bg-border',
  outline: 'border border-border text-content hover:bg-surface-secondary hover:border-border-hover',
  ghost: 'text-content hover:bg-surface-secondary',
  danger: 'bg-error text-content-inverse hover:bg-error/90 shadow-sm',
  success: 'bg-success text-content-inverse hover:bg-success/90 shadow-sm',
  warning: 'bg-warning text-content-inverse hover:bg-warning/90 shadow-sm',
}

const sizeStyles: Record<NonNullable<ButtonProps['size']>, string> = {
  xs: 'h-7 px-2.5 text-xs gap-1',
  sm: 'h-8 px-3 text-sm gap-1.5',
  md: 'h-9 px-4 text-sm gap-2',
  lg: 'h-10 px-5 text-base gap-2',
  xl: 'h-12 px-6 text-base gap-2.5',
}

const spinnerSizeMap: Record<NonNullable<ButtonProps['size']>, 'xs' | 'sm' | 'md' | 'lg' | 'xl'> = {
  xs: 'xs',
  sm: 'xs',
  md: 'sm',
  lg: 'md',
  xl: 'md',
}

export const Button: Component<ButtonProps> = (props) => {
  const [local, rest] = splitProps(props, [
    'variant',
    'size',
    'loading',
    'fullWidth',
    'leftIcon',
    'rightIcon',
    'class',
    'children',
    'disabled',
  ])

  const isDisabled = () => local.disabled || local.loading

  return (
    <button
      {...rest}
      disabled={isDisabled()}
      class={cn(
        'inline-flex items-center justify-center font-medium rounded-xl',
        'transition-all duration-200 ease-[cubic-bezier(0.34,1.56,0.64,1)]',
        'active:scale-[0.96] disabled:opacity-50 disabled:pointer-events-none',
        'cursor-pointer',
        variantStyles[local.variant ?? 'primary'],
        sizeStyles[local.size ?? 'md'],
        local.fullWidth && 'w-full',
        local.class,
      )}
    >
      <Show when={local.loading} fallback={
        <>
          <Show when={local.leftIcon}>
            <span class="inline-flex shrink-0">{local.leftIcon}</span>
          </Show>
          {local.children}
          <Show when={local.rightIcon}>
            <span class="inline-flex shrink-0">{local.rightIcon}</span>
          </Show>
        </>
      }>
        <Spinner
          size={spinnerSizeMap[local.size ?? 'md']}
          color={
            (local.variant === 'outline' || local.variant === 'ghost' || local.variant === 'secondary')
              ? 'content'
              : 'inverse'
          }
        />
      </Show>
    </button>
  )
}
