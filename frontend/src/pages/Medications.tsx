import { createSignal, createResource, Show, For } from 'solid-js'
import { useParams } from '@solidjs/router'
import { api } from '@/api/client'
import type { Medication, DetectedDrug } from '@/api/types'
import { cn } from '@/lib/utils'
import { Button, Card, CardBody, Badge, Modal, Input, useToast, Empty } from '@/components'

export default function Medications() {
  const params = useParams<{ id: string }>()
  const { toast } = useToast()

  // Detected drugs from expense records (primary)
  const [detectedDrugs] = createResource(() => params.id, (id) => api.medications.detectedDrugs(id))

  // Manual medications (secondary)
  const [meds, { refetch }] = createResource(() => params.id, (id) => api.medications.list(id))
  const [showAdd, setShowAdd] = createSignal(false)
  const [editMed, setEditMed] = createSignal<Medication | null>(null)
  const [deleteMedId, setDeleteMedId] = createSignal<string | null>(null)
  const [saving, setSaving] = createSignal(false)

  const [formName, setFormName] = createSignal('')
  const [formDosage, setFormDosage] = createSignal('')
  const [formFreq, setFormFreq] = createSignal('')
  const [formStart, setFormStart] = createSignal('')
  const [formEnd, setFormEnd] = createSignal('')
  const [formNote, setFormNote] = createSignal('')

  // Expand state for detected drug detail
  const [expandedDrug, setExpandedDrug] = createSignal<string | null>(null)

  // Tab: 'detected' | 'manual'
  const [activeTab, setActiveTab] = createSignal<'detected' | 'manual'>('detected')

  function resetForm() {
    setFormName(''); setFormDosage(''); setFormFreq('')
    setFormStart(''); setFormEnd(''); setFormNote('')
  }

  function openAdd() {
    resetForm()
    const now = new Date()
    setFormStart(`${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, '0')}-${String(now.getDate()).padStart(2, '0')}`)
    setShowAdd(true)
  }

  function openEdit(med: Medication) {
    setFormName(med.name); setFormDosage(med.dosage); setFormFreq(med.frequency)
    setFormStart(med.start_date); setFormEnd(med.end_date || ''); setFormNote(med.note)
    setEditMed(med)
  }

  async function handleSave() {
    if (!formName().trim()) { toast('error', '请输入药品名称'); return }
    setSaving(true)
    try {
      const editing = editMed()
      if (editing) {
        await api.medications.update(editing.id, {
          name: formName(), dosage: formDosage(), frequency: formFreq(),
          start_date: formStart(), end_date: formEnd() || undefined, note: formNote(),
        })
        toast('success', '用药记录已更新')
        setEditMed(null)
      } else {
        await api.medications.create(params.id, {
          name: formName(), dosage: formDosage(), frequency: formFreq(),
          start_date: formStart(), end_date: formEnd() || undefined, note: formNote(),
        })
        toast('success', '用药记录已添加')
        setShowAdd(false)
      }
      refetch()
    } catch (err: any) {
      toast('error', err.message || '操作失败')
    } finally {
      setSaving(false)
    }
  }

  async function handleToggleActive(med: Medication) {
    try {
      await api.medications.update(med.id, { active: !med.active })
      toast('success', med.active ? '已停用' : '已启用')
      refetch()
    } catch (err: any) {
      toast('error', err.message || '操作失败')
    }
  }

  async function handleDelete() {
    const id = deleteMedId()
    if (!id) return
    try {
      await api.medications.delete(id)
      toast('success', '用药记录已删除')
      setDeleteMedId(null)
      refetch()
    } catch (err: any) {
      toast('error', err.message || '删除失败')
    }
  }

  const activeMeds = () => (meds() ?? []).filter(m => m.active)
  const inactiveMeds = () => (meds() ?? []).filter(m => !m.active)

  const detectedCount = () => (detectedDrugs() ?? []).length
  const manualCount = () => (meds() ?? []).length
  const activeCount = () => activeMeds().length

  // Max occurrence for frequency bar scaling
  const maxOccurrence = () => {
    const drugs = detectedDrugs() ?? []
    if (drugs.length === 0) return 1
    return Math.max(...drugs.map(d => d.occurrence_count), 1)
  }

  return (
    <div class="page-shell">
      {/* Header */}
      <div class="flex items-center justify-between mb-5">
        <h1 class="page-title">用药管理</h1>
        <Button variant="primary" size="sm" onClick={openAdd}>
          <svg class="w-4 h-4 mr-1" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
            <path stroke-linecap="round" stroke-linejoin="round" d="M12 4v16m8-8H4" />
          </svg>
          添加用药
        </Button>
      </div>

      {/* Summary stats */}
      <div class="grid grid-cols-2 sm:grid-cols-4 gap-3 mb-6">
        <Card variant="outlined">
          <CardBody class="p-3 text-center">
            <div class="text-2xl font-bold text-accent">{detectedCount() + manualCount()}</div>
            <div class="text-xs text-content-tertiary mt-0.5">药品总数</div>
          </CardBody>
        </Card>
        <Card variant="outlined">
          <CardBody class="p-3 text-center">
            <div class="text-2xl font-bold text-success">{detectedCount()}</div>
            <div class="text-xs text-content-tertiary mt-0.5">自动识别</div>
          </CardBody>
        </Card>
        <Card variant="outlined">
          <CardBody class="p-3 text-center">
            <div class="text-2xl font-bold text-info">{activeCount()}</div>
            <div class="text-xs text-content-tertiary mt-0.5">使用中</div>
          </CardBody>
        </Card>
        <Card variant="outlined">
          <CardBody class="p-3 text-center">
            <div class="text-2xl font-bold text-content-secondary">{inactiveMeds().length}</div>
            <div class="text-xs text-content-tertiary mt-0.5">已停用</div>
          </CardBody>
        </Card>
      </div>

      {/* Tab switcher */}
      <div class="flex items-center gap-1 p-1 rounded-xl bg-surface-secondary mb-5">
        <button
          class={cn(
            'flex-1 px-4 py-2 rounded-lg text-sm font-medium transition-all cursor-pointer',
            activeTab() === 'detected'
              ? 'bg-surface text-content shadow-sm'
              : 'text-content-secondary hover:text-content',
          )}
          onClick={() => setActiveTab('detected')}
        >
          <span class="flex items-center justify-center gap-2">
            <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
              <path stroke-linecap="round" stroke-linejoin="round" d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2" />
            </svg>
            消费清单用药
            <Show when={detectedCount() > 0}>
              <Badge variant="accent">{detectedCount()}</Badge>
            </Show>
          </span>
        </button>
        <button
          class={cn(
            'flex-1 px-4 py-2 rounded-lg text-sm font-medium transition-all cursor-pointer',
            activeTab() === 'manual'
              ? 'bg-surface text-content shadow-sm'
              : 'text-content-secondary hover:text-content',
          )}
          onClick={() => setActiveTab('manual')}
        >
          <span class="flex items-center justify-center gap-2">
            <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
              <path stroke-linecap="round" stroke-linejoin="round" d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
            </svg>
            手动记录
            <Show when={manualCount() > 0}>
              <Badge variant="info">{manualCount()}</Badge>
            </Show>
          </span>
        </button>
      </div>

      {/* === Tab: Detected Drugs === */}
      <Show when={activeTab() === 'detected'}>
        <Show when={detectedDrugs.loading}>
          <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3">
            <For each={[1,2,3]}>{() =>
              <Card variant="outlined"><CardBody class="p-4"><div class="animate-pulse space-y-2"><div class="h-4 bg-surface-secondary rounded w-2/3" /><div class="h-3 bg-surface-secondary rounded w-1/2" /><div class="h-2 bg-surface-secondary rounded w-full mt-3" /></div></CardBody></Card>
            }</For>
          </div>
        </Show>

        <Show when={detectedDrugs() && !detectedDrugs.loading}>
          <Show when={detectedCount() > 0} fallback={
            <Empty title="暂无消费清单用药" description="上传消费清单后，药品信息将自动提取到此处" />
          }>
            <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3">
              <For each={detectedDrugs()!}>
                {(drug) => (
                  <DetectedDrugCard
                    drug={drug}
                    maxOccurrence={maxOccurrence()}
                    expanded={expandedDrug() === drug.name}
                    onToggle={() => setExpandedDrug(expandedDrug() === drug.name ? null : drug.name)}
                  />
                )}
              </For>
            </div>
            <p class="text-xs text-content-tertiary mt-4 text-center">
              共识别 {detectedCount()} 种药品 · 数据来源于消费清单药品类目
            </p>
          </Show>
        </Show>
      </Show>

      {/* === Tab: Manual Medications === */}
      <Show when={activeTab() === 'manual'}>
        <Show when={meds.loading}>
          <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3">
            <For each={[1,2,3]}>{() =>
              <Card variant="outlined"><CardBody class="p-4"><div class="animate-pulse space-y-2"><div class="h-4 bg-surface-secondary rounded w-2/3" /><div class="h-3 bg-surface-secondary rounded w-1/2" /></div></CardBody></Card>
            }</For>
          </div>
        </Show>

        <Show when={meds() && !meds.loading}>
          <Show when={manualCount() > 0} fallback={
            <Card variant="outlined">
              <CardBody class="py-12 text-center">
                <svg class="w-12 h-12 mx-auto text-content-tertiary/30 mb-3" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M19.5 14.25v-2.625a3.375 3.375 0 00-3.375-3.375h-1.5A1.125 1.125 0 0113.5 7.125v-1.5a3.375 3.375 0 00-3.375-3.375H8.25m3.75 9v6m3-3H9m1.5-12H5.625c-.621 0-1.125.504-1.125 1.125v17.25c0 .621.504 1.125 1.125 1.125h12.75c.621 0 1.125-.504 1.125-1.125V11.25a9 9 0 00-9-9z" />
                </svg>
                <p class="text-sm text-content-secondary mb-4">可手动添加消费清单中未包含的用药信息</p>
                <Button variant="outline" size="sm" onClick={openAdd}>添加用药</Button>
              </CardBody>
            </Card>
          }>
            {/* Active medications */}
            <Show when={activeMeds().length > 0}>
              <div class="flex items-center gap-2 mb-3">
                <div class="w-2 h-2 rounded-full bg-success" />
                <span class="text-xs font-semibold text-content-secondary uppercase tracking-wider">使用中 ({activeMeds().length})</span>
              </div>
              <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3 mb-6">
                <For each={activeMeds()}>
                  {(med) => (
                    <MedCard
                      med={med}
                      onEdit={() => openEdit(med)}
                      onToggle={() => handleToggleActive(med)}
                      onDelete={() => setDeleteMedId(med.id)}
                    />
                  )}
                </For>
              </div>
            </Show>

            {/* Inactive medications */}
            <Show when={inactiveMeds().length > 0}>
              <div class="flex items-center gap-2 mb-3">
                <div class="w-2 h-2 rounded-full bg-content-tertiary" />
                <span class="text-xs font-semibold text-content-secondary uppercase tracking-wider">已停用 ({inactiveMeds().length})</span>
              </div>
              <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3">
                <For each={inactiveMeds()}>
                  {(med) => (
                    <MedCard
                      med={med}
                      inactive
                      onEdit={() => openEdit(med)}
                      onToggle={() => handleToggleActive(med)}
                      onDelete={() => setDeleteMedId(med.id)}
                    />
                  )}
                </For>
              </div>
            </Show>
          </Show>
        </Show>
      </Show>

      {/* Add/Edit Modal */}
      <Modal
        open={showAdd() || !!editMed()}
        onClose={() => { setShowAdd(false); setEditMed(null) }}
        title={editMed() ? '编辑用药' : '添加用药'}
        footer={
          <>
            <Button variant="outline" onClick={() => { setShowAdd(false); setEditMed(null) }}>取消</Button>
            <Button variant="primary" loading={saving()} onClick={handleSave}>保存</Button>
          </>
        }
      >
        <div class="space-y-3">
          <Input label="药品名称" value={formName()} onInput={(e) => setFormName(e.currentTarget.value)} placeholder="如：恩替卡韦" />
          <div class="grid grid-cols-2 gap-3">
            <Input label="剂量" value={formDosage()} onInput={(e) => setFormDosage(e.currentTarget.value)} placeholder="如：0.5mg" />
            <Input label="频次" value={formFreq()} onInput={(e) => setFormFreq(e.currentTarget.value)} placeholder="如：每日一次" />
          </div>
          <div class="grid grid-cols-2 gap-3">
            <Input label="开始日期" type="date" value={formStart()} onInput={(e) => setFormStart(e.currentTarget.value)} />
            <Input label="结束日期" type="date" value={formEnd()} onInput={(e) => setFormEnd(e.currentTarget.value)} />
          </div>
          <Input label="备注" value={formNote()} onInput={(e) => setFormNote(e.currentTarget.value)} placeholder="可选" />
        </div>
      </Modal>

      {/* Delete Modal */}
      <Modal
        open={!!deleteMedId()}
        onClose={() => setDeleteMedId(null)}
        title="确认删除"
        footer={
          <>
            <Button variant="outline" onClick={() => setDeleteMedId(null)}>取消</Button>
            <Button variant="danger" onClick={handleDelete}>确认删除</Button>
          </>
        }
      >
        <p class="text-content-secondary">确定要删除这条用药记录吗？</p>
      </Modal>
    </div>
  )
}

/* ── Detected Drug Card ── */
function DetectedDrugCard(props: { drug: DetectedDrug; maxOccurrence: number; expanded: boolean; onToggle: () => void }) {
  const dateRange = () => {
    if (props.drug.first_date === props.drug.last_date) return props.drug.first_date
    return `${props.drug.first_date} ~ ${props.drug.last_date}`
  }

  const freqPercent = () => Math.round((props.drug.occurrence_count / props.maxOccurrence) * 100)

  const freqColor = () => {
    const p = freqPercent()
    if (p >= 70) return 'bg-accent'
    if (p >= 40) return 'bg-info'
    return 'bg-content-tertiary/40'
  }

  return (
    <Card variant="outlined" class="overflow-hidden hover:shadow-md hover:-translate-y-0.5 transition-all">
      <CardBody class="p-0">
        {/* Main info */}
        <button
          class="w-full p-4 pb-3 text-left cursor-pointer"
          onClick={props.onToggle}
        >
          <div class="flex items-start justify-between gap-2 mb-2">
            <div class="flex items-center gap-2">
              <div class="w-8 h-8 rounded-lg bg-accent/10 flex items-center justify-center shrink-0">
                <svg class="w-4 h-4 text-accent" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M19.428 15.428a2 2 0 00-1.022-.547l-2.387-.477a6 6 0 00-3.86.517l-.318.158a6 6 0 01-3.86.517L6.05 15.21a2 2 0 00-1.806.547M8 4h8l-1 1v5.172a2 2 0 00.586 1.414l5 5c1.26 1.26.367 3.414-1.415 3.414H4.828c-1.782 0-2.674-2.154-1.414-3.414l5-5A2 2 0 009 10.172V5L8 4z" />
                </svg>
              </div>
              <div class="min-w-0">
                <h3 class="text-sm font-semibold text-content truncate">{props.drug.name}</h3>
                <Show when={props.drug.typical_quantity}>
                  <span class="text-xs text-content-tertiary">{props.drug.typical_quantity}</span>
                </Show>
              </div>
            </div>
            <svg
              class={cn('w-4 h-4 text-content-tertiary shrink-0 transition-transform duration-200', props.expanded && 'rotate-180')}
              fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"
            >
              <path stroke-linecap="round" stroke-linejoin="round" d="M19 9l-7 7-7-7" />
            </svg>
          </div>

          {/* Frequency bar */}
          <div class="mb-2">
            <div class="flex items-center justify-between text-xs mb-1">
              <span class="text-content-secondary">使用频次</span>
              <span class="font-medium text-content">{props.drug.occurrence_count} 天</span>
            </div>
            <div class="w-full h-1.5 rounded-full bg-surface-secondary overflow-hidden">
              <div
                class={cn('h-full rounded-full transition-all duration-500', freqColor())}
                style={{ width: `${freqPercent()}%` }}
              />
            </div>
          </div>

          {/* Date range */}
          <div class="flex items-center gap-1.5 text-xs text-content-tertiary">
            <svg class="w-3.5 h-3.5 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
              <path stroke-linecap="round" stroke-linejoin="round" d="M8 7V3m8 4V3m-9 8h10M5 21h14a2 2 0 002-2V7a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" />
            </svg>
            <span>{dateRange()}</span>
          </div>
        </button>

        {/* Expanded: date list */}
        <Show when={props.expanded}>
          <div class="px-4 pb-4 pt-0">
            <div class="border-t border-border/50 pt-3">
              <p class="text-xs font-medium text-content-secondary mb-2">使用日期明细</p>
              <div class="flex flex-wrap gap-1.5">
                <For each={props.drug.dates}>
                  {(date) => (
                    <span class="inline-block px-2 py-1 text-xs rounded-md bg-accent/8 text-accent font-medium">
                      {date}
                    </span>
                  )}
                </For>
              </div>
            </div>
          </div>
        </Show>
      </CardBody>
    </Card>
  )
}

/* ── Manual Med Card ── */
function MedCard(props: { med: Medication; inactive?: boolean; onEdit: () => void; onToggle: () => void; onDelete: () => void }) {
  return (
    <Card variant="outlined" class={cn('overflow-hidden group hover:shadow-md hover:-translate-y-0.5 transition-all', props.inactive && 'opacity-50')}>
      <CardBody class="p-4">
        {/* Header */}
        <div class="flex items-start justify-between gap-2 mb-3">
          <div class="flex items-center gap-2 min-w-0">
            <div class={cn(
              'w-8 h-8 rounded-lg flex items-center justify-center shrink-0',
              props.med.active ? 'bg-success/10' : 'bg-surface-secondary',
            )}>
              <svg class={cn('w-4 h-4', props.med.active ? 'text-success' : 'text-content-tertiary')} fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                <path stroke-linecap="round" stroke-linejoin="round" d="M4.26 10.147a60.436 60.436 0 00-.491 6.347A48.627 48.627 0 0112 20.904a48.627 48.627 0 018.232-4.41 60.46 60.46 0 00-.491-6.347m-15.482 0a50.57 50.57 0 00-2.658-.813A59.905 59.905 0 0112 3.493a59.902 59.902 0 0110.399 5.84c-.896.248-1.783.52-2.658.814m-15.482 0A50.697 50.697 0 0112 13.489a50.702 50.702 0 017.74-3.342" />
              </svg>
            </div>
            <div class="min-w-0">
              <h3 class="text-sm font-semibold text-content truncate">{props.med.name}</h3>
              <Badge variant={props.med.active ? 'success' : 'warning'} class="mt-0.5">
                {props.med.active ? '使用中' : '已停用'}
              </Badge>
            </div>
          </div>
        </div>

        {/* Detail rows */}
        <div class="space-y-1.5 text-xs">
          <Show when={props.med.dosage}>
            <div class="flex items-center gap-2 text-content-secondary">
              <svg class="w-3.5 h-3.5 shrink-0 text-content-tertiary" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                <path stroke-linecap="round" stroke-linejoin="round" d="M19.5 14.25v-2.625a3.375 3.375 0 00-3.375-3.375h-1.5A1.125 1.125 0 0113.5 7.125v-1.5a3.375 3.375 0 00-3.375-3.375H8.25m0 12.75h7.5m-7.5 3H12M10.5 2.25H5.625c-.621 0-1.125.504-1.125 1.125v17.25c0 .621.504 1.125 1.125 1.125h12.75c.621 0 1.125-.504 1.125-1.125V11.25a9 9 0 00-9-9z" />
              </svg>
              <span>剂量 {props.med.dosage}</span>
            </div>
          </Show>
          <Show when={props.med.frequency}>
            <div class="flex items-center gap-2 text-content-secondary">
              <svg class="w-3.5 h-3.5 shrink-0 text-content-tertiary" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                <path stroke-linecap="round" stroke-linejoin="round" d="M12 6v6h4.5m4.5 0a9 9 0 11-18 0 9 9 0 0118 0z" />
              </svg>
              <span>{props.med.frequency}</span>
            </div>
          </Show>
          <div class="flex items-center gap-2 text-content-secondary">
            <svg class="w-3.5 h-3.5 shrink-0 text-content-tertiary" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
              <path stroke-linecap="round" stroke-linejoin="round" d="M8 7V3m8 4V3m-9 8h10M5 21h14a2 2 0 002-2V7a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" />
            </svg>
            <span>
              {props.med.start_date} 起
              <Show when={props.med.end_date}>{` 至 ${props.med.end_date}`}</Show>
            </span>
          </div>
          <Show when={props.med.note}>
            <div class="flex items-start gap-2 text-content-tertiary">
              <svg class="w-3.5 h-3.5 shrink-0 mt-0.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                <path stroke-linecap="round" stroke-linejoin="round" d="M7.5 8.25h9m-9 3H12m-9.75 1.51c0 1.6 1.123 2.994 2.707 3.227 1.087.16 2.185.283 3.293.369V21l4.076-4.076a1.526 1.526 0 011.037-.443 48.282 48.282 0 005.68-.494c1.584-.233 2.707-1.626 2.707-3.228V6.741c0-1.602-1.123-2.995-2.707-3.228A48.394 48.394 0 0012 3c-2.392 0-4.744.175-7.043.513C3.373 3.746 2.25 5.14 2.25 6.741v6.018z" />
              </svg>
              <span>{props.med.note}</span>
            </div>
          </Show>
        </div>

        {/* Actions - visible on hover */}
        <div class="flex items-center gap-1 mt-3 pt-3 border-t border-border/50 opacity-0 group-hover:opacity-100 transition-opacity">
          <Button variant="ghost" size="sm" onClick={props.onToggle}>
            {props.med.active ? '停用' : '启用'}
          </Button>
          <Button variant="ghost" size="sm" onClick={props.onEdit}>编辑</Button>
          <div class="ml-auto">
            <Button variant="ghost" size="sm" class="text-error hover:bg-error-light" onClick={props.onDelete}>删除</Button>
          </div>
        </div>
      </CardBody>
    </Card>
  )
}
