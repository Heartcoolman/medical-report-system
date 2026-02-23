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
// @ts-expect-error reserved for future use
const MAX_DAYS = 7

// --- Types ---

interface DailyAggregate {
  date: string      // YYYY-MM-DD
  avg: number
  min: number
  max: number
  count: number
}

// --- Helpers ---

function temperatureColor(value: number): string {
  if (value < 36.0) return 'var(--info)'
  if (value <= 37.3) return 'var(--success)'
  if (value <= 38.0) return 'var(--warning, #f59e0b)'
  if (value <= 39.0) return '#f97316'
  return 'var(--error)'
}

function temperatureLabel(value: number): string {
  if (value < 36.0) return '低温'
  if (value <= 37.3) return '正常'
  if (value <= 38.0) return '低热'
  if (value <= 39.0) return '中热'
  return '高热'
}

function formatDate(date: string): string {
  // "YYYY-MM-DD" → "MM-DD"
  const parts = date.split('-')
  if (parts.length !== 3) return date
  return `${parts[1]}-${parts[2]}`
}

/** Build a list of the last 7 calendar days (today → 6 days ago). */
function last7Days(): string[] {
  const days: string[] = []
  const now = new Date()
  for (let i = 6; i >= 0; i--) {
    const d = new Date(now)
    d.setDate(d.getDate() - i)
    const yyyy = d.getFullYear()
    const mm = String(d.getMonth() + 1).padStart(2, '0')
    const dd = String(d.getDate()).padStart(2, '0')
    days.push(`${yyyy}-${mm}-${dd}`)
  }
  return days
}

function aggregateByDay(data: TemperatureRecord[]): Map<string, DailyAggregate> {
  const groups = new Map<string, number[]>()
  for (const r of data) {
    const date = r.recorded_at.split(' ')[0]
    if (!groups.has(date)) groups.set(date, [])
    groups.get(date)!.push(r.value)
  }
  const result = new Map<string, DailyAggregate>()
  for (const [date, values] of groups) {
    const sum = values.reduce((a, b) => a + b, 0)
    result.set(date, {
      date,
      avg: sum / values.length,
      min: Math.min(...values),
      max: Math.max(...values),
      count: values.length,
    })
  }
  return result
}

// --- Component ---

export interface TemperatureWeeklyChartProps {
  data: TemperatureRecord[]
}

export function TemperatureWeeklyChart(props: TemperatureWeeklyChartProps) {
  const [hoveredIndex, setHoveredIndex] = createSignal<number | null>(null)
  const [chartWidth, setChartWidth] = createSignal(500)
  let containerRef: HTMLDivElement | undefined

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

  const effectiveWidth = () => Math.max(chartWidth(), 200)
  const plotWidth = () => Math.max(effectiveWidth() - CHART_PADDING.left - CHART_PADDING.right, 50)
  const plotHeight = () => CHART_HEIGHT - CHART_PADDING.top - CHART_PADDING.bottom

  // Fixed 7-day slots
  const weekDays = createMemo(() => last7Days())
  const dailyMap = createMemo(() => aggregateByDay(props.data))

  // Only days that have data, in order
  const filledSlots = createMemo(() => {
    const days = weekDays()
    const map = dailyMap()
    return days
      .map((date, i) => ({ date, index: i, data: map.get(date) ?? null }))
      .filter(s => s.data !== null) as { date: string; index: number; data: DailyAggregate }[]
  })

  const yRange = createMemo(() => {
    let min = Y_MIN
    let max = Y_MAX
    for (const s of filledSlots()) {
      if (s.data.min < min) min = Math.floor(s.data.min)
      if (s.data.max > max) max = Math.ceil(s.data.max)
    }
    return { min, max }
  })

  // X position by slot index (0-6, always 7 slots)
  const xScale = (slotIdx: number) => {
    return CHART_PADDING.left + (slotIdx / 6) * plotWidth()
  }

  const yScale = (val: number) => {
    const { min, max } = yRange()
    const ratio = (val - min) / (max - min)
    return CHART_PADDING.top + plotHeight() * (1 - ratio)
  }

  const points = createMemo(() => {
    return filledSlots().map(s => ({
      x: xScale(s.index),
      y: yScale(s.data.avg),
    }))
  })

  const gridLines = createMemo(() => {
    const { min, max } = yRange()
    const lines: { y: number; label: string }[] = []
    for (let v = Math.ceil(min); v <= Math.floor(max) + 0.01; v += 1) {
      lines.push({ y: yScale(v), label: v.toFixed(0) })
    }
    return lines
  })

  const feverLineY = createMemo(() => yScale(FEVER_LINE))

  const linePath = createMemo(() => {
    const pts = points()
    if (pts.length === 0) return ''
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
  })

  const areaPath = createMemo(() => {
    const pts = points()
    if (pts.length === 0) return ''
    const baseY = CHART_PADDING.top + plotHeight()
    if (pts.length === 1) {
      const p = pts[0]
      return `M ${p.x} ${baseY} L ${p.x} ${p.y} L ${p.x} ${baseY} Z`
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

  const dateLabels = createMemo(() => {
    return weekDays().map((date, i) => ({
      x: xScale(i),
      label: formatDate(date),
    }))
  })

  return (
    <div class="relative" ref={containerRef}>
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
          <linearGradient id="weeklyTempLineGradient" x1="0%" y1="0%" x2="0%" y2="100%">
            <stop offset="0%" stop-color="var(--accent)" stop-opacity="0.95" />
            <stop offset="100%" stop-color="var(--accent)" stop-opacity="0.35" />
          </linearGradient>
          <linearGradient id="weeklyTempAreaGradient" x1="0%" y1="0%" x2="0%" y2="100%">
            <stop offset="0%" stop-color="var(--accent)" stop-opacity="0.24" />
            <stop offset="100%" stop-color="var(--accent)" stop-opacity="0.02" />
          </linearGradient>
          <style>{`
            @keyframes wkLineDraw {
              from { stroke-dashoffset: 2000; }
              to   { stroke-dashoffset: 0; }
            }
            @keyframes wkAreaFade {
              from { opacity: 0; }
              to   { opacity: 1; }
            }
            @keyframes wkPointPop {
              0%   { r: 0; opacity: 0; }
              60%  { r: 5.2; opacity: 1; }
              100% { r: 4.2; opacity: 1; }
            }
            .wk-line-anim {
              stroke-dasharray: 2000;
              stroke-dashoffset: 2000;
              animation: wkLineDraw 0.8s ease-out forwards;
            }
            .wk-area-anim {
              opacity: 0;
              animation: wkAreaFade 0.6s ease-out 0.3s forwards;
            }
            .wk-point-anim {
              r: 0;
              opacity: 0;
              animation: wkPointPop 0.4s ease-out forwards;
            }
          `}</style>
        </defs>

        {/* Normal range band */}
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

        {/* Area under line */}
        <Show when={areaPath() !== ''}>
          <path d={areaPath()} fill="url(#weeklyTempAreaGradient)" class="wk-area-anim" />
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
                y={CHART_PADDING.top + plotHeight() + 16}
                text-anchor="middle"
                fill="var(--content-secondary)"
                font-size="10"
                fill-opacity="0.85"
              >
                {dl.label}
              </text>
            </g>
          )}
        </For>

        {/* Data line */}
        <Show when={filledSlots().length > 1}>
          <path
            d={linePath()}
            fill="none"
            stroke="url(#weeklyTempLineGradient)"
            stroke-width="2.5"
            stroke-linejoin="round"
            stroke-linecap="round"
            class="wk-line-anim"
          />
        </Show>

        {/* Data points */}
        <For each={filledSlots()}>
          {(slot, si) => (
            <g>
              <circle
                cx={xScale(slot.index)}
                cy={yScale(slot.data.avg)}
                r={hoveredIndex() === si() ? 6.5 : 4.2}
                fill={temperatureColor(slot.data.avg)}
                stroke={hoveredIndex() === si() ? 'var(--surface)' : 'transparent'}
                stroke-width={hoveredIndex() === si() ? '1.8' : '0'}
                class="cursor-pointer wk-point-anim"
                style={{ 'animation-delay': `${0.3 + si() * 0.1}s` }}
                onMouseEnter={() => setHoveredIndex(si())}
                onMouseLeave={() => setHoveredIndex(null)}
              />
              <Show when={hoveredIndex() === si()}>
                <circle
                  cx={xScale(slot.index)}
                  cy={yScale(slot.data.avg)}
                  r="10"
                  fill="none"
                  stroke={temperatureColor(slot.data.avg)}
                  stroke-opacity="0.25"
                  stroke-width="6"
                />
              </Show>

              {/* Min/Max range bar */}
              <Show when={slot.data.count > 1}>
                <line
                  x1={xScale(slot.index)}
                  y1={yScale(slot.data.max)}
                  x2={xScale(slot.index)}
                  y2={yScale(slot.data.min)}
                  stroke={temperatureColor(slot.data.avg)}
                  stroke-opacity="0.3"
                  stroke-width="3"
                  stroke-linecap="round"
                />
              </Show>
            </g>
          )}
        </For>

        {/* Crosshair */}
        <Show when={hoveredIndex() !== null}>
          {(_) => {
            const idx = () => hoveredIndex()!
            const slot = () => filledSlots()[idx()]
            const x = () => xScale(slot().index)
            const y = () => yScale(slot().data.avg)
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
          const slot = () => filledSlots()[idx()]
          const day = () => slot().data
          const x = () => xScale(slot().index)
          const y = () => yScale(day().avg)
          const tooltipX = () => {
            const left = x() - TOOLTIP_WIDTH / 2
            const maxLeft = effectiveWidth() - TOOLTIP_WIDTH - 8
            return Math.min(Math.max(left, 8), maxLeft)
          }
          const isBelow = () => y() < 90
          const tooltipY = () => isBelow() ? y() + 18 : y() - 128
          return (
            <div
              class="absolute bg-surface-elevated/95 shadow-xl rounded-2xl border border-border/40 px-3.5 py-2.5 text-sm pointer-events-none z-10"
              style={{
                left: `${tooltipX()}px`,
                top: `${tooltipY()}px`,
                width: `${TOOLTIP_WIDTH}px`,
              }}
            >
              <Show when={isBelow()}>
                <div class="absolute -top-1.5 left-1/2 -translate-x-1/2 h-2.5 w-2.5 border-l border-t border-border bg-surface-elevated/95 rotate-45" />
              </Show>
              <Show when={!isBelow()}>
                <div class="absolute -bottom-1.5 left-1/2 -translate-x-1/2 h-2.5 w-2.5 border-r border-b border-border bg-surface-elevated/95 rotate-45" />
              </Show>
              <div class="text-content-secondary text-xs mb-1">{day().date}</div>
              <div class="font-semibold text-content">
                日均 {day().avg.toFixed(1)} ℃
                <span
                  class="ml-2 text-xs font-normal"
                  style={{ color: temperatureColor(day().avg) }}
                >
                  {temperatureLabel(day().avg)}
                </span>
              </div>
              <div class="flex gap-3 text-xs text-content-secondary mt-1">
                <span>最高 {day().max.toFixed(1)}℃</span>
                <span>最低 {day().min.toFixed(1)}℃</span>
                <span>{day().count} 次记录</span>
              </div>
            </div>
          )
        }}
      </Show>
    </div>
  )
}
