import { createSignal, createMemo, Show, onCleanup } from 'solid-js'
import { Button, Card, CardBody, Modal, Skeleton, Empty, Input, useToast, TemperatureChart, TemperatureWeeklyChart } from '@/components'
import { api, getErrorMessage } from '@/api/client'
import type { TemperatureRecord } from '@/api/types'
import type { Resource } from 'solid-js'

interface Props {
  patientId: string
  temperatures: Resource<TemperatureRecord[]>
  refetchTemps: () => void
}

export default function PatientTemperatures(props: Props) {
  const { toast } = useToast()

  const [showTempModal, setShowTempModal] = createSignal(false)
  const [tempDate, setTempDate] = createSignal('')
  const [tempTime, setTempTime] = createSignal('')
  const [tempValue, setTempValue] = createSignal('')
  const [tempNote, setTempNote] = createSignal('')
  const [tempLocation, setTempLocation] = createSignal('左腋下')
  const [tempSubmitting, setTempSubmitting] = createSignal(false)
  const [tempViewMode, setTempViewMode] = createSignal<'day' | 'week'>('day')

  const [showTimerModal, setShowTimerModal] = createSignal(false)
  const [timerSeconds, setTimerSeconds] = createSignal(300)
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

  function openTempModal() {
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

  function startTimer() {
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

  const todayStr = () => {
    const now = new Date()
    return `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, '0')}-${String(now.getDate()).padStart(2, '0')}`
  }
  const [selectedTempDate, setSelectedTempDate] = createSignal(todayStr())

  const tempDates = createMemo(() => {
    const all = props.temperatures() ?? []
    const dates = new Set<string>()
    for (const r of all) dates.add(r.recorded_at.split(' ')[0])
    return [...dates].sort()
  })

  const dayTemperatures = createMemo(() => {
    const all = props.temperatures() ?? []
    const date = selectedTempDate()
    return all.filter(r => r.recorded_at.startsWith(date))
  })

  function shiftTempDate(delta: number) {
    const dates = tempDates()
    if (dates.length === 0) return
    const idx = dates.indexOf(selectedTempDate())
    if (idx === -1) {
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
      await api.temperatures.create(props.patientId, {
        recorded_at: `${tempDate()} ${tempTime()}`,
        value,
        location: tempLocation(),
        note: tempNote(),
      })
      toast('success', '体温记录已添加')
      setShowTempModal(false)
      props.refetchTemps()
    } catch (err: unknown) {
      toast('error', getErrorMessage(err) || '添加失败')
    } finally {
      setTempSubmitting(false)
    }
  }

  async function handleDeleteTemp(id: string) {
    try {
      await api.temperatures.delete(id)
      toast('success', '体温记录已删除')
      props.refetchTemps()
    } catch (err: unknown) {
      toast('error', getErrorMessage(err) || '删除失败')
    }
  }

  return (
    <>
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
        <Show when={tempViewMode() === 'day' && (props.temperatures() ?? []).length > 0}>
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
            <Show when={props.temperatures.loading} fallback={
              <Show when={props.temperatures.error} fallback={
                <Show
                  when={props.temperatures() && props.temperatures()!.length > 0}
                  fallback={
                    <Empty
                      title="暂无体温记录"
                      description="点击上方按钮添加体温数据"
                    />
                  }
                >
                  <Show when={tempViewMode() === 'day'} fallback={
                    <TemperatureWeeklyChart data={props.temperatures() ?? []} />
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
                  description={String(props.temperatures.error?.message || props.temperatures.error)}
                />
              </Show>
            }>
              <Skeleton variant="rect" height={190} />
            </Show>
          </CardBody>
        </Card>
      </div>

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
    </>
  )
}
