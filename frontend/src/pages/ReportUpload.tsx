import { createSignal, createMemo, For, Show } from 'solid-js'
import { useNavigate } from '@solidjs/router'
import {
  Button, Card, CardBody, CardHeader, Input, Badge, TestItemStatusBadge,
  Modal, Progress, Spinner, Switch, Table, useToast,
} from '@/components'
import type { TableColumn } from '@/components'
import { api } from '@/api/client'
import type {
  OcrParseResult, ParsedItem, SuggestGroupsResult,
  BatchReportInput, BatchConfirmReq,
  ReportDetail, MergeCheckResult,
} from '@/api/types'

const STEP_LABELS = ['上传文件', '预览编辑', '分组确认', '保存完成'] as const
const ACCEPT = '.pdf,.png,.jpg,.jpeg'
const MAX_SIZE = 50 * 1024 * 1024

function formatSize(bytes: number): string {
  if (bytes < 1024) return bytes + ' B'
  if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + ' KB'
  return (bytes / (1024 * 1024)).toFixed(1) + ' MB'
}

export interface ReportUploadProps {
  patientId: string
  open: boolean
  onClose: () => void
  onComplete?: () => void
}

export default function ReportUpload(props: ReportUploadProps) {
  const navigate = useNavigate()
  const { toast } = useToast()
  let fileInputRef: HTMLInputElement | undefined

  const [step, setStep] = createSignal(0)
  const [files, setFiles] = createSignal<File[]>([])
  const [dragOver, setDragOver] = createSignal(false)

  // Step 2
  const [parsing, setParsing] = createSignal(false)
  const [parseProgress, setParseProgress] = createSignal(0)
  const [parsedResults, setParsedResults] = createSignal<OcrParseResult[]>([])

  // Step 3: raw group data + user merge toggles
  const [grouping, setGrouping] = createSignal(false)
  const [groupResult, setGroupResult] = createSignal<SuggestGroupsResult | null>(null)
  // suggestedGroups[i] = group ID for file i (0 = independent, >0 = merge suggestion)
  const [suggestedGroups, setSuggestedGroups] = createSignal<number[]>([])
  // per-group merge toggle: group ID -> boolean (true = merge, false = keep independent)
  const [groupMergeFlags, setGroupMergeFlags] = createSignal<Map<number, boolean>>(new Map())
  // manual merge selection: set of file indices selected for manual merge
  const [selectedIndependent, setSelectedIndependent] = createSignal<Set<number>>(new Set())

  // Step 4
  const [saving, setSaving] = createSignal(false)
  const [savedReports, setSavedReports] = createSignal<ReportDetail[]>([])

  // Merge check modal
  const [mergeCheckResult, setMergeCheckResult] = createSignal<MergeCheckResult | null>(null)
  const [showMergeConfirm, setShowMergeConfirm] = createSignal(false)
  const [pendingBatchReq, setPendingBatchReq] = createSignal<BatchConfirmReq | null>(null)

  function resetState() {
    setStep(0)
    setFiles([])
    setDragOver(false)
    setParsing(false)
    setParseProgress(0)
    setParsedResults([])
    setGrouping(false)
    setGroupResult(null)
    setSuggestedGroups([])
    setGroupMergeFlags(new Map())
    setSelectedIndependent(new Set<number>())
    setSaving(false)
    setSavedReports([])
    setMergeCheckResult(null)
    setShowMergeConfirm(false)
    setPendingBatchReq(null)
  }

  function handleClose() {
    resetState()
    props.onClose()
  }

  function handleComplete() {
    resetState()
    props.onComplete?.()
    props.onClose()
  }

  // --- Step 1: File handling ---

  function addFiles(newFiles: FileList | File[]) {
    const existing = new Set(files().map(f => f.name))
    const arr = Array.from(newFiles).filter(f => {
      if (existing.has(f.name)) {
        toast('error', `${f.name} 已添加`)
        return false
      }
      if (f.size > MAX_SIZE) {
        toast('error', `${f.name} 超过 50MB 限制`)
        return false
      }
      const ext = '.' + f.name.split('.').pop()?.toLowerCase()
      if (!ACCEPT.split(',').includes(ext)) {
        toast('error', `${f.name} 不支持的文件格式`)
        return false
      }
      existing.add(f.name)
      return true
    })
    setFiles(prev => [...prev, ...arr])
  }

  function removeFile(index: number) {
    setFiles(prev => prev.filter((_, i) => i !== index))
  }

  function handleDrop(e: DragEvent) {
    e.preventDefault()
    setDragOver(false)
    if (e.dataTransfer?.files) addFiles(e.dataTransfer.files)
  }

  function handleDragOver(e: DragEvent) {
    e.preventDefault()
    setDragOver(true)
  }

  function handleDragLeave() {
    setDragOver(false)
  }

  function handleFileInput(e: Event) {
    const input = e.target as HTMLInputElement
    if (input.files) addFiles(input.files)
    input.value = ''
  }

  function openFilePicker() {
    fileInputRef?.click()
  }

  // --- Step 2: Parse files ---

  async function startParsing() {
    setStep(1)
    setParsing(true)
    setParseProgress(0)
    const fileList = files()

    // Dynamic timeout: 90s per-file processing + queuing headroom.
    // Browser limits ~6 concurrent connections per domain, so later files
    // wait in the browser queue while their AbortController timer ticks.
    // Give enough time: 90s base + 15s per file to cover queuing.
    const perFileTimeout = 90_000 + fileList.length * 15_000

    let completed = 0
    const promises = fileList.map((file) =>
      api.ocr.parse(file, perFileTimeout)
        .catch((err: any) => {
          toast('error', `解析 ${file.name} 失败: ${err.message}`)
          return {
            file_id: '',
            file_path: '',
            file_name: file.name,
            parsed: { report_type: '', hospital: '', report_date: '', sample_date: '', items: [] },
          } as OcrParseResult
        })
        .then((result) => {
          completed++
          setParseProgress(completed)
          return result
        }),
    )

    const results = await Promise.all(promises)
    setParsedResults(results)
    setParsing(false)
  }

  function updateParsedField(index: number, field: string, value: string) {
    setParsedResults(prev => {
      const copy = [...prev]
      const r = { ...copy[index], parsed: { ...copy[index].parsed, [field]: value } }
      copy[index] = r
      return copy
    })
  }

  function updateParsedItem(rIndex: number, iIndex: number, field: keyof ParsedItem, value: string) {
    setParsedResults(prev => {
      const copy = [...prev]
      const items = [...copy[rIndex].parsed.items]
      items[iIndex] = { ...items[iIndex], [field]: value }
      copy[rIndex] = { ...copy[rIndex], parsed: { ...copy[rIndex].parsed, items } }
      return copy
    })
  }

  function addParsedItem(rIndex: number) {
    setParsedResults(prev => {
      const copy = [...prev]
      const items = [...copy[rIndex].parsed.items, { name: '', value: '', unit: '', reference_range: '', status: 'normal' as const }]
      copy[rIndex] = { ...copy[rIndex], parsed: { ...copy[rIndex].parsed, items } }
      return copy
    })
  }

  function removeParsedItem(rIndex: number, iIndex: number) {
    setParsedResults(prev => {
      const copy = [...prev]
      const items = copy[rIndex].parsed.items.filter((_, i) => i !== iIndex)
      copy[rIndex] = { ...copy[rIndex], parsed: { ...copy[rIndex].parsed, items } }
      return copy
    })
  }

  // --- Step 3: Grouping ---

  // Compute grouped file indices from suggested groups (only groups with 2+ files)
  const suggestedMergeGroups = createMemo(() => {
    const groups = suggestedGroups()
    const map = new Map<number, number[]>()
    groups.forEach((g, i) => {
      if (g > 0) {
        if (!map.has(g)) map.set(g, [])
        map.get(g)!.push(i)
      }
    })
    // Only keep groups with 2+ files (single-file "groups" are effectively independent)
    const result: Array<{ groupId: number; fileIndices: number[] }> = []
    for (const [groupId, indices] of map) {
      if (indices.length >= 2) {
        result.push({ groupId, fileIndices: indices })
      }
    }
    return result
  })

  // File indices that are independent (group=0 or single-file group or user toggled off merge)
  const independentFileIndices = createMemo(() => {
    const groups = suggestedGroups()
    const flags = groupMergeFlags()
    const mergedGroups = suggestedMergeGroups()
    const mergedIndices = new Set<number>()

    for (const mg of mergedGroups) {
      if (flags.get(mg.groupId)) {
        for (const idx of mg.fileIndices) mergedIndices.add(idx)
      }
    }

    return groups.map((_, i) => i).filter(i => !mergedIndices.has(i))
  })

  // Build batch request from current user selections
  function buildBatchReq(): BatchConfirmReq {
    const results = parsedResults()
    const gr = groupResult()
    const flags = groupMergeFlags()
    const mergedGroups = suggestedMergeGroups()

    const reports: BatchReportInput[] = []

    // Merged groups
    for (const mg of mergedGroups) {
      if (!flags.get(mg.groupId)) continue
      const indices = mg.fileIndices
      const first = results[indices[0]]
      const allItems = indices.flatMap(i => results[i].parsed.items)
      const filePaths = indices.map(i => results[i].file_path).filter(Boolean)
      const existingMerge = gr?.existing_merges.find(m => indices.includes(m.file_index))

      reports.push({
        existing_report_id: existingMerge?.report_id,
        report_type: first.parsed.report_type,
        hospital: first.parsed.hospital,
        report_date: first.parsed.report_date,
        sample_date: first.parsed.sample_date,
        file_paths: filePaths,
        items: allItems,
      })
    }

    // Independent files
    for (const i of independentFileIndices()) {
      const r = results[i]
      const existingMerge = gr?.existing_merges.find(m => m.file_index === i)
      reports.push({
        existing_report_id: existingMerge?.report_id,
        report_type: r.parsed.report_type,
        hospital: r.parsed.hospital,
        report_date: r.parsed.report_date,
        sample_date: r.parsed.sample_date,
        file_paths: r.file_path ? [r.file_path] : [],
        items: [...r.parsed.items],
      })
    }

    return { reports }
  }

  function toggleGroupMerge(groupId: number) {
    setGroupMergeFlags(prev => {
      const next = new Map(prev)
      next.set(groupId, !prev.get(groupId))
      return next
    })
  }

  function toggleSelectIndependent(fileIdx: number) {
    setSelectedIndependent(prev => {
      const next = new Set(prev)
      if (next.has(fileIdx)) next.delete(fileIdx)
      else next.add(fileIdx)
      return next
    })
  }

  function mergeSelected() {
    const indep = independentFileIndices()
    const selected = [...selectedIndependent()].filter(i => indep.includes(i))
    if (selected.length < 2) return

    const maxGroupId = Math.max(0, ...suggestedGroups())
    const newGroupId = maxGroupId + 1

    setSuggestedGroups(prev => {
      const next = [...prev]
      for (const idx of selected) next[idx] = newGroupId
      return next
    })
    setGroupMergeFlags(prev => {
      const next = new Map(prev)
      next.set(newGroupId, true)
      return next
    })
    setSelectedIndependent(new Set<number>())
  }

  function removeFromGroup(fileIdx: number) {
    setSuggestedGroups(prev => {
      const next = [...prev]
      next[fileIdx] = 0
      return next
    })
  }

  async function startGrouping() {
    const validResults = parsedResults().filter(
      r => r.parsed.report_type.trim() && r.parsed.report_date.trim(),
    )
    if (validResults.length === 0) {
      toast('error', '没有有效的解析结果，请检查文件内容')
      return
    }
    if (validResults.length < parsedResults().length) {
      toast('warning', `${parsedResults().length - validResults.length} 个文件解析失败，已跳过`)
      setParsedResults(validResults)
    }

    setStep(2)
    setGrouping(true)
    try {
      const results = validResults
      const suggestReq = {
        patient_id: props.patientId,
        files: results.map(r => ({
          file_name: r.file_name,
          report_type: r.parsed.report_type,
          report_date: r.parsed.report_date,
          sample_date: r.parsed.sample_date,
          item_names: r.parsed.items.map(i => i.name),
        })),
      }
      const gr = await api.ocr.suggestGroups(suggestReq)
      setGroupResult(gr)
      setSuggestedGroups(gr.groups)

      // Initialize merge flags: all suggested merges default to ON
      const flags = new Map<number, boolean>()
      const groupCounts = new Map<number, number>()
      gr.groups.forEach(g => {
        if (g > 0) groupCounts.set(g, (groupCounts.get(g) ?? 0) + 1)
      })
      for (const [gId, count] of groupCounts) {
        if (count >= 2) flags.set(gId, true)
      }
      setGroupMergeFlags(flags)
    } catch (err: any) {
      toast('error', `分组失败: ${err.message}`)
    } finally {
      setGrouping(false)
    }
  }

  // --- Step 4: Save ---

  async function confirmSave() {
    let req = buildBatchReq()

    // Merge check: see if any reports will merge into existing ones
    try {
      const mergeResult = await api.reports.mergeCheck(props.patientId, req)
      if (mergeResult.merges.length > 0) {
        setMergeCheckResult(mergeResult)
        setPendingBatchReq(req)
        setShowMergeConfirm(true)
        return
      }
    } catch {
      // mergeCheck failed — graceful fallback, proceed without check
      toast('warning', '合并检查失败，将直接保存')
    }

    await doSave(req)
  }

  async function confirmMergeAndSave() {
    const req = pendingBatchReq()
    const merges = mergeCheckResult()
    if (!req || !merges) return

    // Apply existing_report_id from merge check result
    const updatedReports = req.reports.map((r, i) => {
      const merge = merges.merges.find(m => m.input_index === i)
      if (merge) {
        return { ...r, existing_report_id: merge.existing_report_id }
      }
      return r
    })
    const updatedReq = { ...req, reports: updatedReports }
    setShowMergeConfirm(false)
    setMergeCheckResult(null)
    setPendingBatchReq(null)
    await doSave(updatedReq)
  }

  async function doSave(req: BatchConfirmReq) {
    setStep(3)
    setSaving(true)
    try {
      // Prefetch normalize
      const nameMap = await api.reports.prefetchNormalize(props.patientId, req)
      req = { ...req, prefetched_name_map: nameMap, skip_merge_check: true }

      const created = await api.reports.batchConfirm(props.patientId, req)
      setSavedReports(created)
      toast('success', `成功保存 ${created.length} 份报告`)
    } catch (err: any) {
      toast('error', `保存失败: ${err.message}`)
      setStep(2) // go back to allow retry
    } finally {
      setSaving(false)
    }
  }

  const GROUP_COLORS = [
    'border-accent/50 bg-accent-light/40',
    'border-success/50 bg-success-light/40',
    'border-warning/50 bg-warning-light/40',
    'border-info/50 bg-info-light/40',
    'border-border-hover bg-surface-secondary',
    'border-content-tertiary/40 bg-surface-tertiary/60',
  ]

  const stepTitle = () => STEP_LABELS[step()] ?? '上传报告'

  // Prevent closing during save
  const canClose = () => !saving() && !parsing()
  const compactInputClass = 'form-control-dense'

  const parsedItemColumns = (rIndex: number): TableColumn<ParsedItem>[] => [
    {
      key: 'name',
      title: '名称',
      width: '28%',
      render: (value: string, _row, iIndex) => (
        <Input
          value={value}
          onInput={(e) => updateParsedItem(rIndex, iIndex, 'name', e.currentTarget.value)}
          class={compactInputClass}
          wrapperClass="min-w-[120px]"
        />
      ),
    },
    {
      key: 'value',
      title: '结果',
      width: '18%',
      render: (value: string, _row, iIndex) => (
        <Input
          value={value}
          onInput={(e) => updateParsedItem(rIndex, iIndex, 'value', e.currentTarget.value)}
          class={compactInputClass}
          wrapperClass="min-w-[100px]"
        />
      ),
    },
    {
      key: 'unit',
      title: '单位',
      width: '12%',
      render: (value: string, _row, iIndex) => (
        <Input
          value={value}
          onInput={(e) => updateParsedItem(rIndex, iIndex, 'unit', e.currentTarget.value)}
          class={compactInputClass}
          wrapperClass="min-w-[80px]"
        />
      ),
    },
    {
      key: 'reference_range',
      title: '参考范围',
      width: '22%',
      render: (value: string, _row, iIndex) => (
        <Input
          value={value}
          onInput={(e) => updateParsedItem(rIndex, iIndex, 'reference_range', e.currentTarget.value)}
          class={compactInputClass}
          wrapperClass="min-w-[120px]"
        />
      ),
    },
    {
      key: 'status',
      title: '状态',
      width: '12%',
      render: (value: string, row) => (
        <div class="whitespace-nowrap">
          <TestItemStatusBadge status={value} value={row.value} referenceRange={row.reference_range} />
        </div>
      ),
    },
    {
      key: 'actions',
      title: '操作',
      width: '8%',
      render: (_value, _row, iIndex) => (
        <Button
          type="button"
          variant="ghost"
          size="xs"
          class="icon-danger-btn-xs"
          onClick={() => removeParsedItem(rIndex, iIndex)}
          aria-label="删除检验项目"
        >
          <svg class="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
            <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
          </svg>
        </Button>
      ),
    },
  ]

  return (
    <>
    <Modal
      open={props.open}
      onClose={() => canClose() && handleClose()}
      title={`上传报告 - ${stepTitle()}`}
      size="4xl"
    >
      <div class="space-y-3">
        {/* Step indicator */}
        <div class="flex items-center gap-1.5">
          <For each={STEP_LABELS}>
            {(label, i) => (
              <>
                <Show when={i() > 0}>
                  <div class={`flex-1 h-0.5 ${i() <= step() ? 'bg-accent' : 'bg-border'}`} />
                </Show>
                <div class="flex items-center gap-1.5">
                  <div class={`step-indicator-dot ${
                    i() <= step() ? 'step-indicator-dot-active' : 'step-indicator-dot-inactive'
                  }`}>
                    {i() + 1}
                  </div>
                  <span class={`step-indicator-label ${
                    i() <= step() ? 'text-content' : 'text-content-tertiary'
                  }`}>{label}</span>
                </div>
              </>
            )}
          </For>
        </div>

        {/* Step 1: File Upload */}
        <Show when={step() === 0}>
          <div
            class={`upload-dropzone ${
              dragOver() ? 'upload-dropzone-active' : 'upload-dropzone-idle'
            }`}
            onDrop={handleDrop}
            onDragOver={handleDragOver}
            onDragLeave={handleDragLeave}
          >
            <svg class="mx-auto h-10 w-10 text-content-tertiary mb-3" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
              <path stroke-linecap="round" stroke-linejoin="round" d="M3 16.5v2.25A2.25 2.25 0 005.25 21h13.5A2.25 2.25 0 0021 18.75V16.5m-13.5-9L12 3m0 0l4.5 4.5M12 3v13.5" />
            </svg>
            <p class="text-content-secondary mb-2">拖拽文件到此处，或</p>
            <input
              ref={(el) => { fileInputRef = el }}
              type="file"
              accept={ACCEPT}
              multiple
              class="hidden"
              onChange={handleFileInput}
            />
            <Button type="button" variant="outline" onClick={openFilePicker}>
              选择文件
            </Button>
            <p class="meta-text mt-2">支持 PDF、PNG、JPG 格式，单个文件不超过 50MB</p>
          </div>

          <Show when={files().length > 0}>
            <div class="space-y-1.5">
              <For each={files()}>
                {(file, i) => (
                  <div class="flex items-center justify-between px-3 py-1.5 bg-surface-secondary rounded-lg">
                    <div class="flex items-center gap-2">
                      <svg class="h-4 w-4 text-content-tertiary shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
                        <path stroke-linecap="round" stroke-linejoin="round" d="M19.5 14.25v-2.625a3.375 3.375 0 00-3.375-3.375h-1.5A1.125 1.125 0 0113.5 7.125v-1.5a3.375 3.375 0 00-3.375-3.375H8.25m2.25 0H5.625c-.621 0-1.125.504-1.125 1.125v17.25c0 .621.504 1.125 1.125 1.125h12.75c.621 0 1.125-.504 1.125-1.125V11.25a9 9 0 00-9-9z" />
                      </svg>
                      <span class="text-sm text-content truncate">{file.name}</span>
                      <span class="meta-text">{formatSize(file.size)}</span>
                    </div>
                    <Button
                      type="button"
                      variant="ghost"
                      size="xs"
                      class="icon-danger-btn-xs"
                      onClick={() => removeFile(i())}
                      aria-label="删除文件"
                    >
                      <svg class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                        <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
                      </svg>
                    </Button>
                  </div>
                )}
              </For>
            </div>
          </Show>

          <div class="flex justify-end">
            <Button
              variant="primary"
              disabled={files().length === 0}
              onClick={startParsing}
            >
              开始解析
            </Button>
          </div>
        </Show>

        {/* Step 2: OCR Preview & Edit */}
        <Show when={step() === 1}>
          <Show when={parsing()}>
            <div class="text-center py-8">
              <Spinner size="lg" />
              <p class="mt-4 text-content-secondary">
                正在解析文件 ({parseProgress()}/{files().length})
              </p>
              <div class="mt-4 max-w-md mx-auto">
                <Progress value={parseProgress()} max={files().length} showPercentage />
              </div>
            </div>
          </Show>

          <Show when={!parsing()}>
            <div class="space-y-4">
              <For each={parsedResults()}>
                {(result, rIndex) => (
                  <Card variant="outlined">
                    <CardHeader>
                      <div class="flex items-center justify-between w-full">
                        <h3 class="font-semibold text-sm">{result.file_name}</h3>
                        <Badge variant="default">文件 {rIndex() + 1}/{parsedResults().length}</Badge>
                      </div>
                    </CardHeader>
                    <CardBody class="p-2">
                      <div class="grid grid-cols-2 md:grid-cols-4 gap-2 mb-3">
                        <Input
                          label="报告类型"
                          value={result.parsed.report_type}
                          onInput={(e) => updateParsedField(rIndex(), 'report_type', e.currentTarget.value)}
                        />
                        <Input
                          label="医院"
                          value={result.parsed.hospital}
                          onInput={(e) => updateParsedField(rIndex(), 'hospital', e.currentTarget.value)}
                        />
                        <Input
                          label="报告日期"
                          type="date"
                          value={result.parsed.report_date}
                          onInput={(e) => updateParsedField(rIndex(), 'report_date', e.currentTarget.value)}
                        />
                        <Input
                          label="采样日期"
                          type="date"
                          value={result.parsed.sample_date}
                          onInput={(e) => updateParsedField(rIndex(), 'sample_date', e.currentTarget.value)}
                        />
                      </div>

                      <div class="space-y-2">
                        <div class="flex items-center justify-between">
                          <h4 class="micro-title">检验项目 ({result.parsed.items.length})</h4>
                          <Button variant="ghost" size="sm" onClick={() => addParsedItem(rIndex())}>
                            + 添加
                          </Button>
                        </div>

                        <Table<ParsedItem>
                          columns={parsedItemColumns(rIndex())}
                          data={result.parsed.items}
                          emptyTitle="暂无检验项目"
                          emptyDescription="可点击上方“+ 添加”补充项目"
                        />
                      </div>
                    </CardBody>
                  </Card>
                )}
              </For>

              <div class="flex justify-between">
                <Button variant="outline" onClick={() => setStep(0)}>
                  上一步
                </Button>
                <Button variant="primary" onClick={startGrouping}>
                  下一步
                </Button>
              </div>
            </div>
          </Show>
        </Show>

        {/* Step 3: Grouping & Review */}
        <Show when={step() === 2}>
          <Show when={grouping()}>
            <div class="text-center py-8">
              <Spinner size="lg" />
              <p class="mt-4 text-content-secondary">正在分析分组...</p>
            </div>
          </Show>

          <Show when={!grouping() && suggestedGroups().length > 0}>
            <div class="space-y-3">
              {/* Merge suggestions */}
              <Show when={suggestedMergeGroups().length > 0}>
                <h4 class="section-subtitle">建议合并的文件</h4>
                <For each={suggestedMergeGroups()}>
                  {(mg, gi) => {
                    const colorClass = () => GROUP_COLORS[gi() % GROUP_COLORS.length]
                    const isMerged = () => groupMergeFlags().get(mg.groupId) ?? false
                    const results = parsedResults()

                    return (
                      <div class={`rounded-2xl border-2 p-3 space-y-2 transition-colors ${
                        isMerged() ? colorClass() : 'border-border/50 bg-surface-secondary'
                      }`}>
                        <div class="flex items-center justify-between">
                          <div class="flex items-center gap-2">
                            <Show when={isMerged()}>
                              <Badge variant="accent">组 {gi() + 1}</Badge>
                            </Show>
                            <span class="text-sm font-medium">
                              {mg.fileIndices.length} 个文件
                            </span>
                          </div>
                          <div class="flex items-center gap-2">
                            <Badge variant={isMerged() ? 'success' : 'default'} dot>
                              {isMerged() ? '合并为一份报告' : '各自独立'}
                            </Badge>
                            <Switch
                              checked={isMerged()}
                              onChange={() => toggleGroupMerge(mg.groupId)}
                              size="sm"
                              class="shrink-0"
                            />
                          </div>
                        </div>

                        <div class="space-y-1.5">
                          <For each={mg.fileIndices}>
                            {(fileIdx) => {
                              const r = results[fileIdx]
                              return (
                                <div class="flex items-center gap-2 px-2 py-1.5 rounded surface-overlay-soft text-sm">
                                  <svg class="h-4 w-4 text-content-tertiary shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
                                    <path stroke-linecap="round" stroke-linejoin="round" d="M19.5 14.25v-2.625a3.375 3.375 0 00-3.375-3.375h-1.5A1.125 1.125 0 0113.5 7.125v-1.5a3.375 3.375 0 00-3.375-3.375H8.25m2.25 0H5.625c-.621 0-1.125.504-1.125 1.125v17.25c0 .621.504 1.125 1.125 1.125h12.75c.621 0 1.125-.504 1.125-1.125V11.25a9 9 0 00-9-9z" />
                                  </svg>
                                  <span class="font-medium truncate">{r.parsed.report_type}</span>
                                  <span class="text-content-secondary">{r.parsed.report_date}</span>
                                  <Show when={r.parsed.hospital}>
                                    <span class="text-content-tertiary truncate">{r.parsed.hospital}</span>
                                  </Show>
                                  <span class="meta-text ml-auto whitespace-nowrap">
                                    {r.parsed.items.length} 项
                                  </span>
                                  <Button
                                    variant="ghost"
                                    size="xs"
                                    class="text-content-tertiary hover:text-danger shrink-0 ml-1"
                                    onClick={() => removeFromGroup(fileIdx)}
                                  >
                                    移出
                                  </Button>
                                </div>
                              )
                            }}
                          </For>
                        </div>

                        <Show when={isMerged()}>
                          <div class="meta-text px-2">
                            合并后共 {mg.fileIndices.reduce((sum, i) => sum + results[i].parsed.items.length, 0)} 个检验项目
                          </div>
                        </Show>
                      </div>
                    )
                  }}
                </For>
              </Show>

              {/* Independent files */}
              <Show when={independentFileIndices().length > 0}>
                <div class="flex items-center justify-between">
                  <div>
                    <h4 class="section-subtitle">
                      独立报告 ({independentFileIndices().length})
                    </h4>
                    <p class="meta-text mt-0.5">勾选报告后可手动合并</p>
                  </div>
                  <Show when={selectedIndependent().size >= 2}>
                    <Button variant="primary" size="sm" onClick={mergeSelected}>
                      合并选中 ({selectedIndependent().size})
                    </Button>
                  </Show>
                </div>
                <div class="space-y-1.5">
                  <For each={independentFileIndices()}>
                    {(fileIdx) => {
                      const r = parsedResults()[fileIdx]
                      return (
                        <div
                          class={`flex items-center gap-2 px-3 py-2 rounded-xl border text-sm cursor-pointer transition-all duration-200 ${
                            selectedIndependent().has(fileIdx) ? 'border-accent bg-accent-light/30 shadow-sm' : 'border-border/50 hover:border-border-hover hover:shadow-sm'
                          }`}
                          onClick={() => toggleSelectIndependent(fileIdx)}
                        >
                          <input
                            type="checkbox"
                            checked={selectedIndependent().has(fileIdx)}
                            class="accent-[var(--color-accent)] h-4 w-4 shrink-0 cursor-pointer"
                            onClick={(e) => e.stopPropagation()}
                            onChange={() => toggleSelectIndependent(fileIdx)}
                          />
                          <svg class="h-4 w-4 text-content-tertiary shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
                            <path stroke-linecap="round" stroke-linejoin="round" d="M19.5 14.25v-2.625a3.375 3.375 0 00-3.375-3.375h-1.5A1.125 1.125 0 0113.5 7.125v-1.5a3.375 3.375 0 00-3.375-3.375H8.25m2.25 0H5.625c-.621 0-1.125.504-1.125 1.125v17.25c0 .621.504 1.125 1.125 1.125h12.75c.621 0 1.125-.504 1.125-1.125V11.25a9 9 0 00-9-9z" />
                          </svg>
                          <span class="font-medium truncate">{r.parsed.report_type}</span>
                          <span class="text-content-secondary">{r.parsed.report_date}</span>
                          <Show when={r.parsed.hospital}>
                            <span class="text-content-tertiary truncate">{r.parsed.hospital}</span>
                          </Show>
                          <span class="meta-text ml-auto whitespace-nowrap">
                            {r.parsed.items.length} 项
                          </span>
                        </div>
                      )
                    }}
                  </For>
                </div>
              </Show>

              <div class="flex justify-between pt-2">
                <Button variant="outline" onClick={() => setStep(1)}>
                  上一步
                </Button>
                <Button variant="primary" onClick={confirmSave}>
                  确认保存
                </Button>
              </div>
            </div>
          </Show>
        </Show>

        {/* Step 4: Saving / Complete */}
        <Show when={step() === 3}>
          <Show when={saving()}>
            <div class="text-center py-8">
              <Spinner size="lg" />
              <p class="mt-4 text-content-secondary">正在保存报告...</p>
            </div>
          </Show>

          <Show when={!saving() && savedReports().length > 0}>
            <div class="space-y-3">
              <p class="text-content-secondary">
                成功创建 {savedReports().length} 份报告：
              </p>
              <For each={savedReports()}>
                {(report) => (
                  <div class="flex items-center justify-between px-3 py-2 bg-surface-secondary rounded-lg">
                    <div class="flex items-center gap-2">
                      <Badge variant="accent">{report.report_type}</Badge>
                      <span class="text-sm">{report.report_date}</span>
                      <span class="text-sm text-content-tertiary">{report.hospital}</span>
                      <span class="meta-text">
                        {report.test_items.length} 个项目
                      </span>
                    </div>
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => navigate(`/reports/${report.id}`)}
                    >
                      查看
                    </Button>
                  </div>
                )}
              </For>

              <div class="flex justify-end pt-2">
                <Button variant="primary" onClick={handleComplete}>
                  完成
                </Button>
              </div>
            </div>
          </Show>
        </Show>
      </div>
    </Modal>

    {/* Merge confirmation modal */}
    <Modal
      open={showMergeConfirm()}
      onClose={() => setShowMergeConfirm(false)}
      title="合并确认"
      footer={
        <>
          <Button variant="outline" onClick={() => { setShowMergeConfirm(false); setPendingBatchReq(null); setMergeCheckResult(null) }}>取消</Button>
          <Button variant="primary" onClick={confirmMergeAndSave}>确认合并保存</Button>
        </>
      }
    >
      <div class="space-y-3">
        <p class="text-sm text-content-secondary">
          以下报告将合并到已有报告中：
        </p>
        <div class="space-y-2">
          <For each={mergeCheckResult()?.merges ?? []}>
            {(merge) => {
              const report = pendingBatchReq()?.reports[merge.input_index]
              return (
                <div class="flex items-center gap-2 px-3 py-2 bg-surface-secondary rounded-lg text-sm">
                  <Badge variant="warning">合并</Badge>
                  <span class="font-medium">{report?.report_type ?? '未知'}</span>
                  <span class="text-content-secondary">{report?.report_date ?? ''}</span>
                  <svg class="w-3 h-3 text-content-tertiary shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                    <path stroke-linecap="round" stroke-linejoin="round" d="M13 7l5 5m0 0l-5 5m5-5H6" />
                  </svg>
                  <span class="text-content-tertiary">{merge.existing_report_type}</span>
                </div>
              )
            }}
          </For>
        </div>
      </div>
    </Modal>
    </>
  )
}