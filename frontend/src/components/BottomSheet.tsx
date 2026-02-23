import type { Component, JSX } from 'solid-js'
import { Show, createEffect, createSignal, onCleanup } from 'solid-js'
import { Portal } from 'solid-js/web'
import { cn } from '@/lib/utils'

let openSheetCount = 0

export interface BottomSheetProps {
  open: boolean
  onClose: () => void
  title?: string
  children: JSX.Element
  class?: string
}

type SheetTransitionState = 'closed' | 'opening' | 'opened' | 'closing'

const SHEET_OPEN_DURATION = 300
const SHEET_CLOSE_DURATION = 220

export const BottomSheet: Component<BottomSheetProps> = (props) => {
  const [visible, setVisible] = createSignal(false)
  const [transitionState, setTransitionState] = createSignal<SheetTransitionState>(
    props.open ? 'opening' : 'closed',
  )
  const [isRendered, setIsRendered] = createSignal(props.open)

  let sheetRef: HTMLDivElement | undefined
  let startY = 0
  let currentY = 0
  let isDragging = false

  let enterTimer: ReturnType<typeof setTimeout> | undefined
  let closeTimer: ReturnType<typeof setTimeout> | undefined

  const clearTimers = () => {
    if (enterTimer) { window.clearTimeout(enterTimer); enterTimer = undefined }
    if (closeTimer) { window.clearTimeout(closeTimer); closeTimer = undefined }
  }

  const scheduleClose = () => {
    closeTimer = window.setTimeout(() => {
      setIsRendered(false)
      setTransitionState('closed')
      setVisible(false)
      closeTimer = undefined
    }, SHEET_CLOSE_DURATION)
  }

  createEffect(() => {
    if (props.open) {
      clearTimers()
      setVisible(true)
      setIsRendered(true)
      setTransitionState('opening')
      openSheetCount++
      document.body.style.overflow = 'hidden'
      enterTimer = window.setTimeout(() => {
        setTransitionState('opened')
        enterTimer = undefined
      }, SHEET_OPEN_DURATION)
    } else {
      clearTimers()
      setTransitionState('closing')
      openSheetCount--
      if (openSheetCount <= 0) {
        openSheetCount = 0
        document.body.style.overflow = ''
      }
      if (visible()) {
        scheduleClose()
      } else {
        setTransitionState('closed')
        setIsRendered(false)
      }
    }
  })

  onCleanup(() => {
    clearTimers()
    if (props.open) {
      openSheetCount--
      if (openSheetCount <= 0) {
        openSheetCount = 0
        document.body.style.overflow = ''
      }
    }
  })

  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === 'Escape') props.onClose()
  }

  createEffect(() => {
    if (props.open) {
      document.addEventListener('keydown', handleKeyDown)
    } else {
      document.removeEventListener('keydown', handleKeyDown)
    }
    onCleanup(() => document.removeEventListener('keydown', handleKeyDown))
  })

  const handleBackdropClick = (e: MouseEvent) => {
    if (e.target === e.currentTarget) props.onClose()
  }

  // Drag-to-dismiss
  const handleDragStart = (e: TouchEvent | MouseEvent) => {
    isDragging = true
    startY = 'touches' in e ? e.touches[0].clientY : e.clientY
    currentY = 0
    if (sheetRef) sheetRef.style.transition = 'none'
  }

  const handleDragMove = (e: TouchEvent | MouseEvent) => {
    if (!isDragging) return
    const y = 'touches' in e ? e.touches[0].clientY : e.clientY
    currentY = Math.max(0, y - startY)
    if (sheetRef) sheetRef.style.transform = `translateY(${currentY}px)`
  }

  const handleDragEnd = () => {
    if (!isDragging) return
    isDragging = false
    if (sheetRef) {
      sheetRef.style.transition = ''
      sheetRef.style.transform = ''
    }
    if (currentY > 100) {
      props.onClose()
    }
    currentY = 0
  }

  const getSheetClass = () => {
    if (transitionState() === 'opening') return 'bottom-sheet-enter'
    if (transitionState() === 'closing') return 'bottom-sheet-leave'
    return ''
  }

  const getBackdropClass = () => {
    if (transitionState() === 'opening') return 'modal-backdrop-enter'
    if (transitionState() === 'closing') return 'modal-backdrop-leave'
    return ''
  }

  return (
    <Show when={isRendered()}>
      <Portal>
        <div
          class="fixed inset-0 z-50 flex items-end justify-center"
          onClick={handleBackdropClick}
        >
          <div class={cn('modal-backdrop', getBackdropClass())} />

          <div
            ref={sheetRef}
            class={cn(
              'relative w-full max-w-lg bg-surface-elevated rounded-t-3xl shadow-xl flex flex-col',
              'max-h-[85vh] transition-transform duration-300 ease-out',
              getSheetClass(),
              props.class,
            )}
            role="dialog"
            aria-modal="true"
            onTouchStart={handleDragStart}
            onTouchMove={handleDragMove}
            onTouchEnd={handleDragEnd}
            onMouseDown={handleDragStart}
            onMouseMove={handleDragMove}
            onMouseUp={handleDragEnd}
          >
            {/* Drag Handle */}
            <div class="flex justify-center pt-3 pb-1 cursor-grab active:cursor-grabbing shrink-0">
              <div class="w-10 h-1 rounded-full bg-content-tertiary/40" />
            </div>

            <Show when={props.title}>
              <div class="px-6 py-3 shrink-0">
                <h2 class="text-lg font-semibold text-content">{props.title}</h2>
              </div>
            </Show>

            <div class="px-6 pb-6 flex-1 min-h-0 overflow-y-auto">
              {props.children}
            </div>
          </div>
        </div>
      </Portal>
    </Show>
  )
}
