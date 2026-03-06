import { createSignal, createResource, For, Show } from 'solid-js'
import { useParams, useNavigate } from '@solidjs/router'
import {
  Button, Card, CardBody, CardHeader, Spinner, useToast, Modal,
} from '@/components'
import { api, getErrorMessage } from '@/api/client'
import type { ExpenseItem } from '@/api/types'
import {
  EXPENSE_CATEGORY_LABELS as CATEGORY_LABELS,
  EXPENSE_CATEGORY_ICONS as CATEGORY_ICONS,
  EXPENSE_CATEGORY_COLORS as CATEGORY_COLORS,
} from '@/lib/expense-constants'

export default function ExpenseDetail() {
  const params = useParams()
  const navigate = useNavigate()
  const { toast } = useToast()

  const [showDeleteModal, setShowDeleteModal] = createSignal(false)
  const [deleting, setDeleting] = createSignal(false)

  const [detail] = createResource(
    () => params.id as string,
    (id) => api.expenses.get(id),
  )

  function groupedItems(): Record<string, ExpenseItem[]> {
    const d = detail()
    if (!d) return {}
    const groups: Record<string, ExpenseItem[]> = {}
    for (const item of d.items) {
      const cat = item.category || 'other'
      if (!groups[cat]) groups[cat] = []
      groups[cat].push(item)
    }
    return groups
  }

  function categorySubtotal(cat: string): number {
    const groups = groupedItems()
    return (groups[cat] || []).reduce((sum, item) => sum + item.amount, 0)
  }

  async function handleDelete() {
    setDeleting(true)
    try {
      await api.expenses.delete(params.id as string)
      toast('success', '消费记录已删除')
      navigate(-1)
    } catch (err: unknown) {
      toast('error', getErrorMessage(err) || '删除失败')
    } finally {
      setDeleting(false)
      setShowDeleteModal(false)
    }
  }

  return (
    <div class="page-shell space-y-4">
      {/* Header */}
      <h1 class="page-title">消费详情</h1>

      <Show when={detail.loading}>
        <div class="flex items-center justify-center py-12">
          <Spinner size="lg" variant="orbital" />
        </div>
      </Show>

      <Show when={detail.error}>
        <Card>
          <CardBody>
            <p class="text-error text-center">加载失败: {(detail.error as Error)?.message}</p>
          </CardBody>
        </Card>
      </Show>

      <Show when={detail()}>
        {(d) => (
          <div class="space-y-4">
            {/* Summary Card */}
            <Card>
              <CardBody>
                <div class="flex items-center justify-between flex-wrap gap-3">
                  <div>
                    <div class="text-content-secondary text-sm">消费日期</div>
                    <div class="text-lg font-bold text-content">{d().expense_date}</div>
                  </div>
                  <div class="text-right">
                    <div class="text-content-secondary text-sm">合计金额</div>
                    <div class="text-2xl font-bold text-accent">¥{d().total_amount.toFixed(2)}</div>
                  </div>
                </div>

                {/* Category summary chips */}
                <div class="flex flex-wrap gap-2 mt-4">
                  <For each={Object.entries(groupedItems())}>
                    {([cat, items]) => (
                      <span class={`inline-flex items-center gap-1 px-2.5 py-1 rounded-full text-xs font-medium ${CATEGORY_COLORS[cat] || CATEGORY_COLORS.other}`}>
                        <svg class="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d={CATEGORY_ICONS[cat] || CATEGORY_ICONS.other} /></svg>
                        {CATEGORY_LABELS[cat] || cat} {items.length}项 ¥{categorySubtotal(cat).toFixed(2)}
                      </span>
                    )}
                  </For>
                </div>
              </CardBody>
            </Card>

            {/* Items grouped by category */}
            <For each={Object.entries(groupedItems())}>
              {([cat, items]) => (
                <Card>
                  <CardHeader>
                    <div class="flex items-center justify-between w-full">
                      <span class={`inline-flex items-center gap-1 px-2 py-0.5 rounded text-xs font-medium ${CATEGORY_COLORS[cat] || CATEGORY_COLORS.other}`}>
                        <svg class="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d={CATEGORY_ICONS[cat] || CATEGORY_ICONS.other} /></svg>
                        {CATEGORY_LABELS[cat] || cat}
                      </span>
                      <span class="text-sm font-medium text-content">
                        小计: ¥{categorySubtotal(cat).toFixed(2)}
                      </span>
                    </div>
                  </CardHeader>
                  <CardBody>
                    <div class="space-y-1">
                      <For each={items}>
                        {(item) => (
                          <div class="flex items-center gap-2 py-1.5 px-2 rounded hover:bg-surface-secondary text-sm">
                            <span class="flex-1 text-content">{item.name}</span>
                            <Show when={item.quantity}>
                              <span class="text-content-secondary text-xs">{item.quantity}</span>
                            </Show>
                            <span class={`font-medium whitespace-nowrap ${item.amount < 0 ? 'text-success' : 'text-content'}`}>
                              ¥{item.amount.toFixed(2)}
                            </span>
                          </div>
                        )}
                      </For>
                    </div>
                  </CardBody>
                </Card>
              )}
            </For>

            {/* AI Analysis */}
            <Show when={d().drug_analysis || d().treatment_analysis}>
              <Card>
                <CardHeader>
                  <div class="flex items-center gap-1.5">
                    <svg class="w-4 h-4 text-accent" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                      <path stroke-linecap="round" stroke-linejoin="round" d="M9.813 15.904L9 18.75l-.813-2.846a4.5 4.5 0 00-3.09-3.09L2.25 12l2.846-.813a4.5 4.5 0 003.09-3.09L9 5.25l.813 2.846a4.5 4.5 0 003.09 3.09L15.75 12l-2.846.813a4.5 4.5 0 00-3.09 3.09zM18.259 8.715L18 9.75l-.259-1.035a3.375 3.375 0 00-2.455-2.456L14.25 6l1.036-.259a3.375 3.375 0 002.455-2.456L18 2.25l.259 1.035a3.375 3.375 0 002.455 2.456L21.75 6l-1.036.259a3.375 3.375 0 00-2.455 2.456zM16.894 20.567L16.5 21.75l-.394-1.183a2.25 2.25 0 00-1.423-1.423L13.5 18.75l1.183-.394a2.25 2.25 0 001.423-1.423l.394-1.183.394 1.183a2.25 2.25 0 001.423 1.423l1.183.394-1.183.394a2.25 2.25 0 00-1.423 1.423z" />
                    </svg>
                    <span class="text-sm font-medium text-content">AI 分析</span>
                  </div>
                </CardHeader>
                <CardBody>
                  <div class="space-y-3 text-sm">
                    <Show when={d().drug_analysis}>
                      <div>
                        <div class="flex items-center gap-1 font-medium text-accent mb-1">
                          <svg class="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M19.428 15.428a2 2 0 00-1.022-.547l-2.387-.477a6 6 0 00-3.86.517l-.318.158a6 6 0 01-3.86.517L6.05 15.21a2 2 0 00-1.806.547M8 4h8l-1 1v5.172a2 2 0 00.586 1.414l5 5c1.26 1.26.367 3.414-1.415 3.414H4.828c-1.782 0-2.674-2.154-1.414-3.414l5-5A2 2 0 009 10.172V5L8 4z" /></svg>
                          用药分析
                        </div>
                        <p class="text-content-secondary leading-relaxed whitespace-pre-wrap">{d().drug_analysis}</p>
                      </div>
                    </Show>
                    <Show when={d().treatment_analysis}>
                      <div>
                        <div class="flex items-center gap-1 font-medium text-accent mb-1">
                          <svg class="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M21 8.25c0-2.485-2.099-4.5-4.688-4.5-1.935 0-3.597 1.126-4.312 2.733-.715-1.607-2.377-2.733-4.313-2.733C5.1 3.75 3 5.765 3 8.25c0 7.22 9 12 9 12s9-4.78 9-12z" /></svg>
                          治疗方案
                        </div>
                        <p class="text-content-secondary leading-relaxed whitespace-pre-wrap">{d().treatment_analysis}</p>
                      </div>
                    </Show>
                  </div>
                </CardBody>
              </Card>
            </Show>

            {/* Delete button */}
            <div class="flex justify-end">
              <Button variant="ghost" onClick={() => setShowDeleteModal(true)}>
                <span class="text-error">删除此记录</span>
              </Button>
            </div>

            {/* Delete confirmation modal */}
            <Modal
              open={showDeleteModal()}
              onClose={() => setShowDeleteModal(false)}
              title="确认删除"
              size="sm"
              footer={
                <>
                  <Button variant="outline" onClick={() => setShowDeleteModal(false)}>取消</Button>
                  <Button variant="danger" loading={deleting()} onClick={handleDelete}>确认删除</Button>
                </>
              }
            >
              <p class="text-content-secondary">
                确定要删除 {d().expense_date} 的消费记录吗？此操作不可撤销。
              </p>
            </Modal>
          </div>
        )}
      </Show>
    </div>
  )
}
