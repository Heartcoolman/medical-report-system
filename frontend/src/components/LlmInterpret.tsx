import { createSignal, createEffect, For, Show, onCleanup } from 'solid-js'
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
  const [points, setPoints] = createSignal<string[] | null>(null)
  const [error, setError] = createSignal('')
  let currentAbort: AbortController | null = null

  function extractPoints(value: unknown): string[] | null {
    if (Array.isArray(value) && value.every((v) => typeof v === 'string')) {
      return value as string[]
    }
    if (value && typeof value === 'object' && 'points' in value) {
      const pts = (value as { points?: unknown }).points
      if (Array.isArray(pts) && pts.every((v) => typeof v === 'string')) {
        return pts as string[]
      }
    }
    return null
  }

  function applyInterpretation(value: unknown) {
    const pts = extractPoints(value)
    if (pts) {
      setPoints(pts)
      setContent('')
      return
    }

    if (typeof value === 'string') {
      setPoints(null)
      setContent(value)
      return
    }

    setPoints(null)
    try {
      setContent(JSON.stringify(value))
    } catch {
      setContent(String(value))
    }
  }

  function cleanup() {
    if (currentAbort) {
      currentAbort.abort()
      currentAbort = null
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
    setPoints(null)
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
      .then((json: { success: boolean; data: { content: unknown; created_at: string } | null }) => {
        // Only apply if url hasn't changed while we were fetching
        if (props.url !== url) return
        if (json.success && json.data && json.data.content !== undefined && json.data.content !== null) {
          applyInterpretation(json.data.content)
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
    setPoints(null)
    setError('')
    setState('loading')

    const abort = new AbortController()
    currentAbort = abort

    const token = localStorage.getItem('auth_token')
    fetch(props.url, {
      headers: token ? { Authorization: `Bearer ${token}` } : {},
      signal: abort.signal,
    })
      .then(async (resp) => {
        if (!resp.ok) {
          throw new Error(`请求失败: ${resp.status}`)
        }
        const reader = resp.body?.getReader()
        if (!reader) throw new Error('无法读取响应流')

        const decoder = new TextDecoder()
        let buffer = ''

        while (true) {
          const { done, value } = await reader.read()
          if (done) break
          buffer += decoder.decode(value, { stream: true })

          while (buffer.includes('\n')) {
            const idx = buffer.indexOf('\n')
            const line = buffer.slice(0, idx).trim()
            buffer = buffer.slice(idx + 1)

            if (!line.startsWith('data:')) continue
            const data = line.slice(5).trim()

            if (data === '[DONE]') {
              currentAbort = null
              setState('done')
              return
            }
            if (data.startsWith('[错误]')) {
              currentAbort = null
              setError(data)
              setState('error')
              return
            }
            setState('streaming')

            const trimmed = data.trim()
            if (trimmed.startsWith('{') || trimmed.startsWith('[')) {
              try {
                const parsed = JSON.parse(trimmed) as unknown
                const pts = extractPoints(parsed)
                if (pts) {
                  setPoints(pts)
                  setContent('')
                  continue
                }
              } catch {
                // fall back to plain text streaming
              }
            }

            setContent((prev) => prev + data)
          }
        }

        // Stream ended without [DONE]
        if (state() === 'streaming') {
          setState('done')
        }
      })
      .catch((err) => {
        if (abort.signal.aborted) return
        setError(err.message || '连接失败，请稍后重试')
        setState('error')
      })
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
            <Show
              when={points() && points()!.length > 0}
              fallback={
                <div class="text-sm text-content leading-relaxed whitespace-pre-wrap break-words">
                  {content()}
                  <Show when={state() === 'streaming'}>
                    <span class="inline-block w-1.5 h-4 bg-accent animate-pulse ml-0.5 align-middle" />
                  </Show>
                </div>
              }
            >
              <div class="text-sm text-content leading-relaxed break-words space-y-2">
                <For each={points() ?? []}>
                  {(p, idx) => (
                    <div class="whitespace-pre-wrap">
                      {idx() + 1}. {p}
                    </div>
                  )}
                </For>
                <Show when={state() === 'streaming'}>
                  <span class="inline-block w-1.5 h-4 bg-accent animate-pulse ml-0.5 align-middle" />
                </Show>
              </div>
            </Show>
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
