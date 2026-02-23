import { createMemo } from 'solid-js'
import type { TrendPoint } from '@/api/types'
import { TestItemStatusBadge, Table } from '@/components'
import type { TableColumn } from '@/components'

export interface TrendTableProps {
  data: TrendPoint[]
}

export function TrendTable(props: TrendTableProps) {
  const sorted = createMemo(() =>
    [...props.data].sort((a, b) => {
      const da = a.report_date || a.sample_date
      const db = b.report_date || b.sample_date
      return db.localeCompare(da)
    }),
  )

  const columns: TableColumn<TrendPoint>[] = [
    {
      key: 'date',
      title: '日期',
      render: (_value, point) => (
        <span class="text-content">{point.report_date || point.sample_date}</span>
      ),
    },
    {
      key: 'value',
      title: '数值',
      render: (value: string) => (
        <span class="font-medium text-content">{value}</span>
      ),
    },
    { key: 'unit', title: '单位' },
    {
      key: 'reference_range',
      title: '参考范围',
      render: (value: string | null | undefined) => value || '-',
    },
    {
      key: 'status',
      title: '状态',
      render: (value: string, point: TrendPoint) => <TestItemStatusBadge status={value} value={point.value} referenceRange={point.reference_range} />,
    },
  ]

  return (
    <Table<TrendPoint> columns={columns} data={sorted()} striped />
  )
}
