import { createResource, createSignal, Show, For } from 'solid-js'
import { useParams } from '@solidjs/router'
import { api, getErrorMessage } from '@/api/client'
import { cn } from '@/lib/utils'
import { Card, CardBody, Badge, Spinner, Empty, Button, useToast } from '@/components'

export default function MedLabCorrelation() {
  const params = useParams<{ id: string }>()
  const { toast } = useToast()
  const [refreshing, setRefreshing] = createSignal(false)

  const [data, { mutate, refetch }] = createResource(() => params.id, (id) => api.medLabCorrelation.get(id))

  const trendLabel = (trend: string) => {
    switch (trend) {
      case 'improved': return '好转'
      case 'worsened': return '恶化'
      default: return '无变化'
    }
  }

  const trendVariant = (trend: string) => {
    switch (trend) {
      case 'improved': return 'success' as const
      case 'worsened': return 'error' as const
      default: return 'info' as const
    }
  }

  const changePctColor = (pct: number) => {
    if (pct > 10) return 'text-error'
    if (pct < -10) return 'text-success'
    return 'text-content-secondary'
  }

  async function handleRefresh() {
    setRefreshing(true)
    try {
      const result = await api.medLabCorrelation.get(params.id, true)
      mutate(result)
      toast('success', '关联分析已更新')
    } catch (err: unknown) {
      toast('error', getErrorMessage(err) || '重新分析失败')
      refetch()
    } finally {
      setRefreshing(false)
    }
  }

  return (
    <div class="page-shell">
      <div class="flex items-center justify-between gap-3 mb-5">
        <h1 class="page-title">用药-检验关联分析</h1>
        <Button variant="outline" size="sm" onClick={handleRefresh} loading={refreshing()}>
          重新分析
        </Button>
      </div>

      <Show when={data.loading || refreshing()}>
        <div class="flex flex-col items-center justify-center py-20 gap-3">
          <Spinner size="xl" variant="orbital" />
          <span class="text-sm text-content-secondary">
            {refreshing() ? '正在重新分析用药与检验数据的关联...' : '正在分析用药与检验数据的关联...'}
          </span>
        </div>
      </Show>

      <Show when={!data.loading && !refreshing() && data.error}>
        <Card variant="outlined">
          <CardBody class="p-8 text-center">
            <div class="w-12 h-12 mx-auto rounded-full bg-error/10 flex items-center justify-center mb-3">
              <svg class="w-6 h-6 text-error" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                <path stroke-linecap="round" stroke-linejoin="round" d="M12 9v3.75m9-.75a9 9 0 11-18 0 9 9 0 0118 0zm-9 3.75h.008v.008H12v-.008z" />
              </svg>
            </div>
            <p class="text-sm font-medium text-error mb-1">加载关联分析失败</p>
            <p class="text-xs text-content-tertiary mb-4">{String(data.error)}</p>
            <Button variant="outline" size="sm" onClick={handleRefresh}>重试</Button>
          </CardBody>
        </Card>
      </Show>

      <Show when={!data.loading && !refreshing() && !data.error && data()}>
        {(result) => (
          <Show
            when={result().correlations.length > 0}
            fallback={
              <Empty title="暂无用药记录或数据不足" description="需要有用药记录和对应时段的检验数据才能进行关联分析" />
            }
          >
            <div class="space-y-5">
              <For each={result().correlations}>
                {(corr) => (
                  <Card variant="outlined">
                    <CardBody class="p-4">
                      {/* Drug header */}
                      <div class="flex items-center justify-between mb-4">
                        <div class="flex items-center gap-3">
                          <div class="w-10 h-10 rounded-xl bg-accent/10 flex items-center justify-center">
                            <svg class="w-5 h-5 text-accent" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                              <path stroke-linecap="round" stroke-linejoin="round" d="M19.428 15.428a2 2 0 00-1.022-.547l-2.387-.477a6 6 0 00-3.86.517l-.318.158a6 6 0 01-3.86.517L6.05 15.21a2 2 0 00-1.806.547M8 4h8l-1 1v5.172a2 2 0 00.586 1.414l5 5c1.26 1.26.367 3.414-1.415 3.414H4.828c-1.782 0-2.674-2.154-1.414-3.414l5-5A2 2 0 009 10.172V5L8 4z" />
                            </svg>
                          </div>
                          <div>
                            <h3 class="text-base font-semibold text-content">{corr.drug_name}</h3>
                            <p class="text-xs text-content-tertiary">
                              {corr.start_date} 至 {corr.end_date || '至今'}
                            </p>
                          </div>
                        </div>
                        <Show when={corr.affected_items.length > 0}>
                          <Badge variant="accent">{corr.affected_items.length} 项变化</Badge>
                        </Show>
                      </div>

                      {/* Affected items table */}
                      <Show when={corr.affected_items.length > 0} fallback={
                        <p class="text-sm text-content-tertiary text-center py-4">
                          用药前后检验指标无显著变化（变化小于10%）
                        </p>
                      }>
                        <div class="overflow-x-auto">
                          <table class="w-full text-sm">
                            <thead>
                              <tr class="border-b border-border/50">
                                <th class="text-left py-2 pr-3 text-xs font-medium text-content-secondary">指标名称</th>
                                <th class="text-right py-2 px-3 text-xs font-medium text-content-secondary">用药前均值</th>
                                <th class="text-right py-2 px-3 text-xs font-medium text-content-secondary">用药期间均值</th>
                                <th class="text-right py-2 px-3 text-xs font-medium text-content-secondary">变化%</th>
                                <th class="text-center py-2 pl-3 text-xs font-medium text-content-secondary">趋势</th>
                              </tr>
                            </thead>
                            <tbody>
                              <For each={corr.affected_items}>
                                {(item) => (
                                  <tr class="border-b border-border/30">
                                    <td class="py-2.5 pr-3">
                                      <span class="font-medium text-content">{item.canonical_name}</span>
                                    </td>
                                    <td class="text-right py-2.5 px-3 text-content-secondary">
                                      {item.before_avg !== null ? item.before_avg.toFixed(2) : '-'} {item.unit}
                                    </td>
                                    <td class="text-right py-2.5 px-3 text-content-secondary">
                                      {item.during_avg !== null ? item.during_avg.toFixed(2) : '-'} {item.unit}
                                    </td>
                                    <td class={cn('text-right py-2.5 px-3 font-medium', changePctColor(item.change_pct))}>
                                      {item.change_pct > 0 ? '+' : ''}{item.change_pct.toFixed(1)}%
                                    </td>
                                    <td class="text-center py-2.5 pl-3">
                                      <Badge variant={trendVariant(item.trend)}>{trendLabel(item.trend)}</Badge>
                                    </td>
                                  </tr>
                                )}
                              </For>
                            </tbody>
                          </table>
                        </div>
                      </Show>

                      {/* LLM summary */}
                      <Show when={corr.llm_summary}>
                        <div class="mt-4 p-3 rounded-lg bg-surface-secondary">
                          <div class="flex items-center gap-2 mb-2">
                            <svg class="w-4 h-4 text-accent" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                              <path stroke-linecap="round" stroke-linejoin="round" d="M9.663 17h4.673M12 3v1m6.364 1.636l-.707.707M21 12h-1M4 12H3m3.343-5.657l-.707-.707m2.828 9.9a5 5 0 117.072 0l-.548.547A3.374 3.374 0 0014 18.469V19a2 2 0 11-4 0v-.531c0-.895-.356-1.754-.988-2.386l-.548-.547z" />
                            </svg>
                            <span class="text-xs font-semibold text-content-secondary">AI 分析</span>
                          </div>
                          <p class="text-sm text-content leading-relaxed">{corr.llm_summary}</p>
                        </div>
                      </Show>
                    </CardBody>
                  </Card>
                )}
              </For>
            </div>
          </Show>
        )}
      </Show>
    </div>
  )
}
