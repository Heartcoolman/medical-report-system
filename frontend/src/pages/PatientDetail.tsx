import { createSignal, createResource, createMemo, Show, For, onCleanup } from 'solid-js'
import { A, useNavigate, useParams } from '@solidjs/router'
import { Button, Card, CardBody, Badge, Modal, Skeleton, Empty, Input, useToast, TemperatureChart, TemperatureWeeklyChart, BottomSheet, FloatingActionButton } from '@/components'
import { cn } from '@/lib/utils'
import { api } from '@/api/client'
import ReportUpload from './ReportUpload'
import ExpenseUpload from './ExpenseUpload'
import { LlmInterpret } from '@/components/LlmInterpret'
import { exportAllReportsCSV } from '@/lib/export'
import { exportAllReportsPDF } from '@/lib/export-pdf'

export default function PatientDetail() {
  const params = useParams<{ id: string }>()
  const navigate = useNavigate()
  const { toast } = useToast()

  const [patient] = createResource(() => params.id, (id) => api.patients.get(id))
  const [reports, { refetch }] = createResource(() => params.id, (id) => api.reports.listByPatient(id, { page_size: 100 }).then(r => r.items))

  const [temperatures, { refetch: refetchTemps }] = createResource(() => params.id, (id) => api.temperatures.list(id, { page_size: 100 }).then(r => r.items))

  const [showDeleteModal, setShowDeleteModal] = createSignal(false)
  const [deleting, setDeleting] = createSignal(false)
  const [showUploadModal, setShowUploadModal] = createSignal(false)
  const [showExpenseModal, setShowExpenseModal] = createSignal(false)
  const [showActionSheet, setShowActionSheet] = createSignal(false)

  const [expenses, { refetch: refetchExpenses }] = createResource(() => params.id, (id) => api.expenses.list(id, { page_size: 100 }).then(r => r.items))

  const [deleteExpenseId, setDeleteExpenseId] = createSignal<string | null>(null)
  const [deletingExpense, setDeletingExpense] = createSignal(false)

  // Detect duplicate expense dates
  const duplicateExpenseDates = createMemo(() => {
    const list = expenses() ?? []
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
      refetchExpenses()
    } catch (err: any) {
      toast('error', err.message || '删除失败')
    } finally {
      setDeletingExpense(false)
      setDeleteExpenseId(null)
    }
  }

  // AI interpretation state
  const [showInterpretModal, setShowInterpretModal] = createSignal(false)
  const [selectedReportIds, setSelectedReportIds] = createSignal<Set<string>>(new Set())
  const [interpretUrl, setInterpretUrl] = createSignal('')
  const [interpretStarted, setInterpretStarted] = createSignal(false)
  const [showTempModal, setShowTempModal] = createSignal(false)
  const [tempDate, setTempDate] = createSignal('')
  const [tempTime, setTempTime] = createSignal('')
  const [tempValue, setTempValue] = createSignal('')
  const [tempNote, setTempNote] = createSignal('')
  const [tempLocation, setTempLocation] = createSignal('左腋下')
  const [tempSubmitting, setTempSubmitting] = createSignal(false)
  const [tempViewMode, setTempViewMode] = createSignal<'day' | 'week'>('day')

  // Temperature measurement timer
  const [showTimerModal, setShowTimerModal] = createSignal(false)
  const [timerSeconds, setTimerSeconds] = createSignal(300) // 5 minutes
  const [timerRunning, setTimerRunning] = createSignal(false)
  let timerInterval: number | undefined
  let alertInterval: number | undefined
  let audioCtx: AudioContext | undefined

  const timerDisplay = createMemo(() => {
    const s = timerSeconds()
    const min = Math.floor(s / 60)
    const sec = s % 60
    return `${String(min).padStart(2, '0')}:${String(sec).padStart(2, '0')}`
  })

  const timerProgress = createMemo(() => 1 - timerSeconds() / 300)

  function ensureAudioCtx() {
    if (!audioCtx) audioCtx = new AudioContext()
    if (audioCtx.state === 'suspended') audioCtx.resume()
    return audioCtx
  }

  function playBeepOnce() {
    try {
      const ctx = ensureAudioCtx()
      const playBeep = (freq: number, startTime: number, duration: number) => {
        const osc = ctx.createOscillator()
        const gain = ctx.createGain()
        osc.connect(gain)
        gain.connect(ctx.destination)
        osc.frequency.value = freq
        osc.type = 'sine'
        gain.gain.setValueAtTime(0.6, startTime)
        gain.gain.exponentialRampToValueAtTime(0.01, startTime + duration)
        osc.start(startTime)
        osc.stop(startTime + duration)
      }
      playBeep(660, ctx.currentTime, 0.2)
      playBeep(880, ctx.currentTime + 0.25, 0.2)
      playBeep(1100, ctx.currentTime + 0.5, 0.3)
    } catch (e) { console.warn('playAlertSound failed:', e) }
    try { navigator.vibrate?.([200, 100, 200, 100, 200]) } catch {}
  }

  function startAlertLoop() {
    stopAlertLoop()
    playBeepOnce()
    alertInterval = window.setInterval(() => playBeepOnce(), 2000)
  }

  function stopAlertLoop() {
    if (alertInterval) {
      clearInterval(alertInterval)
      alertInterval = undefined
    }
  }

  function startTimer() {
    // Create AudioContext during user gesture so it won't be suspended later
    ensureAudioCtx()
    setTimerSeconds(300)
    setTimerRunning(true)
    setShowTimerModal(true)
    timerInterval = window.setInterval(() => {
      const cur = timerSeconds()
      if (cur <= 1) {
        clearInterval(timerInterval)
        setTimerSeconds(0)
        setTimerRunning(false)
        // Delay alert start to escape SolidJS batching
        setTimeout(() => {
          startAlertLoop()
          toast('success', '5分钟到！请测量体温')
        }, 50)
      } else {
        setTimerSeconds(cur - 1)
      }
    }, 1000)
  }

  function cancelTimer() {
    clearInterval(timerInterval)
    stopAlertLoop()
    setTimerRunning(false)
    setShowTimerModal(false)
    setTimerSeconds(300)
  }

  function dismissTimer() {
    stopAlertLoop()
    setShowTimerModal(false)
    openTempModal()
  }

  onCleanup(() => {
    clearInterval(timerInterval)
    stopAlertLoop()
    audioCtx?.close()
  })

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

  // Day view: selected date (default today)
  const todayStr = () => {
    const now = new Date()
    return `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, '0')}-${String(now.getDate()).padStart(2, '0')}`
  }
  const [selectedTempDate, setSelectedTempDate] = createSignal(todayStr())

  // Available dates that have temperature records
  const tempDates = createMemo(() => {
    const all = temperatures() ?? []
    const dates = new Set<string>()
    for (const r of all) dates.add(r.recorded_at.split(' ')[0])
    return [...dates].sort()
  })

  // Temperatures filtered by selected date for day view
  const dayTemperatures = createMemo(() => {
    const all = temperatures() ?? []
    const date = selectedTempDate()
    return all.filter(r => r.recorded_at.startsWith(date))
  })

  function shiftTempDate(delta: number) {
    const dates = tempDates()
    if (dates.length === 0) return
    const idx = dates.indexOf(selectedTempDate())
    if (idx === -1) {
      // Current date has no data; jump to nearest
      if (delta < 0) {
        const prev = dates.filter(d => d < selectedTempDate()).pop()
        if (prev) setSelectedTempDate(prev)
      } else {
        const next = dates.find(d => d > selectedTempDate())
        if (next) setSelectedTempDate(next)
      }
    } else {
      const newIdx = idx + delta
      if (newIdx >= 0 && newIdx < dates.length) setSelectedTempDate(dates[newIdx])
    }
  }

  const PAGE_SIZE = 9
  const [reportPage, setReportPage] = createSignal(1)

  // Batch select mode
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
      let deleted = 0
      for (const id of ids) {
        await api.reports.delete(id)
        deleted++
      }
      toast('success', `已删除 ${deleted} 份报告`)
      setBatchSelected(new Set<string>())
      setBatchMode(false)
      refetch()
    } catch (err: any) {
      toast('error', err.message || '批量删除失败')
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

  const sortedReports = () => {
    const list = reports() ?? []
    return [...list].sort((a, b) => b.report_date.localeCompare(a.report_date))
  }

  const totalReportPages = () => Math.max(1, Math.ceil(sortedReports().length / PAGE_SIZE))

  const pagedReports = () => {
    const start = (reportPage() - 1) * PAGE_SIZE
    return sortedReports().slice(start, start + PAGE_SIZE)
  }

  const stats = createMemo(() => {
    const list = reports() ?? []
    const totalReports = list.length
    const totalItems = list.reduce((sum, r) => sum + (r.item_count ?? 0), 0)
    const totalAbnormal = list.reduce((sum, r) => sum + (r.abnormal_count ?? 0), 0)
    return { totalReports, totalItems, totalAbnormal }
  })

  function openTempModal() {
    // Default to current date/time
    const now = new Date()
    const yyyy = now.getFullYear()
    const mm = String(now.getMonth() + 1).padStart(2, '0')
    const dd = String(now.getDate()).padStart(2, '0')
    const hh = String(now.getHours()).padStart(2, '0')
    const mi = String(now.getMinutes()).padStart(2, '0')
    setTempDate(`${yyyy}-${mm}-${dd}`)
    setTempTime(`${hh}:${mi}`)
    setTempValue('')
    setTempNote('')
    setShowTempModal(true)
  }

  async function handleAddTemp() {
    const value = parseFloat(tempValue())
    if (isNaN(value)) {
      toast('error', '请输入有效的体温数值')
      return
    }
    if (!tempDate() || !tempTime()) {
      toast('error', '请选择记录时间')
      return
    }
    setTempSubmitting(true)
    try {
      await api.temperatures.create(params.id, {
        recorded_at: `${tempDate()} ${tempTime()}`,
        value,
        location: tempLocation(),
        note: tempNote(),
      })
      toast('success', '体温记录已添加')
      setShowTempModal(false)
      refetchTemps()
    } catch (err: any) {
      toast('error', err.message || '添加失败')
    } finally {
      setTempSubmitting(false)
    }
  }

  async function handleDeleteTemp(id: string) {
    try {
      await api.temperatures.delete(id)
      toast('success', '体温记录已删除')
      refetchTemps()
    } catch (err: any) {
      toast('error', err.message || '删除失败')
    }
  }

  async function handleDelete() {
    setDeleting(true)
    try {
      await api.patients.delete(params.id)
      toast('success', '患者已删除')
      navigate('/')
    } catch (err: any) {
      toast('error', err.message || '删除失败')
    } finally {
      setDeleting(false)
      setShowDeleteModal(false)
    }
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
                        {/* 数据录入 */}
                        <div>
                          <p class="micro-title mb-1.5">数据录入</p>
                          <div class="grid grid-cols-2 gap-1.5">
                            <Button
                              variant="primary"
                              size="sm"
                              class="w-full"
                              onClick={() => setShowUploadModal(true)}
                            >
                              上传报告
                            </Button>
                            <Button
                              variant="secondary"
                              size="sm"
                              class="w-full"
                              onClick={() => setShowExpenseModal(true)}
                            >
                              消费清单
                            </Button>
                            <Button
                              variant="secondary"
                              size="sm"
                              class="w-full col-span-2"
                              onClick={() => navigate(`/patients/${params.id}/templates`)}
                            >
                              快捷录入
                            </Button>
                          </div>
                        </div>

                        {/* 分析工具 */}
                        <div>
                          <p class="micro-title mb-1.5">分析工具</p>
                          <div class="grid grid-cols-2 gap-1.5">
                            <Button
                              variant="secondary"
                              size="sm"
                              class="w-full"
                              onClick={() => navigate(`/patients/${params.id}/trends`)}
                            >
                              趋势分析
                            </Button>
                            <Button
                              variant="secondary"
                              size="sm"
                              class="w-full"
                              onClick={() => navigate(`/patients/${params.id}/compare`)}
                            >
                              报告对比
                            </Button>
                            <Button
                              variant="secondary"
                              size="sm"
                              class="w-full"
                              onClick={() => {
                                const list = reports() ?? []
                                setSelectedReportIds(new Set(list.map(r => r.id)))
                                setInterpretUrl('')
                                setInterpretStarted(false)
                                setShowInterpretModal(true)
                              }}
                            >
                              AI 综合解读
                            </Button>
                            <Button
                              variant="secondary"
                              size="sm"
                              class="w-full"
                              onClick={() => navigate(`/patients/${params.id}/health-assessment`)}
                            >
                              AI 健康评估
                            </Button>
                            <Button
                              variant="secondary"
                              size="sm"
                              class="w-full"
                              onClick={() => navigate(`/patients/${params.id}/timeline`)}
                            >
                              健康时间线
                            </Button>
                            <Button
                              variant="secondary"
                              size="sm"
                              class="w-full"
                              onClick={() => navigate(`/patients/${params.id}/medications`)}
                            >
                              用药管理
                            </Button>
                          </div>
                        </div>

                        {/* 管理 */}
                        <div>
                          <p class="micro-title mb-1.5">管理</p>
                          <div class="flex gap-2">
                            <Button
                              variant="outline"
                              size="sm"
                              class="flex-1"
                              onClick={() => navigate(`/patients/${params.id}/edit`)}
                            >
                              编辑
                            </Button>
                            <Button
                              variant="danger"
                              size="sm"
                              class="flex-1"
                              onClick={() => setShowDeleteModal(true)}
                            >
                              删除
                            </Button>
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
              {/* Temperature section */}
              <div class="mb-4">
                <div class="flex items-center justify-between mb-2">
                  <h2 class="section-title">体温记录</h2>
                  <div class="flex items-center gap-2">
                    <div class="inline-flex p-0.5 rounded-xl bg-surface-secondary gap-0.5 text-xs">
                      <button
                        class={`px-3 py-1.5 rounded-lg cursor-pointer transition-all duration-200 ${
                          tempViewMode() === 'day'
                            ? 'bg-surface-elevated text-accent shadow-sm font-medium'
                            : 'text-content-secondary hover:text-content'
                        }`}
                        onClick={() => setTempViewMode('day')}
                      >
                        日视图
                      </button>
                      <button
                        class={`px-3 py-1.5 rounded-lg cursor-pointer transition-all duration-200 ${
                          tempViewMode() === 'week'
                            ? 'bg-surface-elevated text-accent shadow-sm font-medium'
                            : 'text-content-secondary hover:text-content'
                        }`}
                        onClick={() => setTempViewMode('week')}
                      >
                        周视图
                      </button>
                    </div>
                    <Button variant="outline" size="sm" onClick={startTimer}>
                      体温测量
                    </Button>
                    <Button variant="outline" size="sm" onClick={openTempModal}>
                      添加体温
                    </Button>
                  </div>
                </div>
                {/* Day view date navigator */}
                <Show when={tempViewMode() === 'day' && (temperatures() ?? []).length > 0}>
                  <div class="inline-flex items-center gap-1 mb-2 p-1 rounded-xl bg-surface-secondary">
                    <button
                      class="h-7 w-7 flex items-center justify-center rounded-lg text-content-secondary hover:bg-surface-elevated hover:shadow-sm cursor-pointer transition-all duration-200 disabled:opacity-30 disabled:cursor-not-allowed"
                      disabled={tempDates().indexOf(selectedTempDate()) <= 0 && !tempDates().some(d => d < selectedTempDate())}
                      onClick={() => shiftTempDate(-1)}
                    >
                      <svg class="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2.5"><path stroke-linecap="round" stroke-linejoin="round" d="M15 19l-7-7 7-7" /></svg>
                    </button>
                    <input
                      type="date"
                      class="text-xs rounded-lg px-2.5 py-1 bg-surface-elevated text-content border-0 shadow-sm"
                      value={selectedTempDate()}
                      onInput={(e) => setSelectedTempDate(e.currentTarget.value)}
                    />
                    <button
                      class="h-7 w-7 flex items-center justify-center rounded-lg text-content-secondary hover:bg-surface-elevated hover:shadow-sm cursor-pointer transition-all duration-200 disabled:opacity-30 disabled:cursor-not-allowed"
                      disabled={(() => { const d = tempDates(); const i = d.indexOf(selectedTempDate()); return i === d.length - 1 || (!d.some(x => x > selectedTempDate())); })()}
                      onClick={() => shiftTempDate(1)}
                    >
                      <svg class="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2.5"><path stroke-linecap="round" stroke-linejoin="round" d="M9 5l7 7-7 7" /></svg>
                    </button>
                    <button
                      class="px-2.5 py-1 rounded-lg text-xs font-medium text-accent hover:bg-surface-elevated hover:shadow-sm cursor-pointer transition-all duration-200"
                      onClick={() => setSelectedTempDate(todayStr())}
                    >
                      今天
                    </button>
                  </div>
                </Show>
                <Card variant="outlined">
                  <CardBody class="px-4 py-3">
                    <Show when={temperatures.loading} fallback={
                      <Show when={temperatures.error} fallback={
                        <Show
                          when={temperatures() && temperatures()!.length > 0}
                          fallback={
                            <Empty
                              title="暂无体温记录"
                              description="点击上方按钮添加体温数据"
                            />
                          }
                        >
                          <Show when={tempViewMode() === 'day'} fallback={
                            <TemperatureWeeklyChart data={temperatures() ?? []} />
                          }>
                            <Show when={dayTemperatures().length > 0} fallback={
                              <Empty
                                title={`${selectedTempDate()} 暂无体温记录`}
                                description="选择其他日期或添加体温数据"
                              />
                            }>
                              <TemperatureChart
                                data={dayTemperatures()}
                                onDelete={handleDeleteTemp}
                              />
                            </Show>
                          </Show>
                        </Show>
                      }>
                        <Empty
                          title="加载体温记录失败"
                          description={String(temperatures.error?.message || temperatures.error)}
                        />
                      </Show>
                    }>
                      <Skeleton variant="rect" height={190} />
                    </Show>
                  </CardBody>
                </Card>
              </div>

              <div class="flex items-center justify-between mb-3">
                <h2 class="section-title">检查报告</h2>
                <Show when={sortedReports().length > 0}>
                  <div class="flex gap-1">
                    <Button variant="ghost" size="sm" onClick={() => { const p2 = patient(); if (p2) exportAllReportsCSV(sortedReports(), p2.name) }}>
                      导出CSV
                    </Button>
                    <Button variant="ghost" size="sm" onClick={() => { const p2 = patient(); if (p2) exportAllReportsPDF(sortedReports(), p2) }}>
                      导出PDF
                    </Button>
                    <Button variant="ghost" size="sm" onClick={() => { setBatchMode(!batchMode()); setBatchSelected(new Set<string>()) }}>
                      {batchMode() ? '取消多选' : '多选'}
                    </Button>
                  </div>
                </Show>
              </div>

              {/* Batch action bar */}
              <Show when={batchMode() && sortedReports().length > 0}>
                <div class="flex items-center gap-3 mb-3 px-3 py-2 rounded-xl bg-surface-secondary">
                  <button
                    type="button"
                    class="text-xs text-accent hover:underline cursor-pointer"
                    onClick={toggleSelectAll}
                  >
                    {batchSelected().size === sortedReports().length ? '取消全选' : '全选'}
                  </button>
                  <span class="text-xs text-content-secondary">已选 {batchSelected().size} 项</span>
                  <div class="ml-auto flex gap-2">
                    <Button
                      variant="outline"
                      size="sm"
                      disabled={batchSelected().size === 0}
                      onClick={handleBatchExportCSV}
                    >
                      导出选中
                    </Button>
                    <Button
                      variant="danger"
                      size="sm"
                      disabled={batchSelected().size === 0}
                      loading={batchDeleting()}
                      onClick={() => {
                        if (confirm(`确认删除选中的 ${batchSelected().size} 份报告？此操作不可恢复。`)) {
                          handleBatchDelete()
                        }
                      }}
                    >
                      删除选中
                    </Button>
                  </div>
                </div>
              </Show>

              {/* Summary stats bar */}
              <Show when={!reports.loading && sortedReports().length > 0}>
                <div class="flex items-center gap-4 mb-3 px-1">
                  <div>
                    <span class="data-label">报告</span>
                    <span class="data-value ml-1">{stats().totalReports}</span>
                  </div>
                  <div class="w-px h-4 bg-border" />
                  <div>
                    <span class="data-label">检验项</span>
                    <span class="data-value ml-1">{stats().totalItems}</span>
                  </div>
                  <div class="w-px h-4 bg-border" />
                  <div>
                    <span class="data-label">异常</span>
                    <span class={cn('data-value ml-1', stats().totalAbnormal > 0 && 'text-error')}>
                      {stats().totalAbnormal}
                    </span>
                  </div>
                </div>
              </Show>

              <Show when={reports.loading} fallback={
                <Show when={reports.error} fallback={
                  <Show
                    when={sortedReports().length > 0}
                    fallback={
                      <Empty
                        title="暂无报告"
                        description="还没有上传任何检查报告"
                        action={
                          <Button
                            variant="primary"
                            size="sm"
                            onClick={() => setShowUploadModal(true)}
                          >
                            上传报告
                          </Button>
                        }
                      />
                    }
                  >
                    <div class="grid grid-cols-1 sm:grid-cols-2 xl:grid-cols-3 gap-2">
                      <For each={pagedReports()}>
                        {(report) => (
                          <Show when={batchMode()} fallback={
                            <A
                              href={`/reports/${report.id}`}
                              class="block no-underline group"
                            >
                              <Card variant="outlined" class="h-full hover:border-accent hover:-translate-y-0.5 hover:shadow-md transition-all cursor-pointer">
                                <CardBody class="p-3 flex flex-col gap-1">
                                  <div class="flex items-start justify-between gap-2">
                                    <h3 class="text-sm font-semibold text-content truncate min-w-0">{report.report_type}</h3>
                                    <span class="meta-text shrink-0">{report.report_date}</span>
                                  </div>
                                  <Show when={report.hospital}>
                                    <div class="flex items-center gap-1 text-xs text-content-tertiary">
                                      <svg class="w-3 h-3 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                                        <path stroke-linecap="round" stroke-linejoin="round" d="M19 21V5a2 2 0 00-2-2H7a2 2 0 00-2 2v16m14 0h2m-2 0h-5m-9 0H3m2 0h5M9 7h1m-1 4h1m4-4h1m-1 4h1m-5 10v-5a1 1 0 011-1h2a1 1 0 011 1v-5m-4 0h4" />
                                      </svg>
                                      <span class="truncate">{report.hospital}</span>
                                    </div>
                                  </Show>
                                  <Show when={report.sample_date && report.sample_date !== report.report_date}>
                                    <div class="flex items-center gap-1.5 text-xs text-content-tertiary">
                                      <span>采样 {report.sample_date}</span>
                                    </div>
                                  </Show>
                                  <Show when={report.item_count > 0}>
                                    <div class="flex items-center gap-2 text-xs text-content-tertiary">
                                      <span>{report.item_count} 项检验</span>
                                      <Show when={report.abnormal_count > 0}>
                                        <Badge variant="error">{report.abnormal_count} 项异常</Badge>
                                      </Show>
                                    </div>
                                  </Show>
                                  <Show when={report.abnormal_names && report.abnormal_names.length > 0}>
                                    <div class="text-xs text-error truncate">
                                      {report.abnormal_names.slice(0, 3).join('、')}
                                      <Show when={report.abnormal_names.length > 3}>
                                        <span class="text-content-tertiary"> 等{report.abnormal_names.length}项</span>
                                      </Show>
                                    </div>
                                  </Show>
                                </CardBody>
                              </Card>
                            </A>
                          }>
                            {/* Batch mode card with checkbox */}
                            <div
                              class="block cursor-pointer"
                              onClick={() => toggleBatchSelect(report.id)}
                            >
                              <Card variant="outlined" class={cn('h-full transition-all', batchSelected().has(report.id) && 'border-accent bg-accent/5')}>
                                <CardBody class="p-3 flex flex-col gap-1">
                                  <div class="flex items-start justify-between gap-2">
                                    <div class="flex items-center gap-2 min-w-0">
                                      <input
                                        type="checkbox"
                                        checked={batchSelected().has(report.id)}
                                        class="accent-[var(--color-accent)] w-4 h-4 shrink-0 cursor-pointer"
                                        onClick={(e) => e.stopPropagation()}
                                        onChange={() => toggleBatchSelect(report.id)}
                                      />
                                      <h3 class="text-sm font-semibold text-content truncate min-w-0">{report.report_type}</h3>
                                    </div>
                                    <span class="meta-text shrink-0">{report.report_date}</span>
                                  </div>
                                  <Show when={report.item_count > 0}>
                                    <div class="flex items-center gap-2 text-xs text-content-tertiary">
                                      <span>{report.item_count} 项检验</span>
                                      <Show when={report.abnormal_count > 0}>
                                        <Badge variant="error">{report.abnormal_count} 项异常</Badge>
                                      </Show>
                                    </div>
                                  </Show>
                                </CardBody>
                              </Card>
                            </div>
                          </Show>
                        )}
                      </For>
                    </div>

                    {/* Pagination */}
                    <Show when={totalReportPages() > 1}>
                      <div class="flex items-center justify-center gap-1 mt-4">
                        <button
                          class="h-8 w-8 flex items-center justify-center rounded-lg text-content-secondary hover:bg-surface-secondary cursor-pointer transition-all duration-200 disabled:opacity-30 disabled:cursor-not-allowed"
                          disabled={reportPage() <= 1}
                          onClick={() => setReportPage(p => p - 1)}
                        >
                          <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M15 19l-7-7 7-7" /></svg>
                        </button>
                        <For each={Array.from({ length: totalReportPages() }, (_, i) => i + 1)}>
                          {(page) => (
                            <button
                              class={cn(
                                'h-8 min-w-[32px] px-2 rounded-lg text-sm font-medium cursor-pointer transition-all duration-200',
                                page === reportPage()
                                  ? 'bg-accent text-accent-content shadow-sm'
                                  : 'text-content-secondary hover:bg-surface-secondary',
                              )}
                              onClick={() => setReportPage(page)}
                            >
                              {page}
                            </button>
                          )}
                        </For>
                        <button
                          class="h-8 w-8 flex items-center justify-center rounded-lg text-content-secondary hover:bg-surface-secondary cursor-pointer transition-all duration-200 disabled:opacity-30 disabled:cursor-not-allowed"
                          disabled={reportPage() >= totalReportPages()}
                          onClick={() => setReportPage(p => p + 1)}
                        >
                          <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M9 5l7 7-7 7" /></svg>
                        </button>
                      </div>
                    </Show>
                  </Show>
                }>
                  <Empty
                    title="加载报告列表失败"
                    description={String(reports.error?.message || reports.error)}
                  />
                </Show>
              }>
                <div class="grid grid-cols-1 sm:grid-cols-2 xl:grid-cols-3 gap-2">
                  <Skeleton variant="rect" height={100} />
                  <Skeleton variant="rect" height={100} />
                  <Skeleton variant="rect" height={100} />
                </div>
              </Show>

              {/* Expense list section */}
              <h2 class="section-title mb-3 mt-6">消费清单</h2>
              <Show when={expenses.loading} fallback={
                <Show when={expenses.error} fallback={
                  <Show
                    when={(expenses() ?? []).length > 0}
                    fallback={
                      <Empty
                        title="暂无消费清单"
                        description="还没有上传任何消费清单"
                        action={
                          <Button
                            variant="primary"
                            size="sm"
                            onClick={() => setShowExpenseModal(true)}
                          >
                            上传消费清单
                          </Button>
                        }
                      />
                    }
                  >
                    <div class="grid grid-cols-1 sm:grid-cols-2 xl:grid-cols-3 gap-2">
                      <For each={[...(expenses() ?? [])].sort((a, b) => b.expense_date.localeCompare(a.expense_date) || b.created_at.localeCompare(a.created_at))}>
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
                    description={String(expenses.error?.message || expenses.error)}
                  />
                </Show>
              }>
                <div class="grid grid-cols-1 sm:grid-cols-2 xl:grid-cols-3 gap-2">
                  <Skeleton variant="rect" height={80} />
                  <Skeleton variant="rect" height={80} />
                  <Skeleton variant="rect" height={80} />
                </div>
              </Show>
            </div>

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

            {/* Delete patient confirmation modal */}
            <Modal
              open={showDeleteModal()}
              onClose={() => setShowDeleteModal(false)}
              title="确认删除"
              size="sm"
              footer={
                <>
                  <Button variant="outline" onClick={() => setShowDeleteModal(false)}>
                    取消
                  </Button>
                  <Button variant="danger" loading={deleting()} onClick={handleDelete}>
                    确认删除
                  </Button>
                </>
              }
            >
              <p class="text-content">
                确定要删除患者 <span class="font-semibold">{p().name}</span> 吗？
              </p>
              <p class="text-sm text-content-secondary mt-2">
                此操作将同时删除所有相关报告，且不可撤销。
              </p>
            </Modal>

            {/* Add Temperature Modal */}
            <Modal
              open={showTempModal()}
              onClose={() => setShowTempModal(false)}
              title="添加体温记录"
              size="sm"
              footer={
                <>
                  <Button variant="outline" onClick={() => setShowTempModal(false)}>
                    取消
                  </Button>
                  <Button variant="primary" loading={tempSubmitting()} onClick={handleAddTemp}>
                    确认添加
                  </Button>
                </>
              }
            >
              <div class="flex flex-col gap-3">
                <div class="flex gap-2">
                  <div class="flex-1">
                    <label class="data-label mb-1 block">日期</label>
                    <Input
                      type="date"
                      value={tempDate()}
                      onInput={(e) => setTempDate(e.currentTarget.value)}
                    />
                  </div>
                  <div class="flex-1">
                    <label class="data-label mb-1 block">时间</label>
                    <Input
                      type="time"
                      value={tempTime()}
                      onInput={(e) => setTempTime(e.currentTarget.value)}
                    />
                  </div>
                </div>
                <div class="flex gap-2">
                  <div class="flex-1">
                    <label class="data-label mb-1 block">体温 (℃)</label>
                    <Input
                      type="number"
                      placeholder="例如 36.5"
                      value={tempValue()}
                      onInput={(e) => setTempValue(e.currentTarget.value)}
                      step="0.1"
                      min="34"
                      max="43"
                    />
                  </div>
                  <div class="flex-1">
                    <label class="data-label mb-1 block">测量部位</label>
                    <select
                      class="w-full h-9 px-3 rounded-lg border border-border bg-surface text-sm text-content focus:outline-none focus:ring-2 focus:ring-accent/30"
                      value={tempLocation()}
                      onChange={(e) => setTempLocation(e.currentTarget.value)}
                    >
                      <option value="左腋下">左腋下</option>
                      <option value="右腋下">右腋下</option>
                      <option value="口腔">口腔</option>
                      <option value="耳温">耳温</option>
                      <option value="额温">额温</option>
                      <option value="肛温">肛温</option>
                    </select>
                  </div>
                </div>
                <div>
                  <label class="data-label mb-1 block">备注（可选）</label>
                  <Input
                    placeholder="如：饭后测量"
                    value={tempNote()}
                    onInput={(e) => setTempNote(e.currentTarget.value)}
                  />
                </div>
              </div>
            </Modal>

            {/* Temperature Measurement Timer Modal */}
            <Modal
              open={showTimerModal()}
              onClose={() => { if (timerRunning()) cancelTimer() }}
              title="体温测量计时"
              size="sm"
              footer={
                <Show when={timerRunning()} fallback={
                  <Button variant="primary" onClick={dismissTimer}>记录体温</Button>
                }>
                  <Button variant="outline" onClick={cancelTimer}>取消测量</Button>
                </Show>
              }
            >
              <div class="flex flex-col items-center py-6 gap-4">
                <div class="relative w-40 h-40">
                  <svg class="w-full h-full -rotate-90" viewBox="0 0 100 100">
                    <circle cx="50" cy="50" r="45" fill="none" stroke="var(--color-border)" stroke-width="6" />
                    <circle
                      cx="50" cy="50" r="45" fill="none"
                      stroke="var(--color-accent)"
                      stroke-width="6"
                      stroke-linecap="round"
                      stroke-dasharray={`${timerProgress() * 283} 283`}
                      style="transition: stroke-dasharray 0.3s linear"
                    />
                  </svg>
                  <div class="absolute inset-0 flex items-center justify-center">
                    <span class="text-3xl font-mono font-semibold text-content">{timerDisplay()}</span>
                  </div>
                </div>
                <Show when={timerRunning()} fallback={
                  <p class="text-sm text-accent font-medium animate-pulse">测量完成！请点击下方按钮记录体温</p>
                }>
                  <p class="text-sm text-content-secondary">请将体温计放置好，计时结束后将提醒您</p>
                </Show>
              </div>
            </Modal>

            {/* AI Interpretation Modal */}
            <Modal
              open={showInterpretModal()}
              onClose={() => setShowInterpretModal(false)}
              title="AI 综合解读"
              size="lg"
              footer={
                <Show when={!interpretStarted()} fallback={
                  <Button variant="outline" onClick={() => setShowInterpretModal(false)}>关闭</Button>
                }>
                  <>
                    <Button variant="outline" onClick={() => setShowInterpretModal(false)}>取消</Button>
                    <Button
                      variant="primary"
                      disabled={selectedReportIds().size === 0}
                      onClick={() => {
                        const ids = selectedReportIds()
                        const allReports = reports() ?? []
                        if (ids.size === allReports.length) {
                          setInterpretUrl(`/api/patients/${params.id}/interpret-all`)
                        } else {
                          setInterpretUrl(`/api/patients/${params.id}/interpret-multi?report_ids=${[...ids].join(',')}`)
                        }
                        setInterpretStarted(true)
                      }}
                    >
                      开始解读 ({selectedReportIds().size} 份报告)
                    </Button>
                  </>
                </Show>
              }
            >
              <Show when={!interpretStarted()} fallback={
                <LlmInterpret url={interpretUrl()} autoStart />
              }>
                <div class="space-y-2">
                  <div class="flex items-center justify-between mb-2">
                    <span class="text-sm text-content-secondary">选择要分析的报告：</span>
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => {
                        const list = reports() ?? []
                        const all = selectedReportIds().size === list.length
                        setSelectedReportIds(all ? new Set<string>() : new Set(list.map(r => r.id)))
                      }}
                    >
                      {selectedReportIds().size === (reports() ?? []).length ? '取消全选' : '全选'}
                    </Button>
                  </div>
                  <div class="max-h-[50vh] overflow-y-auto space-y-1">
                    <For each={sortedReports()}>
                      {(report) => {
                        const checked = () => selectedReportIds().has(report.id)
                        return (
                          <label class="flex items-center gap-2 px-2 py-1.5 rounded hover:bg-surface-secondary cursor-pointer">
                            <input
                              type="checkbox"
                              checked={checked()}
                              onChange={() => {
                                const next = new Set(selectedReportIds())
                                if (next.has(report.id)) next.delete(report.id)
                                else next.add(report.id)
                                setSelectedReportIds(next)
                              }}
                              class="accent-accent"
                            />
                            <div class="flex-1 min-w-0">
                              <div class="flex items-center gap-2">
                                <Badge variant="accent">{report.report_type}</Badge>
                                <span class="meta-text">{report.report_date}</span>
                              </div>
                              <Show when={report.hospital}>
                                <span class="text-xs text-content-tertiary">{report.hospital}</span>
                              </Show>
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
            <ReportUpload
              patientId={params.id}
              open={showUploadModal()}
              onClose={() => setShowUploadModal(false)}
              onComplete={() => {
                setShowUploadModal(false)
                refetch()
              }}
            />

            {/* Upload Expense Modal */}
            <ExpenseUpload
              patientId={params.id}
              open={showExpenseModal()}
              onClose={() => setShowExpenseModal(false)}
              onComplete={() => {
                setShowExpenseModal(false)
                refetchExpenses()
              }}
            />

            {/* Mobile action FAB + BottomSheet */}
            <div class="lg:hidden">
              <FloatingActionButton
                variant="primary"
                onClick={() => setShowActionSheet(true)}
                icon={
                  <svg class="w-6 h-6" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                    <path stroke-linecap="round" stroke-linejoin="round" d="M12 6.75a.75.75 0 110-1.5.75.75 0 010 1.5zM12 12.75a.75.75 0 110-1.5.75.75 0 010 1.5zM12 18.75a.75.75 0 110-1.5.75.75 0 010 1.5z" />
                  </svg>
                }
              />
            </div>

            <BottomSheet
              open={showActionSheet()}
              onClose={() => setShowActionSheet(false)}
              title="患者操作"
            >
              <div class="flex flex-col gap-1 pb-4">
                {/* 数据录入 */}
                <p class="px-4 pt-1 pb-1 micro-title">数据录入</p>
                <button
                  class="flex items-center gap-3 w-full px-4 py-3 rounded-xl text-left hover:bg-surface-secondary transition-colors cursor-pointer"
                  onClick={() => { setShowActionSheet(false); setShowUploadModal(true) }}
                >
                  <div class="w-10 h-10 rounded-full bg-accent-light flex items-center justify-center">
                    <svg class="w-5 h-5 text-accent" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                      <path stroke-linecap="round" stroke-linejoin="round" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12" />
                    </svg>
                  </div>
                  <div>
                    <div class="font-medium text-content">上传报告</div>
                    <div class="text-xs text-content-secondary">上传新的检查报告</div>
                  </div>
                </button>
                <button
                  class="flex items-center gap-3 w-full px-4 py-3 rounded-xl text-left hover:bg-surface-secondary transition-colors cursor-pointer"
                  onClick={() => { setShowActionSheet(false); setShowExpenseModal(true) }}
                >
                  <div class="w-10 h-10 rounded-full bg-accent-light flex items-center justify-center">
                    <svg class="w-5 h-5 text-accent" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                      <path stroke-linecap="round" stroke-linejoin="round" d="M9 14l6-6m-5.5.5h.01m4.99 5h.01M19 21V5a2 2 0 00-2-2H7a2 2 0 00-2 2v16l3.5-2 3.5 2 3.5-2 3.5 2z" />
                    </svg>
                  </div>
                  <div>
                    <div class="font-medium text-content">上传消费清单</div>
                    <div class="text-xs text-content-secondary">识别用药和治疗方案</div>
                  </div>
                </button>
                <button
                  class="flex items-center gap-3 w-full px-4 py-3 rounded-xl text-left hover:bg-surface-secondary transition-colors cursor-pointer"
                  onClick={() => { setShowActionSheet(false); navigate(`/patients/${params.id}/templates`) }}
                >
                  <div class="w-10 h-10 rounded-full bg-accent-light flex items-center justify-center">
                    <svg class="w-5 h-5 text-accent" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                      <path stroke-linecap="round" stroke-linejoin="round" d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2m-3 7h3m-3 4h3m-6-4h.01M9 16h.01" />
                    </svg>
                  </div>
                  <div>
                    <div class="font-medium text-content">快捷录入</div>
                    <div class="text-xs text-content-secondary">使用模板快速填写报告</div>
                  </div>
                </button>

                {/* 分析工具 */}
                <div class="border-t border-border/50 mx-4 my-1" />
                <p class="px-4 pt-1 pb-1 micro-title">分析工具</p>
                <button
                  class="flex items-center gap-3 w-full px-4 py-3 rounded-xl text-left hover:bg-surface-secondary transition-colors cursor-pointer"
                  onClick={() => { setShowActionSheet(false); navigate(`/patients/${params.id}/trends`) }}
                >
                  <div class="w-10 h-10 rounded-full bg-success-light flex items-center justify-center">
                    <svg class="w-5 h-5 text-success" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                      <path stroke-linecap="round" stroke-linejoin="round" d="M7 12l3-3 3 3 4-4M8 21l4-4 4 4M3 4h18M4 4h16v12a1 1 0 01-1 1H5a1 1 0 01-1-1V4z" />
                    </svg>
                  </div>
                  <div>
                    <div class="font-medium text-content">趋势分析</div>
                    <div class="text-xs text-content-secondary">查看检验指标变化趋势</div>
                  </div>
                </button>
                <button
                  class="flex items-center gap-3 w-full px-4 py-3 rounded-xl text-left hover:bg-surface-secondary transition-colors cursor-pointer"
                  onClick={() => { setShowActionSheet(false); navigate(`/patients/${params.id}/compare`) }}
                >
                  <div class="w-10 h-10 rounded-full bg-success-light flex items-center justify-center">
                    <svg class="w-5 h-5 text-success" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                      <path stroke-linecap="round" stroke-linejoin="round" d="M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z" />
                    </svg>
                  </div>
                  <div>
                    <div class="font-medium text-content">报告对比</div>
                    <div class="text-xs text-content-secondary">对比两份报告的差异</div>
                  </div>
                </button>
                <button
                  class="flex items-center gap-3 w-full px-4 py-3 rounded-xl text-left hover:bg-surface-secondary transition-colors cursor-pointer"
                  onClick={() => {
                    setShowActionSheet(false)
                    const list = reports() ?? []
                    setSelectedReportIds(new Set(list.map(r => r.id)))
                    setInterpretUrl('')
                    setInterpretStarted(false)
                    setShowInterpretModal(true)
                  }}
                >
                  <div class="w-10 h-10 rounded-full bg-info-light flex items-center justify-center">
                    <svg class="w-5 h-5 text-info" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                      <path stroke-linecap="round" stroke-linejoin="round" d="M9.663 17h4.673M12 3v1m6.364 1.636l-.707.707M21 12h-1M4 12H3m3.343-5.657l-.707-.707m2.828 9.9a5 5 0 117.072 0l-.548.547A3.374 3.374 0 0014 18.469V19a2 2 0 11-4 0v-.531c0-.895-.356-1.754-.988-2.386l-.548-.547z" />
                    </svg>
                  </div>
                  <div>
                    <div class="font-medium text-content">AI 综合解读</div>
                    <div class="text-xs text-content-secondary">AI 分析所有报告</div>
                  </div>
                </button>
                <button
                  class="flex items-center gap-3 w-full px-4 py-3 rounded-xl text-left hover:bg-surface-secondary transition-colors cursor-pointer"
                  onClick={() => { setShowActionSheet(false); navigate(`/patients/${params.id}/health-assessment`) }}
                >
                  <div class="w-10 h-10 rounded-full bg-info-light flex items-center justify-center">
                    <svg class="w-5 h-5 text-info" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                      <path stroke-linecap="round" stroke-linejoin="round" d="M9.75 3.104v5.714a2.25 2.25 0 01-.659 1.591L5 14.5M9.75 3.104c-.251.023-.501.05-.75.082m.75-.082a24.301 24.301 0 014.5 0m0 0v5.714a2.25 2.25 0 00.659 1.591L19 14.5M14.25 3.104c.251.023.501.05.75.082M19 14.5l-2.47 5.636a2.25 2.25 0 01-2.061 1.364H9.531a2.25 2.25 0 01-2.061-1.364L5 14.5m14 0H5" />
                    </svg>
                  </div>
                  <div>
                    <div class="font-medium text-content">AI 健康评估</div>
                    <div class="text-xs text-content-secondary">综合评估健康风险</div>
                  </div>
                </button>
                <button
                  class="flex items-center gap-3 w-full px-4 py-3 rounded-xl text-left hover:bg-surface-secondary transition-colors cursor-pointer"
                  onClick={() => { setShowActionSheet(false); navigate(`/patients/${params.id}/timeline`) }}
                >
                  <div class="w-10 h-10 rounded-full bg-success-light flex items-center justify-center">
                    <svg class="w-5 h-5 text-success" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                      <path stroke-linecap="round" stroke-linejoin="round" d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" />
                    </svg>
                  </div>
                  <div>
                    <div class="font-medium text-content">健康时间线</div>
                    <div class="text-xs text-content-secondary">查看健康事件时间轴</div>
                  </div>
                </button>
                <button
                  class="flex items-center gap-3 w-full px-4 py-3 rounded-xl text-left hover:bg-surface-secondary transition-colors cursor-pointer"
                  onClick={() => { setShowActionSheet(false); navigate(`/patients/${params.id}/medications`) }}
                >
                  <div class="w-10 h-10 rounded-full bg-success-light flex items-center justify-center">
                    <svg class="w-5 h-5 text-success" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                      <path stroke-linecap="round" stroke-linejoin="round" d="M19.428 15.428a2 2 0 00-1.022-.547l-2.387-.477a6 6 0 00-3.86.517l-.318.158a6 6 0 01-3.86.517L6.05 15.21a2 2 0 00-1.806.547M8 4h8l-1 1v5.172a2 2 0 00.586 1.414l5 5c1.26 1.26.367 3.414-1.415 3.414H4.828c-1.782 0-2.674-2.154-1.414-3.414l5-5A2 2 0 009 10.172V5L8 4z" />
                    </svg>
                  </div>
                  <div>
                    <div class="font-medium text-content">用药管理</div>
                    <div class="text-xs text-content-secondary">管理用药记录</div>
                  </div>
                </button>

                {/* 管理 */}
                <div class="border-t border-border/50 mx-4 my-1" />
                <p class="px-4 pt-1 pb-1 micro-title">管理</p>
                <button
                  class="flex items-center gap-3 w-full px-4 py-3 rounded-xl text-left hover:bg-surface-secondary transition-colors cursor-pointer"
                  onClick={() => { setShowActionSheet(false); navigate(`/patients/${params.id}/edit`) }}
                >
                  <div class="w-10 h-10 rounded-full bg-warning-light flex items-center justify-center">
                    <svg class="w-5 h-5 text-warning" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                      <path stroke-linecap="round" stroke-linejoin="round" d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
                    </svg>
                  </div>
                  <div>
                    <div class="font-medium text-content">编辑信息</div>
                    <div class="text-xs text-content-secondary">修改患者基本信息</div>
                  </div>
                </button>
                <button
                  class="flex items-center gap-3 w-full px-4 py-3 rounded-xl text-left hover:bg-surface-secondary transition-colors cursor-pointer"
                  onClick={() => { setShowActionSheet(false); setShowDeleteModal(true) }}
                >
                  <div class="w-10 h-10 rounded-full bg-error-light flex items-center justify-center">
                    <svg class="w-5 h-5 text-error" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                      <path stroke-linecap="round" stroke-linejoin="round" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                    </svg>
                  </div>
                  <div>
                    <div class="font-medium text-error">删除患者</div>
                    <div class="text-xs text-content-secondary">永久删除该患者</div>
                  </div>
                </button>
              </div>
            </BottomSheet>
          </div>
        )}
      </Show>
    </div>
  )
}
