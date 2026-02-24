import { createSignal, For, Show } from 'solid-js'
import {
  Button, Card, CardBody, CardHeader,
  Modal, Spinner, useToast,
} from '@/components'
import { api } from '@/api/client'
import type {
  ParsedExpenseItem, ExpenseCategory, DayParseResult,
} from '@/api/types'

const ACCEPT = '.png,.jpg,.jpeg,.webp,.pdf'
const MAX_SIZE = 50 * 1024 * 1024

const CATEGORY_LABELS: Record<string, string> = {
  drug: '药品',
  test: '检查化验',
  treatment: '治疗操作',
  material: '医用材料',
  nursing: '护理',
  other: '其他',
}

const CATEGORY_ICONS: Record<string, string> = {
  drug: 'M19.428 15.428a2 2 0 00-1.022-.547l-2.387-.477a6 6 0 00-3.86.517l-.318.158a6 6 0 01-3.86.517L6.05 15.21a2 2 0 00-1.806.547M8 4h8l-1 1v5.172a2 2 0 00.586 1.414l5 5c1.26 1.26.367 3.414-1.415 3.414H4.828c-1.782 0-2.674-2.154-1.414-3.414l5-5A2 2 0 009 10.172V5L8 4z',
  test: 'M9.75 3.104v5.714a2.25 2.25 0 01-.659 1.591L5 14.5M9.75 3.104c-.251.023-.501.05-.75.082m.75-.082a24.301 24.301 0 014.5 0m0 0v5.714a2.25 2.25 0 00.659 1.591L19 14.5M14.25 3.104c.251.023.501.05.75.082M19 14.5l-2.47 5.636a2.25 2.25 0 01-2.061 1.364H9.531a2.25 2.25 0 01-2.061-1.364L5 14.5m14 0H5',
  treatment: 'M21 8.25c0-2.485-2.099-4.5-4.688-4.5-1.935 0-3.597 1.126-4.312 2.733-.715-1.607-2.377-2.733-4.313-2.733C5.1 3.75 3 5.765 3 8.25c0 7.22 9 12 9 12s9-4.78 9-12z',
  material: 'M9 12h3.75M9 15h3.75M9 18h3.75m3 .75H18a2.25 2.25 0 002.25-2.25V6.108c0-1.135-.845-2.098-1.976-2.192a48.424 48.424 0 00-1.123-.08m-5.801 0c-.065.21-.1.433-.1.664 0 .414.336.75.75.75h4.5a.75.75 0 00.75-.75 2.25 2.25 0 00-.1-.664m-5.8 0A2.251 2.251 0 0113.5 2.25H15a2.25 2.25 0 012.15 1.586m-5.8 0c-.376.023-.75.05-1.124.08C9.095 4.01 8.25 4.973 8.25 6.108V8.25m0 0H4.875c-.621 0-1.125.504-1.125 1.125v11.25c0 .621.504 1.125 1.125 1.125h9.75c.621 0 1.125-.504 1.125-1.125V9.375c0-.621-.504-1.125-1.125-1.125H8.25z',
  nursing: 'M15.75 6a3.75 3.75 0 11-7.5 0 3.75 3.75 0 017.5 0zM4.501 20.118a7.5 7.5 0 0114.998 0A17.933 17.933 0 0112 21.75c-2.676 0-5.216-.584-7.499-1.632z',
  other: 'M8.25 6.75h12M8.25 12h12m-12 5.25h12M3.75 6.75h.007v.008H3.75V6.75zm.375 0a.375.375 0 11-.75 0 .375.375 0 01.75 0zM3.75 12h.007v.008H3.75V12zm.375 0a.375.375 0 11-.75 0 .375.375 0 01.75 0zm-.375 5.25h.007v.008H3.75v-.008zm.375 0a.375.375 0 11-.75 0 .375.375 0 01.75 0z',
}

const CATEGORY_COLORS: Record<string, string> = {
  drug: 'bg-blue-100 text-blue-800',
  test: 'bg-purple-100 text-purple-800',
  treatment: 'bg-green-100 text-green-800',
  material: 'bg-orange-100 text-orange-800',
  nursing: 'bg-pink-100 text-pink-800',
  other: 'bg-gray-100 text-gray-700',
}

export interface ExpenseUploadProps {
  patientId: string
  open: boolean
  onClose: () => void
  onComplete?: () => void
}

interface DayEditState {
  date: string
  total: number
  items: ParsedExpenseItem[]
  drugAnalysis: string
  treatmentAnalysis: string
  isDuplicate: boolean
}

export default function ExpenseUpload(props: ExpenseUploadProps) {
  const { toast } = useToast()
  let fileInputRef: HTMLInputElement | undefined

  const [step, setStep] = createSignal<'upload' | 'preview' | 'done'>('upload')
  const [file, setFile] = createSignal<File | null>(null)
  const [dragOver, setDragOver] = createSignal(false)
  const [parsing, setParsing] = createSignal(false)
  const [saving, setSaving] = createSignal(false)

  // Multi-day edit state
  const [editDays, setEditDays] = createSignal<DayEditState[]>([])
  const [hasDuplicates, setHasDuplicates] = createSignal(false)
  const [confirmOverwrite, setConfirmOverwrite] = createSignal(false)
  const [savedCount, setSavedCount] = createSignal(0)
  const [analyzingDays, setAnalyzingDays] = createSignal<Set<number>>(new Set())

  function resetState() {
    setStep('upload')
    setFile(null)
    setDragOver(false)
    setParsing(false)
    setSaving(false)
    setEditDays([])
    setHasDuplicates(false)
    setConfirmOverwrite(false)
    setSavedCount(0)
    setAnalyzingDays(new Set<number>())
  }

  function handleClose() {
    resetState()
    props.onClose()
  }

  function handleFileSelect(e: Event) {
    const input = e.target as HTMLInputElement
    if (input.files?.[0]) {
      validateAndSetFile(input.files[0])
    }
  }

  function handleDrop(e: DragEvent) {
    e.preventDefault()
    setDragOver(false)
    if (e.dataTransfer?.files?.[0]) {
      validateAndSetFile(e.dataTransfer.files[0])
    }
  }

  function validateAndSetFile(f: File) {
    if (f.size > MAX_SIZE) {
      toast('error', '文件过大，最大支持 50MB')
      return
    }
    setFile(f)
  }

  /** Compress image client-side using Canvas (resize width only + WebP/JPEG). PDF passes through. */
  async function compressImage(f: File, maxWidth = 1500): Promise<File> {
    if (f.type === 'application/pdf' || f.type.startsWith('application/')) return f
    if (f.size < 200 * 1024) return f // skip if already small

    return new Promise<File>((resolve) => {
      const img = new Image()
      img.onload = () => {
        let { width: w, height: h } = img
        if (w > maxWidth) {
          const ratio = maxWidth / w
          w = maxWidth
          h = Math.round(h * ratio)
        }
        const canvas = document.createElement('canvas')
        canvas.width = w
        canvas.height = h
        const ctx = canvas.getContext('2d')!
        ctx.drawImage(img, 0, 0, w, h)
        // Try WebP first, fallback to JPEG
        const mimeType = typeof canvas.toDataURL('image/webp').startsWith('data:image/webp')
          ? 'image/webp' : 'image/jpeg'
        canvas.toBlob(
          (blob) => {
            if (blob && blob.size < f.size) {
              const ext = mimeType === 'image/webp' ? '.webp' : '.jpg'
              resolve(new File([blob], f.name.replace(/\.[^.]+$/, ext), { type: mimeType }))
            } else {
              resolve(f) // compression didn't help
            }
          },
          mimeType,
          0.85,
        )
      }
      img.onerror = () => resolve(f)
      img.src = URL.createObjectURL(f)
    })
  }

  async function handleParse() {
    const f = file()
    if (!f) return

    setParsing(true)
    try {
      const compressed = await compressImage(f)
      const [parseResult, existingList] = await Promise.all([
        api.expenses.parse(props.patientId, compressed),
        api.expenses.list(props.patientId).catch(() => [] as { expense_date: string }[]),
      ])
      const existingDates = new Set(existingList.map(e => e.expense_date))
      finishParse(parseResult.days, existingDates)
    } catch (err: any) {
      toast('error', err.message || '解析失败')
    } finally {
      setParsing(false)
      setMerging(false)
    }
  }

  function finishParse(dayResults: DayParseResult[], existingDates: Set<string>) {
    const days: DayEditState[] = dayResults.map((d: DayParseResult) => ({
      date: d.parsed.expense_date,
      total: d.parsed.total_amount,
      items: [...d.parsed.items],
      drugAnalysis: d.drug_analysis,
      treatmentAnalysis: d.treatment_analysis,
      isDuplicate: existingDates.has(d.parsed.expense_date),
    }))

    setEditDays(days)
    setHasDuplicates(days.some(d => d.isDuplicate))
    setConfirmOverwrite(false)
    setStep('preview')
    setParsing(false)
    setMerging(false)

    // Fire concurrent analysis for each day in background
    const analyzing = new Set(days.map((_, i) => i).filter(i => days[i].items.length > 0))
    setAnalyzingDays(new Set(analyzing))
    const promises = days.map(async (day, idx) => {
      if (day.items.length === 0) return
      try {
        const resp = await api.expenses.analyze({ items: day.items })
        setEditDays(prev => {
          const updated = [...prev]
          updated[idx] = {
            ...updated[idx],
            drugAnalysis: resp.drug_analysis,
            treatmentAnalysis: resp.treatment_analysis,
          }
          return updated
        })
      } catch (e) {
        // Analysis failure is non-blocking
      } finally {
        setAnalyzingDays(prev => {
          const next = new Set(prev)
          next.delete(idx)
          return next
        })
      }
    })
    Promise.all(promises)
  }

  function removeItem(dayIndex: number, itemIndex: number) {
    const days = [...editDays()]
    const day = { ...days[dayIndex], items: [...days[dayIndex].items] }
    day.items.splice(itemIndex, 1)
    day.total = day.items.reduce((sum, item) => sum + item.amount, 0)
    days[dayIndex] = day
    setEditDays(days)
  }

  function updateItemCategory(dayIndex: number, itemIndex: number, category: string) {
    const days = [...editDays()]
    const day = { ...days[dayIndex], items: [...days[dayIndex].items] }
    day.items[itemIndex] = { ...day.items[itemIndex], category }
    days[dayIndex] = day
    setEditDays(days)
  }

  function removeDay(dayIndex: number) {
    const days = [...editDays()]
    days.splice(dayIndex, 1)
    setEditDays(days)
    setHasDuplicates(days.some(d => d.isDuplicate))
  }

  async function handleConfirm() {
    const days = editDays()
    if (days.length === 0) return

    if (hasDuplicates() && !confirmOverwrite()) {
      setConfirmOverwrite(true)
      return
    }

    setSaving(true)
    try {
      const validDays = days.filter(d => d.items.length > 0)
      const batchData = {
        days: validDays.map(day => ({
          expense_date: day.date,
          total_amount: day.total,
          drug_analysis: day.drugAnalysis,
          treatment_analysis: day.treatmentAnalysis,
          items: day.items.map(item => ({
            name: item.name,
            category: item.category as ExpenseCategory,
            quantity: item.quantity,
            amount: item.amount,
            note: item.note,
          })),
        })),
      }
      const results = await api.expenses.batchConfirm(props.patientId, batchData)
      const saved = results.length
      setSavedCount(saved)
      toast('success', `${saved} 天消费记录保存成功`)
      setStep('done')
      props.onComplete?.()
    } catch (err: any) {
      toast('error', err.message || '保存失败')
    } finally {
      setSaving(false)
    }
  }

  function groupItemsByCategory(items: ParsedExpenseItem[]) {
    const groups: Record<string, ParsedExpenseItem[]> = {}
    for (const item of items) {
      const cat = item.category || 'other'
      if (!groups[cat]) groups[cat] = []
      groups[cat].push(item)
    }
    return groups
  }

  function categorySubtotal(items: ParsedExpenseItem[], cat: string) {
    return items.filter(i => (i.category || 'other') === cat).reduce((sum, i) => sum + i.amount, 0)
  }

  function totalAllDays() {
    return editDays().reduce((sum, d) => sum + d.total, 0)
  }

  function totalAllItems() {
    return editDays().reduce((sum, d) => sum + d.items.length, 0)
  }

  return (
    <Modal open={props.open} onClose={handleClose} title="上传消费清单" size="3xl">
      <div class="space-y-4">
        {/* Step: Upload */}
        <Show when={step() === 'upload'}>
          <div
            class={`border-2 border-dashed rounded-xl p-8 text-center transition-colors cursor-pointer ${
              dragOver() ? 'border-accent bg-accent/5' : 'border-border hover:border-accent/50'
            }`}
            onDragOver={(e) => { e.preventDefault(); setDragOver(true) }}
            onDragLeave={() => setDragOver(false)}
            onDrop={handleDrop}
            onClick={() => fileInputRef?.click()}
          >
            <input
              ref={fileInputRef}
              type="file"
              accept={ACCEPT}
              class="hidden"
              onChange={handleFileSelect}
            />
            <div class="mb-3">
              <svg class="w-10 h-10 mx-auto text-content-tertiary" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
                <path stroke-linecap="round" stroke-linejoin="round" d="M3 16.5v2.25A2.25 2.25 0 005.25 21h13.5A2.25 2.25 0 0021 18.75V16.5m-13.5-9L12 3m0 0l4.5 4.5M12 3v13.5" />
              </svg>
            </div>
            <p class="text-content font-medium">
              {file() ? file()!.name : '点击或拖拽上传消费清单截图'}
            </p>
            <p class="text-content-secondary text-sm mt-1">
              支持 PNG、JPG、PDF 格式，最大 50MB
            </p>
          </div>

          <Show when={file()}>
            <div class="flex justify-end gap-3">
              <Button variant="ghost" onClick={() => setFile(null)}>清除</Button>
              <Button onClick={handleParse} disabled={parsing()}>
                <Show when={parsing()} fallback="开始识别">
                  <Spinner size="sm" />
                  <span class="ml-2">识别中...</span>
                </Show>
              </Button>
            </div>
          </Show>
        </Show>

        {/* Step: Preview & Edit */}
        <Show when={step() === 'preview'}>
          <div class="space-y-4 max-h-[70vh] overflow-y-auto pr-1">
            {/* Summary bar */}
            <div class="flex items-center gap-4 flex-wrap px-1">
              <span class="text-sm text-content-secondary">识别到 <strong class="text-content">{editDays().length}</strong> 天</span>
              <span class="text-sm text-content-secondary">共 <strong class="text-content">{totalAllItems()}</strong> 项</span>
              <span class="text-sm font-bold text-accent ml-auto">总计 ¥{totalAllDays().toFixed(2)}</span>
            </div>

            {/* Duplicate warning */}
            <Show when={hasDuplicates()}>
              <div class="flex items-start gap-2 p-3 rounded-lg bg-warning-light border border-warning/30">
                <svg class="w-5 h-5 text-warning shrink-0 mt-0.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M12 9v3.75m-9.303 3.376c-.866 1.5.217 3.374 1.948 3.374h14.71c1.73 0 2.813-1.874 1.948-3.374L13.949 3.378c-.866-1.5-3.032-1.5-3.898 0L2.697 16.126zM12 15.75h.007v.008H12v-.008z" />
                </svg>
                <div class="text-sm">
                  <p class="font-medium text-warning">部分日期已存在消费清单</p>
                  <p class="text-content-secondary mt-0.5">
                    标记为“重复”的日期已有记录，可点“移除该天”或继续保存。
                  </p>
                </div>
              </div>
            </Show>

            {/* Per-day sections */}
            <For each={editDays()}>
              {(day, dayIdx) => {
                const grouped = () => groupItemsByCategory(day.items)
                return (
                  <div class="border border-border rounded-xl overflow-hidden">
                    {/* Day header */}
                    <div class="flex items-center gap-3 px-4 py-2.5 bg-surface-secondary">
                      <input
                        type="date"
                        class="px-2 py-1 rounded border border-border bg-surface text-content text-sm font-semibold"
                        value={day.date}
                        onInput={(e) => {
                          const days = [...editDays()]
                          days[dayIdx()] = { ...days[dayIdx()], date: e.currentTarget.value }
                          setEditDays(days)
                        }}
                      />
                      <Show when={day.isDuplicate}>
                        <span class="text-[10px] px-1.5 py-0.5 rounded bg-warning-light text-warning font-medium">重复</span>
                      </Show>
                      <span class="text-sm font-bold text-accent">¥{day.total.toFixed(2)}</span>
                      <span class="text-xs text-content-tertiary">{day.items.length} 项</span>
                      <Show when={editDays().length > 1}>
                        <button
                          class="ml-auto text-xs text-red-400 hover:text-red-600 cursor-pointer"
                          onClick={() => removeDay(dayIdx())}
                        >移除该天</button>
                      </Show>
                    </div>

                    {/* Items grouped by category */}
                    <div class="p-3 space-y-2">
                      <For each={Object.entries(grouped())}>
                        {([cat, catItems]) => (
                          <Card>
                            <CardHeader>
                              <div class="flex items-center justify-between w-full">
                                <span class={`inline-flex items-center gap-1 px-2 py-0.5 rounded text-xs font-medium ${CATEGORY_COLORS[cat] || CATEGORY_COLORS.other}`}>
                                  <svg class="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d={CATEGORY_ICONS[cat] || CATEGORY_ICONS.other} /></svg>
                                  {CATEGORY_LABELS[cat] || cat}
                                </span>
                                <span class="text-sm text-content-secondary">
                                  小计: ¥{categorySubtotal(day.items, cat).toFixed(2)}
                                </span>
                              </div>
                            </CardHeader>
                            <CardBody>
                              <div class="space-y-1">
                                <For each={catItems}>
                                  {(item) => {
                                    const itemIdx = () => day.items.indexOf(item)
                                    return (
                                      <div class="flex items-center gap-2 py-1.5 px-2 rounded hover:bg-surface-secondary text-sm group">
                                        <span class="flex-1 text-content truncate" title={item.name}>
                                          {item.name}
                                        </span>
                                        <Show when={item.quantity}>
                                          <span class="text-content-secondary text-xs">{item.quantity}</span>
                                        </Show>
                                        <span class={`font-medium whitespace-nowrap ${item.amount < 0 ? 'text-green-600' : 'text-content'}`}>
                                          ¥{item.amount.toFixed(2)}
                                        </span>
                                        <select
                                          class="text-xs border border-border rounded px-1 py-0.5 bg-surface opacity-0 group-hover:opacity-100 transition-opacity"
                                          value={item.category}
                                          onChange={(e) => updateItemCategory(dayIdx(), itemIdx(), e.currentTarget.value)}
                                        >
                                          <For each={Object.entries(CATEGORY_LABELS)}>
                                            {([val, label]) => (
                                              <option value={val}>{label}</option>
                                            )}
                                          </For>
                                        </select>
                                        <button
                                          class="text-red-400 hover:text-red-600 opacity-0 group-hover:opacity-100 transition-opacity text-xs"
                                          onClick={() => removeItem(dayIdx(), itemIdx())}
                                          title="移除此项"
                                        >✕</button>
                                      </div>
                                    )
                                  }}
                                </For>
                              </div>
                            </CardBody>
                          </Card>
                        )}
                      </For>

                      {/* AI Analysis for this day */}
                      <Show when={analyzingDays().has(dayIdx())}>
                        <Card>
                          <CardBody>
                            <div class="flex items-center gap-2 text-sm text-content-secondary">
                              <Spinner size="sm" />
                              <span>AI 分析中...</span>
                            </div>
                          </CardBody>
                        </Card>
                      </Show>
                      <Show when={!analyzingDays().has(dayIdx()) && (day.drugAnalysis || day.treatmentAnalysis)}>
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
                              <Show when={day.drugAnalysis}>
                                <div>
                                  <div class="flex items-center gap-1 font-medium text-accent mb-1">
                                    <svg class="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M19.428 15.428a2 2 0 00-1.022-.547l-2.387-.477a6 6 0 00-3.86.517l-.318.158a6 6 0 01-3.86.517L6.05 15.21a2 2 0 00-1.806.547M8 4h8l-1 1v5.172a2 2 0 00.586 1.414l5 5c1.26 1.26.367 3.414-1.415 3.414H4.828c-1.782 0-2.674-2.154-1.414-3.414l5-5A2 2 0 009 10.172V5L8 4z" /></svg>
                                    用药分析
                                  </div>
                                  <p class="text-content-secondary leading-relaxed whitespace-pre-wrap">{day.drugAnalysis}</p>
                                </div>
                              </Show>
                              <Show when={day.treatmentAnalysis}>
                                <div>
                                  <div class="flex items-center gap-1 font-medium text-accent mb-1">
                                    <svg class="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M21 8.25c0-2.485-2.099-4.5-4.688-4.5-1.935 0-3.597 1.126-4.312 2.733-.715-1.607-2.377-2.733-4.313-2.733C5.1 3.75 3 5.765 3 8.25c0 7.22 9 12 9 12s9-4.78 9-12z" /></svg>
                                    治疗方案
                                  </div>
                                  <p class="text-content-secondary leading-relaxed whitespace-pre-wrap">{day.treatmentAnalysis}</p>
                                </div>
                              </Show>
                            </div>
                          </CardBody>
                        </Card>
                      </Show>
                    </div>
                  </div>
                )
              }}
            </For>
          </div>

          <div class="flex justify-end gap-3 pt-2 border-t border-border">
            <Button variant="ghost" onClick={() => { setStep('upload'); setFile(null) }}>
              重新上传
            </Button>
            <Show when={confirmOverwrite()} fallback={
              <Button onClick={handleConfirm} disabled={saving() || totalAllItems() === 0}>
                <Show when={saving()} fallback={`确认保存${editDays().length > 1 ? ` (${editDays().length}天)` : ''}`}>
                  <Spinner size="sm" /> <span class="ml-2">保存中 ({savedCount()}/{editDays().length})...</span>
                </Show>
              </Button>
            }>
              <Button variant="outline" onClick={() => setConfirmOverwrite(false)}>
                取消
              </Button>
              <Button variant="danger" onClick={() => { setSaving(true); handleConfirm() }} disabled={saving()}>
                <Show when={saving()} fallback="确认重复保存">
                  <Spinner size="sm" /> <span class="ml-2">保存中 ({savedCount()}/{editDays().length})...</span>
                </Show>
              </Button>
            </Show>
          </div>
        </Show>

        {/* Step: Done */}
        <Show when={step() === 'done'}>
          <div class="text-center py-8">
            <div class="text-5xl mb-4">✅</div>
            <h3 class="text-lg font-bold text-content mb-2">消费记录已保存</h3>
            <p class="text-content-secondary text-sm mb-6">
              共 {savedCount()} 天的消费清单已成功记录
            </p>
            <div class="flex justify-center gap-3">
              <Button variant="ghost" onClick={handleClose}>关闭</Button>
              <Button onClick={() => resetState()}>继续上传</Button>
            </div>
          </div>
        </Show>
      </div>
    </Modal>
  )
}
