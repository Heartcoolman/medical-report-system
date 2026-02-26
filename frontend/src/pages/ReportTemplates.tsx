import { createSignal, Show, For } from 'solid-js'
import { useParams, useNavigate } from '@solidjs/router'
import { api } from '@/api/client'
import { Button, Card, CardBody, Badge, Input, useToast } from '@/components'

interface TemplateItem {
  name: string
  unit: string
  reference_range: string
}

interface ReportTemplate {
  name: string
  report_type: string
  items: TemplateItem[]
}

const TEMPLATES: ReportTemplate[] = [
  {
    name: '血常规',
    report_type: '血常规',
    items: [
      { name: '白细胞计数', unit: '10^9/L', reference_range: '3.5-9.5' },
      { name: '红细胞计数', unit: '10^12/L', reference_range: '4.3-5.8' },
      { name: '血红蛋白', unit: 'g/L', reference_range: '130-175' },
      { name: '血小板计数', unit: '10^9/L', reference_range: '125-350' },
      { name: '中性粒细胞百分比', unit: '%', reference_range: '40-75' },
      { name: '淋巴细胞百分比', unit: '%', reference_range: '20-50' },
      { name: '单核细胞百分比', unit: '%', reference_range: '3-10' },
      { name: '嗜酸性粒细胞百分比', unit: '%', reference_range: '0.4-8' },
      { name: '红细胞压积', unit: '%', reference_range: '40-50' },
      { name: '平均红细胞体积', unit: 'fL', reference_range: '82-100' },
    ],
  },
  {
    name: '肝功能',
    report_type: '肝功能',
    items: [
      { name: '丙氨酸氨基转移酶', unit: 'U/L', reference_range: '9-50' },
      { name: '天门冬氨酸氨基转移酶', unit: 'U/L', reference_range: '15-40' },
      { name: '总蛋白', unit: 'g/L', reference_range: '65-85' },
      { name: '白蛋白', unit: 'g/L', reference_range: '40-55' },
      { name: '球蛋白', unit: 'g/L', reference_range: '20-40' },
      { name: '总胆红素', unit: 'μmol/L', reference_range: '0-26' },
      { name: '直接胆红素', unit: 'μmol/L', reference_range: '0-8' },
      { name: '间接胆红素', unit: 'μmol/L', reference_range: '0-18' },
      { name: 'γ-谷氨酰转肽酶', unit: 'U/L', reference_range: '10-60' },
      { name: '碱性磷酸酶', unit: 'U/L', reference_range: '45-125' },
    ],
  },
  {
    name: '肾功能',
    report_type: '肾功能',
    items: [
      { name: '肌酐', unit: 'μmol/L', reference_range: '57-111' },
      { name: '尿素', unit: 'mmol/L', reference_range: '3.1-8.0' },
      { name: '尿酸', unit: 'μmol/L', reference_range: '208-428' },
      { name: '胱抑素C', unit: 'mg/L', reference_range: '0.51-1.09' },
      { name: 'β2-微球蛋白', unit: 'mg/L', reference_range: '0.7-1.8' },
    ],
  },
  {
    name: '血脂',
    report_type: '血脂四项',
    items: [
      { name: '总胆固醇', unit: 'mmol/L', reference_range: '0-5.18' },
      { name: '甘油三酯', unit: 'mmol/L', reference_range: '0-1.7' },
      { name: '高密度脂蛋白', unit: 'mmol/L', reference_range: '1.04-1.55' },
      { name: '低密度脂蛋白', unit: 'mmol/L', reference_range: '0-3.37' },
    ],
  },
  {
    name: '甲状腺功能',
    report_type: '甲功五项',
    items: [
      { name: '促甲状腺激素', unit: 'mIU/L', reference_range: '0.27-4.2' },
      { name: '游离三碘甲状腺原氨酸', unit: 'pmol/L', reference_range: '3.1-6.8' },
      { name: '游离甲状腺素', unit: 'pmol/L', reference_range: '12.0-22.0' },
      { name: '总三碘甲状腺原氨酸', unit: 'nmol/L', reference_range: '1.3-3.1' },
      { name: '总甲状腺素', unit: 'nmol/L', reference_range: '66-181' },
    ],
  },
  {
    name: '凝血功能',
    report_type: '凝血四项',
    items: [
      { name: '凝血酶原时间', unit: 's', reference_range: '9.4-12.5' },
      { name: '活化部分凝血活酶时间', unit: 's', reference_range: '25.1-36.5' },
      { name: '凝血酶时间', unit: 's', reference_range: '14-21' },
      { name: '纤维蛋白原', unit: 'g/L', reference_range: '2-4' },
      { name: '国际标准化比值', unit: '', reference_range: '0.8-1.2' },
      { name: 'D-二聚体', unit: 'mg/L FEU', reference_range: '0-0.55' },
    ],
  },
  {
    name: '乙肝五项',
    report_type: '乙肝五项',
    items: [
      { name: '乙肝表面抗原', unit: 'IU/mL', reference_range: '0-0.05' },
      { name: '乙肝表面抗体', unit: 'mIU/mL', reference_range: '0-10' },
      { name: '乙肝e抗原', unit: 'S/CO', reference_range: '0-1' },
      { name: '乙肝e抗体', unit: 'S/CO', reference_range: '0-1' },
      { name: '乙肝核心抗体', unit: 'S/CO', reference_range: '0-1' },
    ],
  },
  {
    name: '尿常规',
    report_type: '尿常规',
    items: [
      { name: '尿蛋白', unit: '', reference_range: '阴性' },
      { name: '尿糖', unit: '', reference_range: '阴性' },
      { name: '尿隐血', unit: '', reference_range: '阴性' },
      { name: '尿白细胞', unit: '/μL', reference_range: '0-25' },
      { name: '尿红细胞', unit: '/μL', reference_range: '0-25' },
      { name: '尿比重', unit: '', reference_range: '1.005-1.030' },
      { name: '尿pH', unit: '', reference_range: '4.5-8.0' },
    ],
  },
]

export default function ReportTemplatesPage() {
  const params = useParams<{ id: string }>()
  const navigate = useNavigate()
  const { toast } = useToast()

  const [selected, setSelected] = createSignal<ReportTemplate | null>(null)
  const [values, setValues] = createSignal<Record<number, string>>({})
  const [hospital, setHospital] = createSignal('')
  const todayStr = () => {
    const d = new Date()
    return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`
  }
  const [reportDate, setReportDate] = createSignal(todayStr())
  const [saving, setSaving] = createSignal(false)

  function selectTemplate(t: ReportTemplate) {
    setSelected(t)
    setValues({})
  }

  function updateValue(idx: number, val: string) {
    setValues(prev => ({ ...prev, [idx]: val }))
  }

  function determineStatus(value: string, refRange: string): string {
    const num = parseFloat(value)
    if (isNaN(num)) return 'normal'
    const match = refRange.match(/([\d.]+)-([\d.]+)/)
    if (!match) return 'normal'
    const low = parseFloat(match[1])
    const high = parseFloat(match[2])
    if (num > high * 2) return 'critical_high'
    if (num > high) return 'high'
    if (num < low * 0.5 && low > 0) return 'critical_low'
    if (num < low) return 'low'
    return 'normal'
  }

  async function handleSave() {
    const tpl = selected()
    if (!tpl) return
    if (!hospital().trim()) { toast('error', '请输入医院名称'); return }

    const filledItems = tpl.items
      .map((item, idx) => ({ ...item, value: values()[idx] || '' }))
      .filter(item => item.value.trim())

    if (filledItems.length === 0) { toast('error', '请至少填写一个检验项目'); return }

    setSaving(true)
    try {
      const report = await api.reports.create(params.id, {
        report_type: tpl.report_type,
        hospital: hospital(),
        report_date: reportDate(),
        sample_date: reportDate(),
      })

      for (const item of filledItems) {
        await api.testItems.create({
          report_id: report.id,
          name: item.name,
          value: item.value,
          unit: item.unit,
          reference_range: item.reference_range,
          status: determineStatus(item.value, item.reference_range) as any,
        })
      }

      toast('success', '报告已创建')
      navigate(`/reports/${report.id}`)
    } catch (err: any) {
      toast('error', err.message || '创建失败')
    } finally {
      setSaving(false)
    }
  }

  return (
    <div class="page-shell">
      <div class="max-w-3xl mx-auto">
        <h1 class="page-title mb-6">快捷录入</h1>

        <Show when={!selected()} fallback={
          <div>
            <div class="flex items-center justify-between mb-4">
              <div class="flex items-center gap-2">
                <Button variant="ghost" size="sm" onClick={() => setSelected(null)}>
                  <svg class="w-4 h-4 mr-1" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                    <path stroke-linecap="round" stroke-linejoin="round" d="M15 19l-7-7 7-7" />
                  </svg>
                  返回
                </Button>
                <h2 class="text-lg font-semibold text-content">{selected()!.name}</h2>
              </div>
            </div>

            <div class="grid grid-cols-1 sm:grid-cols-2 gap-3 mb-4">
              <Input label="医院" value={hospital()} onInput={(e) => setHospital(e.currentTarget.value)} placeholder="请输入医院名称" />
              <Input label="报告日期" type="date" value={reportDate()} onInput={(e) => setReportDate(e.currentTarget.value)} />
            </div>

            <Card variant="outlined">
              <CardBody class="p-0">
                <table class="w-full text-sm">
                  <thead>
                    <tr class="border-b border-border bg-surface-secondary/50">
                      <th class="px-4 py-2 text-left text-xs font-medium text-content-secondary">项目</th>
                      <th class="px-4 py-2 text-left text-xs font-medium text-content-secondary">结果</th>
                      <th class="px-4 py-2 text-left text-xs font-medium text-content-secondary">单位</th>
                      <th class="px-4 py-2 text-left text-xs font-medium text-content-secondary">参考范围</th>
                    </tr>
                  </thead>
                  <tbody>
                    <For each={selected()!.items}>
                      {(item, idx) => (
                        <tr class="border-b border-border/50">
                          <td class="px-4 py-2 font-medium text-content">{item.name}</td>
                          <td class="px-4 py-1">
                            <input
                              type="text"
                              class="w-full px-2 py-1 text-sm border border-border rounded bg-surface text-content focus:outline-none focus:ring-1 focus:ring-accent"
                              value={values()[idx()] || ''}
                              onInput={(e) => updateValue(idx(), e.currentTarget.value)}
                              placeholder="输入结果"
                            />
                          </td>
                          <td class="px-4 py-2 text-content-secondary">{item.unit}</td>
                          <td class="px-4 py-2 text-content-tertiary">{item.reference_range}</td>
                        </tr>
                      )}
                    </For>
                  </tbody>
                </table>
              </CardBody>
            </Card>

            <div class="flex justify-end mt-4">
              <Button variant="primary" loading={saving()} onClick={handleSave}>保存报告</Button>
            </div>
          </div>
        }>
          <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3">
            <For each={TEMPLATES}>
              {(tpl) => (
                <Card variant="elevated" interactive class="cursor-pointer hover:-translate-y-0.5 hover:shadow-lg transition-all" onClick={() => selectTemplate(tpl)}>
                  <CardBody class="p-4">
                    <h3 class="text-base font-semibold text-content mb-1">{tpl.name}</h3>
                    <p class="text-xs text-content-secondary">{tpl.items.length} 个检验项目</p>
                    <div class="flex flex-wrap gap-1 mt-2">
                      <For each={tpl.items.slice(0, 4)}>
                        {(item) => <Badge variant="info">{item.name}</Badge>}
                      </For>
                      <Show when={tpl.items.length > 4}>
                        <Badge variant="info">+{tpl.items.length - 4}</Badge>
                      </Show>
                    </div>
                  </CardBody>
                </Card>
              )}
            </For>
          </div>
        </Show>
      </div>
    </div>
  )
}
