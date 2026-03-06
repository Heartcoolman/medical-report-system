import { createSignal, createResource, Show, For } from 'solid-js'
import { useParams } from '@solidjs/router'
import { api, getErrorMessage } from '@/api/client'
import type { RiskPrediction, RiskFactor } from '@/api/types'
import { Button, Card, CardBody, Spinner, useToast } from '@/components'

const LEVEL_CONFIG: Record<string, { color: string; bg: string; icon: string; label: string }> = {
  '低': {
    color: 'text-success',
    bg: 'bg-success-light',
    icon: '✓',
    label: '低风险',
  },
  '中': {
    color: 'text-warning',
    bg: 'bg-warning-light',
    icon: '!',
    label: '中风险',
  },
  '高': {
    color: 'text-error',
    bg: 'bg-error-light',
    icon: '!!',
    label: '高风险',
  },
}

const SEVERITY_BADGE: Record<string, string> = {
  '低': 'bg-success-light text-success',
  '中': 'bg-warning-light text-warning',
  '高': 'bg-error-light text-error',
}

const TREND_ICON: Record<string, string> = {
  '稳定': '→',
  '好转': '↓',
  '恶化': '↑',
  '波动': '~',
}

function formatTime(ts: string): string {
  return ts.replace('T', ' ').slice(0, 16)
}

export default function RiskPredictionPage() {
  const params = useParams<{ id: string }>()
  const { toast } = useToast()

  const [patient] = createResource(() => params.id, (id) => api.patients.get(id))
  const [refreshing, setRefreshing] = createSignal(false)
  const [prediction, setPrediction] = createSignal<RiskPrediction | null>(null)

  const [initialError, setInitialError] = createSignal<string | null>(null)
  const [initialPrediction] = createResource(
    () => params.id,
    async (id) => {
      try {
        setInitialError(null)
        return await api.riskPrediction.get(id)
      } catch (e: unknown) {
        setInitialError(getErrorMessage(e) || '加载预测数据失败')
        return null
      }
    }
  )

  const effectivePrediction = (): RiskPrediction | null => {
    return prediction() ?? initialPrediction() ?? null
  }

  async function handleRefresh() {
    setRefreshing(true)
    try {
      const result = await api.riskPrediction.get(params.id, true)
      setPrediction(result)
      toast('success', '风险预测已更新')
    } catch (err: unknown) {
      toast('error', getErrorMessage(err) || '刷新失败')
    } finally {
      setRefreshing(false)
    }
  }

  const levelConfig = () => {
    const p = effectivePrediction()
    if (!p) return LEVEL_CONFIG['低']
    return LEVEL_CONFIG[p.risk_level] ?? LEVEL_CONFIG['低']
  }

  return (
    <div class="page-shell">
      <div class="max-w-2xl mx-auto">
        <h1 class="page-title mb-1">风险预测</h1>
        <Show when={patient()}>
          <p class="sub-text mb-6">基于 {patient()!.name} 的历史检验趋势预测健康风险</p>
        </Show>

        {/* 加载失败 */}
        <Show when={initialError() && !prediction()}>
          <Card variant="elevated">
            <CardBody class="p-8 text-center">
              <div class="w-12 h-12 mx-auto rounded-full bg-error/10 flex items-center justify-center mb-3">
                <svg class="w-6 h-6 text-error" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M12 9v3.75m9-.75a9 9 0 11-18 0 9 9 0 0118 0zm-9 3.75h.008v.008H12v-.008z" />
                </svg>
              </div>
              <p class="text-sm font-medium text-error mb-1">加载预测数据失败</p>
              <p class="text-xs text-content-tertiary mb-4">{initialError()}</p>
              <Button variant="outline" size="sm" onClick={handleRefresh}>重试</Button>
            </CardBody>
          </Card>
        </Show>

        {/* 加载中 */}
        <Show when={initialPrediction.loading && !prediction()}>
          <Card variant="elevated">
            <CardBody class="p-8 flex flex-col items-center gap-4">
              <Spinner size="lg" variant="orbital" />
              <p class="text-sm text-content-secondary">正在分析数据...</p>
            </CardBody>
          </Card>
        </Show>

        {/* 刷新中 */}
        <Show when={refreshing()}>
          <Card variant="elevated">
            <CardBody class="p-8 flex flex-col items-center gap-4">
              <Spinner size="lg" variant="orbital" />
              <p class="text-sm text-content-secondary">AI 正在重新评估，请稍候（约10-20秒）...</p>
            </CardBody>
          </Card>
        </Show>

        {/* 结果展示 */}
        <Show when={effectivePrediction() && !refreshing()}>
          {(_) => {
            const p = effectivePrediction()!
            const cfg = levelConfig()
            return (
              <div class="space-y-4">
                {/* 风险等级卡片 */}
                <Card variant="elevated">
                  <CardBody class="p-6">
                    <div class="flex items-center justify-between mb-4">
                      <div class="flex items-center gap-3">
                        <div class={`w-12 h-12 rounded-full flex items-center justify-center text-xl font-bold ${cfg.bg} ${cfg.color}`}>
                          {cfg.icon}
                        </div>
                        <div>
                          <div class={`text-2xl font-bold ${cfg.color}`}>{cfg.label}</div>
                          <div class="text-sm text-content-tertiary">综合评分 {p.risk_score}/100</div>
                        </div>
                      </div>
                      <Button
                        variant="outline"
                        size="sm"
                        onClick={handleRefresh}
                        disabled={refreshing()}
                      >
                        重新评估
                      </Button>
                    </div>

                    {/* 评分进度条 */}
                    <div class="w-full bg-surface-secondary rounded-full h-2 mb-2">
                      <div
                        class={`h-2 rounded-full transition-all ${
                          p.risk_score >= 51 ? 'bg-error' :
                          p.risk_score >= 21 ? 'bg-warning' : 'bg-success'
                        }`}
                        style={{ width: `${p.risk_score}%` }}
                      />
                    </div>
                    <div class="flex justify-between text-xs text-content-tertiary">
                      <span>低风险</span>
                      <span>中风险</span>
                      <span>高风险</span>
                    </div>

                    <div class="mt-3 text-xs text-content-tertiary flex items-center gap-2">
                      <span>{p.cached ? '📋 来自缓存' : '✨ 最新预测'}</span>
                      <span>·</span>
                      <span>{formatTime(p.generated_at)}</span>
                    </div>
                  </CardBody>
                </Card>

                {/* 风险因素 */}
                <Show when={p.risk_factors?.length > 0}>
                  <Card variant="elevated">
                    <CardBody class="p-5">
                      <p class="micro-title mb-3">风险因素</p>
                      <div class="space-y-3">
                        <For each={p.risk_factors}>
                          {(factor: RiskFactor) => (
                            <div class="p-3 bg-surface-secondary rounded-lg">
                              <div class="flex items-start justify-between gap-2 mb-1">
                                <p class="text-sm font-medium text-content-primary flex-1">{factor.description}</p>
                                <span class={`text-xs px-2 py-0.5 rounded-full font-medium shrink-0 ${SEVERITY_BADGE[factor.severity] ?? ''}`}>
                                  {factor.severity}
                                </span>
                              </div>
                              <div class="flex items-center gap-3 text-xs text-content-secondary">
                                <span>{factor.category}</span>
                                <span>·</span>
                                <span>
                                  趋势 {TREND_ICON[factor.trend] ?? ''} {factor.trend}
                                </span>
                                <Show when={factor.last_value}>
                                  <span>·</span>
                                  <span>当前值 {factor.last_value}</span>
                                </Show>
                              </div>
                            </div>
                          )}
                        </For>
                      </div>
                    </CardBody>
                  </Card>
                </Show>

                {/* 建议 */}
                <Show when={p.recommendations?.length > 0}>
                  <Card variant="elevated">
                    <CardBody class="p-5">
                      <p class="micro-title mb-3">健康建议</p>
                      <ul class="space-y-2">
                        <For each={p.recommendations}>
                          {(rec: string) => (
                            <li class="flex items-start gap-2 text-sm text-content-primary">
                              <span class="text-accent mt-0.5 shrink-0">•</span>
                              <span>{rec}</span>
                            </li>
                          )}
                        </For>
                      </ul>
                    </CardBody>
                  </Card>
                </Show>

                {/* 下次复查 */}
                <Show when={p.next_review_date}>
                  <Card variant="elevated">
                    <CardBody class="p-5">
                      <div class="flex items-center gap-3">
                        <div class="w-8 h-8 rounded-lg bg-accent/10 flex items-center justify-center text-accent text-sm">📅</div>
                        <div>
                          <p class="micro-title">建议复查日期</p>
                          <p class="text-sm font-medium text-content-primary">{p.next_review_date}</p>
                        </div>
                      </div>
                    </CardBody>
                  </Card>
                </Show>

                <p class="text-xs text-content-tertiary text-center px-4">
                  以上预测仅供参考，不能替代医生诊断。如有不适请及时就医。
                </p>
              </div>
            )
          }}
        </Show>

        {/* 无数据提示 */}
        <Show when={!initialPrediction.loading && !effectivePrediction() && !refreshing()}>
          <Card variant="elevated">
            <CardBody class="p-8 text-center">
              <p class="text-content-secondary mb-4">暂无预测数据，点击下方按钮生成风险预测</p>
              <Button variant="primary" onClick={handleRefresh} disabled={refreshing()}>
                生成风险预测
              </Button>
            </CardBody>
          </Card>
        </Show>
      </div>
    </div>
  )
}
