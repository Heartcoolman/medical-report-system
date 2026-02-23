import { createSignal, createResource, Show, For, onCleanup } from 'solid-js'
import { A, useNavigate } from '@solidjs/router'
import { api } from '@/api/client'
import type { PatientWithStats } from '@/api/types'
import { cn } from '@/lib/utils'
import { Button, Card, CardBody, Input, Pagination, Skeleton, Empty, Badge, SearchBar, FloatingActionButton } from '@/components'

export default function Dashboard() {
  const navigate = useNavigate()
  const [search, setSearch] = createSignal('')
  const [page, setPage] = createSignal(1)
  const [debouncedSearch, setDebouncedSearch] = createSignal('')
  let debounceTimer: ReturnType<typeof setTimeout> | undefined
  onCleanup(() => clearTimeout(debounceTimer))

  const handleSearch = (value: string) => {
    setSearch(value)
    clearTimeout(debounceTimer)
    debounceTimer = setTimeout(() => {
      setDebouncedSearch(value)
      setPage(1)
    }, 300)
  }

  const [data] = createResource(
    () => ({ search: debouncedSearch(), page: page() }),
    (params) => api.patients.list({ search: params.search || undefined, page: params.page, page_size: 12 }),
  )

  return (
    <div class="space-y-6">
      {/* Page Header */}
      <div class="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4">
        <div>
          <h1 class="page-title">患者管理</h1>
          <p class="sub-text">管理患者信息和医疗报告</p>
        </div>
        <A href="/patients/new">
          <Button variant="primary">
            <svg class="w-4 h-4 mr-2" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
              <path stroke-linecap="round" stroke-linejoin="round" d="M12 4v16m8-8H4" />
            </svg>
            新增患者
          </Button>
        </A>
      </div>

      {/* Search — PC uses Input, mobile uses SearchBar */}
      <div class="max-w-md hidden md:block">
        <Input
          placeholder="搜索姓名、电话或身份证号..."
          value={search()}
          onInput={(e) => handleSearch(e.currentTarget.value)}
          leftIcon={
            <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
              <path stroke-linecap="round" stroke-linejoin="round" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
            </svg>
          }
        />
      </div>
      <div class="md:hidden">
        <SearchBar
          placeholder="搜索姓名、电话或身份证号..."
          value={search()}
          onInput={(value) => handleSearch(value)}
          onClear={() => handleSearch('')}
        />
      </div>

      {/* Patient Grid */}
      <Show when={data.loading} fallback={
        <Show when={data.error} fallback={
          <Show
            when={data()?.items.length}
            fallback={
              <Empty
                title={debouncedSearch() ? '未找到匹配的患者' : '暂无患者'}
                description={debouncedSearch() ? '请尝试其他搜索关键词' : '点击"新增患者"开始添加'}
              />
            }
          >
            <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4">
              <For each={data()!.items}>
                {(patient, index) => (
                  <PatientCard patient={patient} index={index()} />
                )}
              </For>
            </div>

            {/* Pagination */}
            <Show when={data()!.total > 12}>
              <div class="flex justify-center pt-4">
                <Pagination
                  current={page()}
                  total={data()!.total}
                  pageSize={12}
                  onChange={setPage}
                />
              </div>
            </Show>
          </Show>
        }>
          <Empty
            title="加载失败"
            description={String(data.error?.message || data.error)}
          />
        </Show>
      }>
        <PatientListSkeleton />
      </Show>

      {/* FAB — mobile only */}
      <div class="md:hidden">
        <FloatingActionButton
          extended
          label="新增患者"
          onClick={() => navigate('/patients/new')}
          icon={
            <svg class="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
              <path stroke-linecap="round" stroke-linejoin="round" d="M12 4v16m8-8H4" />
            </svg>
          }
        />
      </div>
    </div>
  )
}

function PatientCard(props: { patient: PatientWithStats; index?: number }) {
  const genderColor = () => props.patient.gender === '男' ? 'info' : 'error'

  return (
    <A
      href={`/patients/${props.patient.id}`}
      class="block no-underline group"
    >
      <Card variant="elevated" interactive class="hover:-translate-y-0.5 hover:shadow-lg">
        <CardBody class="p-4">
          <div class="flex items-start justify-between mb-3">
            <div class="flex items-center gap-2">
              <div class={cn(
                'w-10 h-10 rounded-full flex items-center justify-center text-sm font-semibold',
                props.patient.gender === '男'
                  ? 'bg-info-light text-info'
                  : 'bg-error-light text-error',
              )}>
                {props.patient.name.slice(0, 1)}
              </div>
              <div>
                <h3 class="text-base font-semibold text-content group-hover:text-accent transition-colors duration-[var(--transition-fast)]">
                  {props.patient.name}
                </h3>
                <Badge variant={genderColor()} class="mt-0.5">{props.patient.gender}</Badge>
              </div>
            </div>
          </div>

          <div class="space-y-1.5 text-sm">
            <Show when={props.patient.phone}>
              <div class="flex items-center gap-2 text-content-secondary">
                <svg class="w-3.5 h-3.5 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M3 5a2 2 0 012-2h3.28a1 1 0 01.948.684l1.498 4.493a1 1 0 01-.502 1.21l-2.257 1.13a11.042 11.042 0 005.516 5.516l1.13-2.257a1 1 0 011.21-.502l4.493 1.498a1 1 0 01.684.949V19a2 2 0 01-2 2h-1C9.716 21 3 14.284 3 6V5z" />
                </svg>
                <span>{props.patient.phone}</span>
              </div>
            </Show>
            <Show when={props.patient.id_number}>
              <div class="flex items-center gap-2 text-content-secondary">
                <svg class="w-3.5 h-3.5 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M10 6H5a2 2 0 00-2 2v9a2 2 0 002 2h14a2 2 0 002-2V8a2 2 0 00-2-2h-5m-4 0V5a2 2 0 114 0v1m-4 0a2 2 0 104 0" />
                </svg>
                <span class="truncate">{props.patient.id_number}</span>
              </div>
            </Show>
            <Show when={props.patient.dob}>
              <div class="flex items-center gap-2 text-content-secondary">
                <svg class="w-3.5 h-3.5 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M8 7V3m8 4V3m-9 8h10M5 21h14a2 2 0 002-2V7a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" />
                </svg>
                <span>{props.patient.dob}</span>
              </div>
            </Show>
          </div>

          {/* Report stats */}
          <Show when={props.patient.report_count > 0}>
            <div class="mt-2.5 pt-2 border-t border-border flex items-center gap-3 text-xs">
              <div class="flex items-center gap-1 text-content-secondary">
                <svg class="w-3.5 h-3.5 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
                </svg>
                <span>{props.patient.report_count} 份报告</span>
              </div>
              <Show when={props.patient.last_report_date}>
                <span class="text-content-tertiary">{props.patient.last_report_date}</span>
              </Show>
              <Show when={props.patient.total_abnormal > 0}>
                <Badge variant="error">{props.patient.total_abnormal} 项异常</Badge>
              </Show>
            </div>
          </Show>
        </CardBody>
      </Card>
    </A>
  )
}

function PatientListSkeleton() {
  return (
    <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4">
      <For each={Array(8).fill(0)}>
        {() => (
          <Card
            variant="elevated"
          >
            <CardBody class="p-4 space-y-3">
              <div class="flex items-center gap-2">
                <Skeleton variant="circle" width={40} height={40} />
                <div class="space-y-1">
                  <Skeleton variant="text" width={80} />
                  <Skeleton variant="text" width={40} height={20} />
                </div>
              </div>
              <div class="space-y-1.5">
                <Skeleton variant="text" width="70%" />
                <Skeleton variant="text" width="90%" />
              </div>
            </CardBody>
          </Card>
        )}
      </For>
    </div>
  )
}
