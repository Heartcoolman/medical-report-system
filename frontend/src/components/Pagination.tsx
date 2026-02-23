import type { Component } from 'solid-js'
import { For, Show, createMemo } from 'solid-js'
import { cn } from '@/lib/utils'

export interface PaginationProps {
  current: number
  total: number
  pageSize: number
  pageSizeOptions?: number[]
  onChange: (page: number) => void
  onPageSizeChange?: (size: number) => void
  class?: string
}

export const Pagination: Component<PaginationProps> = (props) => {
  const totalPages = createMemo(() => Math.max(1, Math.ceil(props.total / props.pageSize)))

  const pages = createMemo(() => {
    const total = totalPages()
    const current = props.current
    const result: (number | 'ellipsis')[] = []

    if (total <= 7) {
      for (let i = 1; i <= total; i++) result.push(i)
      return result
    }

    result.push(1)

    if (current > 3) {
      result.push('ellipsis')
    }

    const start = Math.max(2, current - 1)
    const end = Math.min(total - 1, current + 1)

    for (let i = start; i <= end; i++) {
      result.push(i)
    }

    if (current < total - 2) {
      result.push('ellipsis')
    }

    result.push(total)

    return result
  })

  const buttonBase = 'h-8 min-w-[32px] px-2 rounded-md text-sm font-medium transition-all duration-[var(--transition-fast)] cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed'

  return (
    <div class={cn('flex items-center justify-between gap-4 flex-wrap', props.class)}>
      <Show when={props.total > 0}>
        <span class="text-sm text-content-secondary">
          共 {props.total} 条
        </span>
      </Show>

      <div class="flex items-center gap-1">
        <button
          class={cn(buttonBase, 'text-content-secondary hover:bg-surface-secondary')}
          disabled={props.current <= 1}
          onClick={() => props.onChange(props.current - 1)}
          aria-label="上一页"
        >
          <svg class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
            <path stroke-linecap="round" stroke-linejoin="round" d="M15 19l-7-7 7-7" />
          </svg>
        </button>

        <For each={pages()}>
          {(page) => (
            <Show
              when={page !== 'ellipsis'}
              fallback={
                <span class="h-8 min-w-[32px] flex items-center justify-center text-sm text-content-tertiary">
                  ...
                </span>
              }
            >
              <button
                class={cn(
                  buttonBase,
                  (page as number) === props.current
                    ? 'bg-accent text-accent-content'
                    : 'text-content-secondary hover:bg-surface-secondary',
                )}
                onClick={() => props.onChange(page as number)}
                aria-current={(page as number) === props.current ? 'page' : undefined}
                aria-label={`第 ${page as number} 页`}
              >
                {page as number}
              </button>
            </Show>
          )}
        </For>

        <button
          class={cn(buttonBase, 'text-content-secondary hover:bg-surface-secondary')}
          disabled={props.current >= totalPages()}
          onClick={() => props.onChange(props.current + 1)}
          aria-label="下一页"
        >
          <svg class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
            <path stroke-linecap="round" stroke-linejoin="round" d="M9 5l7 7-7 7" />
          </svg>
        </button>

        <Show when={props.pageSizeOptions && props.pageSizeOptions.length > 0}>
          <select
            class="ml-2 control-select-compact"
            value={props.pageSize}
            onChange={(e) => props.onPageSizeChange?.(Number(e.currentTarget.value))}
          >
            <For each={props.pageSizeOptions}>
              {(size) => (
                <option value={size}>{size} 条/页</option>
              )}
            </For>
          </select>
        </Show>
      </div>
    </div>
  )
}
