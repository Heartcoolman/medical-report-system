import { createSignal, createResource, createEffect, createMemo, Show, For, onCleanup } from 'solid-js'
import { A, useParams } from '@solidjs/router'
import { api } from '@/api/client'
import type { TrendPoint, TrendItemInfo } from '@/api/types'
import { cn } from '@/lib/utils'
import { pinyinMatch } from '@/lib/pinyin'
import { groupItems } from '@/lib/trend-helpers'
import { Button, Card, CardBody, Input, Badge, Spinner, Empty, Skeleton } from '@/components'
import { TrendChart } from '@/components/TrendChart2'
import { TrendTable } from '@/components/TrendTable2'
import { LlmInterpret } from '@/components/LlmInterpret'

// --- Main Component ---
export default function TrendAnalysis() {
  const params = useParams()

  // Item selector state
  const [searchText, setSearchText] = createSignal('')
  const [expandedGroups, setExpandedGroups] = createSignal<Set<string>>(new Set())
  const [selectedItem, setSelectedItem] = createSignal<TrendItemInfo | null>(null)

  // View toggle
  const [viewMode, setViewMode] = createSignal<'chart' | 'table'>('chart')

  // Chart hover
  const [hoveredIndex, setHoveredIndex] = createSignal<number | null>(null)

  // SVG container width (responsive)
  const [chartWidth, setChartWidth] = createSignal(600)
  const [chartContainerEl, setChartContainerEl] = createSignal<HTMLDivElement | null>(null)

  // Fetch items
  const [items] = createResource(
    () => params.id,
    (patientId) => api.trends.getItems(patientId),
  )

  // Fetch trend data when item selected
  const [trendData] = createResource(
    () => {
      const item = selectedItem()
      if (!item) return null
      return { patientId: params.id as string, itemName: item.item_name, reportType: item.report_type }
    },
    (args) => {
      if (!args) return Promise.resolve([] as TrendPoint[])
      return api.trends.getData(args.patientId, args.itemName, args.reportType)
    },
  )

  // Expand all groups when search is active, collapse all by default
  createEffect(() => {
    const data = items()
    if (!data) return
    if (searchText()) {
      const groups = filteredGroups()
      setExpandedGroups(new Set<string>(groups.keys()))
    } else {
      setExpandedGroups(new Set<string>())
    }
  })

  // ResizeObserver for chart container — reactive to chartContainerEl signal
  createEffect(() => {
    const el = chartContainerEl()
    if (!el) return
    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) {
        setChartWidth(entry.contentRect.width)
      }
    })
    observer.observe(el)
    onCleanup(() => observer.disconnect())
  })

  // Filtered and grouped items
  const filteredGroups = createMemo(() => {
    const data = items()
    if (!data) return new Map<string, TrendItemInfo[]>()
    const search = searchText().toLowerCase()
    const filtered = search
      ? data.filter((item) => pinyinMatch(item.item_name, search) || pinyinMatch(item.report_type, search))
      : data
    return groupItems(filtered)
  })

  // Check if values are all numeric
  const isNumeric = createMemo(() => {
    const data = trendData()
    if (!data || data.length === 0) return true
    return data.every((p) => !isNaN(parseFloat(p.value)))
  })

  // Toggle group expand/collapse
  const toggleGroup = (group: string) => {
    const current = expandedGroups()
    const next = new Set(current)
    if (next.has(group)) next.delete(group)
    else next.add(group)
    setExpandedGroups(next)
  }

  // AI interpret mode
  const [interpretMode, setInterpretMode] = createSignal<'none' | 'trend' | 'time'>('none')

  // Build interpret URL for current selection
  const interpretUrl = createMemo(() => {
    const item = selectedItem()
    if (!item) return ''
    const base = `/api/patients/${params.id}/trends/${encodeURIComponent(item.item_name)}`
    const rt = item.report_type ? `?report_type=${encodeURIComponent(item.report_type)}` : ''
    if (interpretMode() === 'trend') return `${base}/interpret${rt}`
    if (interpretMode() === 'time') return `${base}/interpret-time${rt}`
    return ''
  })

  // Select item
  const handleSelectItem = (item: TrendItemInfo) => {
    setSelectedItem(item)
    setViewMode('chart')
    setHoveredIndex(null)
    setInterpretMode('none')
  }

  return (
    <div class="space-y-4">
      {/* Back link */}
      <A href={`/patients/${params.id}`} class="inline-flex items-center gap-1 text-sm text-accent hover:underline">
        <svg class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
          <path stroke-linecap="round" stroke-linejoin="round" d="M15 19l-7-7 7-7" />
        </svg>
        返回患者详情
      </A>

      {/* Two-panel layout */}
      <div class="flex flex-col md:flex-row gap-4">
        {/* Left Panel: Item Selector */}
        <Card variant="outlined" class="md:w-64 shrink-0">
          <CardBody class="p-3 space-y-3">
            <Input
              placeholder="搜索检验项目..."
              value={searchText()}
              onInput={(e) => setSearchText(e.currentTarget.value)}
              leftIcon={
                <svg class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
                </svg>
              }
            />

            <Show when={items.loading} fallback={
              <Show when={items.error} fallback={
                <Show when={items() && items()!.length > 0} fallback={
                  <Empty title="暂无检验项目" description="该患者尚未上传检验报告" />
                }>
                  <div class="space-y-1 max-h-[60vh] overflow-y-auto">
                    <For each={[...filteredGroups().entries()]}>
                      {([groupName, groupItems]) => (
                        <div>
                          {/* Group header */}
                          <Button
                            type="button"
                            variant="ghost"
                            size="sm"
                            class="w-full justify-between px-2 text-content-secondary hover:bg-surface-secondary"
                            onClick={() => toggleGroup(groupName)}
                          >
                            <span class="truncate">{groupName}</span>
                            <div class="flex items-center gap-1.5 shrink-0">
                              <Badge variant="default">{groupItems.length}</Badge>
                              <svg
                                class={cn('h-4 w-4 transition-transform', expandedGroups().has(groupName) && 'rotate-180')}
                                fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"
                              >
                                <path stroke-linecap="round" stroke-linejoin="round" d="M19 9l-7 7-7-7" />
                              </svg>
                            </div>
                          </Button>

                          {/* Group items */}
                          <Show when={expandedGroups().has(groupName)}>
                            <div class="ml-1 space-y-0.5">
                              <For each={groupItems}>
                                {(item) => {
                                  const isSelected = () => {
                                    const sel = selectedItem()
                                    return sel !== null && sel.item_name === item.item_name && sel.report_type === item.report_type
                                  }
                                  return (
                                    <Button
                                      type="button"
                                      variant="ghost"
                                      size="sm"
                                      class={cn(
                                        'w-full justify-between px-2',
                                        isSelected()
                                          ? 'bg-accent-light text-accent font-medium hover:bg-accent-light'
                                          : 'text-content hover:bg-surface-secondary',
                                      )}
                                      onClick={() => handleSelectItem(item)}
                                    >
                                      <span class="truncate">{item.item_name}</span>
                                      <Badge variant={isSelected() ? 'accent' : 'default'} class="shrink-0 ml-1">
                                        {item.count}
                                      </Badge>
                                    </Button>
                                  )
                                }}
                              </For>
                            </div>
                          </Show>
                        </div>
                      )}
                    </For>
                  </div>
                </Show>
              }>
                <Empty title="加载失败" description={String(items.error?.message || items.error)} />
              </Show>
            }>
              <div class="space-y-2">
                <Skeleton variant="text" />
                <Skeleton variant="text" width="80%" />
                <Skeleton variant="text" width="60%" />
                <Skeleton variant="text" />
                <Skeleton variant="text" width="70%" />
              </div>
            </Show>
          </CardBody>
        </Card>

        {/* Right Panel: Chart / Table */}
        <Card variant="outlined" class="flex-1 min-w-0">
          <CardBody>
            <Show when={selectedItem()} fallback={
              <Empty
                title="请从左侧选择一个检验项目以查看趋势"
                description="选择检验项目后，这里将显示趋势图表"
              />
            }>
              {(item) => (
                <div class="space-y-4">
                  {/* Heading + view toggle */}
                  <div class="flex items-center justify-between flex-wrap gap-2">
                <h2 class="section-title">{item().item_name}</h2>
                    <div class="inline-flex p-0.5 rounded-xl bg-surface-secondary gap-0.5 text-sm">
                      <button
                        type="button"
                        class={`px-3 py-1.5 rounded-lg cursor-pointer transition-all duration-200 font-medium ${
                          viewMode() === 'chart'
                            ? 'bg-surface-elevated text-accent shadow-sm'
                            : 'text-content-secondary hover:text-content'
                        }`}
                        onClick={() => setViewMode('chart')}
                      >
                        图表
                      </button>
                      <button
                        type="button"
                        class={`px-3 py-1.5 rounded-lg cursor-pointer transition-all duration-200 font-medium ${
                          viewMode() === 'table'
                            ? 'bg-surface-elevated text-accent shadow-sm'
                            : 'text-content-secondary hover:text-content'
                        }`}
                        onClick={() => setViewMode('table')}
                      >
                        表格
                      </button>
                    </div>
                  </div>

                  {/* Content */}
                  <Show when={!trendData.loading} fallback={
                    <div class="flex items-center justify-center py-16">
                      <Spinner size="lg" />
                    </div>
                  }>
                    <Show when={!trendData.error} fallback={
                      <Empty title="加载失败" description={String(trendData.error?.message || trendData.error)} />
                    }>
                    <Show when={trendData() && trendData()!.length > 0} fallback={
                      <Empty title="暂无趋势数据" />
                    }>
                      <Show when={viewMode() === 'chart'} fallback={
                        <TrendTable data={trendData()!} />
                      }>
                        <Show when={isNumeric()} fallback={
                          <div class="text-center py-12 text-content-secondary text-sm">
                            无法绘制图表，请查看表格
                          </div>
                        }>
                          <TrendChart
                            data={trendData()!}
                            width={chartWidth()}
                            containerRef={(el) => { setChartContainerEl(el) }}
                            hoveredIndex={hoveredIndex()}
                            onHover={setHoveredIndex}
                          />
                        </Show>
                      </Show>
                    </Show>
                    </Show>
                  </Show>

                  {/* AI Interpretation buttons */}
                  <Show when={trendData() && trendData()!.length > 0}>
                    <div class="flex items-center gap-2 pt-2 border-t border-border">
                      <Button
                        variant={interpretMode() === 'trend' ? 'primary' : 'secondary'}
                        size="sm"
                        onClick={() => setInterpretMode(interpretMode() === 'trend' ? 'none' : 'trend')}
                      >
                        <svg class="w-3.5 h-3.5 mr-1" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                          <path stroke-linecap="round" stroke-linejoin="round" d="M9.813 15.904L9 18.75l-.813-2.846a4.5 4.5 0 00-3.09-3.09L2.25 12l2.846-.813a4.5 4.5 0 003.09-3.09L9 5.25l.813 2.846a4.5 4.5 0 003.09 3.09L15.75 12l-2.846.813a4.5 4.5 0 00-3.09 3.09z" />
                        </svg>
                        AI 趋势解读
                      </Button>
                      <Show when={trendData()!.length >= 2}>
                        <Button
                          variant={interpretMode() === 'time' ? 'primary' : 'secondary'}
                          size="sm"
                          onClick={() => setInterpretMode(interpretMode() === 'time' ? 'none' : 'time')}
                        >
                          <svg class="w-3.5 h-3.5 mr-1" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                            <path stroke-linecap="round" stroke-linejoin="round" d="M9.813 15.904L9 18.75l-.813-2.846a4.5 4.5 0 00-3.09-3.09L2.25 12l2.846-.813a4.5 4.5 0 003.09-3.09L9 5.25l.813 2.846a4.5 4.5 0 003.09 3.09L15.75 12l-2.846.813a4.5 4.5 0 00-3.09 3.09z" />
                          </svg>
                          AI 变化分析
                        </Button>
                      </Show>
                    </div>
                  </Show>

                  {/* AI Interpretation result */}
                  <Show when={interpretMode() !== 'none' && interpretUrl()}>
                    <LlmInterpret
                      url={interpretUrl()}
                      autoStart
                      buttonLabel={interpretMode() === 'trend' ? 'AI 趋势解读' : 'AI 变化分析'}
                    />
                  </Show>
                </div>
              )}
            </Show>
          </CardBody>
        </Card>
      </div>
    </div>
  )
}
