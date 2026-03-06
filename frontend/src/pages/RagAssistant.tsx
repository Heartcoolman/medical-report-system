import { createSignal, createResource, Show, For } from 'solid-js'
import { useParams } from '@solidjs/router'
import { Button, Input, Spinner, useToast } from '@/components'
import { api, getErrorMessage } from '@/api/client'
import type { RagMessage } from '@/api/types'

export default function RagAssistant() {
  const params = useParams<{ id: string }>()
  const { toast } = useToast()

  const [status, { refetch: refetchStatus }] = createResource(
    () => params.id,
    (id) => api.rag.status(id),
  )

  const [messages, setMessages] = createSignal<RagMessage[]>([])
  const [input, setInput] = createSignal('')
  const [querying, setQuerying] = createSignal(false)
  const [building, setBuilding] = createSignal(false)

  let messagesEndRef: HTMLDivElement | undefined

  function scrollToBottom() {
    messagesEndRef?.scrollIntoView({ behavior: 'smooth' })
  }

  async function handleBuild() {
    setBuilding(true)
    try {
      const result = await api.rag.build(params.id)
      toast('success', `知识库已构建，共索引 ${result.chunks_indexed} 个数据块`)
      refetchStatus()
    } catch (e: unknown) {
      toast('error', getErrorMessage(e) || '构建失败')
    } finally {
      setBuilding(false)
    }
  }

  async function handleQuery() {
    const q = input().trim()
    if (!q) return

    setMessages((m) => [...m, { role: 'user', content: q }])
    setInput('')
    setQuerying(true)
    setTimeout(scrollToBottom, 50)

    try {
      const result = await api.rag.query(params.id, q)
      setMessages((m) => [
        ...m,
        { role: 'assistant', content: result.answer, sources: result.sources },
      ])
    } catch (e: unknown) {
      toast('error', '查询失败：' + (getErrorMessage(e) || '未知错误'))
    } finally {
      setQuerying(false)
      setTimeout(scrollToBottom, 50)
    }
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault()
      handleQuery()
    }
  }

  return (
    <div class="flex flex-col h-[calc(100vh-4rem)] max-w-4xl mx-auto p-4 gap-4">
      {/* Header */}
      <div class="flex items-center justify-between bg-surface rounded-xl p-4 shadow-sm border border-border">
        <div>
          <h1 class="font-semibold text-content text-lg">AI 医学问答助手</h1>
          <Show
            when={!status.loading}
            fallback={<p class="text-sm text-content-secondary">加载中...</p>}
          >
            <Show when={status.error}>
              <p class="text-sm text-error">加载知识库状态失败</p>
            </Show>
            <Show when={!status.error && status()}>
              <p class="text-sm text-content-secondary">
                知识库：{status()!.indexed_chunks} 个数据块
                {status()!.last_built
                  ? `（更新于 ${status()!.last_built}）`
                  : '（未构建）'}
              </p>
            </Show>
          </Show>
        </div>
        <Button
          onClick={handleBuild}
          disabled={building()}
          variant="secondary"
          size="sm"
        >
          {building() ? '构建中...' : '重建知识库'}
        </Button>
      </div>

      {/* Messages */}
      <div class="flex-1 overflow-y-auto flex flex-col gap-3 min-h-0 px-1">
        <Show when={messages().length === 0}>
          <div class="text-center text-content-secondary mt-12">
            <p class="text-base">请先构建知识库，然后提问</p>
            <p class="text-sm mt-2 opacity-70">
              示例：为什么我的肌酐一直偏高？最近血糖改善了吗？
            </p>
          </div>
        </Show>

        <For each={messages()}>
          {(msg) => (
            <div
              class={`flex ${msg.role === 'user' ? 'justify-end' : 'justify-start'}`}
            >
              <div
                class={`max-w-[80%] rounded-xl p-3 ${
                  msg.role === 'user'
                    ? 'bg-accent text-white'
                    : 'bg-surface border border-border shadow-sm'
                }`}
              >
                <p class="whitespace-pre-wrap text-sm leading-relaxed">
                  {msg.content}
                </p>
                <Show when={msg.sources && msg.sources!.length > 0}>
                  <details class="mt-2">
                    <summary class="text-xs opacity-60 cursor-pointer">
                      参考数据来源 ({msg.sources!.length})
                    </summary>
                    <For each={msg.sources}>
                      {(src) => (
                        <p class="text-xs opacity-50 mt-1 bg-surface-secondary p-1.5 rounded">
                          相关度 {(src.score * 100).toFixed(0)}%:{' '}
                          {src.content_preview}
                        </p>
                      )}
                    </For>
                  </details>
                </Show>
              </div>
            </div>
          )}
        </For>

        <Show when={querying()}>
          <div class="flex justify-start">
            <div class="bg-surface border border-border shadow-sm rounded-xl p-3 flex items-center gap-2">
              <Spinner size="sm" />
              <span class="text-sm text-content-secondary">AI 正在分析...</span>
            </div>
          </div>
        </Show>

        <div ref={messagesEndRef} />
      </div>

      {/* Input */}
      <div class="flex gap-2">
        <Input
          placeholder="输入问题，例如：为什么我的肌酐偏高？"
          value={input()}
          onInput={(e) => setInput(e.currentTarget.value)}
          onKeyDown={handleKeyDown}
          disabled={querying()}
          class="flex-1"
        />
        <Button
          onClick={handleQuery}
          disabled={querying() || !input().trim()}
        >
          发送
        </Button>
      </div>
    </div>
  )
}
