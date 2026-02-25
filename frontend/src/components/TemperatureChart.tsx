import { createSignal, createMemo, createEffect, onCleanup, Show, For } from 'solid-js'
import type { TemperatureRecord } from '@/api/types'

// --- Constants ---
const CHART_PADDING = { top: 20, right: 20, bottom: 44, left: 50 }
const CHART_HEIGHT = 240
const Y_MIN = 35
const Y_MAX = 42
const FEVER_LINE = 37.3
const TOOLTIP_WIDTH = 182
const SMOOTH_CONTROL_RATIO = 0.35

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
  // "YYYY-MM-DD HH:MM" → "MM-DD HH:MM"
  const parts = recorded_at.split(' ')
  if (parts.length !== 2) return recorded_at
  const dateParts = parts[0].split('-')
  if (dateParts.length !== 3) return recorded_at
  return `${dateParts[1]}-${dateParts[2]} ${parts[1]}`
}

function parseRecordedAt(recorded_at: string): number {
  // "YYYY-MM-DD HH:MM" → timestamp ms
  return new Date(recorded_at.replace(' ', 'T') + ':00').getTime()
}

// --- Component ---

export interface TemperatureChartProps {
  data: TemperatureRecord[]
  onDelete?: (id: string) => void
}

export function TemperatureChart(props: TemperatureChartProps) {
  const [hoveredIndex, setHoveredIndex] = createSignal<number | null>(null)
  const [chartWidth, setChartWidth] = createSignal(500)
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

  const effectiveWidth = () => Math.max(chartWidth(), 200)
  const plotWidth = () => Math.max(effectiveWidth() - CHART_PADDING.left - CHART_PADDING.right, 50)
  const plotHeight = () => CHART_HEIGHT - CHART_PADDING.top - CHART_PADDING.bottom

  // Compute nice y range including all data
  const yRange = createMemo(() => {
    let min = Y_MIN
    let max = Y_MAX
    for (const r of props.data) {
      if (r.value < min) min = Math.floor(r.value)
      if (r.value > max) max = Math.ceil(r.value)
    }
    return { min, max }
  })

  // Time-proportional x scale
  const timeRange = createMemo(() => {
    if (props.data.length <= 1) return null
    const times = props.data.map(r => parseRecordedAt(r.recorded_at))
    return { min: Math.min(...times), max: Math.max(...times) }
  })

  const xScale = (i: number) => {
    const tr = timeRange()
    if (!tr || tr.max === tr.min) return CHART_PADDING.left + plotWidth() / 2
    const t = parseRecordedAt(props.data[i].recorded_at)
    return CHART_PADDING.left + ((t - tr.min) / (tr.max - tr.min)) * plotWidth()
  }

  const yScale = (val: number) => {
    const { min, max } = yRange()
    const ratio = (val - min) / (max - min)
    return CHART_PADDING.top + plotHeight() * (1 - ratio)
  }

  const points = createMemo(() => {
    return props.data.map((record, i) => ({
      x: xScale(i),
      y: yScale(record.value),
    }))
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

  const linePath = createMemo(() => {
    const pts = points()
    if (pts.length === 0) return ''
    if (pts.length === 1) {
      const p = pts[0]
      return `M ${p.x} ${p.y}`
    }
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

  // X-axis labels — show date+time, skip labels if too dense
  const dateLabels = createMemo(() => {
    const all = props.data.map((r, i) => ({
      x: xScale(i),
      label: formatDateTime(r.recorded_at),
    }))
    // Skip labels when they would overlap; labels are rotated -30° so horizontal footprint ~50px
    if (all.length <= 1) return all
    const minSpacing = 50
    const totalWidth = plotWidth()
    const step = Math.max(1, Math.ceil((all.length * minSpacing) / totalWidth))
    return all.filter((_, i) => i % step === 0 || i === all.length - 1)
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

        {/* Temperature area under line */}
        <Show when={areaPath() !== ''}>
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

        {/* Data line */}
        <Show when={props.data.length > 1}>
          <path
            d={linePath()}
            fill="none"
            stroke="url(#tempLineGradient)"
            stroke-width="2.5"
            stroke-linejoin="round"
            stroke-linecap="round"
            class="temp-line-anim"
          />
        </Show>
        <Show when={props.data.length === 1}>
          <line
            x1={xScale(0) - 0.8}
            y1={yScale(props.data[0].value) - 16}
            x2={xScale(0) + 0.8}
            y2={yScale(props.data[0].value) - 16}
            stroke="url(#tempLineGradient)"
            stroke-width="2.2"
            stroke-linecap="round"
          />
        </Show>

        {/* Data points */}
        <For each={props.data}>
          {(record, i) => (
            <g>
              <circle
                cx={xScale(i())}
                cy={yScale(record.value)}
                r={hoveredIndex() === i() ? 6.5 : 4.2}
                fill={temperatureColor(record.value)}
                stroke={hoveredIndex() === i() ? 'var(--surface)' : 'transparent'}
                stroke-width={hoveredIndex() === i() ? '1.8' : '0'}
                class="cursor-pointer temp-point-anim"
                style={{ 'animation-delay': `${0.3 + i() * 0.08}s` }}
                onMouseEnter={() => hoverIn(i())}
                onMouseLeave={hoverOut}
              />
              <Show when={hoveredIndex() === i()}>
                <circle
                  cx={xScale(i())}
                  cy={yScale(record.value)}
                  r="10"
                  fill="none"
                  stroke={temperatureColor(record.value)}
                  stroke-opacity="0.25"
                  stroke-width="6"
                />
              </Show>
            </g>
          )}
        </For>

        {/* Crosshair */}
        <Show when={hoveredIndex() !== null}>
          {(_) => {
            const idx = () => hoveredIndex()!
            const x = () => xScale(idx())
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
          const x = () => xScale(idx())
          const y = () => yScale(record().value)
          const tooltipX = () => {
            const left = x() - TOOLTIP_WIDTH / 2
            const maxLeft = effectiveWidth() - TOOLTIP_WIDTH - 8
            return Math.min(Math.max(left, 8), maxLeft)
          }
          const isBelow = () => y() < 90
          const tooltipY = () => isBelow() ? y() + 18 : y() - 108
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
              <div class="text-content-secondary text-xs mb-1">{formatDateTime(record().recorded_at)}</div>
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
