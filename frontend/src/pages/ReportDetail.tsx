import { createSignal, createResource, createMemo, Show, For } from 'solid-js'
import { useParams, useNavigate, A } from '@solidjs/router'
import {
  Button, Card, CardBody, CardHeader, Input, Badge, TestItemStatusBadge,
  Modal, Select, Spinner, useToast,
} from '@/components'
import type { TableColumn } from '@/components'
import { Table } from '@/components'
import { cn } from '@/lib/utils'
import { api } from '@/api/client'
import type { TestItem, CreateTestItemReq, UpdateTestItemReq, ItemStatus } from '@/api/types'
import { LlmInterpret } from '@/components/LlmInterpret'
import { exportReportCSV } from '@/lib/export'
import { exportReportPDF } from '@/lib/export-pdf'

export default function ReportDetail() {
  const params = useParams<{ id: string }>()
  const navigate = useNavigate()
  const { toast } = useToast()

  const [report, { refetch }] = createResource(() => params.id, (id) => api.reports.get(id))

  // Patient info for header
  const [patient] = createResource(
    () => report()?.patient_id,
    (patientId) => api.patients.get(patientId),
  )

  // Sibling reports for sidebar
  const [siblingReports] = createResource(
    () => report()?.patient_id,
    (patientId) => api.reports.listByPatient(patientId, { page_size: 100 }).then(r => r.items),
  )

  const sortedSiblings = createMemo(() => {
    const list = siblingReports() ?? []
    return [...list].sort((a, b) => b.report_date.localeCompare(a.report_date))
  })

  // Edit modal
  const [editOpen, setEditOpen] = createSignal(false)
  const [editForm, setEditForm] = createSignal({ report_type: '', hospital: '', report_date: '', sample_date: '' })
  const [editLoading, setEditLoading] = createSignal(false)

  // Delete modal
  const [deleteOpen, setDeleteOpen] = createSignal(false)
  const [deleteLoading, setDeleteLoading] = createSignal(false)

  // Add test item modal
  const [addItemOpen, setAddItemOpen] = createSignal(false)
  const [itemForm, setItemForm] = createSignal<Omit<CreateTestItemReq, 'report_id'>>({
    name: '', value: '', unit: '', reference_range: '', status: 'normal',
  })
  const [addItemLoading, setAddItemLoading] = createSignal(false)

  // Edit test item modal
  const [editItemOpen, setEditItemOpen] = createSignal(false)
  const [editItemId, setEditItemId] = createSignal('')
  const [editItemForm, setEditItemForm] = createSignal<UpdateTestItemReq>({
    name: '', value: '', unit: '', reference_range: '', status: 'normal',
  })
  const [editItemLoading, setEditItemLoading] = createSignal(false)

  // Delete test item modal
  const [deleteItemOpen, setDeleteItemOpen] = createSignal(false)
  const [deleteItemTarget, setDeleteItemTarget] = createSignal<TestItem | null>(null)
  const [deleteItemLoading, setDeleteItemLoading] = createSignal(false)

  function openEditModal() {
    const r = report()
    if (!r) return
    setEditForm({
      report_type: r.report_type,
      hospital: r.hospital,
      report_date: r.report_date,
      sample_date: r.sample_date,
    })
    setEditOpen(true)
  }

  async function handleEdit() {
    setEditLoading(true)
    try {
      await api.reports.update(params.id, editForm())
      toast('success', '报告已更新')
      setEditOpen(false)
      refetch()
    } catch (err: any) {
      toast('error', err.message)
    } finally {
      setEditLoading(false)
    }
  }

  async function handleDelete() {
    setDeleteLoading(true)
    try {
      const r = report()
      await api.reports.delete(params.id)
      toast('success', '报告已删除')
      setDeleteOpen(false)
      navigate(r ? `/patients/${r.patient_id}` : '/')
    } catch (err: any) {
      toast('error', err.message)
    } finally {
      setDeleteLoading(false)
    }
  }

  async function handleAddItem() {
    setAddItemLoading(true)
    try {
      await api.testItems.create({ ...itemForm(), report_id: params.id })
      toast('success', '检验项目已添加')
      setAddItemOpen(false)
      setItemForm({ name: '', value: '', unit: '', reference_range: '', status: 'normal' })
      refetch()
    } catch (err: any) {
      toast('error', err.message)
    } finally {
      setAddItemLoading(false)
    }
  }

  function openEditItemModal(item: TestItem) {
    setEditItemId(item.id)
    setEditItemForm({
      name: item.name,
      value: item.value,
      unit: item.unit,
      reference_range: item.reference_range,
      status: item.status,
    })
    setEditItemOpen(true)
  }

  async function handleEditItem() {
    setEditItemLoading(true)
    try {
      await api.testItems.update(editItemId(), editItemForm())
      toast('success', '检验项目已更新')
      setEditItemOpen(false)
      refetch()
    } catch (err: any) {
      toast('error', err.message)
    } finally {
      setEditItemLoading(false)
    }
  }

  async function handleDeleteItem() {
    const target = deleteItemTarget()
    if (!target) return
    setDeleteItemLoading(true)
    try {
      await api.testItems.delete(target.id)
      toast('success', '检验项目已删除')
      setDeleteItemOpen(false)
      setDeleteItemTarget(null)
      refetch()
    } catch (err: any) {
      toast('error', err.message)
    } finally {
      setDeleteItemLoading(false)
    }
  }

  const statusColorMap: Record<string, string> = {
    critical_high: 'text-error',
    high: 'text-error',
    low: 'text-info',
    critical_low: 'text-error',
  }

  const columns: TableColumn<TestItem>[] = [
    { key: 'name', title: '名称' },
    {
      key: 'value',
      title: '结果',
      render: (val: string, row: TestItem) => {
        const colorClass = statusColorMap[row.status]
        return <span class={colorClass ? `${colorClass} font-medium` : ''}>{val}</span>
      },
    },
    { key: 'unit', title: '单位' },
    { key: 'reference_range', title: '参考范围' },
    {
      key: 'status',
      title: '状态',
      render: (val: string, row: TestItem) => <TestItemStatusBadge status={val} value={row.value} referenceRange={row.reference_range} />,
    },
    {
      key: 'id',
      title: '',
      width: '72px',
      render: (_val: string, row: TestItem) => (
        <div class="flex items-center justify-end gap-0.5">
          <button
            class="inline-flex items-center justify-center w-7 h-7 rounded-lg text-content-tertiary hover:text-accent hover:bg-accent-light transition-colors"
            title="编辑"
            onClick={() => openEditItemModal(row)}
          >
            <svg class="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
              <path stroke-linecap="round" stroke-linejoin="round" d="M16.862 4.487l1.687-1.688a1.875 1.875 0 112.652 2.652L10.582 16.07a4.5 4.5 0 01-1.897 1.13L6 18l.8-2.685a4.5 4.5 0 011.13-1.897l8.932-8.931zm0 0L19.5 7.125M18 14v4.75A2.25 2.25 0 0115.75 21H5.25A2.25 2.25 0 013 18.75V8.25A2.25 2.25 0 015.25 6H10" />
            </svg>
          </button>
          <button
            class="inline-flex items-center justify-center w-7 h-7 rounded-lg text-content-tertiary hover:text-error hover:bg-error/10 transition-colors"
            title="删除"
            onClick={() => { setDeleteItemTarget(row); setDeleteItemOpen(true) }}
          >
            <svg class="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
              <path stroke-linecap="round" stroke-linejoin="round" d="M14.74 9l-.346 9m-4.788 0L9.26 9m9.968-3.21c.342.052.682.107 1.022.166m-1.022-.165L18.16 19.673a2.25 2.25 0 01-2.244 2.077H8.084a2.25 2.25 0 01-2.244-2.077L4.772 5.79m14.456 0a48.108 48.108 0 00-3.478-.397m-12 .562c.34-.059.68-.114 1.022-.165m0 0a48.11 48.11 0 013.478-.397m7.5 0v-.916c0-1.18-.91-2.164-2.09-2.201a51.964 51.964 0 00-3.32 0c-1.18.037-2.09 1.022-2.09 2.201v.916m7.5 0a48.667 48.667 0 00-7.5 0" />
            </svg>
          </button>
        </div>
      ),
    },
  ]

  return (
    <div class="page-shell flex flex-col lg:flex-row gap-6">
      {/* Left sidebar: report list */}
      <div class="lg:w-64 shrink-0">
        <div class="lg:sticky lg:top-20">
          <Card variant="outlined">
            <CardBody class="p-3">
              <Show when={report()}>
                {(r) => (
                  <A
                    href={`/patients/${r().patient_id}`}
                    class="flex items-center gap-1 text-sm text-accent hover:underline mb-3"
                  >
                    <svg class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                      <path stroke-linecap="round" stroke-linejoin="round" d="M15 19l-7-7 7-7" />
                    </svg>
                    返回患者详情
                  </A>
                )}
              </Show>

              <h3 class="text-sm font-semibold text-content-secondary px-2 mb-2">全部报告</h3>

              <Show when={siblingReports.loading} fallback={
                <Show when={siblingReports.error} fallback={
                  <div class="space-y-0.5 max-h-[70vh] overflow-y-auto">
                    <For each={sortedSiblings()}>
                      {(sib) => {
                        const isCurrent = () => sib.id === params.id
                        return (
                          <A
                            href={`/reports/${sib.id}`}
                            class={cn(
                              'block px-2 py-2 rounded-md text-sm transition-colors no-underline',
                              isCurrent()
                                ? 'bg-accent-light text-accent font-medium'
                                : 'text-content hover:bg-surface-secondary',
                            )}
                          >
                            <div class="flex items-center justify-between gap-2">
                              <span class="truncate">{sib.report_type}</span>
                              <Show when={sib.item_count > 0}>
                                <span class={cn(
                                  'text-xs shrink-0',
                                  sib.abnormal_count > 0 ? 'text-error' : 'text-content-tertiary',
                                )}>
                                  {sib.abnormal_count > 0 ? `${sib.abnormal_count}/` : ''}{sib.item_count}
                                </span>
                              </Show>
                            </div>
                            <div class="meta-text-tight">{sib.report_date}</div>
                          </A>
                        )
                      }}
                    </For>
                  </div>
                }>
                  <div class="px-2 py-3">
                    <p class="text-sm text-content-tertiary">
                      加载关联报告失败：{String(siblingReports.error?.message || siblingReports.error)}
                    </p>
                  </div>
                </Show>
              }>
                <div class="flex justify-center py-4">
                  <Spinner size="sm" />
                </div>
              </Show>
            </CardBody>
          </Card>
        </div>
      </div>

      {/* Right main area: report detail */}
      <div class="flex-1 min-w-0 space-y-6">
        <Show when={report.loading}>
          <div class="flex justify-center py-12">
            <Spinner size="lg" variant="orbital" />
          </div>
        </Show>

        <Show when={report.error}>
          <Card>
            <CardBody>
              <p class="text-error text-center">加载报告失败: {report.error?.message}</p>
            </CardBody>
          </Card>
        </Show>

        <Show when={report()}>
          {(r) => (
            <>
              {/* Header */}
              <div class="flex items-center justify-between">
                <div class="flex items-center gap-3">
                  <Show when={patient()}>
                    <span class="text-content-secondary text-sm">{patient()!.name}</span>
                    <span class="text-border">/</span>
                  </Show>
                  <h1 class="hero-title">{r().report_type}</h1>
                  <Badge variant="accent">{r().report_date}</Badge>
                </div>
                <div class="flex items-center gap-2">
                  <Button variant="outline" size="sm" onClick={() => { const rr = report(); if (rr) exportReportCSV(rr) }}>
                    导出CSV
                  </Button>
                  <Button variant="outline" size="sm" onClick={() => { const rr = report(); if (rr) exportReportPDF(rr, patient() ?? undefined) }}>
                    导出PDF
                  </Button>
                  <Button variant="outline" size="sm" onClick={openEditModal}>
                    编辑
                  </Button>
                  <Button variant="danger" size="sm" onClick={() => setDeleteOpen(true)}>
                    删除
                  </Button>
                </div>
              </div>

              {/* Report info */}
              <Card>
                <CardHeader>
                  <h2 class="section-title">报告信息</h2>
                </CardHeader>
                <CardBody>
                  <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
                    <div>
                      <p class="data-label">报告类型</p>
                      <p class="data-value">{r().report_type}</p>
                    </div>
                    <div>
                      <p class="data-label">医院</p>
                      <p class="data-value">{r().hospital || '-'}</p>
                    </div>
                    <div>
                      <p class="data-label">报告日期</p>
                      <p class="data-value">{r().report_date || '-'}</p>
                    </div>
                    <div>
                      <p class="data-label">采样日期</p>
                      <p class="data-value">{r().sample_date || '-'}</p>
                    </div>
                  </div>
                </CardBody>
              </Card>

              {/* Test items */}
              <Card>
                <CardHeader>
                  <div class="flex items-center justify-between w-full">
                    <h2 class="section-title">检验项目</h2>
                    <Button variant="primary" size="sm" onClick={() => setAddItemOpen(true)}>
                      添加项目
                    </Button>
                  </div>
                </CardHeader>
                <CardBody>
                  <Table<TestItem>
                    columns={columns}
                    data={r().test_items}
                    emptyTitle="暂无检验项目"
                    emptyDescription="点击上方按钮添加检验项目"
                  />
                </CardBody>
              </Card>

              {/* AI Interpretation */}
              <Card>
                <CardHeader>
                  <h2 class="section-title">AI 智能解读</h2>
                </CardHeader>
                <CardBody class="p-0">
                  <LlmInterpret
                    url={`/api/reports/${params.id}/interpret`}
                    buttonLabel="AI 解读此报告"
                  />
                </CardBody>
              </Card>

              {/* Edit Modal */}
              <Modal
                open={editOpen()}
                onClose={() => setEditOpen(false)}
                title="编辑报告"
                size="lg"
                footer={
                  <>
                    <Button variant="outline" onClick={() => setEditOpen(false)}>取消</Button>
                    <Button variant="primary" loading={editLoading()} onClick={handleEdit}>保存</Button>
                  </>
                }
              >
                <div class="space-y-4">
                  <Input
                    label="报告类型"
                    value={editForm().report_type}
                    onInput={(e) => setEditForm(f => ({ ...f, report_type: e.currentTarget.value }))}
                  />
                  <Input
                    label="医院"
                    value={editForm().hospital}
                    onInput={(e) => setEditForm(f => ({ ...f, hospital: e.currentTarget.value }))}
                  />
                  <Input
                    label="报告日期"
                    type="date"
                    value={editForm().report_date}
                    onInput={(e) => setEditForm(f => ({ ...f, report_date: e.currentTarget.value }))}
                  />
                  <Input
                    label="采样日期"
                    type="date"
                    value={editForm().sample_date}
                    onInput={(e) => setEditForm(f => ({ ...f, sample_date: e.currentTarget.value }))}
                  />
                </div>
              </Modal>

              {/* Delete Modal */}
              <Modal
                open={deleteOpen()}
                onClose={() => setDeleteOpen(false)}
                title="确认删除"
                footer={
                  <>
                    <Button variant="outline" onClick={() => setDeleteOpen(false)}>取消</Button>
                    <Button variant="danger" loading={deleteLoading()} onClick={handleDelete}>确认删除</Button>
                  </>
                }
              >
                <p class="text-content-secondary">
                  确定要删除这份报告吗？此操作无法撤销。
                </p>
              </Modal>

              {/* Edit Test Item Modal */}
              <Modal
                open={editItemOpen()}
                onClose={() => setEditItemOpen(false)}
                title="编辑检验项目"
                size="lg"
                footer={
                  <>
                    <Button variant="outline" onClick={() => setEditItemOpen(false)}>取消</Button>
                    <Button variant="primary" loading={editItemLoading()} onClick={handleEditItem}>保存</Button>
                  </>
                }
              >
                <div class="space-y-4">
                  <Input
                    label="名称"
                    value={editItemForm().name ?? ''}
                    onInput={(e) => setEditItemForm(f => ({ ...f, name: e.currentTarget.value }))}
                  />
                  <Input
                    label="结果值"
                    value={editItemForm().value ?? ''}
                    onInput={(e) => setEditItemForm(f => ({ ...f, value: e.currentTarget.value }))}
                  />
                  <Input
                    label="单位"
                    value={editItemForm().unit ?? ''}
                    onInput={(e) => setEditItemForm(f => ({ ...f, unit: e.currentTarget.value }))}
                  />
                  <Input
                    label="参考范围"
                    value={editItemForm().reference_range ?? ''}
                    onInput={(e) => setEditItemForm(f => ({ ...f, reference_range: e.currentTarget.value }))}
                  />
                  <div class="flex flex-col gap-1.5">
                    <label class="form-label">状态</label>
                    <select
                      class="form-control-base form-control-select"
                      value={editItemForm().status ?? 'normal'}
                      onChange={(e) => setEditItemForm(f => ({ ...f, status: e.currentTarget.value as ItemStatus }))}
                    >
                      <option value="critical_high">严重偏高</option>
                      <option value="high">偏高</option>
                      <option value="normal">正常</option>
                      <option value="low">偏低</option>
                      <option value="critical_low">严重偏低</option>
                    </select>
                  </div>
                </div>
              </Modal>

              {/* Delete Test Item Modal */}
              <Modal
                open={deleteItemOpen()}
                onClose={() => setDeleteItemOpen(false)}
                title="确认删除检验项目"
                footer={
                  <>
                    <Button variant="outline" onClick={() => setDeleteItemOpen(false)}>取消</Button>
                    <Button variant="danger" loading={deleteItemLoading()} onClick={handleDeleteItem}>确认删除</Button>
                  </>
                }
              >
                <p class="text-content-secondary">
                  确定要删除检验项目「{deleteItemTarget()?.name}」吗？此操作无法撤销。
                </p>
              </Modal>

              {/* Add Test Item Modal */}
              <Modal
                open={addItemOpen()}
                onClose={() => setAddItemOpen(false)}
                title="添加检验项目"
                size="lg"
                footer={
                  <>
                    <Button variant="outline" onClick={() => setAddItemOpen(false)}>取消</Button>
                    <Button variant="primary" loading={addItemLoading()} onClick={handleAddItem}>添加</Button>
                  </>
                }
              >
                <div class="space-y-4">
                  <Input
                    label="名称"
                    value={itemForm().name}
                    onInput={(e) => setItemForm(f => ({ ...f, name: e.currentTarget.value }))}
                  />
                  <Input
                    label="结果值"
                    value={itemForm().value}
                    onInput={(e) => setItemForm(f => ({ ...f, value: e.currentTarget.value }))}
                  />
                  <Input
                    label="单位"
                    value={itemForm().unit}
                    onInput={(e) => setItemForm(f => ({ ...f, unit: e.currentTarget.value }))}
                  />
                  <Input
                    label="参考范围"
                    value={itemForm().reference_range}
                    onInput={(e) => setItemForm(f => ({ ...f, reference_range: e.currentTarget.value }))}
                  />
                  <div class="flex flex-col gap-1.5">
                    <label class="form-label">状态</label>
                    <select
                      class="form-control-base form-control-select"
                      value={itemForm().status}
                      onChange={(e) => setItemForm(f => ({ ...f, status: e.currentTarget.value as ItemStatus }))}
                    >
                      <option value="critical_high">严重偏高</option>
                      <option value="high">偏高</option>
                      <option value="normal">正常</option>
                      <option value="low">偏低</option>
                      <option value="critical_low">严重偏低</option>
                    </select>
                  </div>
                </div>
              </Modal>
            </>
          )}
        </Show>
      </div>
    </div>
  )
}
