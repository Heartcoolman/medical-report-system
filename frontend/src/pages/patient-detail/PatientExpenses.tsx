import { createSignal, createMemo, Show, For } from 'solid-js'
import { A } from '@solidjs/router'
import { Button, Card, CardBody, Badge, Modal, Skeleton, Empty, useToast } from '@/components'
import { api, getErrorMessage } from '@/api/client'
import type { DailyExpenseSummary } from '@/api/types'
import type { Resource } from 'solid-js'

interface Props {
  expenses: Resource<DailyExpenseSummary[]>
  refetchExpenses: () => void
  onOpenExpenseModal: () => void
}

export default function PatientExpenses(props: Props) {
  const { toast } = useToast()
  const [deleteExpenseId, setDeleteExpenseId] = createSignal<string | null>(null)
  const [deletingExpense, setDeletingExpense] = createSignal(false)

  const duplicateExpenseDates = createMemo(() => {
    const list = props.expenses() ?? []
    const counts: Record<string, number> = {}
    for (const e of list) {
      counts[e.expense_date] = (counts[e.expense_date] || 0) + 1
    }
    const dups = new Set<string>()
    for (const [date, count] of Object.entries(counts)) {
      if (count > 1) dups.add(date)
    }
    return dups
  })

  async function handleDeleteExpense() {
    const id = deleteExpenseId()
    if (!id) return
    setDeletingExpense(true)
    try {
      await api.expenses.delete(id)
      toast('success', '消费记录已删除')
      props.refetchExpenses()
    } catch (err: unknown) {
      toast('error', getErrorMessage(err) || '删除失败')
    } finally {
      setDeletingExpense(false)
      setDeleteExpenseId(null)
    }
  }

  return (
    <>
      <h2 class="section-title mb-3 mt-6">消费清单</h2>
      <Show when={props.expenses.loading} fallback={
        <Show when={props.expenses.error} fallback={
          <Show
            when={(props.expenses() ?? []).length > 0}
            fallback={
              <Empty
                title="暂无消费清单"
                description="还没有上传任何消费清单"
                action={
                  <Button
                    variant="primary"
                    size="sm"
                    onClick={props.onOpenExpenseModal}
                  >
                    上传消费清单
                  </Button>
                }
              />
            }
          >
            <div class="grid grid-cols-1 sm:grid-cols-2 xl:grid-cols-3 gap-2">
              <For each={[...(props.expenses() ?? [])].sort((a, b) => b.expense_date.localeCompare(a.expense_date) || b.created_at.localeCompare(a.created_at))}>
                {(expense) => (
                  <A
                    href={`/expenses/${expense.id}`}
                    class="block no-underline group"
                  >
                    <Card variant="outlined" class="h-full hover:border-accent hover:-translate-y-0.5 hover:shadow-md transition-all cursor-pointer relative">
                      <CardBody class="p-3 flex flex-col gap-1">
                        <div class="flex items-start justify-between gap-2">
                          <div class="flex items-center gap-1.5 min-w-0">
                            <h3 class="text-sm font-semibold text-content">{expense.expense_date}</h3>
                            <Show when={duplicateExpenseDates().has(expense.expense_date)}>
                              <span class="text-[10px] px-1.5 py-0.5 rounded bg-warning-light text-warning font-medium">重复</span>
                            </Show>
                          </div>
                          <div class="flex items-center gap-1 shrink-0">
                            <span class="meta-text">¥{expense.total_amount.toFixed(2)}</span>
                            <button
                              class="w-6 h-6 flex items-center justify-center rounded text-content-tertiary hover:text-error hover:bg-error-light opacity-0 group-hover:opacity-100 transition-all cursor-pointer"
                              title="删除"
                              onClick={(e) => { e.preventDefault(); e.stopPropagation(); setDeleteExpenseId(expense.id) }}
                            >
                              <svg class="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                                <path stroke-linecap="round" stroke-linejoin="round" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                              </svg>
                            </button>
                          </div>
                        </div>
                        <div class="flex items-center gap-2 text-xs text-content-tertiary flex-wrap">
                          <span>{expense.item_count} 项</span>
                          <Show when={expense.drug_count > 0}>
                            <Badge variant="accent">{expense.drug_count} 药品</Badge>
                          </Show>
                          <Show when={expense.test_count > 0}>
                            <Badge variant="info">{expense.test_count} 检查</Badge>
                          </Show>
                          <Show when={expense.treatment_count > 0}>
                            <Badge variant="success">{expense.treatment_count} 治疗</Badge>
                          </Show>
                        </div>
                      </CardBody>
                    </Card>
                  </A>
                )}
              </For>
            </div>
          </Show>
        }>
          <Empty
            title="加载消费清单失败"
            description={String(props.expenses.error?.message || props.expenses.error)}
          />
        </Show>
      }>
        <div class="grid grid-cols-1 sm:grid-cols-2 xl:grid-cols-3 gap-2">
          <Skeleton variant="rect" height={80} />
          <Skeleton variant="rect" height={80} />
          <Skeleton variant="rect" height={80} />
        </div>
      </Show>

      {/* Delete expense confirmation modal */}
      <Modal
        open={!!deleteExpenseId()}
        onClose={() => setDeleteExpenseId(null)}
        title="确认删除消费记录"
        size="sm"
        footer={
          <>
            <Button variant="outline" onClick={() => setDeleteExpenseId(null)}>
              取消
            </Button>
            <Button variant="danger" loading={deletingExpense()} onClick={handleDeleteExpense}>
              确认删除
            </Button>
          </>
        }
      >
        <p class="text-content">确定要删除该消费记录吗？</p>
        <p class="text-sm text-content-secondary mt-2">此操作不可撤销。</p>
      </Modal>
    </>
  )
}
