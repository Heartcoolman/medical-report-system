import type { JSX } from 'solid-js'
import { For, Show, createSignal } from 'solid-js'
import { cn } from '@/lib/utils'
import { Skeleton } from './Skeleton'
import { Empty } from './Empty'

export interface TableColumn<T> {
  key: string
  title: string
  width?: string
  sortable?: boolean
  render?: (value: any, row: T, index: number) => JSX.Element
}

export type SortDirection = 'asc' | 'desc' | null

export interface TableProps<T> {
  columns: TableColumn<T>[]
  data: T[]
  loading?: boolean
  loadingRows?: number
  striped?: boolean
  rowKey?: (row: T) => string | number
  onSort?: (key: string, direction: SortDirection) => void
  emptyTitle?: string
  emptyDescription?: string
  class?: string
}

export function Table<T extends Record<string, any>>(props: TableProps<T>): JSX.Element {
  const [sortKey, setSortKey] = createSignal<string | null>(null)
  const [sortDir, setSortDir] = createSignal<SortDirection>(null)

  const handleSort = (key: string) => {
    let nextDir: SortDirection
    if (sortKey() === key) {
      nextDir = sortDir() === 'asc' ? 'desc' : sortDir() === 'desc' ? null : 'asc'
    } else {
      nextDir = 'asc'
    }
    setSortKey(nextDir ? key : null)
    setSortDir(nextDir)
    props.onSort?.(key, nextDir)
  }

  const loadingRows = () => props.loadingRows ?? 5

  return (
    <div class={cn('w-full overflow-x-auto rounded-2xl border border-border/50 bg-surface-elevated', props.class)}>
      <table class="w-full text-sm">
        <thead>
          <tr class="border-b border-border bg-surface-secondary">
            <For each={props.columns}>
              {(col) => (
                <th
                  class={cn(
                    'table-header',
                    col.sortable && 'cursor-pointer select-none hover:text-content',
                  )}
                  style={col.width ? { width: col.width } : undefined}
                  onClick={() => col.sortable && handleSort(col.key)}
                  aria-sort={col.sortable ? (sortKey() === col.key && sortDir() === 'asc' ? 'ascending' : sortKey() === col.key && sortDir() === 'desc' ? 'descending' : 'none') : undefined}
                >
                  <div class="flex items-center gap-1">
                    {col.title}
                    <Show when={col.sortable}>
                      <span class="inline-flex flex-col ml-0.5">
                        <svg
                          class={cn('h-3 w-3', sortKey() === col.key && sortDir() === 'asc' ? 'text-accent' : 'text-content-tertiary')}
                          fill="currentColor"
                          viewBox="0 0 24 24"
                        >
                          <path d="M12 8l-6 6h12z" />
                        </svg>
                        <svg
                          class={cn('h-3 w-3 -mt-1', sortKey() === col.key && sortDir() === 'desc' ? 'text-accent' : 'text-content-tertiary')}
                          fill="currentColor"
                          viewBox="0 0 24 24"
                        >
                          <path d="M12 16l6-6H6z" />
                        </svg>
                      </span>
                    </Show>
                  </div>
                </th>
              )}
            </For>
          </tr>
        </thead>
        <tbody>
          <Show
            when={!props.loading}
            fallback={
              <For each={Array.from({ length: loadingRows() })}>
                {() => (
                  <tr class="border-b border-border last:border-b-0">
                    <For each={props.columns}>
                      {() => (
                        <td class="table-cell">
                          <Skeleton variant="text" />
                        </td>
                      )}
                    </For>
                  </tr>
                )}
              </For>
            }
          >
            <Show
              when={props.data.length > 0}
              fallback={
                <tr>
                  <td
                    colSpan={props.columns.length}
                    class="px-4 py-8"
                  >
                    <Empty
                      title={props.emptyTitle ?? '暂无数据'}
                      description={props.emptyDescription}
                    />
                  </td>
                </tr>
              }
            >
              <For each={props.data}>
                {(row, index) => (
                  <tr
                    class={cn(
                      'border-b border-border last:border-b-0 transition-colors duration-[var(--transition-fast)]',
                      'hover:bg-surface-secondary',
                      props.striped && index() % 2 === 1 && 'table-row-striped',
                    )}
                  >
                    <For each={props.columns}>
                      {(col) => (
                        <td class="table-cell">
                          {col.render
                            ? col.render(row[col.key], row, index())
                            : row[col.key]}
                        </td>
                      )}
                    </For>
                  </tr>
                )}
              </For>
            </Show>
          </Show>
        </tbody>
      </table>
    </div>
  )
}
