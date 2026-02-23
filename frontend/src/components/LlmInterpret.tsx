import { createSignal, createEffect, Show, onCleanup } from 'solid-js'
import { Button, Card, CardBody, Spinner } from '@/components'

export interface LlmInterpretProps {
  /** SSE endpoint URL to connect to, e.g. /api/reports/:id/interpret */
  url: string
  /** Button label (default: "AI 解读") */
  buttonLabel?: string
  /** Whether to auto-start on mount (default: false, show button) */
  autoStart?: boolean
}

type InterpretState = 'checking' | 'idle' | 'loading' | 'streaming' | 'done' | 'error'

export function LlmInterpret(props: LlmInterpretProps) {
  const [state, setState] = createSignal<InterpretState>('checking')
  const [content, setContent] = createSignal('')
  const [error, setError] = createSignal('')
  let eventSource: EventSource | null = null

  function cleanup() {
    if (eventSource) {
      eventSource.close()
      eventSource = null
    }
  }

  // Derive cache URL from the SSE url: /api/reports/:id/interpret -> /api/reports/:id/interpret-cache
  // Returns null for URLs without a cache endpoint (e.g. trend interpret, interpret-time)
  function cacheUrl(): string | null {
    const match = props.url.match(/^(.+)\/interpret(\?.*)?$/)
    if (!match) return null
    return `${match[1]}/interpret-cache${match[2] || ''}`
  }

  // Fetch cached interpretation from DB whenever url changes
  createEffect(() => {
    const url = props.url
    cleanup()
    setContent('')
    setError('')

    const cache = cacheUrl()
    if (!cache) {
      // No cache endpoint for this URL — go straight to idle/autoStart
      if (props.autoStart) {
        start()
      } else {
        setState('idle')
      }
      return
    }

    setState('checking')

    fetch(cache)
      .then(res => res.json())
      .then((json: { success: boolean; data: { content: string; created_at: string } | null }) => {
        // Only apply if url hasn't changed while we were fetching
        if (props.url !== url) return
        if (json.success && json.data && json.data.content) {
          setContent(json.data.content)
          setState('done')
        } else if (props.autoStart) {
          start()
        } else {
          setState('idle')
        }
      })
      .catch(() => {
        if (props.url !== url) return
        if (props.autoStart) {
          start()
        } else {
          setState('idle')
        }
      })
  })

  onCleanup(cleanup)

  function start() {
    cleanup()
    setContent('')
    setError('')
    setState('loading')

    // Use relative URL so requests go through Vite proxy in dev mode (same-origin, no CORS issues)
    const sseUrl = props.url
    const es = new EventSource(sseUrl)
    eventSource = es

    es.onmessage = (event) => {
      const data = event.data
      if (data === '[DONE]') {
        es.close()
        eventSource = null
        setState('done')
        return
      }
      if (data.startsWith('[错误]')) {
        es.close()
        eventSource = null
        setError(data)
        setState('error')
        return
      }
      setState('streaming')
      setContent((prev) => prev + data)
    }

    es.onerror = (ev) => {
      // EventSource fires error on close or network issues
      const readyState = es.readyState // 0=CONNECTING, 1=OPEN, 2=CLOSED
      console.error('[LlmInterpret] SSE error', { url: sseUrl, readyState, state: state(), event: ev })
      if (state() === 'loading') {
        setError(`连接失败 (readyState=${readyState})，请稍后重试`)
        setState('error')
      }
      es.close()
      eventSource = null
      // If we were streaming, treat as done (stream ended)
      if (state() === 'streaming') {
        setState('done')
      }
    }
  }

  return (
    <Card variant="outlined">
      <CardBody>
        <Show when={state() === 'checking'}>
          <div class="flex items-center gap-2 py-4 justify-center text-content-tertiary">
            <Spinner size="sm" />
            <span class="text-xs">检查缓存...</span>
          </div>
        </Show>

        <Show when={state() === 'idle'}>
          <div class="flex justify-center py-4">
            <Button variant="secondary" size="sm" onClick={start}>
              <svg class="w-4 h-4 mr-1.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                <path stroke-linecap="round" stroke-linejoin="round" d="M9.813 15.904L9 18.75l-.813-2.846a4.5 4.5 0 00-3.09-3.09L2.25 12l2.846-.813a4.5 4.5 0 003.09-3.09L9 5.25l.813 2.846a4.5 4.5 0 003.09 3.09L15.75 12l-2.846.813a4.5 4.5 0 00-3.09 3.09zM18.259 8.715L18 9.75l-.259-1.035a3.375 3.375 0 00-2.455-2.456L14.25 6l1.036-.259a3.375 3.375 0 002.455-2.456L18 2.25l.259 1.035a3.375 3.375 0 002.455 2.456L21.75 6l-1.036.259a3.375 3.375 0 00-2.455 2.456z" />
              </svg>
              {props.buttonLabel ?? 'AI 解读'}
            </Button>
          </div>
        </Show>

        <Show when={state() === 'loading'}>
          <div class="flex items-center gap-2 py-6 justify-center text-content-secondary">
            <Spinner size="sm" />
            <span class="text-sm">AI 正在分析中...</span>
          </div>
        </Show>

        <Show when={state() === 'streaming' || state() === 'done'}>
          <div class="space-y-2">
            <div class="flex items-center justify-between">
              <div class="flex items-center gap-1.5 text-xs text-content-tertiary">
                <svg class="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M9.813 15.904L9 18.75l-.813-2.846a4.5 4.5 0 00-3.09-3.09L2.25 12l2.846-.813a4.5 4.5 0 003.09-3.09L9 5.25l.813 2.846a4.5 4.5 0 003.09 3.09L15.75 12l-2.846.813a4.5 4.5 0 00-3.09 3.09zM18.259 8.715L18 9.75l-.259-1.035a3.375 3.375 0 00-2.455-2.456L14.25 6l1.036-.259a3.375 3.375 0 002.455-2.456L18 2.25l.259 1.035a3.375 3.375 0 002.455 2.456L21.75 6l-1.036.259a3.375 3.375 0 00-2.455 2.456z" />
                </svg>
                <span>AI 解读</span>
                <Show when={state() === 'streaming'}>
                  <Spinner size="sm" />
                </Show>
              </div>
              <Show when={state() === 'done'}>
                <Button variant="ghost" size="sm" onClick={start}>
                  重新生成
                </Button>
              </Show>
            </div>
            <div class="text-sm text-content leading-relaxed whitespace-pre-wrap break-words">
              {content()}
              <Show when={state() === 'streaming'}>
                <span class="inline-block w-1.5 h-4 bg-accent animate-pulse ml-0.5 align-middle" />
              </Show>
            </div>
            <Show when={state() === 'done'}>
              <div class="text-xs text-content-tertiary border-t border-border pt-2 mt-2">
                以上解读由 AI 生成，仅供参考，不能替代医生的专业诊断。
              </div>
            </Show>
          </div>
        </Show>

        <Show when={state() === 'error'}>
          <div class="text-center py-4 space-y-2">
            <p class="text-sm text-error">{error()}</p>
            <Button variant="outline" size="sm" onClick={start}>
              重试
            </Button>
          </div>
        </Show>
      </CardBody>
    </Card>
  )
}
