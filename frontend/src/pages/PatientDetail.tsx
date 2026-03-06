import { createSignal, createResource, createMemo, Show, For } from 'solid-js'
import { A, useNavigate, useParams } from '@solidjs/router'
import { Button, Card, CardBody, Badge, Modal, Skeleton, Empty, useToast, BottomSheet, FloatingActionButton } from '@/components'
import { cn } from '@/lib/utils'
import { api, getErrorMessage } from '@/api/client'
import ReportUpload from './ReportUpload'
import ExpenseUpload from './ExpenseUpload'
import { LlmInterpret } from '@/components/LlmInterpret'
import { exportAllReportsCSV } from '@/lib/export'
import { exportAllReportsPDF } from '@/lib/export-pdf'
import PatientTemperatures from './patient-detail/PatientTemperatures'
import PatientExpenses from './patient-detail/PatientExpenses'

export default function PatientDetail() {
  const params = useParams<{ id: string }>()
  const navigate = useNavigate()
  const { toast } = useToast()

  const [patient] = createResource(() => params.id, (id) => api.patients.get(id))
  const [reports, { refetch }] = createResource(() => params.id, (id) => api.reports.listAllByPatient(id))
  const [temperatures, { refetch: refetchTemps }] = createResource(() => params.id, (id) => api.temperatures.listAll(id))
  const [expenses, { refetch: refetchExpenses }] = createResource(() => params.id, (id) => api.expenses.listAll(id))

  const [showDeleteModal, setShowDeleteModal] = createSignal(false)
  const [deleting, setDeleting] = createSignal(false)
  const [showUploadModal, setShowUploadModal] = createSignal(false)
  const [showExpenseModal, setShowExpenseModal] = createSignal(false)
  const [showActionSheet, setShowActionSheet] = createSignal(false)

  const [showInterpretModal, setShowInterpretModal] = createSignal(false)
  const [selectedReportIds, setSelectedReportIds] = createSignal<Set<string>>(new Set())
  const [interpretUrl, setInterpretUrl] = createSignal('')
  const [interpretStarted, setInterpretStarted] = createSignal(false)

  const computeAge = (dob: string) => {
    if (!dob) return null
    const [y, m, d] = dob.split('-').map(Number)
    if (!y || !m || !d) return null
    const now = new Date()
    let age = now.getFullYear() - y
    if (now.getMonth() + 1 < m || (now.getMonth() + 1 === m && now.getDate() < d)) age--
    return age
  }

  const latestTemp = createMemo(() => {
    const all = temperatures() ?? []
    if (all.length === 0) return null
    return [...all].sort((a, b) => b.recorded_at.localeCompare(a.recorded_at))[0]
  })

  const formatDateTime = (iso: string) => {
    if (!iso) return ''
    const d = new Date(iso)
    if (isNaN(d.getTime())) return iso.slice(0, 10)
    return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`
  }

  const PAGE_SIZE = 9
  const [reportPage, setReportPage] = createSignal(1)

  const [batchMode, setBatchMode] = createSignal(false)
  const [batchSelected, setBatchSelected] = createSignal<Set<string>>(new Set())
  const [batchDeleting, setBatchDeleting] = createSignal(false)

  function toggleBatchSelect(id: string) {
    const s = new Set(batchSelected())
    if (s.has(id)) s.delete(id); else s.add(id)
    setBatchSelected(s)
  }

  function toggleSelectAll() {
    const all = sortedReports()
    if (batchSelected().size === all.length) {
      setBatchSelected(new Set<string>())
    } else {
      setBatchSelected(new Set<string>(all.map(r => r.id)))
    }
  }

  async function handleBatchDelete() {
    const ids = Array.from(batchSelected())
    if (ids.length === 0) return
    setBatchDeleting(true)
    try {
      await api.reports.deleteBatch(ids)
      toast('success', `已删除 ${ids.length} 份报告`)
      setBatchSelected(new Set<string>())
      setBatchMode(false)
      refetch()
    } catch (err: unknown) {
      toast('error', getErrorMessage(err) || '批量删除失败')
    } finally {
      setBatchDeleting(false)
    }
  }

  function handleBatchExportCSV() {
    const ids = batchSelected()
    const selected = sortedReports().filter(r => ids.has(r.id))
    const p2 = patient()
    if (p2 && selected.length > 0) exportAllReportsCSV(selected, p2.name)
  }

  const sortedReports = createMemo(() => {
    const list = reports() ?? []
    return [...list].sort((a, b) => b.report_date.localeCompare(a.report_date))
  })

  const totalReportPages = createMemo(() => Math.max(1, Math.ceil(sortedReports().length / PAGE_SIZE)))

  const pagedReports = createMemo(() => {
    const start = (reportPage() - 1) * PAGE_SIZE
    return sortedReports().slice(start, start + PAGE_SIZE)
  })

  const stats = createMemo(() => {
    const list = reports() ?? []
    const totalReports = list.length
    const totalItems = list.reduce((sum, r) => sum + (r.item_count ?? 0), 0)
    const totalAbnormal = list.reduce((sum, r) => sum + (r.abnormal_count ?? 0), 0)
    return { totalReports, totalItems, totalAbnormal }
  })

  async function handleDelete() {
    setDeleting(true)
    try {
      await api.patients.delete(params.id)
      toast('success', '患者已删除')
      navigate('/')
    } catch (err: unknown) {
      toast('error', getErrorMessage(err) || '删除失败')
    } finally {
      setDeleting(false)
      setShowDeleteModal(false)
    }
  }

  function openInterpretModal() {
    const list = reports() ?? []
    setSelectedReportIds(new Set(list.map(r => r.id)))
    setInterpretUrl('')
    setInterpretStarted(false)
    setShowInterpretModal(true)
  }

  return (
    <div class="page-shell">
      {/* Loading skeleton */}
      <Show when={patient.loading}>
        <div class="flex flex-col lg:flex-row gap-6">
          <div class="lg:w-80 shrink-0">
            <Card variant="elevated">
              <CardBody>
                <div class="flex flex-col gap-4">
                  <Skeleton variant="text" width="60%" height={32} />
                  <div class="flex gap-2">
                    <Skeleton variant="rect" width={48} height={24} />
                    <Skeleton variant="text" width="40%" />
                  </div>
                  <Skeleton variant="text" width="80%" />
                  <Skeleton variant="text" width="70%" />
                  <Skeleton variant="text" width="90%" />
                </div>
              </CardBody>
            </Card>
          </div>
          <div class="flex-1 min-w-0">
            <Skeleton variant="text" width="20%" height={24} />
            <div class="mt-4 flex flex-col gap-3">
              <Skeleton variant="rect" height={80} />
              <Skeleton variant="rect" height={80} />
              <Skeleton variant="rect" height={80} />
            </div>
          </div>
        </div>
      </Show>

      {/* Error state */}
      <Show when={patient.error}>
        <Card variant="elevated">
          <CardBody>
            <div class="text-center py-8">
              <p class="text-error text-lg">加载患者信息失败</p>
              <Button variant="outline" class="mt-4" onClick={() => navigate('/')}>
                返回首页
              </Button>
            </div>
          </CardBody>
        </Card>
      </Show>

      {/* Patient loaded */}
      <Show when={patient()}>
        {(p) => (
          <div class="flex flex-col lg:flex-row gap-6">
            {/* Left sidebar: Patient info */}
            <div class="lg:w-80 shrink-0">
              <div class="lg:sticky lg:top-20">
                <Card variant="elevated">
                  <CardBody>
                    <div class="flex flex-col gap-3">
                      <div class="flex items-center gap-3">
                        <div class={cn(
                          'w-12 h-12 rounded-full flex items-center justify-center text-lg font-semibold',
                          p().gender === '男'
                            ? 'bg-info-light text-info'
                            : 'bg-error-light text-error',
                        )}>
                          {p().name.slice(0, 1)}
                        </div>
                        <div>
                          <h1 class="hero-title">{p().name}</h1>
                          <Badge variant={p().gender === '男' ? 'info' : 'accent'}>
                            {p().gender}
                          </Badge>
                        </div>
                      </div>

                      <div class="border-t border-border pt-3 flex flex-col gap-1.5 text-sm text-content-secondary">
                        <Show when={p().dob}>
                          <div class="flex items-center gap-2">
                            <svg class="w-4 h-4 shrink-0 text-content-tertiary" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                              <path stroke-linecap="round" stroke-linejoin="round" d="M8 7V3m8 4V3m-9 8h10M5 21h14a2 2 0 002-2V7a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" />
                            </svg>
                            <span>{p().dob}</span>
                            <Show when={computeAge(p().dob) !== null}>
                              <span class="text-content-tertiary">({computeAge(p().dob)}岁)</span>
                            </Show>
                          </div>
                        </Show>
                        <div class="flex items-center gap-2">
                          <svg class="w-4 h-4 shrink-0 text-content-tertiary" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                            <path stroke-linecap="round" stroke-linejoin="round" d="M3 5a2 2 0 012-2h3.28a1 1 0 01.948.684l1.498 4.493a1 1 0 01-.502 1.21l-2.257 1.13a11.042 11.042 0 005.516 5.516l1.13-2.257a1 1 0 011.21-.502l4.493 1.498a1 1 0 01.684.949V19a2 2 0 01-2 2h-1C9.716 21 3 14.284 3 6V5z" />
                          </svg>
                          <span>{p().phone}</span>
                        </div>
                        <div class="flex items-center gap-2">
                          <svg class="w-4 h-4 shrink-0 text-content-tertiary" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                            <path stroke-linecap="round" stroke-linejoin="round" d="M10 6H5a2 2 0 00-2 2v9a2 2 0 002 2h14a2 2 0 002-2V8a2 2 0 00-2-2h-5m-4 0V5a2 2 0 114 0v1m-4 0a2 2 0 104 0" />
                          </svg>
                          <span class="break-all">{p().id_number}</span>
                        </div>
                        <Show when={p().notes}>
                          <div class="flex items-start gap-2">
                            <svg class="w-4 h-4 shrink-0 mt-0.5 text-content-tertiary" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                              <path stroke-linecap="round" stroke-linejoin="round" d="M7 8h10M7 12h4m1 8l-4-4H5a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v8a2 2 0 01-2 2h-3l-4 4z" />
                            </svg>
                            <span>{p().notes}</span>
                          </div>
                        </Show>
                        <Show when={p().created_at}>
                          <div class="flex items-center gap-2">
                            <svg class="w-4 h-4 shrink-0 text-content-tertiary" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                              <path stroke-linecap="round" stroke-linejoin="round" d="M12 6v6h4.5m4.5 0a9 9 0 11-18 0 9 9 0 0118 0z" />
                            </svg>
                            <span>建档 {formatDateTime(p().created_at)}</span>
                          </div>
                        </Show>
                        <Show when={latestTemp()}>
                          <div class="flex items-center gap-2">
                            <svg class="w-4 h-4 shrink-0 text-content-tertiary" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                              <path stroke-linecap="round" stroke-linejoin="round" d="M12 9v3m0 0v3m0-3h3m-3 0H9m12 0a9 9 0 11-18 0 9 9 0 0118 0z" />
                            </svg>
                            <span class={latestTemp()!.value >= 37.3 ? 'text-error font-medium' : ''}>
                              {latestTemp()!.value}℃
                            </span>
                            <span class="text-content-tertiary text-xs">{latestTemp()!.recorded_at}</span>
                          </div>
                        </Show>
                      </div>

                      <div class="border-t border-border pt-3 flex flex-col gap-3">
                        <div>
                          <p class="micro-title mb-1.5">数据录入</p>
                          <div class="grid grid-cols-2 gap-1.5">
                            <Button variant="primary" size="sm" class="w-full" onClick={() => setShowUploadModal(true)}>上传报告</Button>
                            <Button variant="secondary" size="sm" class="w-full" onClick={() => setShowExpenseModal(true)}>消费清单</Button>
                            <Button variant="secondary" size="sm" class="w-full col-span-2" onClick={() => navigate(`/patients/${params.id}/templates`)}>快捷录入</Button>
                          </div>
                        </div>

                        <div>
                          <p class="micro-title mb-1.5">分析工具</p>
                          <div class="grid grid-cols-2 gap-1.5">
                            <Button variant="secondary" size="sm" class="w-full" onClick={() => navigate(`/patients/${params.id}/trends`)}>趋势分析</Button>
                            <Button variant="secondary" size="sm" class="w-full" onClick={() => navigate(`/patients/${params.id}/compare`)}>报告对比</Button>
                            <Button variant="secondary" size="sm" class="w-full" onClick={openInterpretModal}>AI 综合解读</Button>
                            <Button variant="secondary" size="sm" class="w-full" onClick={() => navigate(`/patients/${params.id}/health-assessment`)}>AI 健康评估</Button>
                            <Button variant="secondary" size="sm" class="w-full" onClick={() => navigate(`/patients/${params.id}/risk-prediction`)}>风险预测</Button>
                            <Button variant="secondary" size="sm" class="w-full" onClick={() => navigate(`/patients/${params.id}/rag-assistant`)}>AI 问答助手</Button>
                            <Button variant="secondary" size="sm" class="w-full" onClick={() => navigate(`/patients/${params.id}/timeline`)}>健康时间线</Button>
                            <Button variant="secondary" size="sm" class="w-full" onClick={() => navigate(`/patients/${params.id}/medications`)}>用药管理</Button>
                            <Button variant="secondary" size="sm" class="w-full" onClick={() => navigate(`/patients/${params.id}/drug-interaction`)}>药物相互作用检查</Button>
                            <Button variant="secondary" size="sm" class="w-full" onClick={() => navigate(`/patients/${params.id}/med-lab-correlation`)}>用药-检验关联</Button>
                          </div>
                        </div>

                        <div>
                          <p class="micro-title mb-1.5">管理</p>
                          <div class="flex gap-2">
                            <Button variant="outline" size="sm" class="flex-1" onClick={() => navigate(`/patients/${params.id}/edit`)}>编辑</Button>
                            <Button variant="danger" size="sm" class="flex-1" onClick={() => setShowDeleteModal(true)}>删除</Button>
                          </div>
                        </div>
                      </div>
                    </div>
                  </CardBody>
                </Card>
              </div>
            </div>

            {/* Right main area */}
            <div class="flex-1 min-w-0">
              {/* Temperature section (extracted component) */}
              <PatientTemperatures
                patientId={params.id}
                temperatures={temperatures}
                refetchTemps={refetchTemps}
              />

              {/* Reports section */}
              <div class="flex items-center justify-between mb-3">
                <h2 class="section-title">检查报告</h2>
                <Show when={sortedReports().length > 0}>
                  <div class="flex gap-1">
                    <Button variant="ghost" size="sm" onClick={() => { const p2 = patient(); if (p2) exportAllReportsCSV(sortedReports(), p2.name) }}>导出CSV</Button>
                    <Button variant="ghost" size="sm" onClick={() => { const p2 = patient(); if (p2) exportAllReportsPDF(sortedReports(), p2) }}>导出PDF</Button>
                    <Button variant="ghost" size="sm" onClick={() => { setBatchMode(!batchMode()); setBatchSelected(new Set<string>()) }}>{batchMode() ? '取消多选' : '多选'}</Button>
                  </div>
                </Show>
              </div>

              <Show when={batchMode() && sortedReports().length > 0}>
                <div class="flex items-center gap-3 mb-3 px-3 py-2 rounded-xl bg-surface-secondary">
                  <button type="button" class="text-xs text-accent hover:underline cursor-pointer" onClick={toggleSelectAll}>
                    {batchSelected().size === sortedReports().length ? '取消全选' : '全选'}
                  </button>
                  <span class="text-xs text-content-secondary">已选 {batchSelected().size} 项</span>
                  <div class="ml-auto flex gap-2">
                    <Button variant="outline" size="sm" disabled={batchSelected().size === 0} onClick={handleBatchExportCSV}>导出选中</Button>
                    <Button variant="danger" size="sm" disabled={batchSelected().size === 0} loading={batchDeleting()} onClick={() => { if (confirm(`确认删除选中的 ${batchSelected().size} 份报告？此操作不可恢复。`)) handleBatchDelete() }}>删除选中</Button>
                  </div>
                </div>
              </Show>

              <Show when={!reports.loading && sortedReports().length > 0}>
                <div class="flex items-center gap-4 mb-3 px-1">
                  <div><span class="data-label">报告</span><span class="data-value ml-1">{stats().totalReports}</span></div>
                  <div class="w-px h-4 bg-border" />
                  <div><span class="data-label">检验项</span><span class="data-value ml-1">{stats().totalItems}</span></div>
                  <div class="w-px h-4 bg-border" />
                  <div><span class="data-label">异常</span><span class={cn('data-value ml-1', stats().totalAbnormal > 0 && 'text-error')}>{stats().totalAbnormal}</span></div>
                </div>
              </Show>

              <Show when={reports.loading} fallback={
                <Show when={reports.error} fallback={
                  <Show when={sortedReports().length > 0} fallback={
                    <Empty title="暂无报告" description="还没有上传任何检查报告" action={<Button variant="primary" size="sm" onClick={() => setShowUploadModal(true)}>上传报告</Button>} />
                  }>
                    <div class="grid grid-cols-1 sm:grid-cols-2 xl:grid-cols-3 gap-2">
                      <For each={pagedReports()}>
                        {(report) => (
                          <Show when={batchMode()} fallback={
                            <A href={`/reports/${report.id}`} class="block no-underline group">
                              <Card variant="outlined" class="h-full hover:border-accent hover:-translate-y-0.5 hover:shadow-md transition-all cursor-pointer">
                                <CardBody class="p-3 flex flex-col gap-1">
                                  <div class="flex items-start justify-between gap-2">
                                    <h3 class="text-sm font-semibold text-content truncate min-w-0">{report.report_type}</h3>
                                    <span class="meta-text shrink-0">{report.report_date}</span>
                                  </div>
                                  <Show when={report.hospital}>
                                    <div class="flex items-center gap-1 text-xs text-content-tertiary">
                                      <svg class="w-3 h-3 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M19 21V5a2 2 0 00-2-2H7a2 2 0 00-2 2v16m14 0h2m-2 0h-5m-9 0H3m2 0h5M9 7h1m-1 4h1m4-4h1m-1 4h1m-5 10v-5a1 1 0 011-1h2a1 1 0 011 1v-5m-4 0h4" /></svg>
                                      <span class="truncate">{report.hospital}</span>
                                    </div>
                                  </Show>
                                  <Show when={report.sample_date && report.sample_date !== report.report_date}>
                                    <div class="flex items-center gap-1.5 text-xs text-content-tertiary"><span>采样 {report.sample_date}</span></div>
                                  </Show>
                                  <Show when={report.item_count > 0}>
                                    <div class="flex items-center gap-2 text-xs text-content-tertiary">
                                      <span>{report.item_count} 项检验</span>
                                      <Show when={report.abnormal_count > 0}><Badge variant="error">{report.abnormal_count} 项异常</Badge></Show>
                                    </div>
                                  </Show>
                                  <Show when={report.abnormal_names && report.abnormal_names.length > 0}>
                                    <div class="text-xs text-error truncate">
                                      {report.abnormal_names.slice(0, 3).join('、')}
                                      <Show when={report.abnormal_names.length > 3}><span class="text-content-tertiary"> 等{report.abnormal_names.length}项</span></Show>
                                    </div>
                                  </Show>
                                </CardBody>
                              </Card>
                            </A>
                          }>
                            <div class="block cursor-pointer" onClick={() => toggleBatchSelect(report.id)}>
                              <Card variant="outlined" class={cn('h-full transition-all', batchSelected().has(report.id) && 'border-accent bg-accent/5')}>
                                <CardBody class="p-3 flex flex-col gap-1">
                                  <div class="flex items-start justify-between gap-2">
                                    <div class="flex items-center gap-2 min-w-0">
                                      <input type="checkbox" checked={batchSelected().has(report.id)} class="accent-[var(--color-accent)] w-4 h-4 shrink-0 cursor-pointer" onClick={(e) => e.stopPropagation()} onChange={() => toggleBatchSelect(report.id)} />
                                      <h3 class="text-sm font-semibold text-content truncate min-w-0">{report.report_type}</h3>
                                    </div>
                                    <span class="meta-text shrink-0">{report.report_date}</span>
                                  </div>
                                  <Show when={report.item_count > 0}>
                                    <div class="flex items-center gap-2 text-xs text-content-tertiary">
                                      <span>{report.item_count} 项检验</span>
                                      <Show when={report.abnormal_count > 0}><Badge variant="error">{report.abnormal_count} 项异常</Badge></Show>
                                    </div>
                                  </Show>
                                </CardBody>
                              </Card>
                            </div>
                          </Show>
                        )}
                      </For>
                    </div>

                    <Show when={totalReportPages() > 1}>
                      <div class="flex items-center justify-center gap-1 mt-4">
                        <button class="h-8 w-8 flex items-center justify-center rounded-lg text-content-secondary hover:bg-surface-secondary cursor-pointer transition-all duration-200 disabled:opacity-30 disabled:cursor-not-allowed" disabled={reportPage() <= 1} onClick={() => setReportPage(p => p - 1)}>
                          <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M15 19l-7-7 7-7" /></svg>
                        </button>
                        <For each={Array.from({ length: totalReportPages() }, (_, i) => i + 1)}>
                          {(page) => (
                            <button class={cn('h-8 min-w-[32px] px-2 rounded-lg text-sm font-medium cursor-pointer transition-all duration-200', page === reportPage() ? 'bg-accent text-accent-content shadow-sm' : 'text-content-secondary hover:bg-surface-secondary')} onClick={() => setReportPage(page)}>{page}</button>
                          )}
                        </For>
                        <button class="h-8 w-8 flex items-center justify-center rounded-lg text-content-secondary hover:bg-surface-secondary cursor-pointer transition-all duration-200 disabled:opacity-30 disabled:cursor-not-allowed" disabled={reportPage() >= totalReportPages()} onClick={() => setReportPage(p => p + 1)}>
                          <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M9 5l7 7-7 7" /></svg>
                        </button>
                      </div>
                    </Show>
                  </Show>
                }>
                  <Empty title="加载报告列表失败" description={String(reports.error?.message || reports.error)} />
                </Show>
              }>
                <div class="grid grid-cols-1 sm:grid-cols-2 xl:grid-cols-3 gap-2">
                  <Skeleton variant="rect" height={100} />
                  <Skeleton variant="rect" height={100} />
                  <Skeleton variant="rect" height={100} />
                </div>
              </Show>

              {/* Expenses section (extracted component) */}
              <PatientExpenses
                expenses={expenses}
                refetchExpenses={refetchExpenses}
                onOpenExpenseModal={() => setShowExpenseModal(true)}
              />
            </div>

            {/* Delete patient confirmation modal */}
            <Modal open={showDeleteModal()} onClose={() => setShowDeleteModal(false)} title="确认删除" size="sm" footer={<><Button variant="outline" onClick={() => setShowDeleteModal(false)}>取消</Button><Button variant="danger" loading={deleting()} onClick={handleDelete}>确认删除</Button></>}>
              <p class="text-content">确定要删除患者 <span class="font-semibold">{p().name}</span> 吗？</p>
              <p class="text-sm text-content-secondary mt-2">此操作将同时删除所有相关报告，且不可撤销。</p>
            </Modal>

            {/* AI Interpretation Modal */}
            <Modal open={showInterpretModal()} onClose={() => setShowInterpretModal(false)} title="AI 综合解读" size="lg" footer={
              <Show when={!interpretStarted()} fallback={<Button variant="outline" onClick={() => setShowInterpretModal(false)}>关闭</Button>}>
                <><Button variant="outline" onClick={() => setShowInterpretModal(false)}>取消</Button>
                <Button variant="primary" disabled={selectedReportIds().size === 0} onClick={() => {
                  const ids = selectedReportIds()
                  const allReports = reports() ?? []
                  if (ids.size === allReports.length) { setInterpretUrl(`/api/patients/${params.id}/interpret-all`) } else { setInterpretUrl(`/api/patients/${params.id}/interpret-multi?report_ids=${[...ids].join(',')}`) }
                  setInterpretStarted(true)
                }}>开始解读 ({selectedReportIds().size} 份报告)</Button></>
              </Show>
            }>
              <Show when={!interpretStarted()} fallback={<LlmInterpret url={interpretUrl()} autoStart />}>
                <div class="space-y-2">
                  <div class="flex items-center justify-between mb-2">
                    <span class="text-sm text-content-secondary">选择要分析的报告：</span>
                    <Button variant="ghost" size="sm" onClick={() => { const list = reports() ?? []; const all = selectedReportIds().size === list.length; setSelectedReportIds(all ? new Set<string>() : new Set(list.map(r => r.id))) }}>
                      {selectedReportIds().size === (reports() ?? []).length ? '取消全选' : '全选'}
                    </Button>
                  </div>
                  <div class="max-h-[50vh] overflow-y-auto space-y-1">
                    <For each={sortedReports()}>
                      {(report) => {
                        const checked = () => selectedReportIds().has(report.id)
                        return (
                          <label class="flex items-center gap-2 px-2 py-1.5 rounded hover:bg-surface-secondary cursor-pointer">
                            <input type="checkbox" checked={checked()} onChange={() => { const next = new Set(selectedReportIds()); if (next.has(report.id)) next.delete(report.id); else next.add(report.id); setSelectedReportIds(next) }} class="accent-accent" />
                            <div class="flex-1 min-w-0">
                              <div class="flex items-center gap-2"><Badge variant="accent">{report.report_type}</Badge><span class="meta-text">{report.report_date}</span></div>
                              <Show when={report.hospital}><span class="text-xs text-content-tertiary">{report.hospital}</span></Show>
                            </div>
                          </label>
                        )
                      }}
                    </For>
                  </div>
                </div>
              </Show>
            </Modal>

            {/* Upload Report Modal */}
            <ReportUpload patientId={params.id} open={showUploadModal()} onClose={() => setShowUploadModal(false)} onComplete={() => { setShowUploadModal(false); refetch() }} />

            {/* Upload Expense Modal */}
            <ExpenseUpload patientId={params.id} open={showExpenseModal()} onClose={() => setShowExpenseModal(false)} onComplete={() => { setShowExpenseModal(false); refetchExpenses() }} />

            {/* Mobile action FAB + BottomSheet */}
            <div class="lg:hidden">
              <FloatingActionButton variant="primary" onClick={() => setShowActionSheet(true)} icon={<svg class="w-6 h-6" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M12 6.75a.75.75 0 110-1.5.75.75 0 010 1.5zM12 12.75a.75.75 0 110-1.5.75.75 0 010 1.5zM12 18.75a.75.75 0 110-1.5.75.75 0 010 1.5z" /></svg>} />
            </div>

            <BottomSheet open={showActionSheet()} onClose={() => setShowActionSheet(false)} title="患者操作">
              <div class="flex flex-col gap-1 pb-4">
                <p class="px-4 pt-1 pb-1 micro-title">数据录入</p>
                <button class="flex items-center gap-3 w-full px-4 py-3 rounded-xl text-left hover:bg-surface-secondary transition-colors cursor-pointer" onClick={() => { setShowActionSheet(false); setShowUploadModal(true) }}>
                  <div class="w-10 h-10 rounded-full bg-accent-light flex items-center justify-center"><svg class="w-5 h-5 text-accent" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12" /></svg></div>
                  <div><div class="font-medium text-content">上传报告</div><div class="text-xs text-content-secondary">上传新的检查报告</div></div>
                </button>
                <button class="flex items-center gap-3 w-full px-4 py-3 rounded-xl text-left hover:bg-surface-secondary transition-colors cursor-pointer" onClick={() => { setShowActionSheet(false); setShowExpenseModal(true) }}>
                  <div class="w-10 h-10 rounded-full bg-accent-light flex items-center justify-center"><svg class="w-5 h-5 text-accent" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M9 14l6-6m-5.5.5h.01m4.99 5h.01M19 21V5a2 2 0 00-2-2H7a2 2 0 00-2 2v16l3.5-2 3.5 2 3.5-2 3.5 2z" /></svg></div>
                  <div><div class="font-medium text-content">上传消费清单</div><div class="text-xs text-content-secondary">识别用药和治疗方案</div></div>
                </button>
                <button class="flex items-center gap-3 w-full px-4 py-3 rounded-xl text-left hover:bg-surface-secondary transition-colors cursor-pointer" onClick={() => { setShowActionSheet(false); navigate(`/patients/${params.id}/templates`) }}>
                  <div class="w-10 h-10 rounded-full bg-accent-light flex items-center justify-center"><svg class="w-5 h-5 text-accent" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2m-3 7h3m-3 4h3m-6-4h.01M9 16h.01" /></svg></div>
                  <div><div class="font-medium text-content">快捷录入</div><div class="text-xs text-content-secondary">使用模板快速填写报告</div></div>
                </button>

                <div class="border-t border-border/50 mx-4 my-1" />
                <p class="px-4 pt-1 pb-1 micro-title">分析工具</p>
                <button class="flex items-center gap-3 w-full px-4 py-3 rounded-xl text-left hover:bg-surface-secondary transition-colors cursor-pointer" onClick={() => { setShowActionSheet(false); navigate(`/patients/${params.id}/trends`) }}>
                  <div class="w-10 h-10 rounded-full bg-success-light flex items-center justify-center"><svg class="w-5 h-5 text-success" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M7 12l3-3 3 3 4-4M8 21l4-4 4 4M3 4h18M4 4h16v12a1 1 0 01-1 1H5a1 1 0 01-1-1V4z" /></svg></div>
                  <div><div class="font-medium text-content">趋势分析</div><div class="text-xs text-content-secondary">查看检验指标变化趋势</div></div>
                </button>
                <button class="flex items-center gap-3 w-full px-4 py-3 rounded-xl text-left hover:bg-surface-secondary transition-colors cursor-pointer" onClick={() => { setShowActionSheet(false); navigate(`/patients/${params.id}/compare`) }}>
                  <div class="w-10 h-10 rounded-full bg-success-light flex items-center justify-center"><svg class="w-5 h-5 text-success" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z" /></svg></div>
                  <div><div class="font-medium text-content">报告对比</div><div class="text-xs text-content-secondary">对比两份报告的差异</div></div>
                </button>
                <button class="flex items-center gap-3 w-full px-4 py-3 rounded-xl text-left hover:bg-surface-secondary transition-colors cursor-pointer" onClick={() => { setShowActionSheet(false); openInterpretModal() }}>
                  <div class="w-10 h-10 rounded-full bg-info-light flex items-center justify-center"><svg class="w-5 h-5 text-info" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M9.663 17h4.673M12 3v1m6.364 1.636l-.707.707M21 12h-1M4 12H3m3.343-5.657l-.707-.707m2.828 9.9a5 5 0 117.072 0l-.548.547A3.374 3.374 0 0014 18.469V19a2 2 0 11-4 0v-.531c0-.895-.356-1.754-.988-2.386l-.548-.547z" /></svg></div>
                  <div><div class="font-medium text-content">AI 综合解读</div><div class="text-xs text-content-secondary">AI 分析所有报告</div></div>
                </button>
                <button class="flex items-center gap-3 w-full px-4 py-3 rounded-xl text-left hover:bg-surface-secondary transition-colors cursor-pointer" onClick={() => { setShowActionSheet(false); navigate(`/patients/${params.id}/health-assessment`) }}>
                  <div class="w-10 h-10 rounded-full bg-info-light flex items-center justify-center"><svg class="w-5 h-5 text-info" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M9.75 3.104v5.714a2.25 2.25 0 01-.659 1.591L5 14.5M9.75 3.104c-.251.023-.501.05-.75.082m.75-.082a24.301 24.301 0 014.5 0m0 0v5.714a2.25 2.25 0 00.659 1.591L19 14.5M14.25 3.104c.251.023.501.05.75.082M19 14.5l-2.47 5.636a2.25 2.25 0 01-2.061 1.364H9.531a2.25 2.25 0 01-2.061-1.364L5 14.5m14 0H5" /></svg></div>
                  <div><div class="font-medium text-content">AI 健康评估</div><div class="text-xs text-content-secondary">综合评估健康风险</div></div>
                </button>
                <button class="flex items-center gap-3 w-full px-4 py-3 rounded-xl text-left hover:bg-surface-secondary transition-colors cursor-pointer" onClick={() => { setShowActionSheet(false); navigate(`/patients/${params.id}/rag-assistant`) }}>
                  <div class="w-10 h-10 rounded-full bg-accent/10 flex items-center justify-center"><svg class="w-5 h-5 text-accent" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M8.625 12a.375.375 0 11-.75 0 .375.375 0 01.75 0zm0 0H8.25m4.125 0a.375.375 0 11-.75 0 .375.375 0 01.75 0zm0 0H12m4.125 0a.375.375 0 11-.75 0 .375.375 0 01.75 0zm0 0h-.375M21 12c0 4.556-4.03 8.25-9 8.25a9.764 9.764 0 01-2.555-.337A5.972 5.972 0 015.41 20.97a5.969 5.969 0 01-.474-.065 4.48 4.48 0 00.978-2.025c.09-.457-.133-.901-.467-1.226C3.93 16.178 3 14.189 3 12c0-4.556 4.03-8.25 9-8.25s9 3.694 9 8.25z" /></svg></div>
                  <div><div class="font-medium text-content">AI 问答助手</div><div class="text-xs text-content-secondary">基于检验数据智能问答</div></div>
                </button>
                <button class="flex items-center gap-3 w-full px-4 py-3 rounded-xl text-left hover:bg-surface-secondary transition-colors cursor-pointer" onClick={() => { setShowActionSheet(false); navigate(`/patients/${params.id}/timeline`) }}>
                  <div class="w-10 h-10 rounded-full bg-success-light flex items-center justify-center"><svg class="w-5 h-5 text-success" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" /></svg></div>
                  <div><div class="font-medium text-content">健康时间线</div><div class="text-xs text-content-secondary">查看健康事件时间轴</div></div>
                </button>
                <button class="flex items-center gap-3 w-full px-4 py-3 rounded-xl text-left hover:bg-surface-secondary transition-colors cursor-pointer" onClick={() => { setShowActionSheet(false); navigate(`/patients/${params.id}/medications`) }}>
                  <div class="w-10 h-10 rounded-full bg-success-light flex items-center justify-center"><svg class="w-5 h-5 text-success" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M19.428 15.428a2 2 0 00-1.022-.547l-2.387-.477a6 6 0 00-3.86.517l-.318.158a6 6 0 01-3.86.517L6.05 15.21a2 2 0 00-1.806.547M8 4h8l-1 1v5.172a2 2 0 00.586 1.414l5 5c1.26 1.26.367 3.414-1.415 3.414H4.828c-1.782 0-2.674-2.154-1.414-3.414l5-5A2 2 0 009 10.172V5L8 4z" /></svg></div>
                  <div><div class="font-medium text-content">用药管理</div><div class="text-xs text-content-secondary">管理用药记录</div></div>
                </button>
                <button class="flex items-center gap-3 w-full px-4 py-3 rounded-xl text-left hover:bg-surface-secondary transition-colors cursor-pointer" onClick={() => { setShowActionSheet(false); navigate(`/patients/${params.id}/drug-interaction`) }}>
                  <div class="w-10 h-10 rounded-full bg-error-light flex items-center justify-center"><svg class="w-5 h-5 text-error" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" /></svg></div>
                  <div><div class="font-medium text-content">药物相互作用检查</div><div class="text-xs text-content-secondary">检查用药间的相互作用</div></div>
                </button>
                <button class="flex items-center gap-3 w-full px-4 py-3 rounded-xl text-left hover:bg-surface-secondary transition-colors cursor-pointer" onClick={() => { setShowActionSheet(false); navigate(`/patients/${params.id}/med-lab-correlation`) }}>
                  <div class="w-10 h-10 rounded-full bg-accent/10 flex items-center justify-center"><svg class="w-5 h-5 text-accent" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z" /></svg></div>
                  <div><div class="font-medium text-content">用药-检验关联</div><div class="text-xs text-content-secondary">分析用药与检验指标变化的关联</div></div>
                </button>

                <div class="border-t border-border/50 mx-4 my-1" />
                <p class="px-4 pt-1 pb-1 micro-title">管理</p>
                <button class="flex items-center gap-3 w-full px-4 py-3 rounded-xl text-left hover:bg-surface-secondary transition-colors cursor-pointer" onClick={() => { setShowActionSheet(false); navigate(`/patients/${params.id}/edit`) }}>
                  <div class="w-10 h-10 rounded-full bg-warning-light flex items-center justify-center"><svg class="w-5 h-5 text-warning" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" /></svg></div>
                  <div><div class="font-medium text-content">编辑信息</div><div class="text-xs text-content-secondary">修改患者基本信息</div></div>
                </button>
                <button class="flex items-center gap-3 w-full px-4 py-3 rounded-xl text-left hover:bg-surface-secondary transition-colors cursor-pointer" onClick={() => { setShowActionSheet(false); setShowDeleteModal(true) }}>
                  <div class="w-10 h-10 rounded-full bg-error-light flex items-center justify-center"><svg class="w-5 h-5 text-error" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" /></svg></div>
                  <div><div class="font-medium text-error">删除患者</div><div class="text-xs text-content-secondary">永久删除该患者</div></div>
                </button>
              </div>
            </BottomSheet>
          </div>
        )}
      </Show>
    </div>
  )
}
