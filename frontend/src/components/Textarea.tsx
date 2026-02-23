import type { Component, JSX } from 'solid-js'
import { Show, splitProps } from 'solid-js'
import { cn } from '@/lib/utils'

let textareaIdCounter = 0

export interface TextareaProps extends Omit<JSX.TextareaHTMLAttributes<HTMLTextAreaElement>, 'id'> {
  id?: string
  label?: string
  error?: string
  hint?: string
  class?: string
  wrapperClass?: string
}

export const Textarea: Component<TextareaProps> = (props) => {
  const [local, rest] = splitProps(props, [
    'id',
    'label',
    'error',
    'hint',
    'class',
    'wrapperClass',
  ])

  const generatedId = `textarea-${++textareaIdCounter}`
  const textareaId = () => local.id ?? generatedId
  const errorId = () => `${textareaId()}-error`
  const hintId = () => `${textareaId()}-hint`

  return (
    <div class={cn('flex flex-col gap-1.5', local.wrapperClass)}>
      <Show when={local.label}>
        <label for={textareaId()} class="form-label">
          {local.label}
        </label>
      </Show>

      <textarea
        {...rest}
        id={textareaId()}
        rows={rest.rows ?? 3}
        aria-invalid={local.error ? true : undefined}
        aria-describedby={
          local.error ? errorId() : local.hint ? hintId() : undefined
        }
        class={cn(
          'form-control-base form-control-textarea',
          'placeholder:text-content-tertiary',
          local.error && 'form-control-error',
          local.class,
        )}
      />

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
