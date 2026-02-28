import { createSignal, createResource, Show, For } from 'solid-js'
import {
  Button, Card, CardBody, Spinner, Empty,
} from '@/components'
import { api } from '@/api/client'

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
  create: { text: '新增', variant: 'bg-success-light text-success' },
  update: { text: '修改', variant: 'bg-info-light text-info' },
  delete: { text: '删除', variant: 'bg-error-light text-error' },
  login: { text: '登录', variant: 'bg-accent-light text-accent' },
  logout: { text: '登出', variant: 'bg-surface-secondary text-content-secondary' },
}

export default function AdminAuditLogs() {
  const [page, setPage] = createSignal(1)
  const pageSize = 20

  const [logs, { refetch }] = createResource(
    () => ({ page: page(), page_size: pageSize }),
    (params) => api.admin.auditLogs(params),
  )

  const totalPages = () => {
    const data = logs()
    if (!data) return 1
    return Math.max(1, Math.ceil(data.total / data.page_size))
  }

  return (
    <div class="page-shell max-w-4xl mx-auto space-y-6">
      <div class="flex items-center justify-between">
        <h1 class="page-title">审计日志</h1>
        <Show when={logs()}>
          <span class="text-sm text-content-tertiary">
            共 {logs()!.total} 条记录
          </span>
        </Show>
      </div>

      <Show when={logs.loading}>
        <div class="flex justify-center py-12">
          <Spinner size="lg" variant="orbital" />
        </div>
      </Show>

      <Show when={logs.error}>
        <Card>
          <CardBody>
            <p class="text-error text-center">加载日志失败: {logs.error?.message}</p>
          </CardBody>
        </Card>
      </Show>

      <Show when={logs()}>
        {(data) => (
          <>
            <Show when={data().items.length === 0}>
              <Empty
                title="暂无审计日志"
                description="系统操作记录将在此显示"
              />
            </Show>

            <Show when={data().items.length > 0}>
              <div class="space-y-3">
                <For each={data().items}>
                  {(log) => {
                    const actionInfo = actionLabels[log.action] ?? { text: log.action, variant: 'bg-surface-secondary text-content-secondary' }

                    return (
                      <Card variant="outlined">
                        <CardBody class="p-4">
                          <div class="flex items-start justify-between gap-3">
                            <div class="flex-1 min-w-0">
                              <div class="flex items-center gap-2 flex-wrap mb-1.5">
                                <span class={`inline-flex items-center px-2 py-0.5 rounded text-xs font-medium ${actionInfo.variant}`}>
                                  {actionInfo.text}
                                </span>
                                <Show when={log.target_type}>
                                  <span class="text-xs text-content-tertiary">
                                    {log.target_type}
                                  </span>
                                </Show>
                                <span class="text-border">·</span>
                                <span class="text-xs text-content-secondary">
                                  {log.username}
                                </span>
                                <Show when={log.ip_address}>
                                  <span class="text-border">·</span>
                                  <span class="text-xs text-content-tertiary">
                                    {log.ip_address}
                                  </span>
                                </Show>
                              </div>
                              <Show when={log.detail}>
                                <p class="text-sm text-content">{log.detail}</p>
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
