import type { Component, JSX } from 'solid-js'
import { For, Show, createSignal } from 'solid-js'
import { cn } from '@/lib/utils'

export interface TabItem {
  key: string
  label: string
  disabled?: boolean
}

export interface TabsProps {
  items: TabItem[]
  activeKey?: string
  onChange?: (key: string) => void
  class?: string
  children: (activeKey: string) => JSX.Element
}

export interface TabPanelProps {
  key: string
  activeKey: string
  children: JSX.Element
  class?: string
}

export const TabPanel: Component<TabPanelProps> = (props) => {
  return (
    <Show when={props.activeKey === props.key}>
      <div
        role="tabpanel"
        id={`tabpanel-${props.key}`}
        aria-labelledby={`tab-${props.key}`}
        class={cn('py-4 tabs-content', props.class)}
      >
        {props.children}
      </div>
    </Show>
  )
}

export const Tabs: Component<TabsProps> = (props) => {
  const [activeKey, setActiveKey] = createSignal(props.activeKey ?? props.items[0]?.key ?? '')

  const currentKey = () => props.activeKey ?? activeKey()

  const handleSelect = (key: string) => {
    setActiveKey(key)
    props.onChange?.(key)
  }

  const handleKeyDown = (e: KeyboardEvent) => {
    const items = props.items.filter((i) => !i.disabled)
    const currentIndex = items.findIndex((i) => i.key === currentKey())
    let nextIndex = currentIndex

    if (e.key === 'ArrowRight' || e.key === 'ArrowDown') {
      e.preventDefault()
      nextIndex = (currentIndex + 1) % items.length
    } else if (e.key === 'ArrowLeft' || e.key === 'ArrowUp') {
      e.preventDefault()
      nextIndex = (currentIndex - 1 + items.length) % items.length
    } else if (e.key === 'Home') {
      e.preventDefault()
      nextIndex = 0
    } else if (e.key === 'End') {
      e.preventDefault()
      nextIndex = items.length - 1
    } else {
      return
    }

    const nextKey = items[nextIndex].key
    handleSelect(nextKey)
    const el = document.getElementById(`tab-${nextKey}`)
    el?.focus()
  }

  return (
    <div class={cn(props.class)}>
      <div
        role="tablist"
        class="inline-flex p-1 rounded-xl bg-surface-secondary gap-0.5"
        onKeyDown={handleKeyDown}
      >
        <For each={props.items}>
          {(item) => (
            <button
              id={`tab-${item.key}`}
              role="tab"
              type="button"
              aria-selected={currentKey() === item.key}
              aria-controls={`tabpanel-${item.key}`}
              tabIndex={currentKey() === item.key ? 0 : -1}
              disabled={item.disabled}
              onClick={() => handleSelect(item.key)}
              class={cn(
                'relative px-4 py-2 text-sm font-medium cursor-pointer rounded-lg',
                'transition-all duration-200 ease-[cubic-bezier(0.34,1.56,0.64,1)]',
                'disabled:opacity-50 disabled:cursor-not-allowed',
                currentKey() === item.key
                  ? 'bg-surface-elevated text-accent shadow-sm'
                  : 'text-content-secondary hover:text-content hover:bg-surface-elevated/50',
              )}
            >
              {item.label}
            </button>
          )}
        </For>
      </div>

      {props.children(currentKey())}
    </div>
  )
}
