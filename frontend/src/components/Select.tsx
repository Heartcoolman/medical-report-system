import type { Component, JSX } from 'solid-js'
import { Show, splitProps } from 'solid-js'
import { cn } from '@/lib/utils'

let selectIdCounter = 0

export interface SelectProps extends Omit<JSX.SelectHTMLAttributes<HTMLSelectElement>, 'id'> {
  id?: string
  label?: string
  error?: string
  hint?: string
  class?: string
  wrapperClass?: string
}

export const Select: Component<SelectProps> = (props) => {
  const [local, rest] = splitProps(props, [
    'id',
    'label',
    'error',
    'hint',
    'class',
    'wrapperClass',
    'children',
    'value',
  ])

  const generatedId = `select-${++selectIdCounter}`
  const selectId = () => local.id ?? generatedId
  const errorId = () => `${selectId()}-error`
  const hintId = () => `${selectId()}-hint`

  return (
    <div class={cn('flex flex-col gap-1.5', local.wrapperClass)}>
      <Show when={local.label}>
        <label for={selectId()} class="form-label">
          {local.label}
        </label>
      </Show>

      <div class="relative">
        <select
          {...rest}
          value={local.value}
          id={selectId()}
          aria-invalid={local.error ? true : undefined}
          aria-describedby={
            local.error ? errorId() : local.hint ? hintId() : undefined
          }
          class={cn(
            'form-control-base form-control-select',
            local.error && 'form-control-error',
            local.class,
          )}
        >
          {local.children}
        </select>
        <span class="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 text-content-tertiary">
          <svg class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
            <path stroke-linecap="round" stroke-linejoin="round" d="M19 9l-7 7-7-7" />
          </svg>
        </span>
      </div>

      <Show when={local.error}>
        <p id={errorId()} role="alert" class="error-text">
          {local.error}
        </p>
      </Show>

      <Show when={local.hint && !local.error}>
        <p id={hintId()} class="helper-text">
          {local.hint}
        </p>
      </Show>
    </div>
  )
}
