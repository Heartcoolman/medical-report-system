import { createResource, Show, For } from 'solid-js'
import { useParams, A } from '@solidjs/router'
import { api } from '@/api/client'
import { cn } from '@/lib/utils'
import { Card, CardBody, Badge, Spinner, Empty } from '@/components'

const EVENT_CONFIG: Record<string, { color: string; icon: string; label: string }> = {
  report: {
    color: 'bg-accent text-white',
    icon: 'M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z',
    label: '检查报告',
  },
  temperature: {
    color: 'bg-warning text-white',
    icon: 'M12 9v3m0 0v3m0-3h3m-3 0H9m12 0a9 9 0 11-18 0 9 9 0 0118 0z',
    label: '体温记录',
  },
  expense: {
    color: 'bg-info text-white',
    icon: 'M12 8c-1.657 0-3 .895-3 2s1.343 2 3 2 3 .895 3 2-1.343 2-3 2m0-8c1.11 0 2.08.402 2.599 1M12 8V7m0 1v8m0 0v1m0-1c-1.11 0-2.08-.402-2.599-1M21 12a9 9 0 11-18 0 9 9 0 0118 0z',
    label: '消费记录',
  },
  medication: {
    color: 'bg-success text-white',
    icon: 'M19.428 15.428a2 2 0 00-1.022-.547l-2.387-.477a6 6 0 00-3.86.517l-.318.158a6 6 0 01-3.86.517L6.05 15.21a2 2 0 00-1.806.547M8 4h8l-1 1v5.172a2 2 0 00.586 1.414l5 5c1.26 1.26.367 3.414-1.415 3.414H4.828c-1.782 0-2.674-2.154-1.414-3.414l5-5A2 2 0 009 10.172V5L8 4z',
    label: '用药记录',
  },
}

export default function Timeline() {
  const params = useParams<{ id: string }>()
  const [patient] = createResource(() => params.id, (id) => api.patients.get(id))
  const [events] = createResource(() => params.id, (id) => api.timeline.get(id))

  const groupedByDate = () => {
    const list = events() ?? []
    const groups = new Map<string, typeof list>()
    for (const e of list) {
      const date = e.event_date
      if (!groups.has(date)) groups.set(date, [])
      groups.get(date)!.push(e)
    }
    return Array.from(groups.entries()).sort(([a], [b]) => b.localeCompare(a))
  }

  function linkFor(event: { event_type: string; related_id: string }) {
    switch (event.event_type) {
      case 'report': return `/reports/${event.related_id}`
      case 'expense': return `/expenses/${event.related_id}`
      default: return ''
    }
  }

  return (
    <div class="page-shell">
      <div class="max-w-2xl mx-auto">
        <h1 class="page-title mb-1">健康时间线</h1>
        <Show when={patient()}>
          <p class="sub-text mb-6">{patient()!.name} 的完整就医记录</p>
        </Show>

        <Show when={events.loading}>
          <div class="flex justify-center py-12"><Spinner size="lg" variant="orbital" /></div>
        </Show>

        <Show when={events() && !events.loading}>
          <Show when={(events() ?? []).length > 0} fallback={
            <Empty title="暂无记录" description="还没有任何医疗事件" />
          }>
            <div class="relative">
              {/* Vertical line */}
              <div class="absolute left-5 top-0 bottom-0 w-0.5 bg-border" />

              <For each={groupedByDate()}>
                {([date, dayEvents]) => (
                  <div class="mb-6">
                    <div class="flex items-center gap-3 mb-3">
                      <div class="w-10 h-10 rounded-full bg-surface-elevated border-2 border-border flex items-center justify-center z-10">
                        <span class="text-xs font-bold text-content-secondary">{date.slice(5, 7)}/{date.slice(8, 10)}</span>
                      </div>
                      <span class="text-sm font-semibold text-content">{date}</span>
                      <Badge variant="info">{dayEvents.length}</Badge>
                    </div>
                    <div class="ml-12 space-y-2">
                      <For each={dayEvents}>
                        {(event) => {
                          const config = EVENT_CONFIG[event.event_type] || EVENT_CONFIG.report
                          const link = linkFor(event)
                          const Wrapper = link
                            ? (p: any) => <A href={link} class="block no-underline">{p.children}</A>
                            : (p: any) => <div>{p.children}</div>
                          return (
                            <Wrapper>
                              <Card variant="outlined" class={cn(link && 'hover:border-accent cursor-pointer transition-colors')}>
                                <CardBody class="p-3 flex items-center gap-3">
                                  <div class={cn('w-8 h-8 rounded-lg flex items-center justify-center shrink-0', config.color)}>
                                    <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                                      <path stroke-linecap="round" stroke-linejoin="round" d={config.icon} />
                                    </svg>
                                  </div>
                                  <div class="min-w-0 flex-1">
                                    <div class="flex items-center gap-2">
                                      <span class="text-sm font-medium text-content truncate">{event.title}</span>
                                      <Badge variant="info" class="shrink-0">{config.label}</Badge>
                                    </div>
                                    <Show when={event.description}>
                                      <div class="text-xs text-content-tertiary mt-0.5 truncate">{event.description}</div>
                                    </Show>
                                  </div>
                                </CardBody>
                              </Card>
                            </Wrapper>
                          )
                        }}
                      </For>
                    </div>
                  </div>
                )}
              </For>
            </div>
          </Show>
        </Show>
      </div>
    </div>
  )
}
