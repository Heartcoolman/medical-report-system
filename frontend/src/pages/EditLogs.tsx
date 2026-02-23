import { createSignal, createResource, Show, For } from 'solid-js'
import { A } from '@solidjs/router'
import {
  Button, Card, CardBody, Spinner,
} from '@/components'
import { api } from '@/api/client'
import type { EditLog } from '@/api/types'

function formatTime(iso: string) {
  try {
    const d = new Date(iso)
    const pad = (n: number) => String(n).padStart(2, '0')
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`
  } catch {
    return iso
  }
}

const actionLabels: Record<string, { text: string; variant: string }> = {
  create: { text: '新增', variant: 'bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-400' },
  update: { text: '修改', variant: 'bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400' },
  delete: { text: '删除', variant: 'bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400' },
}

const targetLabels: Record<string, string> = {
  report: '报告',
  test_item: '检验项目',
}

export default function EditLogs() {
  const [page, setPage] = createSignal(1)
  const pageSize = 20

  const [logs, { refetch }] = createResource(
    () => ({ page: page(), page_size: pageSize }),
    (params) => api.editLogs.list(params),
  )

  // Fetch patient names for display
  const [patientNames, setPatientNames] = createSignal<Record<string, string>>({})

  // Lazy-load patient names when logs change
  const loadPatientNames = async (logItems: EditLog[]) => {
    const ids = [...new Set(logItems.map(l => l.patient_id).filter(Boolean))]
    const existing = patientNames()
    const missing = ids.filter(id => !(id in existing))
    if (missing.length === 0) return

    const names: Record<string, string> = { ...existing }
    await Promise.all(
      missing.map(async (id) => {
        try {
          const p = await api.patients.get(id)
          names[id] = p.name
        } catch {
          names[id] = id.slice(0, 8)
        }
      }),
    )
    setPatientNames(names)
  }

  // Trigger patient name loading when logs load
  const logItems = () => {
    const data = logs()
    if (data?.items) {
      loadPatientNames(data.items)
    }
    return data
  }

  const totalPages = () => {
    const data = logItems()
    if (!data) return 1
    return Math.max(1, Math.ceil(data.total / data.page_size))
  }

  return (
    <div class="max-w-4xl mx-auto space-y-6">
      <div class="flex items-center justify-between">
        <h1 class="hero-title">修改日志</h1>
        <Show when={logItems()}>
          <span class="text-sm text-content-tertiary">
            共 {logItems()!.total} 条记录
          </span>
        </Show>
      </div>

      <Show when={logs.loading}>
        <div class="flex justify-center py-12">
          <Spinner size="lg" />
        </div>
      </Show>

      <Show when={logs.error}>
        <Card>
          <CardBody>
            <p class="text-error text-center">加载日志失败: {logs.error?.message}</p>
          </CardBody>
        </Card>
      </Show>

      <Show when={logItems()}>
        {(data) => (
          <>
            <Show when={data().items.length === 0}>
              <Card>
                <CardBody>
                  <div class="text-center py-12">
                    <svg class="mx-auto h-12 w-12 text-content-tertiary/40" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
                      <path stroke-linecap="round" stroke-linejoin="round" d="M19.5 14.25v-2.625a3.375 3.375 0 00-3.375-3.375h-1.5A1.125 1.125 0 0113.5 7.125v-1.5a3.375 3.375 0 00-3.375-3.375H8.25m0 12.75h7.5m-7.5 3H12M10.5 2.25H5.625c-.621 0-1.125.504-1.125 1.125v17.25c0 .621.504 1.125 1.125 1.125h12.75c.621 0 1.125-.504 1.125-1.125V11.25a9 9 0 00-9-9z" />
                    </svg>
                    <p class="mt-3 text-content-secondary">暂无修改记录</p>
                    <p class="mt-1 text-sm text-content-tertiary">对报告或检验项目进行编辑后，修改记录将在此显示</p>
                  </div>
                </CardBody>
              </Card>
            </Show>

            <Show when={data().items.length > 0}>
              <div class="space-y-3">
                <For each={data().items}>
                  {(log) => {
                    const actionInfo = actionLabels[log.action] ?? { text: log.action, variant: 'bg-gray-100 text-gray-700' }
                    const patientName = () => patientNames()[log.patient_id] || log.patient_id.slice(0, 8)

                    return (
                      <Card variant="outlined">
                        <CardBody class="p-4">
                          <div class="flex items-start justify-between gap-3">
                            <div class="flex-1 min-w-0">
                              <div class="flex items-center gap-2 flex-wrap mb-1.5">
                                <span class={`inline-flex items-center px-2 py-0.5 rounded text-xs font-medium ${actionInfo.variant}`}>
                                  {actionInfo.text}
                                </span>
                                <span class="text-xs text-content-tertiary">
                                  {targetLabels[log.target_type] ?? log.target_type}
                                </span>
                                <span class="text-border">·</span>
                                <A
                                  href={`/patients/${log.patient_id}`}
                                  class="text-xs text-accent hover:underline"
                                >
                                  {patientName()}
                                </A>
                                <span class="text-border">·</span>
                                <A
                                  href={`/reports/${log.report_id}`}
                                  class="text-xs text-accent hover:underline"
                                >
                                  查看报告
                                </A>
                              </div>
                              <p class="text-sm text-content">{log.summary}</p>

                              <Show when={log.changes.length > 0}>
                                <div class="mt-2 space-y-1">
                                  <For each={log.changes}>
                                    {(change) => (
                                      <div class="flex items-center gap-2 text-xs">
                                        <span class="text-content-secondary font-medium shrink-0">{change.field}:</span>
                                        <span class="text-error line-through truncate max-w-[200px]" title={change.old_value}>
                                          {change.old_value || '(空)'}
                                        </span>
                                        <svg class="w-3 h-3 text-content-tertiary shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                                          <path stroke-linecap="round" stroke-linejoin="round" d="M13 7l5 5m0 0l-5 5m5-5H6" />
                                        </svg>
                                        <span class="text-green-600 dark:text-green-400 truncate max-w-[200px]" title={change.new_value}>
                                          {change.new_value || '(空)'}
                                        </span>
                                      </div>
                                    )}
                                  </For>
                                </div>
                              </Show>
                            </div>

                            <span class="text-xs text-content-tertiary whitespace-nowrap shrink-0">
                              {formatTime(log.created_at)}
                            </span>
                          </div>
                        </CardBody>
                      </Card>
                    )
                  }}
                </For>
              </div>

              {/* Pagination */}
              <Show when={totalPages() > 1}>
                <div class="flex items-center justify-center gap-2 pt-4">
                  <Button
                    variant="outline"
                    size="sm"
                    disabled={page() <= 1}
                    onClick={() => { setPage(p => Math.max(1, p - 1)); refetch() }}
                  >
                    上一页
                  </Button>
                  <span class="text-sm text-content-secondary px-3">
                    {page()} / {totalPages()}
                  </span>
                  <Button
                    variant="outline"
                    size="sm"
                    disabled={page() >= totalPages()}
                    onClick={() => { setPage(p => p + 1); refetch() }}
                  >
                    下一页
                  </Button>
                </div>
              </Show>
            </Show>
          </>
        )}
      </Show>
    </div>
  )
}
