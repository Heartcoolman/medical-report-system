import { createSignal, onMount, Show } from 'solid-js'

const DISMISSED_KEY = 'pwa-install-prompt-dismissed'

function isIOSSafariStandalone(): boolean {
  const ua = navigator.userAgent
  const isIOS = /iPhone|iPad|iPod/.test(ua)
  const isStandalone = ('standalone' in navigator && (navigator as any).standalone === true)
    || window.matchMedia('(display-mode: standalone)').matches
  return isIOS && !isStandalone
}

export default function PWAInstallPrompt() {
  const [visible, setVisible] = createSignal(false)

  onMount(() => {
    if (localStorage.getItem(DISMISSED_KEY)) return
    if (isIOSSafariStandalone()) {
      setVisible(true)
    }
  })

  const dismiss = () => {
    setVisible(false)
    localStorage.setItem(DISMISSED_KEY, '1')
  }

  return (
    <Show when={visible()}>
      <div class="fixed bottom-20 left-1/2 -translate-x-1/2 z-[89] pointer-events-none w-[calc(100%-2rem)] max-w-[420px]">
        <div class="pointer-events-auto flex items-center gap-3 rounded-2xl bg-surface-elevated px-5 py-3 shadow-xl toast-item-enter">
          <p class="flex-1 text-sm text-content">
            点击 <span class="inline-block align-middle">
              <svg class="h-4 w-4 text-accent inline" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                <path stroke-linecap="round" stroke-linejoin="round" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12" />
              </svg>
            </span> 然后「添加到主屏幕」，获得更好体验
          </p>
          <button
            onClick={dismiss}
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
