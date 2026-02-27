import { createSignal, createResource, createMemo, createEffect, Show, For } from 'solid-js'
import { useParams } from '@solidjs/router'
import { api } from '@/api/client'
import type { TestItem } from '@/api/types'
import { cn } from '@/lib/utils'
import { Card, CardBody, Select, Spinner, Empty, Badge } from '@/components'
import { LlmInterpret } from '@/components/LlmInterpret'

const STATUS_COLORS: Record<string, string> = {
  critical_high: 'text-error font-bold',
  high: 'text-warning font-medium',
  normal: 'text-success',
  low: 'text-info font-medium',
  critical_low: 'text-error font-bold',
}

export default function ReportCompare() {
  const params = useParams<{ id: string }>()
  const [reports] = createResource(
    () => params.id,
    (patientId) => api.reports.listByPatient(patientId, { page_size: 100 }).then(r => r.items),
  )

  const [leftId, setLeftId] = createSignal('')
  const [rightId, setRightId] = createSignal('')
  const [autoSelected, setAutoSelected] = createSignal(false)

  // Auto-select the latest two same-type reports
  createEffect(() => {
    if (autoSelected()) return
    const list = sortedReports()
    if (list.length < 2) return
    // Find first pair with same report_type
    for (let i = 0; i < list.length - 1; i++) {
      for (let j = i + 1; j < list.length; j++) {
        if (list[i].report_type === list[j].report_type) {
          setLeftId(list[j].id) // older one on left
          setRightId(list[i].id) // newer one on right
          setAutoSelected(true)
          return
        }
      }
    }
    // Fallback: just pick the latest two
    setLeftId(list[1].id)
    setRightId(list[0].id)
    setAutoSelected(true)
  })

  const [leftReport] = createResource(
    () => leftId() || null,
    (id) => id ? api.reports.get(id) : null,
  )

  const [rightReport] = createResource(
    () => rightId() || null,
    (id) => id ? api.reports.get(id) : null,
  )

  const sortedReports = createMemo(() => {
    const list = reports() ?? []
    return [...list].sort((a, b) => b.report_date.localeCompare(a.report_date))
  })

  const comparedItems = createMemo(() => {
    const left = leftReport()
    const right = rightReport()
    if (!left || !right) return []

    const allNames = new Set<string>()
    const leftMap = new Map<string, TestItem>()
    const rightMap = new Map<string, TestItem>()

    for (const item of left.test_items) {
      const key = item.canonical_name || item.name
      leftMap.set(key, item)
      allNames.add(key)
    }
    for (const item of right.test_items) {
      const key = item.canonical_name || item.name
      rightMap.set(key, item)
      allNames.add(key)
    }

    return Array.from(allNames).map(name => ({
      name,
      left: leftMap.get(name) || null,
      right: rightMap.get(name) || null,
    }))
  })

  const comparisonStats = createMemo(() => {
    const items = comparedItems()
    let increased = 0, decreased = 0, statusChanged = 0, total = items.length
    for (const row of items) {
      const lv = row.left ? parseFloat(row.left.value) : NaN
      const rv = row.right ? parseFloat(row.right.value) : NaN
      if (!isNaN(lv) && !isNaN(rv)) {
        if (rv > lv) increased++
        else if (rv < lv) decreased++
      }
      if (row.left && row.right && row.left.status !== row.right.status) statusChanged++
    }
    return { total, increased, decreased, statusChanged }
  })

  // AI interpret compare
  const [interpretUrl, setInterpretUrl] = createSignal('')
  const [showInterpret, setShowInterpret] = createSignal(false)

  return (
    <div class="page-shell">
      <h1 class="page-title mb-4">报告对比</h1>

      <Show when={reports.loading}>
        <div class="flex justify-center py-12"><Spinner size="lg" variant="orbital" /></div>
      </Show>

      <Show when={reports() && !reports.loading}>
        <div class="grid grid-cols-1 md:grid-cols-2 gap-4 mb-6">
          <div>
            <label class="form-label mb-1">左侧报告</label>
            <Select value={leftId()} onChange={(e) => setLeftId(e.currentTarget.value)}>
              <option value="">选择报告...</option>
              <For each={sortedReports()}>
                {(r) => <option value={r.id}>{r.report_date} - {r.report_type}</option>}
              </For>
            </Select>
          </div>
          <div>
            <label class="form-label mb-1">右侧报告</label>
            <Select value={rightId()} onChange={(e) => setRightId(e.currentTarget.value)}>
              <option value="">选择报告...</option>
              <For each={sortedReports()}>
                {(r) => <option value={r.id}>{r.report_date} - {r.report_type}</option>}
              </For>
            </Select>
          </div>
        </div>

        <Show when={leftReport.loading || rightReport.loading}>
          <div class="flex justify-center py-8"><Spinner size="lg" variant="orbital" /></div>
        </Show>

        <Show when={leftReport() && rightReport()}>
          {/* Report headers */}
          <div class="grid grid-cols-2 gap-4 mb-4">
            <Card variant="outlined">
              <CardBody class="p-3">
                <div class="text-sm font-semibold text-content">{leftReport()!.report_type}</div>
                <div class="text-xs text-content-secondary">{leftReport()!.report_date} · {leftReport()!.hospital}</div>
              </CardBody>
            </Card>
            <Card variant="outlined">
              <CardBody class="p-3">
                <div class="text-sm font-semibold text-content">{rightReport()!.report_type}</div>
                <div class="text-xs text-content-secondary">{rightReport()!.report_date} · {rightReport()!.hospital}</div>
              </CardBody>
            </Card>
          </div>

          {/* Comparison table */}
          <Card variant="elevated">
            <CardBody class="p-0">
              <div class="overflow-x-auto">
                <table class="w-full text-sm">
                  <thead>
                    <tr class="border-b border-border bg-surface-secondary/50">
                      <th class="px-4 py-2.5 text-left text-xs font-medium text-content-secondary">检验项目</th>
                      <th class="px-4 py-2.5 text-right text-xs font-medium text-content-secondary">左侧结果</th>
                      <th class="px-4 py-2.5 text-center text-xs font-medium text-content-secondary">变化</th>
                      <th class="px-4 py-2.5 text-left text-xs font-medium text-content-secondary">右侧结果</th>
                      <th class="px-4 py-2.5 text-left text-xs font-medium text-content-secondary">参考范围</th>
                    </tr>
                  </thead>
                  <tbody>
                    <For each={comparedItems()}>
                      {(row) => {
                        const leftVal = () => row.left ? parseFloat(row.left.value) : NaN
                        const rightVal = () => row.right ? parseFloat(row.right.value) : NaN
                        const diff = () => {
                          const l = leftVal(), r = rightVal()
                          if (isNaN(l) || isNaN(r)) return null
                          return r - l
                        }
                        const statusChanged = () => {
                          if (!row.left || !row.right) return true
                          return row.left.status !== row.right.status
                        }

                        return (
                          <tr class={cn('border-b border-border/50', statusChanged() && 'bg-warning-light/10')}>
                            <td class="px-4 py-2 text-content font-medium">{row.name}</td>
                            <td class={cn('px-4 py-2 text-right', row.left ? STATUS_COLORS[row.left.status] : 'text-content-tertiary')}>
                              {row.left ? `${row.left.value} ${row.left.unit}` : '—'}
                            </td>
                            <td class="px-4 py-2 text-center">
                              <Show when={diff() !== null} fallback={<span class="text-content-tertiary">—</span>}>
                                <span class={cn('text-xs font-medium', diff()! > 0 ? 'text-error' : diff()! < 0 ? 'text-info' : 'text-content-tertiary')}>
                                  {diff()! > 0 ? '↑' : diff()! < 0 ? '↓' : '='}{Math.abs(diff()!).toFixed(2)}
                                </span>
                              </Show>
                            </td>
                            <td class={cn('px-4 py-2', row.right ? STATUS_COLORS[row.right.status] : 'text-content-tertiary')}>
                              {row.right ? `${row.right.value} ${row.right.unit}` : '—'}
                            </td>
                            <td class="px-4 py-2 text-content-tertiary text-xs">
                              {row.left?.reference_range || row.right?.reference_range || ''}
                            </td>
                          </tr>
                        )
                      }}
                    </For>
                  </tbody>
                </table>
              </div>
            </CardBody>
          </Card>

          {/* Comparison summary */}
          <div class="mt-3 flex flex-wrap items-center gap-3 text-xs">
            <span class="text-content-tertiary">共 {comparisonStats().total} 项对比</span>
            <Show when={comparisonStats().increased > 0}>
              <Badge variant="error">↑ 升高 {comparisonStats().increased} 项</Badge>
            </Show>
            <Show when={comparisonStats().decreased > 0}>
              <Badge variant="info">↓ 降低 {comparisonStats().decreased} 项</Badge>
            </Show>
            <Show when={comparisonStats().statusChanged > 0}>
              <Badge variant="warning">状态变化 {comparisonStats().statusChanged} 项</Badge>
            </Show>
            <button
              type="button"
              class="ml-auto text-accent hover:underline cursor-pointer text-xs font-medium"
              onClick={() => {
                const l = leftReport(), r = rightReport()
                if (!l || !r) return
                setInterpretUrl(`/api/patients/${params.id}/interpret-multi?report_ids=${l.id},${r.id}`)
                setShowInterpret(true)
              }}
            >
              AI 解读对比
            </button>
          </div>

          {/* AI Interpret */}
          <Show when={showInterpret() && interpretUrl()}>
            <div class="mt-4">
              <Card variant="elevated">
                <CardBody class="p-4">
                  <h3 class="text-sm font-semibold text-content mb-3">AI 对比解读</h3>
                  <LlmInterpret url={interpretUrl()} />
                </CardBody>
              </Card>
            </div>
          </Show>
        </Show>

        <Show when={!leftId() || !rightId()}>
          <Empty title="选择两份报告进行对比" description="从上方选择左右两份报告，即可查看检验项目的变化" />
        </Show>
      </Show>
    </div>
  )
}
