import { createSignal, createResource, Show, For } from 'solid-js'
import { useParams } from '@solidjs/router'
import { api } from '@/api/client'
import type { InteractionCheckResult, DrugInteraction as DrugInteractionType } from '@/api/types'
import { cn } from '@/lib/utils'
import { Button, Card, CardBody, Badge, Input, useToast, Spinner } from '@/components'

export default function DrugInteraction() {
  const params = useParams<{ id: string }>()
  const { toast } = useToast()

  const [newDrug, setNewDrug] = createSignal('')
  const [result, setResult] = createSignal<InteractionCheckResult | null>(null)
  const [loading, setLoading] = createSignal(false)

  // Load current medications
  const [meds] = createResource(() => params.id, (id) => api.medications.list(id))

  async function handleCheck() {
    setLoading(true)
    try {
      const drug = newDrug().trim() || undefined
      const data = await api.medications.checkPatientInteractions(params.id, drug)
      setResult(data)
      if (data.interactions.length === 0) {
        toast('success', '未发现药物相互作用')
      }
    } catch (err: any) {
      toast('error', err.message || '检查失败')
    } finally {
      setLoading(false)
    }
  }

  const highInteractions = () => (result()?.interactions ?? []).filter(i => i.severity === 'high')
  const mediumInteractions = () => (result()?.interactions ?? []).filter(i => i.severity === 'medium')
  const lowInteractions = () => (result()?.interactions ?? []).filter(i => i.severity === 'low')

  return (
    <div class="page-shell">
      <h1 class="page-title mb-5">药物相互作用检查</h1>

      {/* Current medications */}
      <Card variant="outlined" class="mb-5">
        <CardBody class="p-4">
          <h2 class="text-sm font-semibold text-content mb-3">当前用药</h2>
          <Show when={!meds.loading} fallback={<Spinner size="sm" />}>
            <Show when={!meds.error} fallback={
              <p class="text-sm text-error">加载用药记录失败: {String(meds.error)}</p>
            }>
              <Show when={(meds() ?? []).filter(m => m.active).length > 0} fallback={
                <p class="text-sm text-content-tertiary">暂无用药记录</p>
              }>
                <div class="flex flex-wrap gap-2">
                  <For each={(meds() ?? []).filter(m => m.active)}>
                    {(med) => (
                      <Badge variant="accent">{med.name}</Badge>
                    )}
                  </For>
                </div>
              </Show>
            </Show>
          </Show>
        </CardBody>
      </Card>

      {/* Input for new drug */}
      <Card variant="outlined" class="mb-5">
        <CardBody class="p-4">
          <h2 class="text-sm font-semibold text-content mb-3">新增药物（可选）</h2>
          <div class="flex gap-3 items-end">
            <div class="flex-1">
              <Input
                placeholder="输入新药名称，如：布洛芬"
                value={newDrug()}
                onInput={(e) => setNewDrug(e.currentTarget.value)}
                onKeyDown={(e: KeyboardEvent) => { if (e.key === 'Enter') handleCheck() }}
              />
            </div>
            <Button variant="primary" loading={loading()} onClick={handleCheck}>
              检查相互作用
            </Button>
          </div>
          <p class="text-xs text-content-tertiary mt-2">
            不填写新药名称则检查现有用药之间的相互作用
          </p>
        </CardBody>
      </Card>

      {/* Results */}
      <Show when={result()}>
        {(res) => (
          <>
            {/* Checked drugs */}
            <div class="flex items-center gap-2 mb-4 flex-wrap">
              <span class="text-xs text-content-secondary">已检查药物：</span>
              <For each={res().checked_drugs}>
                {(drug) => <Badge variant="info">{drug}</Badge>}
              </For>
            </div>

            {/* No interactions */}
            <Show when={res().interactions.length === 0}>
              <Card variant="outlined">
                <CardBody class="py-10 text-center">
                  <div class="w-12 h-12 mx-auto rounded-full bg-success/10 flex items-center justify-center mb-3">
                    <svg class="w-6 h-6 text-success" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                      <path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7" />
                    </svg>
                  </div>
                  <p class="text-sm font-medium text-success">未发现药物相互作用</p>
                  <p class="text-xs text-content-tertiary mt-1">已检查的药物之间未发现已知的相互作用</p>
                </CardBody>
              </Card>
            </Show>

            {/* High severity */}
            <Show when={highInteractions().length > 0}>
              <div class="mb-5">
                <div class="flex items-center gap-2 mb-3">
                  <div class="w-3 h-3 rounded-full bg-error" />
                  <span class="text-sm font-semibold text-error">高危 ({highInteractions().length})</span>
                </div>
                <div class="space-y-3">
                  <For each={highInteractions()}>
                    {(interaction) => <InteractionCard interaction={interaction} severity="high" />}
                  </For>
                </div>
              </div>
            </Show>

            {/* Medium severity */}
            <Show when={mediumInteractions().length > 0}>
              <div class="mb-5">
                <div class="flex items-center gap-2 mb-3">
                  <div class="w-3 h-3 rounded-full bg-warning" />
                  <span class="text-sm font-semibold text-warning">中危 ({mediumInteractions().length})</span>
                </div>
                <div class="space-y-3">
                  <For each={mediumInteractions()}>
                    {(interaction) => <InteractionCard interaction={interaction} severity="medium" />}
                  </For>
                </div>
              </div>
            </Show>

            {/* Low severity */}
            <Show when={lowInteractions().length > 0}>
              <div class="mb-5">
                <div class="flex items-center gap-2 mb-3">
                  <div class="w-3 h-3 rounded-full bg-info" />
                  <span class="text-sm font-semibold text-info">低危 ({lowInteractions().length})</span>
                </div>
                <div class="space-y-3">
                  <For each={lowInteractions()}>
                    {(interaction) => <InteractionCard interaction={interaction} severity="low" />}
                  </For>
                </div>
              </div>
            </Show>
          </>
        )}
      </Show>
    </div>
  )
}

function InteractionCard(props: { interaction: DrugInteractionType; severity: 'high' | 'medium' | 'low' }) {
  const borderColor = () => {
    switch (props.severity) {
      case 'high': return 'border-l-error'
      case 'medium': return 'border-l-warning'
      case 'low': return 'border-l-info'
    }
  }

  const bgColor = () => {
    switch (props.severity) {
      case 'high': return 'bg-error/5'
      case 'medium': return 'bg-warning/5'
      case 'low': return 'bg-info/5'
    }
  }

  return (
    <Card variant="outlined" class={cn('border-l-4 overflow-hidden', borderColor())}>
      <CardBody class={cn('p-4', bgColor())}>
        <div class="flex items-center gap-2 mb-2">
          <Badge variant={props.severity === 'high' ? 'error' : props.severity === 'medium' ? 'warning' : 'info'}>
            {props.interaction.drug1}
          </Badge>
          <svg class="w-4 h-4 text-content-tertiary" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
            <path stroke-linecap="round" stroke-linejoin="round" d="M8 7h12m0 0l-4-4m4 4l-4 4m0 6H4m0 0l4 4m-4-4l4-4" />
          </svg>
          <Badge variant={props.severity === 'high' ? 'error' : props.severity === 'medium' ? 'warning' : 'info'}>
            {props.interaction.drug2}
          </Badge>
        </div>
        <p class="text-sm text-content mb-2">{props.interaction.description}</p>
        <div class="flex items-start gap-1.5">
          <svg class="w-4 h-4 text-accent shrink-0 mt-0.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
            <path stroke-linecap="round" stroke-linejoin="round" d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
          </svg>
          <p class="text-xs text-content-secondary">{props.interaction.recommendation}</p>
        </div>
      </CardBody>
    </Card>
  )
}
