import type { Component, JSX } from 'solid-js'
import { Show, createSignal, splitProps } from 'solid-js'
import { cn } from '@/lib/utils'

export interface SearchBarProps {
  value?: string
  placeholder?: string
  onInput?: (value: string) => void
  onClear?: () => void
  onFocus?: () => void
  onBlur?: () => void
  class?: string
  wrapperClass?: string
  autoFocus?: boolean
}

export const SearchBar: Component<SearchBarProps> = (props) => {
  const [local, _rest] = splitProps(props, [
    'value',
    'placeholder',
    'onInput',
    'onClear',
    'onFocus',
    'onBlur',
    'class',
    'wrapperClass',
    'autoFocus',
  ])

  const [focused, setFocused] = createSignal(false)
  let inputRef: HTMLInputElement | undefined

  const hasValue = () => (local.value?.length ?? 0) > 0

  const handleInput: JSX.EventHandler<HTMLInputElement, InputEvent> = (e) => {
    local.onInput?.(e.currentTarget.value)
  }

  const handleClear = () => {
    local.onInput?.('')
    local.onClear?.()
    inputRef?.focus()
  }

  const handleFocus = () => {
    setFocused(true)
    local.onFocus?.()
  }

  const handleBlur = () => {
    setFocused(false)
    local.onBlur?.()
  }

  const handleCancel = () => {
    local.onInput?.('')
    local.onClear?.()
    inputRef?.blur()
  }

  return (
    <div class={cn('flex items-center gap-2', local.wrapperClass)}>
      <div
        class={cn(
          'relative flex-1 flex items-center transition-all duration-200',
          local.class,
        )}
      >
        {/* Search icon */}
        <span
          class={cn(
            'absolute left-3.5 top-1/2 -translate-y-1/2 transition-all duration-200',
            focused() ? 'text-accent scale-110' : 'text-content-tertiary',
          )}
        >
          <svg class="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
            <path stroke-linecap="round" stroke-linejoin="round" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
          </svg>
        </span>

        <input
          ref={inputRef}
          type="text"
          value={local.value ?? ''}
          placeholder={local.placeholder ?? '搜索...'}
          onInput={handleInput}
          onFocus={handleFocus}
          onBlur={handleBlur}
          autofocus={local.autoFocus}
          class={cn(
            'w-full h-11 pl-11 pr-10 rounded-2xl bg-surface-secondary text-content text-sm',
            'placeholder:text-content-tertiary',
            'transition-all duration-200',
            'border-2 border-transparent',
            'focus:outline-none focus:bg-surface-elevated focus:border-accent focus:ring-4 focus:ring-accent/15 focus:shadow-md',
            'hover:bg-surface-tertiary',
          )}
        />

        {/* Clear button */}
        <Show when={hasValue()}>
          <button
            type="button"
            onClick={handleClear}
            class={cn(
              'absolute right-2.5 top-1/2 -translate-y-1/2',
              'p-0.5 rounded-full bg-content-tertiary/30 hover:bg-content-tertiary/50',
              'transition-all duration-[var(--transition-fast)] cursor-pointer',
              'searchbar-clear-enter',
            )}
            aria-label="清除搜索"
          >
            <svg class="w-3.5 h-3.5 text-content-secondary" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2.5">
              <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </Show>
      </div>

      {/* Cancel button — shows when focused or has value */}
      <Show when={focused() || hasValue()}>
        <button
          type="button"
          onMouseDown={(e) => e.preventDefault()}
          onClick={handleCancel}
          class={cn(
            'text-sm text-accent font-medium cursor-pointer',
            'hover:text-accent-hover transition-colors duration-[var(--transition-fast)]',
            'searchbar-cancel-enter whitespace-nowrap',
          )}
        >
          取消
        </button>
      </Show>
    </div>
  )
}
