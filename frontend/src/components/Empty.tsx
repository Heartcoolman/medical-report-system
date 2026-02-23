import type { Component, JSX } from 'solid-js'
import { Show } from 'solid-js'
import { cn } from '@/lib/utils'

export interface EmptyProps {
  title?: string
  description?: string
  action?: JSX.Element
  icon?: JSX.Element
  class?: string
}

const DefaultIcon: Component = () => (
  <svg
    aria-hidden="true"
    class="h-16 w-16 text-content-tertiary"
    fill="none"
    viewBox="0 0 64 64"
    stroke="currentColor"
    stroke-width="1.5"
  >
    <rect x="12" y="8" width="40" height="48" rx="4" />
    <path d="M22 22h20M22 30h20M22 38h12" stroke-linecap="round" />
  </svg>
)

export const Empty: Component<EmptyProps> = (props) => {
  return (
    <div class={cn('flex flex-col items-center justify-center py-12 px-4 text-center', props.class)}>
      <Show when={props.icon} fallback={<DefaultIcon />}>
        <span aria-hidden="true">{props.icon}</span>
      </Show>

      <Show when={props.title}>
        <h3 class="empty-title">
          {props.title}
        </h3>
      </Show>

      <Show when={props.description}>
        <p class="empty-description">
          {props.description}
        </p>
      </Show>

      <Show when={props.action}>
        <div class="mt-4">{props.action}</div>
      </Show>
    </div>
  )
}
