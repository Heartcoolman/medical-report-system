import {
  type ParentProps,
  createEffect,
  createSignal,
  onCleanup,
  on,
  Show,
} from 'solid-js'
import { A, useLocation, useNavigate } from '@solidjs/router'
import { cn } from '@/lib/utils'
import { useTheme } from '@/lib/theme'
import { Button } from '@/components'
import { currentUser, logout } from '@/stores/auth'

type MobileMenuTransitionState = 'closed' | 'opening' | 'open' | 'closing'

const ENTER_DURATION = 240
const MOBILE_MENU_DURATION = 180

export default function AppLayout(props: ParentProps) {
  const { theme, toggleTheme } = useTheme()
  const navigate = useNavigate()
  const [mobileMenuOpen, setMobileMenuOpen] = createSignal(false)
  const location = useLocation()
  const routeKey = () => `${location.pathname}${location.search}`
  const [entering, setEntering] = createSignal(false)
  const [mobileMenuState, setMobileMenuState] = createSignal<MobileMenuTransitionState>('closed')
  const [mobileMenuVisible, setMobileMenuVisible] = createSignal(false)

  let enterTimer: ReturnType<typeof setTimeout> | undefined
  let mobileMenuTimer: ReturnType<typeof setTimeout> | undefined

  const clearEnterTimer = () => {
    if (enterTimer) {
      window.clearTimeout(enterTimer)
      enterTimer = undefined
    }
  }

  const clearMobileMenuTimer = () => {
    if (mobileMenuTimer) {
      window.clearTimeout(mobileMenuTimer)
      mobileMenuTimer = undefined
    }
  }

  const syncMobileMenuState = () => {
    if (mobileMenuOpen()) {
      clearMobileMenuTimer()
      setMobileMenuVisible(true)
      setMobileMenuState('opening')
      mobileMenuTimer = window.setTimeout(() => {
        setMobileMenuState('open')
        mobileMenuTimer = undefined
      }, MOBILE_MENU_DURATION)
      return
    }

    if (mobileMenuVisible()) {
      setMobileMenuState('closing')
      clearMobileMenuTimer()
      mobileMenuTimer = window.setTimeout(() => {
        setMobileMenuVisible(false)
        setMobileMenuState('closed')
        mobileMenuTimer = undefined
      }, MOBILE_MENU_DURATION)
    }
  }

  const getMobileMenuClass = () => {
    if (mobileMenuState() === 'opening') return 'mobile-menu-enter'
    if (mobileMenuState() === 'closing') return 'mobile-menu-leave'
    return ''
  }

  const getMobileMenuRender = () => {
    if (mobileMenuVisible()) return true
    return mobileMenuState() === 'open'
  }

  // Trigger enter animation on route change.
  // Using `on()` with defer=true so this only runs on *changes*, not on initial render.
  // Critically, props.children is NOT accessed here — it stays in the JSX render tree
  // where its reactive scope is owned by the router, not by this effect.
  createEffect(on(routeKey, () => {
    clearEnterTimer()
    setEntering(true)
    enterTimer = window.setTimeout(() => {
      setEntering(false)
      enterTimer = undefined
    }, ENTER_DURATION)
  }, { defer: true }))

  createEffect(() => {
    syncMobileMenuState()
  })

  onCleanup(() => {
    clearEnterTimer()
    clearMobileMenuTimer()
  })

  const navLinks = [
    {
      href: '/',
      label: '患者管理',
      icon: <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M17 20h5v-2a3 3 0 00-5.356-1.857M17 20H7m10 0v-2c0-.656-.126-1.283-.356-1.857M7 20H2v-2a3 3 0 015.356-1.857M7 20v-2c0-.656.126-1.283.356-1.857m0 0a5.002 5.002 0 019.288 0M15 7a3 3 0 11-6 0 3 3 0 016 0z" /></svg>,
    },
    {
      href: '/edit-logs',
      label: '修改日志',
      icon: <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" /></svg>,
    },
  ]

  const handleLogout = () => {
    logout()
    navigate('/login', { replace: true })
  }

  const isActive = (href: string) => {
    if (href === '/') return location.pathname === '/'
    return location.pathname.startsWith(href)
  }

  return (
    <div class="min-h-screen bg-surface text-content transition-colors duration-200">
      {/* Top Navigation Bar */}
      <nav class="sticky top-0 z-40 border-b border-border/50 bg-surface/80 backdrop-blur-xl shadow-sm">
        <div class="max-w-screen-2xl mx-auto px-4 lg:px-8">
          <div class="flex items-center justify-between h-14">
            {/* Left: Logo + Nav Links */}
            <div class="flex items-center gap-6">
              <A href="/" class="app-header-link">
                <svg class="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
                </svg>
                <span>医疗报告管理</span>
              </A>

              {/* Desktop Nav Links */}
              <div class="hidden md:flex items-center gap-1">
                {navLinks.map((link) => (
                  <A
                    href={link.href}
                    class={cn(
                      'nav-link-base flex items-center gap-1.5',
                      isActive(link.href)
                        ? 'nav-link-active'
                        : 'text-content-secondary hover:text-content hover:bg-surface-secondary',
                    )}
                  >
                    {link.icon}
                    {link.label}
                  </A>
                ))}
              </div>
            </div>

            {/* Right Side: User Info + Theme Toggle + Mobile Menu */}
            <div class="flex items-center gap-1">
              {/* Desktop user menu */}
              <div class="hidden md:flex items-center gap-2">
                <Show when={currentUser()}>
                  <span class="text-sm text-content-secondary">{currentUser()!.username}</span>
                </Show>
                <A
                  href="/settings"
                  class={cn(
                    'nav-link-base',
                    isActive('/settings')
                      ? 'nav-link-active'
                      : 'text-content-secondary hover:text-content hover:bg-surface-secondary',
                  )}
                >
                  设置
                </A>
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  class="text-content-secondary hover:text-content"
                  onClick={handleLogout}
                >
                  退出
                </Button>
              </div>

              {/* Theme Toggle */}
              <Button
                type="button"
                variant="ghost"
                size="sm"
                class="nav-icon-btn h-9 w-9 !px-0"
                onClick={toggleTheme}
                aria-label={theme() === 'dark' ? '切换到亮色模式' : '切换到暗色模式'}
              >
                <Show
                  when={theme() === 'dark'}
                  fallback={
                    <svg class="w-[18px] h-[18px]" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                      <path stroke-linecap="round" stroke-linejoin="round" d="M20.354 15.354A9 9 0 018.646 3.646 9.003 9.003 0 0012 21a9.003 9.003 0 008.354-5.646z" />
                    </svg>
                  }
                >
                  <svg class="w-[18px] h-[18px]" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                    <path stroke-linecap="round" stroke-linejoin="round" d="M12 3v1m0 16v1m9-9h-1M4 12H3m15.364 6.364l-.707-.707M6.343 6.343l-.707-.707m12.728 0l-.707.707M6.343 17.657l-.707.707M16 12a4 4 0 11-8 0 4 4 0 018 0z" />
                  </svg>
                </Show>
              </Button>

              {/* Mobile Menu Button */}
              <Button
                type="button"
                variant="ghost"
                size="sm"
                class="md:hidden nav-icon-btn h-9 w-9 !px-0"
                onClick={() => setMobileMenuOpen(!mobileMenuOpen())}
                aria-label="切换导航菜单"
                aria-expanded={mobileMenuOpen()}
              >
                <Show
                  when={!mobileMenuOpen()}
                  fallback={
                    <svg class="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                      <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
                    </svg>
                  }
                >
                  <svg class="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                    <path stroke-linecap="round" stroke-linejoin="round" d="M4 6h16M4 12h16M4 18h16" />
                  </svg>
                </Show>
              </Button>
            </div>
          </div>
        </div>

        {/* Mobile Menu */}
        <Show when={getMobileMenuRender()}>
          <div class={cn('md:hidden border-t border-border/50 bg-surface/80 backdrop-blur-xl', getMobileMenuClass())}>
            <div class="max-w-screen-2xl mx-auto px-4 lg:px-8 py-2">
              {navLinks.map((link) => (
                <A
                  href={link.href}
                  class={cn(
                    'flex items-center gap-2 nav-link-base',
                    isActive(link.href)
                      ? 'nav-link-active'
                      : 'text-content-secondary hover:text-content hover:bg-surface-secondary',
                  )}
                  onClick={() => setMobileMenuOpen(false)}
                >
                  {link.icon}
                  {link.label}
                </A>
              ))}
              <div class="border-t border-border/50 mt-2 pt-2">
                <Show when={currentUser()}>
                  <div class="px-3 py-2 text-sm text-content-tertiary">{currentUser()!.username}</div>
                </Show>
                <A
                  href="/settings"
                  class={cn(
                    'block nav-link-base',
                    isActive('/settings')
                      ? 'nav-link-active'
                      : 'text-content-secondary hover:text-content hover:bg-surface-secondary',
                  )}
                  onClick={() => setMobileMenuOpen(false)}
                >
                  设置
                </A>
                <button
                  type="button"
                  class="block w-full text-left nav-link-base text-content-secondary hover:text-content hover:bg-surface-secondary cursor-pointer"
                  onClick={() => { setMobileMenuOpen(false); handleLogout() }}
                >
                  退出
                </button>
              </div>
            </div>
          </div>
        </Show>
      </nav>

      {/* Main Content */}
      <main class="max-w-screen-2xl mx-auto px-4 lg:px-8 py-6">
        {/* Breadcrumb — inline at top of content */}
        <Show when={location.pathname !== '/'}>
          <div class="mb-4">
            <Breadcrumbs />
          </div>
        </Show>
        <div class={cn('route-view-shell', entering() && 'route-view-enter')}>
          {props.children}
        </div>
      </main>
    </div>
  )
}

function Breadcrumbs() {
  const location = useLocation()

  const crumbs = () => {
    const path = location.pathname
    const parts: Array<{ label: string; href?: string }> = [
      { label: '首页', href: '/' },
    ]

    if (path.startsWith('/patients/new')) {
      parts.push({ label: '新增患者' })
    } else if (path.match(/^\/patients\/[^/]+\/edit$/)) {
      parts.push({ label: '患者详情', href: path.replace('/edit', '') })
      parts.push({ label: '编辑' })
    } else if (path.match(/^\/patients\/[^/]+\/trends$/)) {
      parts.push({ label: '患者详情', href: path.replace('/trends', '') })
      parts.push({ label: '趋势分析' })
    } else if (path.match(/^\/patients\/[^/]+$/)) {
      parts.push({ label: '患者详情' })
    } else if (path.match(/^\/reports\/[^/]+$/)) {
      parts.push({ label: '报告详情' })
    } else if (path === '/settings') {
      parts.push({ label: '用户设置' })
    } else if (path === '/admin/users') {
      parts.push({ label: '用户管理' })
    } else if (path.match(/^\/patients\/[^/]+\/compare$/)) {
      parts.push({ label: '患者详情', href: path.replace('/compare', '') })
      parts.push({ label: '报告对比' })
    } else if (path.match(/^\/patients\/[^/]+\/timeline$/)) {
      parts.push({ label: '患者详情', href: path.replace('/timeline', '') })
      parts.push({ label: '健康时间线' })
    } else if (path.match(/^\/patients\/[^/]+\/medications$/)) {
      parts.push({ label: '患者详情', href: path.replace('/medications', '') })
      parts.push({ label: '用药管理' })
    } else if (path.match(/^\/patients\/[^/]+\/templates$/)) {
      parts.push({ label: '患者详情', href: path.replace('/templates', '') })
      parts.push({ label: '快捷录入' })
    } else if (path.match(/^\/patients\/[^/]+\/health-assessment$/)) {
      parts.push({ label: '患者详情', href: path.replace('/health-assessment', '') })
      parts.push({ label: 'AI 健康评估' })
    } else if (path.match(/^\/expenses\/[^/]+$/)) {
      parts.push({ label: '消费详情' })
    }

    return parts
  }

  return (
    <nav class="breadcrumb" aria-label="面包屑">
      {crumbs().map((crumb, i) => (
        <>
          <Show when={i > 0}>
            <svg class="w-4 h-4 text-content-tertiary" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
              <path stroke-linecap="round" stroke-linejoin="round" d="M9 5l7 7-7 7" />
            </svg>
          </Show>
          <Show
            when={crumb.href}
            fallback={<span class="text-content-secondary">{crumb.label}</span>}
          >
            <A href={crumb.href!} class="breadcrumb-link">
              {crumb.label}
            </A>
          </Show>
        </>
      ))}
    </nav>
  )
}
