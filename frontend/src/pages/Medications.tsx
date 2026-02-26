import { createSignal, createResource, Show, For } from 'solid-js'
import { useParams } from '@solidjs/router'
import { api } from '@/api/client'
import type { Medication, DetectedDrug } from '@/api/types'
import { Button, Card, CardBody, Badge, Modal, Input, useToast, Spinner, Empty } from '@/components'

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

  return (
    <div class="page-shell">
      <div class="max-w-5xl mx-auto">
        <div class="flex items-center justify-between mb-6">
          <h1 class="page-title">用药管理</h1>
          <Button variant="outline" size="sm" onClick={openAdd}>手动添加</Button>
        </div>

        {/* === Section 1: Detected Drugs from Expense Records (Primary) === */}
        <div class="mb-8">
          <div class="flex items-center gap-2 mb-3">
            <h2 class="section-title">消费清单用药</h2>
            <Badge variant="accent">自动识别</Badge>
          </div>

          <Show when={detectedDrugs.loading}>
            <div class="flex justify-center py-8"><Spinner size="md" variant="orbital" /></div>
          </Show>

          <Show when={detectedDrugs() && !detectedDrugs.loading}>
            <Show when={(detectedDrugs() ?? []).length > 0} fallback={
              <Empty title="暂无消费清单用药" description="上传消费清单后，药品信息将自动提取到此处" />
            }>
              <div class="space-y-2">
                <For each={detectedDrugs()!}>
                  {(drug) => <DetectedDrugCard drug={drug} expanded={expandedDrug() === drug.name} onToggle={() => setExpandedDrug(expandedDrug() === drug.name ? null : drug.name)} />}
                </For>
              </div>
              <p class="text-xs text-content-tertiary mt-3">
                共识别 {detectedDrugs()!.length} 种药品，数据来源于消费清单中的药品类目
              </p>
            </Show>
          </Show>
        </div>

        {/* === Section 2: Manual Medications (Secondary) === */}
        <div>
          <div class="flex items-center gap-2 mb-3">
            <h2 class="section-title">手动记录</h2>
            <Badge variant="info">自定义</Badge>
          </div>

          <Show when={meds.loading}>
            <div class="flex justify-center py-8"><Spinner size="md" variant="orbital" /></div>
          </Show>

          <Show when={meds() && !meds.loading}>
            <Show when={(meds() ?? []).length > 0} fallback={
              <Card variant="outlined">
                <CardBody class="py-6 text-center">
                  <p class="text-sm text-content-secondary mb-3">可手动添加消费清单中未包含的用药信息</p>
                  <Button variant="outline" size="sm" onClick={openAdd}>添加用药</Button>
                </CardBody>
              </Card>
            }>
              <Show when={activeMeds().length > 0}>
                <p class="micro-title mb-2">当前用药</p>
                <div class="space-y-2 mb-4">
                  <For each={activeMeds()}>
                    {(med) => <MedCard med={med} onEdit={() => openEdit(med)} onToggle={() => handleToggleActive(med)} onDelete={() => setDeleteMedId(med.id)} />}
                  </For>
                </div>
              </Show>

              <Show when={inactiveMeds().length > 0}>
                <p class="micro-title mb-2">已停用</p>
                <div class="space-y-2 opacity-60">
                  <For each={inactiveMeds()}>
                    {(med) => <MedCard med={med} onEdit={() => openEdit(med)} onToggle={() => handleToggleActive(med)} onDelete={() => setDeleteMedId(med.id)} />}
                  </For>
                </div>
              </Show>
            </Show>
          </Show>
        </div>

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
            <Input label="剂量" value={formDosage()} onInput={(e) => setFormDosage(e.currentTarget.value)} placeholder="如：0.5mg" />
            <Input label="频次" value={formFreq()} onInput={(e) => setFormFreq(e.currentTarget.value)} placeholder="如：每日一次" />
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
    </div>
  )
}

function DetectedDrugCard(props: { drug: DetectedDrug; expanded: boolean; onToggle: () => void }) {
  const dateRange = () => {
    if (props.drug.first_date === props.drug.last_date) return props.drug.first_date
    return `${props.drug.first_date} ~ ${props.drug.last_date}`
  }

  return (
    <Card variant="outlined" class="overflow-hidden">
      <CardBody class="p-3">
        <button
          class="w-full flex items-center justify-between gap-3 text-left cursor-pointer"
          onClick={props.onToggle}
        >
          <div class="min-w-0 flex-1">
            <div class="flex items-center gap-2 flex-wrap">
              <span class="text-sm font-semibold text-content">{props.drug.name}</span>
              <Show when={props.drug.typical_quantity}>
                <Badge variant="info">{props.drug.typical_quantity}</Badge>
              </Show>
            </div>
            <div class="text-xs text-content-secondary mt-0.5">
              {dateRange()} · 出现 {props.drug.occurrence_count} 天
            </div>
          </div>
          <svg
            class={`w-4 h-4 text-content-tertiary shrink-0 transition-transform duration-200 ${props.expanded ? 'rotate-180' : ''}`}
            fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"
          >
            <path stroke-linecap="round" stroke-linejoin="round" d="M19 9l-7 7-7-7" />
          </svg>
        </button>

        <Show when={props.expanded}>
          <div class="mt-2 pt-2 border-t border-border/50">
            <p class="micro-title mb-1.5">使用日期</p>
            <div class="flex flex-wrap gap-1">
              <For each={props.drug.dates}>
                {(date) => (
                  <span class="inline-block px-2 py-0.5 text-xs rounded-md bg-surface-secondary text-content-secondary">
                    {date}
                  </span>
                )}
              </For>
            </div>
          </div>
        </Show>
      </CardBody>
    </Card>
  )
}

function MedCard(props: { med: Medication; onEdit: () => void; onToggle: () => void; onDelete: () => void }) {
  return (
    <Card variant="outlined">
      <CardBody class="p-3 flex items-center justify-between gap-3">
        <div class="min-w-0 flex-1">
          <div class="flex items-center gap-2">
            <span class="text-sm font-semibold text-content">{props.med.name}</span>
            <Badge variant={props.med.active ? 'success' : 'warning'}>
              {props.med.active ? '使用中' : '已停用'}
            </Badge>
          </div>
          <div class="text-xs text-content-secondary mt-0.5">
            {props.med.dosage} · {props.med.frequency} · {props.med.start_date} 起
            <Show when={props.med.end_date}> 至 {props.med.end_date}</Show>
          </div>
          <Show when={props.med.note}>
            <div class="text-xs text-content-tertiary mt-0.5">{props.med.note}</div>
          </Show>
        </div>
        <div class="flex items-center gap-1 shrink-0">
          <Button variant="ghost" size="sm" onClick={props.onToggle}>
            {props.med.active ? '停用' : '启用'}
          </Button>
          <Button variant="ghost" size="sm" onClick={props.onEdit}>编辑</Button>
          <Button variant="ghost" size="sm" class="text-error" onClick={props.onDelete}>删除</Button>
        </div>
      </CardBody>
    </Card>
  )
}
