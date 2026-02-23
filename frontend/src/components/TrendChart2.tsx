import { createMemo, Show, For } from 'solid-js'
import type { TrendPoint } from '@/api/types'
import { parseReferenceRange, niceRange, formatDate, statusColor } from '@/lib/trend-helpers'
import { TestItemStatusBadge } from '@/components'

// --- Constants ---
const CHART_PADDING = { top: 20, right: 30, bottom: 50, left: 60 }
const CHART_HEIGHT = 300

// --- SVG Trend Chart ---

export interface TrendChartProps {
  data: TrendPoint[]
  width: number
  containerRef: (el: HTMLDivElement) => void
  hoveredIndex: number | null
  onHover: (index: number | null) => void
}

export function TrendChart(props: TrendChartProps) {
  const effectiveWidth = () => Math.max(props.width, 200)

  const numericValues = createMemo(() =>
    props.data.map((p) => parseFloat(p.value)),
  )

  const yRange = createMemo(() => {
    const vals = numericValues()
    let min = Math.min(...vals)
    let max = Math.max(...vals)
    // Include reference range in y-axis range
    const ref = parseReferenceRange(props.data[0]?.reference_range ?? '')
    if (ref) {
      min = Math.min(min, ref.min)
      max = Math.max(max, ref.max)
    }
    return niceRange(min, max)
  })

  const plotWidth = () => Math.max(effectiveWidth() - CHART_PADDING.left - CHART_PADDING.right, 50)
  const plotHeight = () => CHART_HEIGHT - CHART_PADDING.top - CHART_PADDING.bottom

  const xScale = (i: number) => {
    const n = props.data.length
    if (n <= 1) return CHART_PADDING.left + plotWidth() / 2
    return CHART_PADDING.left + (i / (n - 1)) * plotWidth()
  }

  const yScale = (val: number) => {
    const { lo, hi } = yRange()
    const ratio = (val - lo) / (hi - lo)
    return CHART_PADDING.top + plotHeight() * (1 - ratio)
  }

  // Grid lines
  const gridLines = createMemo(() => {
    const { lo, hi, step } = yRange()
    const lines: { y: number; label: string }[] = []
    for (let v = lo; v <= hi + step * 0.001; v += step) {
      lines.push({ y: yScale(v), label: String(Math.round(v * 1000) / 1000) })
    }
    return lines
  })

  // Reference range band
  const refBand = createMemo(() => {
    const ref = parseReferenceRange(props.data[0]?.reference_range ?? '')
    if (!ref) return null
    const y1 = yScale(ref.max)
    const y2 = yScale(ref.min)
    return { y: y1, height: y2 - y1 }
  })

  // Polyline points
  const polylinePoints = createMemo(() =>
    props.data.map((_, i) => `${xScale(i)},${yScale(numericValues()[i])}`).join(' '),
  )

  // Date labels
  const dateLabels = createMemo(() =>
    props.data.map((p, i) => ({
      x: xScale(i),
      label: formatDate(p.report_date || p.sample_date),
    })),
  )

  // Unit label
  const unit = createMemo(() => props.data[0]?.unit ?? '')

  return (
    <div class="relative" ref={props.containerRef}>
      <svg
        viewBox={`0 0 ${effectiveWidth()} ${CHART_HEIGHT}`}
        class="w-full"
        style={{ height: `${CHART_HEIGHT}px` }}
      >
        {/* Reference range band */}
        <Show when={refBand()}>
          {(band) => (
            <rect
              x={CHART_PADDING.left}
              y={band().y}
              width={plotWidth()}
              height={band().height}
              fill="var(--success)"
              opacity="0.08"
            />
          )}
        </Show>

        {/* Horizontal grid lines */}
        <For each={gridLines()}>
          {(line) => (
            <>
              <line
                x1={CHART_PADDING.left}
                y1={line.y}
                x2={CHART_PADDING.left + plotWidth()}
                y2={line.y}
                stroke="var(--border)"
                stroke-dasharray="4 3"
                stroke-width="1"
              />
              <text
                x={CHART_PADDING.left - 8}
                y={line.y + 4}
                text-anchor="end"
                fill="var(--content-secondary)"
                font-size="11"
              >
                {line.label}
              </text>
            </>
          )}
        </For>

        {/* Y-axis unit label */}
        <Show when={unit()}>
          <text
            x={CHART_PADDING.left - 8}
            y={CHART_PADDING.top - 6}
            text-anchor="end"
            fill="var(--content-tertiary)"
            font-size="10"
          >
            {unit()}
          </text>
        </Show>

        {/* X-axis date labels */}
        <For each={dateLabels()}>
          {(dl) => (
            <text
              x={dl.x}
              y={CHART_HEIGHT - 6}
              text-anchor="end"
              fill="var(--content-secondary)"
              font-size="11"
              transform={`rotate(-45, ${dl.x}, ${CHART_HEIGHT - 6})`}
            >
              {dl.label}
            </text>
          )}
        </For>

        {/* Data line */}
        <Show when={props.data.length > 1}>
          <polyline
            points={polylinePoints()}
            fill="none"
            stroke="var(--accent)"
            stroke-width="2"
            stroke-linejoin="round"
            stroke-linecap="round"
          />
        </Show>

        {/* Data points */}
        <For each={props.data}>
          {(point, i) => (
            <circle
              cx={xScale(i())}
              cy={yScale(numericValues()[i()])}
              r={props.hoveredIndex === i() ? 6 : 4}
              fill={statusColor(point.status)}
              stroke="var(--surface)"
              stroke-width="2"
              class="cursor-pointer transition-[r]"
              onMouseEnter={() => props.onHover(i())}
              onMouseLeave={() => props.onHover(null)}
            />
          )}
        </For>
      </svg>

      {/* Hover tooltip */}
      <Show when={props.hoveredIndex !== null && props.hoveredIndex !== undefined}>
        <Tooltip
          data={props.data}
          index={props.hoveredIndex!}
          xScale={xScale}
          yScale={(v: number) => yScale(v)}
          numericValues={numericValues()}
          chartWidth={effectiveWidth()}
        />
      </Show>
    </div>
  )
}

// --- Tooltip ---

interface TooltipProps {
  data: TrendPoint[]
  index: number
  xScale: (i: number) => number
  yScale: (v: number) => number
  numericValues: number[]
  chartWidth: number
}

function Tooltip(props: TooltipProps) {
  const point = () => props.data[props.index]
  const tooltipX = () => {
    const x = props.xScale(props.index)
    if (x > props.chartWidth - 180) return x - 160
    return x + 12
  }
  const tooltipY = () => {
    const y = props.yScale(props.numericValues[props.index])
    if (y < 60) return y + 20
    return y - 80
  }

  return (
    <div
      class="absolute bg-surface-elevated shadow-xl rounded-2xl border border-border/40 px-3.5 py-2.5 text-sm pointer-events-none z-10 min-w-[140px]"
      style={{
        left: `${tooltipX()}px`,
        top: `${tooltipY()}px`,
      }}
    >
      <div class="text-content-secondary text-xs mb-1">{point().report_date || point().sample_date}</div>
      <div class="font-semibold text-content">
        {point().value} {point().unit}
      </div>
      <div class="flex items-center gap-2 mt-1">
        <TestItemStatusBadge status={point().status} value={point().value} referenceRange={point().reference_range} dot />
      </div>
      <Show when={point().reference_range}>
        <div class="meta-text mt-1">
          参考: {point().reference_range}
        </div>
      </Show>
    </div>
  )
}
