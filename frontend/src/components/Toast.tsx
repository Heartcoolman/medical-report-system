import type { Component, JSX } from 'solid-js'
import { For, createContext, createSignal, onCleanup, useContext } from 'solid-js'
import { Portal } from 'solid-js/web'
import { cn } from '@/lib/utils'

export type ToastVariant = 'success' | 'error' | 'warning' | 'info'

export interface ToastAction {
  label: string
  onClick: () => void
}

export interface ToastMessage {
  id: number
  variant: ToastVariant
  message: string
  duration?: number
  action?: ToastAction
}

interface ToastContextValue {
  toast: (variant: ToastVariant, message: string, options?: { duration?: number; action?: ToastAction }) => void
}

const ToastContext = createContext<ToastContextValue>()

export function useToast(): ToastContextValue {
  const ctx = useContext(ToastContext)
  if (!ctx) throw new Error('useToast must be used within a ToastProvider')
  return ctx
}

let toastId = 0
const TOAST_AUTO_CLOSE_DURATION = 4000
const TOAST_EXIT_DURATION = 180

export const ToastProvider: Component<{ children: JSX.Element }> = (props) => {
  const [toasts, setToasts] = createSignal<ToastMessage[]>([])
  const [exitingToasts, setExitingToasts] = createSignal<Set<number>>(new Set())
  const autoCloseTimers = new Map<number, ReturnType<typeof setTimeout>>()
  const exitTimers = new Map<number, ReturnType<typeof setTimeout>>()

  const finalizeRemoveToast = (id: number) => {
    setToasts((prev) => prev.filter((t) => t.id !== id))
    setExitingToasts((prev) => {
      const next = new Set(prev)
      next.delete(id)
      return next
    })
    autoCloseTimers.delete(id)
    exitTimers.delete(id)
  }

  const removeToast = (id: number) => {
    if (exitingToasts().has(id)) return

    setExitingToasts((prev) => {
      const next = new Set(prev)
      next.add(id)
      return next
    })

    const exitTimer = window.setTimeout(() => {
      finalizeRemoveToast(id)
    }, TOAST_EXIT_DURATION)
    exitTimers.set(id, exitTimer)
  }

  const clearAllTimers = () => {
    autoCloseTimers.forEach(clearTimeout)
    exitTimers.forEach(clearTimeout)
    autoCloseTimers.clear()
    exitTimers.clear()
  }

  const toast = (variant: ToastVariant, message: string, options?: { duration?: number; action?: ToastAction }) => {
    const id = ++toastId
    const duration = options?.duration ?? TOAST_AUTO_CLOSE_DURATION
    setToasts((prev) => [...prev, { id, variant, message, duration, action: options?.action }])

    if (duration > 0) {
      const timer = window.setTimeout(() => removeToast(id), duration)
      autoCloseTimers.set(id, timer)
    }
  }

  const handleClose = (id: number) => {
    const timer = autoCloseTimers.get(id)
    if (timer) {
      clearTimeout(timer)
      autoCloseTimers.delete(id)
    }
    removeToast(id)
  }

  onCleanup(() => {
    clearAllTimers()
  })

  return (
    <ToastContext.Provider value={{ toast }}>
      {props.children}
      <Portal>
        <div class="fixed bottom-6 left-1/2 -translate-x-1/2 z-[100] flex flex-col-reverse gap-2 pointer-events-none items-center" aria-live="polite" role="status">
          <For each={toasts()}>
            {(t) => (
              <ToastItem
                variant={t.variant}
                message={t.message}
                onClose={() => handleClose(t.id)}
                exiting={exitingToasts().has(t.id)}
                action={t.action}
              />
            )}
          </For>
        </div>
      </Portal>
    </ToastContext.Provider>
  )
}

const variantStyles: Record<ToastVariant, string> = {
  success: 'bg-success-light border-success text-success',
  error: 'bg-error-light border-error text-error',
  warning: 'bg-warning-light border-warning text-warning',
  info: 'bg-info-light border-info text-info',
}

const variantIcons: Record<ToastVariant, string> = {
  success: 'M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z',
  error: 'M10 14l2-2m0 0l2-2m-2 2l-2-2m2 2l2 2m7-2a9 9 0 11-18 0 9 9 0 0118 0z',
  warning: 'M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4c-.77-.833-1.964-.833-2.732 0L3.34 16.5c-.77.833.192 2.5 1.732 2.5z',
  info: 'M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z',
}

interface ToastItemProps {
  variant: ToastVariant
  message: string
  onClose: () => void
  exiting?: boolean
  action?: ToastAction
}

const ToastItem: Component<ToastItemProps> = (props) => {
  return (
    <div
      class={cn(
        'pointer-events-auto flex items-center gap-3 rounded-2xl border px-5 py-3 shadow-xl',
        'min-w-[280px] max-w-[420px]',
        props.exiting ? 'toast-item-leave' : 'toast-item-enter',
        variantStyles[props.variant],
      )}
      role="alert"
    >
      <svg class="h-5 w-5 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
        <path stroke-linecap="round" stroke-linejoin="round" d={variantIcons[props.variant]} />
      </svg>

      <p class="flex-1 text-sm font-medium">{props.message}</p>

      {props.action && (
        <button
          onClick={() => { props.action!.onClick(); props.onClose() }}
          class="shrink-0 text-sm font-semibold underline-offset-2 hover:underline transition-all duration-[var(--transition-fast)] cursor-pointer opacity-90 hover:opacity-100"
        >
          {props.action.label}
        </button>
      )}

      <button
        onClick={() => props.onClose()}
        class="shrink-0 p-0.5 rounded hover:bg-content-tertiary/15 transition-all duration-[var(--transition-fast)] cursor-pointer"
        aria-label="关闭通知"
      >
        <svg class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
          <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
        </svg>
      </button>
    </div>
  )
}
