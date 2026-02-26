import { createSignal, createResource, Show, For } from 'solid-js'
import { useParams } from '@solidjs/router'
import { api } from '@/api/client'
import type { HealthAssessment } from '@/api/types'
import { cn } from '@/lib/utils'
import { Button, Card, CardBody, Badge, Spinner, useToast } from '@/components'

const RISK_COLORS: Record<string, string> = {
  '低': 'bg-success-light text-success',
  '中': 'bg-warning-light text-warning',
  '高': 'bg-error-light text-error',
}

const STATUS_COLORS: Record<string, string> = {
  '正常': 'text-success',
  '需关注': 'text-warning',
  '需就医': 'text-error',
}

export default function HealthAssessmentPage() {
  const params = useParams<{ id: string }>()
  const { toast } = useToast()
  const [patient] = createResource(() => params.id, (id) => api.patients.get(id))

  const [loading, setLoading] = createSignal(false)
  const [assessment, setAssessment] = createSignal<HealthAssessment | null>(null)
  const [error, setError] = createSignal('')

  async function startAssessment() {
    setLoading(true)
    setError('')
    setAssessment(null)
    try {
      const token = localStorage.getItem('auth_token')
      const resp = await fetch(`/api/patients/${params.id}/health-assessment`, {
        headers: { Authorization: `Bearer ${token}` },
      })

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
          if (data === '[DONE]') continue
          if (data.startsWith('[错误]')) {
            setError(data)
            continue
          }

          try {
            const parsed = JSON.parse(data) as HealthAssessment
            setAssessment(parsed)
          } catch {}
        }
      }
    } catch (err: any) {
      setError(err.message || '评估失败')
      toast('error', err.message || '评估失败')
    } finally {
      setLoading(false)
    }
  }

  return (
    <div class="page-shell">
      <div class="max-w-2xl mx-auto">
        <h1 class="page-title mb-1">AI 健康评估</h1>
        <Show when={patient()}>
          <p class="sub-text mb-6">基于 {patient()!.name} 的全部医疗数据生成综合评估</p>
        </Show>

        <Show when={!assessment() && !loading()}>
          <Card variant="elevated">
            <CardBody class="p-8 text-center">
              <svg class="w-16 h-16 mx-auto text-accent/30 mb-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
                <path stroke-linecap="round" stroke-linejoin="round" d="M9.75 3.104v5.714a2.25 2.25 0 01-.659 1.591L5 14.5M9.75 3.104c-.251.023-.501.05-.75.082m.75-.082a24.301 24.301 0 014.5 0m0 0v5.714a2.25 2.25 0 00.659 1.591L19 14.5M14.25 3.104c.251.023.501.05.75.082M19 14.5l-2.47 5.636a2.25 2.25 0 01-2.061 1.364H9.531a2.25 2.25 0 01-2.061-1.364L5 14.5m14 0H5" />
              </svg>
              <p class="text-content-secondary mb-4">AI 将综合分析所有报告、用药和体温数据，生成健康风险评估报告</p>
              <Button variant="primary" onClick={startAssessment}>开始评估</Button>
            </CardBody>
          </Card>
        </Show>

        <Show when={loading()}>
          <Card variant="elevated">
            <CardBody class="p-8 flex flex-col items-center gap-4">
              <Spinner size="lg" variant="orbital" />
              <p class="text-sm text-content-secondary">AI 正在分析数据，请稍候...</p>
            </CardBody>
          </Card>
        </Show>

        <Show when={error()}>
          <Card variant="outlined" class="border-error/30">
            <CardBody class="p-4">
              <p class="text-sm text-error">{error()}</p>
              <Button variant="outline" size="sm" class="mt-2" onClick={startAssessment}>重试</Button>
            </CardBody>
          </Card>
        </Show>

        <Show when={assessment()}>
          {(a) => (
            <div class="space-y-4">
              {/* Header */}
              <Card variant="elevated">
                <CardBody class="p-6">
                  <div class="flex items-center justify-between mb-4">
                    <div>
                      <span class={cn('text-lg font-bold', STATUS_COLORS[a().overall_status] || 'text-content')}>
                        {a().overall_status}
                      </span>
                    </div>
                    <Badge class={cn('px-3 py-1 text-sm', RISK_COLORS[a().risk_level] || 'bg-surface-secondary')}>
                      风险等级: {a().risk_level}
                    </Badge>
                  </div>
                  <p class="text-sm text-content-secondary">{a().summary}</p>
                </CardBody>
              </Card>

              {/* Findings */}
              <Show when={a().findings?.length}>
                <Card variant="outlined">
                  <CardBody class="p-4">
                    <h3 class="text-sm font-semibold text-content mb-3 flex items-center gap-2">
                      <svg class="w-4 h-4 text-info" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                        <path stroke-linecap="round" stroke-linejoin="round" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
                      </svg>
                      主要发现
                    </h3>
                    <ul class="space-y-2">
                      <For each={a().findings}>
                        {(f) => <li class="text-sm text-content-secondary flex gap-2"><span class="text-accent shrink-0">•</span>{f}</li>}
                      </For>
                    </ul>
                  </CardBody>
                </Card>
              </Show>

              {/* Recommendations */}
              <Show when={a().recommendations?.length}>
                <Card variant="outlined">
                  <CardBody class="p-4">
                    <h3 class="text-sm font-semibold text-content mb-3 flex items-center gap-2">
                      <svg class="w-4 h-4 text-success" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                        <path stroke-linecap="round" stroke-linejoin="round" d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z" />
                      </svg>
                      建议
                    </h3>
                    <ul class="space-y-2">
                      <For each={a().recommendations}>
                        {(r) => <li class="text-sm text-content-secondary flex gap-2"><span class="text-success shrink-0">•</span>{r}</li>}
                      </For>
                    </ul>
                  </CardBody>
                </Card>
              </Show>

              {/* Follow-up suggestions */}
              <Show when={a().follow_up_suggestions?.length}>
                <Card variant="outlined">
                  <CardBody class="p-4">
                    <h3 class="text-sm font-semibold text-content mb-3 flex items-center gap-2">
                      <svg class="w-4 h-4 text-warning" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                        <path stroke-linecap="round" stroke-linejoin="round" d="M8 7V3m8 4V3m-9 8h10M5 21h14a2 2 0 002-2V7a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" />
                      </svg>
                      随访建议
                    </h3>
                    <ul class="space-y-2">
                      <For each={a().follow_up_suggestions}>
                        {(s) => <li class="text-sm text-content-secondary flex gap-2"><span class="text-warning shrink-0">•</span>{s}</li>}
                      </For>
                    </ul>
                  </CardBody>
                </Card>
              </Show>

              {/* Disclaimer */}
              <Show when={a().disclaimer}>
                <p class="text-xs text-content-tertiary text-center py-2">{a().disclaimer}</p>
              </Show>

              <div class="text-center">
                <Button variant="outline" size="sm" onClick={startAssessment}>重新评估</Button>
              </div>
            </div>
          )}
        </Show>
      </div>
    </div>
  )
}
