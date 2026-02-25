import { Show } from 'solid-js'
import { useRegisterSW } from 'virtual:pwa-register/solid'

export default function ReloadPrompt() {
  const {
    needRefresh: [needRefresh, setNeedRefresh],
    updateServiceWorker,
  } = useRegisterSW()

  const close = () => setNeedRefresh(false)

  return (
    <Show when={needRefresh()}>
      <div class="fixed bottom-6 left-1/2 -translate-x-1/2 z-[90] pointer-events-none">
        <div class="pointer-events-auto flex items-center gap-3 rounded-2xl bg-surface-elevated px-5 py-3 shadow-xl min-w-[280px] max-w-[420px] toast-item-enter">
          <p class="flex-1 text-sm font-medium text-content">有新版本可用</p>
          <button
            onClick={() => updateServiceWorker(true)}
            class="shrink-0 text-sm font-semibold text-accent hover:text-accent-hover transition-colors duration-[var(--transition-fast)] cursor-pointer"
          >
            立即更新
          </button>
          <button
            onClick={close}
            class="shrink-0 p-0.5 rounded hover:bg-content-tertiary/15 transition-all duration-[var(--transition-fast)] cursor-pointer"
            aria-label="关闭"
          >
            <svg class="h-4 w-4 text-content-tertiary" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
              <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>
      </div>
    </Show>
  )
}
