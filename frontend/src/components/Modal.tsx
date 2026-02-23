import type { Component, JSX } from 'solid-js'
import { Show, createEffect, createSignal, createUniqueId, onCleanup } from 'solid-js'
import { Portal } from 'solid-js/web'
import { cn } from '@/lib/utils'

let openModalCount = 0

export interface ModalProps {
  open: boolean
  onClose: () => void
  size?: 'sm' | 'md' | 'lg' | 'xl' | '2xl' | '3xl' | '4xl'
  title?: string
  children: JSX.Element
  footer?: JSX.Element
  class?: string
}

const sizeStyles: Record<NonNullable<ModalProps['size']>, string> = {
  sm: 'max-w-sm',
  md: 'max-w-md',
  lg: 'max-w-lg',
  xl: 'max-w-xl',
  '2xl': 'max-w-2xl',
  '3xl': 'max-w-3xl',
  '4xl': 'max-w-4xl',
}

type ModalTransitionState = 'closed' | 'opening' | 'opened' | 'closing'

const MODAL_OPEN_DURATION = 220
const MODAL_CLOSE_DURATION = 180

export const Modal: Component<ModalProps> = (props) => {
  let dialogRef: HTMLDivElement | undefined
  const titleId = createUniqueId()
  const [visible, setVisible] = createSignal(false)
  const [transitionState, setTransitionState] = createSignal<ModalTransitionState>(
    props.open ? 'opening' : 'closed',
  )
  const [isRendered, setIsRendered] = createSignal(props.open)

  let enterTimer: ReturnType<typeof setTimeout> | undefined
  let closeTimer: ReturnType<typeof setTimeout> | undefined

  const clearTransitionTimers = () => {
    if (enterTimer) {
      window.clearTimeout(enterTimer)
      enterTimer = undefined
    }

    if (closeTimer) {
      window.clearTimeout(closeTimer)
      closeTimer = undefined
    }
  }

  const scheduleRenderClose = () => {
    closeTimer = window.setTimeout(() => {
      setIsRendered(false)
      setTransitionState('closed')
      setVisible(false)
      closeTimer = undefined
    }, MODAL_CLOSE_DURATION)
  }

  createEffect(() => {
    if (props.open) {
      clearTransitionTimers()
      setVisible(true)
      setIsRendered(true)
      setTransitionState('opening')
      openModalCount++
      document.body.style.overflow = 'hidden'
      enterTimer = window.setTimeout(() => {
        setTransitionState('opened')
        enterTimer = undefined
      }, MODAL_OPEN_DURATION)
    } else {
      clearTransitionTimers()
      setTransitionState('closing')
      openModalCount--
      if (openModalCount <= 0) {
        openModalCount = 0
        document.body.style.overflow = ''
      }
      if (visible()) {
        scheduleRenderClose()
      } else {
        setTransitionState('closed')
        setIsRendered(false)
      }
    }
  })

  onCleanup(() => {
    clearTransitionTimers()
    if (props.open) {
      openModalCount--
      if (openModalCount <= 0) {
        openModalCount = 0
        document.body.style.overflow = ''
      }
    }
  })

  const getModalClass = () => {
    if (transitionState() === 'opening') return 'modal-enter'
    if (transitionState() === 'closing') return 'modal-leave'
    return ''
  }

  const getBackdropClass = () => {
    if (transitionState() === 'opening') return 'modal-backdrop-enter'
    if (transitionState() === 'closing') return 'modal-backdrop-leave'
    return ''
  }

  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === 'Escape') {
      props.onClose()
      return
    }
    if (e.key === 'Tab' && dialogRef) {
      const focusable = dialogRef.querySelectorAll<HTMLElement>(
        'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])'
      )
      if (focusable.length === 0) return
      const first = focusable[0]
      const last = focusable[focusable.length - 1]
      if (e.shiftKey) {
        if (document.activeElement === first) {
          e.preventDefault()
          last.focus()
        }
      } else {
        if (document.activeElement === last) {
          e.preventDefault()
          first.focus()
        }
      }
    }
  }

  createEffect(() => {
    if (props.open) {
      document.addEventListener('keydown', handleKeyDown)
    } else {
      document.removeEventListener('keydown', handleKeyDown)
    }
    onCleanup(() => document.removeEventListener('keydown', handleKeyDown))
  })

  createEffect(() => {
    if (props.open && dialogRef) {
      const focusable = dialogRef.querySelectorAll<HTMLElement>(
        'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])'
      )
      if (focusable.length > 0) {
        focusable[0].focus()
      }
    }
  })

  const handleBackdropClick = (e: MouseEvent) => {
    if (e.target === e.currentTarget) {
      props.onClose()
    }
  }

  return (
    <Show when={isRendered()}>
      <Portal>
        <div
          class="fixed inset-0 z-50 flex items-center justify-center p-4"
          role="dialog"
          aria-modal="true"
          aria-labelledby={props.title ? titleId : undefined}
          onClick={handleBackdropClick}
        >
          <div class={cn('modal-backdrop', getBackdropClass())} />

          <div
            ref={dialogRef}
            class={cn(
              'relative w-full bg-surface-elevated rounded-2xl shadow-xl flex flex-col max-h-[calc(100vh-2rem)]',
              getModalClass(),
              sizeStyles[props.size ?? 'md'],
              props.class,
            )}
          >
            <Show when={props.title}>
              <div class="flex items-center justify-between px-6 py-4 border-b border-border shrink-0">
                <h2 id={titleId} class="section-title">{props.title}</h2>
                <button
                  onClick={() => props.onClose()}
                  class="p-1.5 rounded-xl text-content-tertiary hover:text-content hover:bg-surface-secondary transition-all duration-200 cursor-pointer"
                  aria-label="关闭"
                >
                  <svg class="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                    <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
                  </svg>
                </button>
              </div>
            </Show>

            <div class="px-6 py-4 flex-1 min-h-0 overflow-y-auto">
              {props.children}
            </div>

            <Show when={props.footer}>
              <div class="px-6 py-4 border-t border-border flex justify-end gap-2 shrink-0">
                {props.footer}
              </div>
            </Show>
          </div>
        </div>
      </Portal>
    </Show>
  )
}
