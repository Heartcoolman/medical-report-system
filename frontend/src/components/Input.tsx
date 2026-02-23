import type { Component, JSX } from 'solid-js'
import { Show, splitProps } from 'solid-js'
import { cn } from '@/lib/utils'

let inputIdCounter = 0

export interface InputProps extends Omit<JSX.InputHTMLAttributes<HTMLInputElement>, 'id'> {
  id?: string
  label?: string
  error?: string
  hint?: string
  leftIcon?: JSX.Element
  rightIcon?: JSX.Element
  class?: string
  wrapperClass?: string
}

export const Input: Component<InputProps> = (props) => {
  const [local, rest] = splitProps(props, [
    'id',
    'label',
    'error',
    'hint',
    'leftIcon',
    'rightIcon',
    'class',
    'wrapperClass',
  ])

  const generatedId = `input-${++inputIdCounter}`
  const inputId = () => local.id ?? generatedId
  const errorId = () => `${inputId()}-error`
  const hintId = () => `${inputId()}-hint`

  return (
    <div class={cn('flex flex-col gap-1.5', local.wrapperClass)}>
      <Show when={local.label}>
        <label
          for={inputId()}
          class="form-label"
        >
          {local.label}
        </label>
      </Show>

      <div class="relative">
        <Show when={local.leftIcon}>
          <span class="absolute left-3 top-1/2 -translate-y-1/2 text-content-tertiary">
            {local.leftIcon}
          </span>
        </Show>

        <input
          {...rest}
          id={inputId()}
          aria-invalid={local.error ? true : undefined}
          aria-describedby={
            local.error ? errorId() : local.hint ? hintId() : undefined
          }
          class={cn(
            'form-control-base form-control-input',
            'placeholder:text-content-tertiary',
            local.leftIcon && 'pl-10',
            local.rightIcon && 'pr-10',
            local.error && 'form-control-error',
            local.class,
          )}
        />

        <Show when={local.rightIcon}>
          <span class="absolute right-3 top-1/2 -translate-y-1/2 text-content-tertiary">
            {local.rightIcon}
          </span>
        </Show>
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
