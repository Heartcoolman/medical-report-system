import type { ReportDetail, TestItem, ReportSummary } from '@/api/types'

export function exportReportCSV(report: ReportDetail) {
  const rows: string[][] = [
    ['报告类型', report.report_type],
    ['医院', report.hospital],
    ['报告日期', report.report_date],
    ['采样日期', report.sample_date || ''],
    [],
    ['检验项目', '结果', '单位', '参考范围', '状态'],
  ]
  for (const item of report.test_items) {
    rows.push([item.name, item.value, item.unit, item.reference_range, statusLabel(item.status)])
  }
  downloadCSV(rows, `报告_${report.report_type}_${report.report_date}.csv`)
}

export function exportAllReportsCSV(reports: ReportSummary[], patientName: string) {
  const rows: string[][] = [
    ['报告类型', '医院', '报告日期', '采样日期', '检验项数', '异常项数', '异常项目'],
  ]
  for (const r of reports) {
    rows.push([
      r.report_type,
      r.hospital,
      r.report_date,
      r.sample_date || '',
      String(r.item_count ?? 0),
      String(r.abnormal_count ?? 0),
      (r.abnormal_names ?? []).join('、'),
    ])
  }
  downloadCSV(rows, `${patientName}_全部报告.csv`)
}

export function exportTestItemsCSV(items: TestItem[], reportType: string, reportDate: string) {
  const rows: string[][] = [
    ['检验项目', '结果', '单位', '参考范围', '状态'],
  ]
  for (const item of items) {
    rows.push([item.name, item.value, item.unit, item.reference_range, statusLabel(item.status)])
  }
  downloadCSV(rows, `${reportType}_${reportDate}_检验项.csv`)
}

function statusLabel(status: string): string {
  const map: Record<string, string> = {
    critical_high: '危急偏高',
    high: '偏高',
    normal: '正常',
    low: '偏低',
    critical_low: '危急偏低',
  }
  return map[status] || status
}

function downloadCSV(rows: string[][], filename: string) {
  const BOM = '\uFEFF'
  const csv = rows.map(row =>
    row.map(cell => `"${String(cell).replace(/"/g, '""')}"`).join(',')
  ).join('\n')
  const blob = new Blob([BOM + csv], { type: 'text/csv;charset=utf-8' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = filename
  a.click()
  URL.revokeObjectURL(url)
}
