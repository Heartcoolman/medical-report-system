import { createSignal, createMemo, createEffect, onCleanup, Show, For } from 'solid-js'
import type { TemperatureRecord } from '@/api/types'

// --- Constants ---
const CHART_PADDING = { top: 20, right: 20, bottom: 44, left: 50 }
const CHART_HEIGHT = 240
const Y_MIN = 35
const Y_MAX = 42
const FEVER_LINE = 37.3
const TOOLTIP_WIDTH = 200
const SMOOTH_CONTROL_RATIO = 0.35

// Location color palette
const LOCATION_COLORS: Record<string, string> = {
  '左腋下': '#3b82f6', // blue
  '右腋下': '#10b981', // green
  '口腔':   '#f59e0b', // amber
  '耳温':   '#8b5cf6', // purple
  '额温':   '#06b6d4', // cyan
  '肛温':   '#ef4444', // red
  '':       'var(--accent)',
}
const FALLBACK_COLORS = ['#ec4899', '#6366f1', '#14b8a6', '#f97316', '#a855f7', '#64748b']

function getLocationColor(loc: string, idx: number): string {
  return LOCATION_COLORS[loc] ?? FALLBACK_COLORS[idx % FALLBACK_COLORS.length]
}

function locationLabel(loc: string): string {
  return loc || '未标注'
}

// --- Helpers ---

function temperatureColor(value: number): string {
  if (value < 36.0) return 'var(--info)'       // 低温
  if (value <= 37.3) return 'var(--success)'    // 正常
  if (value <= 38.0) return 'var(--warning, #f59e0b)' // 低热
  if (value <= 39.0) return '#f97316'           // 中热
  return 'var(--error)'                         // 高热
}

function temperatureLabel(value: number): string {
  if (value < 36.0) return '低温'
  if (value <= 37.3) return '正常'
  if (value <= 38.0) return '低热'
  if (value <= 39.0) return '中热'
  return '高热'
}

function formatDateTime(recorded_at: string): string {
  // "YYYY-MM-DD HH:MM" → "HH:MM"
  const parts = recorded_at.split(' ')
  if (parts.length !== 2) return recorded_at
  return parts[1]
}

function parseRecordedAt(recorded_at: string): number {
  // "YYYY-MM-DD HH:MM" → timestamp ms
  return new Date(recorded_at.replace(' ', 'T') + ':00').getTime()
}

interface LocationSeries {
  location: string
  color: string
  records: TemperatureRecord[]
  indices: number[] // indices into original data array
}

// --- Component ---

export interface TemperatureChartProps {
  data: TemperatureRecord[]
  onDelete?: (id: string) => void
}

export function TemperatureChart(props: TemperatureChartProps) {
  const [hoveredIndex, setHoveredIndex] = createSignal<number | null>(null)
  const [chartWidth, setChartWidth] = createSignal(500)
  const [hiddenLocations, setHiddenLocations] = createSignal<Set<string>>(new Set())
  let containerRef: HTMLDivElement | undefined
  let hideTimer: number | undefined

  function hoverIn(i: number) {
    window.clearTimeout(hideTimer)
    setHoveredIndex(i)
  }
  function hoverOut() {
    hideTimer = window.setTimeout(() => setHoveredIndex(null), 150)
  }
  onCleanup(() => window.clearTimeout(hideTimer))

  // ResizeObserver
  createEffect(() => {
    if (!containerRef) return
    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) {
        setChartWidth(entry.contentRect.width)
      }
    })
    observer.observe(containerRef)
    onCleanup(() => observer.disconnect())
  })

  // Group data by location
  const locationSeries = createMemo((): LocationSeries[] => {
    const map = new Map<string, { records: TemperatureRecord[]; indices: number[] }>()
    props.data.forEach((r, i) => {
      const loc = r.location || ''
      if (!map.has(loc)) map.set(loc, { records: [], indices: [] })
      const entry = map.get(loc)!
      entry.records.push(r)
      entry.indices.push(i)
    })
    const result: LocationSeries[] = []
    let ci = 0
    for (const [loc, { records, indices }] of map) {
      result.push({ location: loc, color: getLocationColor(loc, ci), records, indices })
      ci++
    }
    return result
  })

  const hasMultipleLocations = createMemo(() => locationSeries().length > 1)

  // Visible data (filtered by hiddenLocations)
  const visibleData = createMemo(() => {
    const hidden = hiddenLocations()
    if (hidden.size === 0) return props.data
    return props.data.filter(r => !hidden.has(r.location || ''))
  })

  function toggleLocation(loc: string) {
    const s = new Set(hiddenLocations())
    if (s.has(loc)) s.delete(loc); else s.add(loc)
    setHiddenLocations(s)
  }

  const effectiveWidth = () => Math.max(chartWidth(), 200)
  const plotWidth = () => Math.max(effectiveWidth() - CHART_PADDING.left - CHART_PADDING.right, 50)
  const plotHeight = () => CHART_HEIGHT - CHART_PADDING.top - CHART_PADDING.bottom

  // Compute nice y range including all visible data
  const yRange = createMemo(() => {
    let min = Y_MIN
    let max = Y_MAX
    for (const r of visibleData()) {
      if (r.value < min) min = Math.floor(r.value)
      if (r.value > max) max = Math.ceil(r.value)
    }
    return { min, max }
  })

  // Time-proportional x scale based on all data (not just visible)
  const MIN_TIME_SPAN = 24 * 60 * 60 * 1000
  const timeRange = createMemo(() => {
    if (props.data.length <= 1) return null
    const times = props.data.map(r => parseRecordedAt(r.recorded_at))
    let min = Math.min(...times)
    let max = Math.max(...times)
    const span = max - min
    if (span < MIN_TIME_SPAN) {
      const center = (min + max) / 2
      min = center - MIN_TIME_SPAN / 2
      max = center + MIN_TIME_SPAN / 2
    }
    return { min, max }
  })

  // Compute jitter offset for same-timestamp records
  const getJitterOffset = (dataIdx: number) => {
    const record = props.data[dataIdx]
    const sameTimeRecords = props.data.filter(r => r.recorded_at === record.recorded_at)
    if (sameTimeRecords.length <= 1) return 0
    const index = sameTimeRecords.findIndex(r => r.id === record.id)
    const totalWidth = Math.min(sameTimeRecords.length * 8, 24)
    return (index - (sameTimeRecords.length - 1) / 2) * (totalWidth / (sameTimeRecords.length - 1 || 1))
  }

  const xScaleByIndex = (i: number) => {
    const tr = timeRange()
    if (!tr || tr.max === tr.min) return CHART_PADDING.left + plotWidth() / 2
    const t = parseRecordedAt(props.data[i].recorded_at)
    const baseX = CHART_PADDING.left + ((t - tr.min) / (tr.max - tr.min)) * plotWidth()
    return baseX + getJitterOffset(i)
  }

  const xScaleByTime = (recorded_at: string) => {
    const tr = timeRange()
    if (!tr || tr.max === tr.min) return CHART_PADDING.left + plotWidth() / 2
    const t = parseRecordedAt(recorded_at)
    return CHART_PADDING.left + ((t - tr.min) / (tr.max - tr.min)) * plotWidth()
  }

  const yScale = (val: number) => {
    const { min, max } = yRange()
    const ratio = (val - min) / (max - min)
    return CHART_PADDING.top + plotHeight() * (1 - ratio)
  }

  // Build smooth path for a series of records using their data indices
  function buildLinePath(indices: number[]): string {
    if (indices.length === 0) return ''
    const pts = indices.map(i => ({ x: xScaleByIndex(i), y: yScale(props.data[i].value) }))
    if (pts.length === 1) return `M ${pts[0].x} ${pts[0].y}`
    let path = `M ${pts[0].x} ${pts[0].y}`
    for (let i = 1; i < pts.length; i++) {
      const prev = pts[i - 1]
      const curr = pts[i]
      const dx = curr.x - prev.x
      const cp1X = prev.x + dx * SMOOTH_CONTROL_RATIO
      const cp2X = curr.x - dx * SMOOTH_CONTROL_RATIO
      path += ` C ${cp1X} ${prev.y} ${cp2X} ${curr.y} ${curr.x} ${curr.y}`
    }
    return path
  }

  // Single-series area path (only used when single location)
  const areaPath = createMemo(() => {
    if (hasMultipleLocations()) return ''
    const records = visibleData()
    if (records.length === 0) return ''
    const pts = records.map(r => ({ x: xScaleByTime(r.recorded_at), y: yScale(r.value) }))
    const baseY = CHART_PADDING.top + plotHeight()
    if (pts.length === 1) {
      return `M ${pts[0].x} ${baseY} L ${pts[0].x} ${pts[0].y} L ${pts[0].x} ${baseY} Z`
    }
    let path = `M ${pts[0].x} ${baseY} L ${pts[0].x} ${pts[0].y}`
    for (let i = 1; i < pts.length; i++) {
      const prev = pts[i - 1]
      const curr = pts[i]
      const dx = curr.x - prev.x
      const cp1X = prev.x + dx * SMOOTH_CONTROL_RATIO
      const cp2X = curr.x - dx * SMOOTH_CONTROL_RATIO
      path += ` C ${cp1X} ${prev.y} ${cp2X} ${curr.y} ${curr.x} ${curr.y}`
    }
    path += ` L ${pts[pts.length - 1].x} ${baseY} Z`
    return path
  })

  // Grid lines: every 1°C
  const gridLines = createMemo(() => {
    const { min, max } = yRange()
    const lines: { y: number; label: string }[] = []
    for (let v = Math.ceil(min); v <= Math.floor(max) + 0.01; v += 1) {
      lines.push({ y: yScale(v), label: v.toFixed(0) })
    }
    return lines
  })

  // Fever reference band: 37.3°C line
  const feverLineY = createMemo(() => yScale(FEVER_LINE))

  // X-axis labels (deduplicated by timestamp, using base position without jitter)
  const dateLabels = createMemo(() => {
    const seen = new Set<string>()
    const unique: { x: number; label: string }[] = []
    for (const r of props.data) {
      if (seen.has(r.recorded_at)) continue
      seen.add(r.recorded_at)
      unique.push({ x: xScaleByTime(r.recorded_at), label: formatDateTime(r.recorded_at) })
    }
    if (unique.length <= 1) return unique
    const minSpacing = 50
    const totalWidth = plotWidth()
    const step = Math.max(1, Math.ceil((unique.length * minSpacing) / totalWidth))
    return unique.filter((_, i) => i % step === 0 || i === unique.length - 1)
  })

  return (
    <div class="relative" ref={containerRef}>
      {/* Legend (only when multiple locations) */}
      <Show when={hasMultipleLocations()}>
        <div class="flex flex-wrap items-center gap-2 mb-2">
          <For each={locationSeries()}>
            {(series) => {
              const isHidden = () => hiddenLocations().has(series.location)
              return (
                <button
                  class="flex items-center gap-1.5 px-2.5 py-1 rounded-lg text-xs font-medium transition-all cursor-pointer border"
                  classList={{
                    'border-border/50 bg-surface-secondary/50 text-content-tertiary line-through': isHidden(),
                    'border-transparent bg-surface text-content shadow-sm': !isHidden(),
                  }}
                  onClick={() => toggleLocation(series.location)}
                >
                  <span
                    class="w-2.5 h-2.5 rounded-full shrink-0"
                    classList={{ 'opacity-30': isHidden() }}
                    style={{ background: series.color }}
                  />
                  {locationLabel(series.location)} ({series.records.length})
                </button>
              )
            }}
          </For>
        </div>
      </Show>

      <svg
        viewBox={`0 0 ${effectiveWidth()} ${CHART_HEIGHT}`}
        class="w-full"
        style={{ height: `${CHART_HEIGHT}px` }}
      >
        <rect
          x={CHART_PADDING.left - 2}
          y={CHART_PADDING.top - 2}
          width={Math.max(plotWidth() + 4, 1)}
          height={Math.max(plotHeight() + 4, 1)}
          rx="12"
          fill="var(--surface-secondary)"
          fill-opacity="0.38"
          stroke="var(--border)"
          stroke-opacity="0.45"
        />

        <defs>
          <linearGradient id="tempLineGradient" x1="0%" y1="0%" x2="0%" y2="100%">
            <stop offset="0%" stop-color="var(--accent)" stop-opacity="0.95" />
            <stop offset="100%" stop-color="var(--accent)" stop-opacity="0.35" />
          </linearGradient>
          <linearGradient id="tempAreaGradient" x1="0%" y1="0%" x2="0%" y2="100%">
            <stop offset="0%" stop-color="var(--accent)" stop-opacity="0.24" />
            <stop offset="100%" stop-color="var(--accent)" stop-opacity="0.02" />
          </linearGradient>
          <style>{`
            @keyframes tempLineDraw {
              from { stroke-dashoffset: 2000; }
              to   { stroke-dashoffset: 0; }
            }
            @keyframes tempAreaFade {
              from { opacity: 0; }
              to   { opacity: 1; }
            }
            @keyframes tempPointPop {
              0%   { transform: scale(0); opacity: 0; }
              60%  { transform: scale(1.2); opacity: 1; }
              100% { transform: scale(1); opacity: 1; }
            }
            .temp-line-anim {
              stroke-dasharray: 2000;
              stroke-dashoffset: 2000;
              animation: tempLineDraw 0.8s ease-out forwards;
            }
            .temp-area-anim {
              opacity: 0;
              animation: tempAreaFade 0.6s ease-out 0.3s forwards;
            }
            .temp-point-anim {
              transform: scale(0);
              transform-box: fill-box;
              transform-origin: center;
              opacity: 0;
              animation: tempPointPop 0.4s ease-out forwards;
            }
          `}</style>
        </defs>

        {/* Normal range band (below 37.3) */}
        <rect
          x={CHART_PADDING.left}
          y={feverLineY()}
          width={plotWidth()}
          height={yScale(yRange().min) - feverLineY()}
          fill="var(--success)"
          opacity="0.10"
        />

        {/* Fever line at 37.3°C */}
        <line
          x1={CHART_PADDING.left}
          y1={feverLineY()}
          x2={CHART_PADDING.left + plotWidth()}
          y2={feverLineY()}
          stroke="var(--error)"
          stroke-dasharray="6 3"
          stroke-width="1"
          opacity="0.5"
        />
        <text
          x={CHART_PADDING.left + 4}
          y={feverLineY() - 3}
          fill="var(--error)"
          font-size="9"
          opacity="0.65"
        >
          37.3℃ 发热线
        </text>

        {/* Horizontal grid lines */}
        <line
          x1={CHART_PADDING.left}
          y1={CHART_PADDING.top}
          x2={CHART_PADDING.left + plotWidth()}
          y2={CHART_PADDING.top}
          stroke="var(--border)"
          stroke-opacity="0.55"
          stroke-width="1"
        />
        <line
          x1={CHART_PADDING.left}
          y1={CHART_PADDING.top + plotHeight()}
          x2={CHART_PADDING.left + plotWidth()}
          y2={CHART_PADDING.top + plotHeight()}
          stroke="var(--border)"
          stroke-opacity="0.55"
          stroke-width="1"
        />
        {/* Y-axis vertical line */}
        <line
          x1={CHART_PADDING.left}
          y1={CHART_PADDING.top}
          x2={CHART_PADDING.left}
          y2={CHART_PADDING.top + plotHeight()}
          stroke="var(--border)"
          stroke-opacity="0.55"
          stroke-width="1"
        />
        <For each={gridLines()}>
          {(line) => (
            <>
              <line
                x1={CHART_PADDING.left}
                y1={line.y}
                x2={CHART_PADDING.left + plotWidth()}
                y2={line.y}
                stroke="var(--border)"
                stroke-opacity="0.4"
                stroke-dasharray="4 5"
                stroke-width="1"
              />
              {/* Y-axis tick */}
              <line
                x1={CHART_PADDING.left - 4}
                y1={line.y}
                x2={CHART_PADDING.left}
                y2={line.y}
                stroke="var(--border)"
                stroke-opacity="0.7"
                stroke-width="1"
              />
              <text
                x={CHART_PADDING.left - 8}
                y={line.y + 4}
                text-anchor="end"
                fill="var(--content-secondary)"
                font-size="10"
                fill-opacity="0.8"
              >
                {line.label}
              </text>
            </>
          )}
        </For>

        {/* Single-series area (only when one location) */}
        <Show when={!hasMultipleLocations() && areaPath() !== ''}>
          <path d={areaPath()} fill="url(#tempAreaGradient)" class="temp-area-anim" />
        </Show>

        {/* Y-axis unit */}
        <text
          x={CHART_PADDING.left - 6}
          y={CHART_PADDING.top - 6}
          text-anchor="end"
          fill="var(--content-tertiary)"
          font-size="9"
        >
          ℃
        </text>

        {/* X-axis labels */}
        <For each={dateLabels()}>
          {(dl) => (
            <g>
              {/* tick mark */}
              <line
                x1={dl.x}
                y1={CHART_PADDING.top + plotHeight()}
                x2={dl.x}
                y2={CHART_PADDING.top + plotHeight() + 4}
                stroke="var(--border)"
                stroke-opacity="0.6"
                stroke-width="1"
              />
              <text
                x={dl.x}
                y={CHART_PADDING.top + plotHeight() + 14}
                text-anchor="middle"
                transform={`rotate(-30, ${dl.x}, ${CHART_PADDING.top + plotHeight() + 14})`}
                fill="var(--content-secondary)"
                font-size="9"
                fill-opacity="0.85"
              >
                {dl.label}
              </text>
            </g>
          )}
        </For>

        {/* Per-location lines and points */}
        <For each={locationSeries()}>
          {(series, _si) => {
            const hidden = () => hiddenLocations().has(series.location)
            return (
              <Show when={!hidden()}>
                {/* Line */}
                <Show when={series.records.length > 1}>
                  <path
                    d={buildLinePath(series.indices)}
                    fill="none"
                    stroke={hasMultipleLocations() ? series.color : 'url(#tempLineGradient)'}
                    stroke-width="2.5"
                    stroke-linejoin="round"
                    stroke-linecap="round"
                    stroke-opacity={hasMultipleLocations() ? '0.85' : '1'}
                    class="temp-line-anim"
                  />
                </Show>

                {/* Data points */}
                <For each={series.indices}>
                  {(dataIdx, pi) => {
                    const record = () => props.data[dataIdx]
                    return (
                      <g>
                        <circle
                          cx={xScaleByIndex(dataIdx)}
                          cy={yScale(record().value)}
                          r={hoveredIndex() === dataIdx ? 6.5 : 4.2}
                          fill={hasMultipleLocations() ? series.color : temperatureColor(record().value)}
                          stroke={hoveredIndex() === dataIdx ? 'var(--surface)' : 'transparent'}
                          stroke-width={hoveredIndex() === dataIdx ? '1.8' : '0'}
                          class="cursor-pointer temp-point-anim"
                          style={{ 'animation-delay': `${0.3 + pi() * 0.08}s` }}
                          onMouseEnter={() => hoverIn(dataIdx)}
                          onMouseLeave={hoverOut}
                        />
                        <Show when={hoveredIndex() === dataIdx}>
                          <circle
                            cx={xScaleByIndex(dataIdx)}
                            cy={yScale(record().value)}
                            r="10"
                            fill="none"
                            stroke={hasMultipleLocations() ? series.color : temperatureColor(record().value)}
                            stroke-opacity="0.25"
                            stroke-width="6"
                          />
                        </Show>
                      </g>
                    )
                  }}
                </For>
              </Show>
            )
          }}
        </For>

        {/* Crosshair */}
        <Show when={hoveredIndex() !== null}>
          {(_) => {
            const idx = () => hoveredIndex()!
            const x = () => xScaleByIndex(idx())
            const y = () => yScale(props.data[idx()].value)
            return (
              <>
                <line
                  x1={x()}
                  y1={CHART_PADDING.top}
                  x2={x()}
                  y2={CHART_PADDING.top + plotHeight()}
                  stroke="var(--content-tertiary)"
                  stroke-opacity="0.45"
                  stroke-width="1"
                  stroke-dasharray="3 4"
                />
                <line
                  x1={CHART_PADDING.left}
                  y1={y()}
                  x2={CHART_PADDING.left + plotWidth()}
                  y2={y()}
                  stroke="var(--content-tertiary)"
                  stroke-opacity="0.45"
                  stroke-width="1"
                  stroke-dasharray="3 4"
                />
              </>
            )
          }}
        </Show>
      </svg>

      {/* Tooltip */}
      <Show when={hoveredIndex() !== null}>
        {(_) => {
          const idx = () => hoveredIndex()!
          const record = () => props.data[idx()]
          const x = () => xScaleByIndex(idx())
          const y = () => yScale(record().value)
          const seriesColor = () => {
            const loc = record().location || ''
            const s = locationSeries().find(s => s.location === loc)
            return s?.color ?? 'var(--accent)'
          }
          const tooltipX = () => {
            const left = x() - TOOLTIP_WIDTH / 2
            const maxLeft = effectiveWidth() - TOOLTIP_WIDTH - 8
            return Math.min(Math.max(left, 8), maxLeft)
          }
          const isBelow = () => y() < 90
          const tooltipY = () => isBelow() ? y() + 18 : y() - 120
          return (
            <div
              class="absolute bg-surface-elevated/95 shadow-xl rounded-2xl border border-border/40 px-3.5 py-2.5 text-sm z-10"
              style={{
                left: `${tooltipX()}px`,
                top: `${tooltipY()}px`,
                width: `${TOOLTIP_WIDTH}px`,
              }}
              onMouseEnter={() => window.clearTimeout(hideTimer)}
              onMouseLeave={hoverOut}
            >
              {/* Arrow: points UP when tooltip is below point, DOWN when above */}
              <Show when={isBelow()}>
                <div class="absolute -top-1.5 left-1/2 -translate-x-1/2 h-2.5 w-2.5 border-l border-t border-border bg-surface-elevated/95 rotate-45" />
              </Show>
              <Show when={!isBelow()}>
                <div class="absolute -bottom-1.5 left-1/2 -translate-x-1/2 h-2.5 w-2.5 border-r border-b border-border bg-surface-elevated/95 rotate-45" />
              </Show>
              <div class="flex items-center gap-1.5 text-content-secondary text-xs mb-1">
                <span>{formatDateTime(record().recorded_at)}</span>
                <Show when={record().location}>
                  <span
                    class="inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-xs font-medium"
                    style={{ background: `${seriesColor()}18`, color: seriesColor() }}
                  >
                    <span class="w-1.5 h-1.5 rounded-full" style={{ background: seriesColor() }} />
                    {record().location}
                  </span>
                </Show>
              </div>
              <div class="font-semibold text-content">
                {record().value.toFixed(1)} ℃
                <span
                  class="ml-2 text-xs font-normal"
                  style={{ color: temperatureColor(record().value) }}
                >
                  {temperatureLabel(record().value)}
                </span>
              </div>
              <Show when={record().note}>
                <div class="meta-text mt-1">{record().note}</div>
              </Show>
              <Show when={props.onDelete}>
                <button
                  class="mt-1.5 text-xs text-error hover:underline cursor-pointer"
                  onClick={(e) => {
                    e.stopPropagation()
                    props.onDelete?.(record().id)
                    setHoveredIndex(null)
                  }}
                >
                  删除此记录
                </button>
              </Show>
            </div>
          )
        }}
      </Show>
    </div>
  )
}
