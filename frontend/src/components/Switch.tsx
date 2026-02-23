import type { Component, JSX } from 'solid-js'
import { Show, splitProps } from 'solid-js'
import { cn } from '@/lib/utils'

let switchIdCounter = 0

export interface SwitchProps {
  checked?: boolean
  onChange?: (checked: boolean) => void
  label?: string
  disabled?: boolean
  size?: 'sm' | 'md' | 'lg'
  id?: string
  class?: string
  name?: string
}

const trackSizeStyles: Record<NonNullable<SwitchProps['size']>, string> = {
  sm: 'w-9 h-[18px]',
  md: 'w-11 h-[22px]',
  lg: 'w-[52px] h-[26px]',
}

const thumbSizeStyles: Record<NonNullable<SwitchProps['size']>, string> = {
  sm: 'h-3.5 w-3.5 active:w-[18px]',
  md: 'h-[16px] w-[16px] active:w-[22px]',
  lg: 'h-[20px] w-[20px] active:w-[26px]',
}

const thumbTranslateStyles: Record<NonNullable<SwitchProps['size']>, string> = {
  sm: 'translate-x-4',
  md: 'translate-x-5',
  lg: 'translate-x-6',
}

export const Switch: Component<SwitchProps> = (props) => {
  const [local, _rest] = splitProps(props, [
    'checked',
    'onChange',
    'label',
    'disabled',
    'size',
    'id',
    'class',
    'name',
  ])

  const generatedId = `switch-${++switchIdCounter}`
  const switchId = () => local.id ?? generatedId
  const size = () => local.size ?? 'md'

  const handleClick = () => {
    if (!local.disabled) {
      local.onChange?.(!local.checked)
    }
  }

  const handleKeyDown: JSX.EventHandler<HTMLButtonElement, KeyboardEvent> = (e) => {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault()
      handleClick()
    }
  }

  return (
    <div class={cn('inline-flex items-center gap-2', local.class)}>
      <button
        id={switchId()}
        type="button"
        role="switch"
        aria-checked={local.checked ?? false}
        aria-label={local.label}
        disabled={local.disabled}
        onClick={handleClick}
        onKeyDown={handleKeyDown}
        class={cn(
          'relative inline-flex shrink-0 items-center rounded-full cursor-pointer',
          'transition-all duration-[var(--transition-fast)]',
          'disabled:opacity-50 disabled:cursor-not-allowed',
          trackSizeStyles[size()],
          local.checked ? 'bg-accent' : 'bg-border',
        )}
      >
        <span
          class={cn(
            'inline-block rounded-full bg-surface-elevated shadow-sm',
            'transition-all duration-250 ease-[cubic-bezier(0.34,1.56,0.64,1)]',
            'translate-x-[3px]',
            thumbSizeStyles[size()],
            local.checked && thumbTranslateStyles[size()],
          )}
        />
      </button>

      <Show when={local.label}>
        <label
          for={switchId()}
          class={cn(
            'text-sm text-content cursor-pointer select-none',
            local.disabled && 'opacity-50 cursor-not-allowed',
          )}
        >
          {local.label}
        </label>
      </Show>
    </div>
  )
}
