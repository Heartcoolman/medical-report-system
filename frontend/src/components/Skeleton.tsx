import type { Component, JSX } from 'solid-js'
import { cn } from '@/lib/utils'

export interface SkeletonProps extends JSX.HTMLAttributes<HTMLDivElement> {
  variant?: 'text' | 'circle' | 'rect' | 'custom'
  width?: string | number
  height?: string | number
}

const variantStyles: Record<NonNullable<SkeletonProps['variant']>, string> = {
  text: 'h-4 w-full rounded-md',
  circle: 'rounded-full',
  rect: 'rounded-lg',
  custom: '',
}

export const Skeleton: Component<SkeletonProps> = (props) => {
  const style = (): JSX.CSSProperties => {
    const s: JSX.CSSProperties = {}
    if (props.width) s.width = typeof props.width === 'number' ? `${props.width}px` : props.width
    if (props.height) s.height = typeof props.height === 'number' ? `${props.height}px` : props.height
    return s
  }

  return (
    <div
      {...props}
      aria-hidden="true"
      class={cn(
        'bg-surface-tertiary',
        'bg-[length:200%_100%] bg-gradient-to-r from-surface-tertiary via-surface-secondary to-surface-tertiary',
        'animate-[shimmer_1.5s_ease-in-out_infinite]',
        variantStyles[props.variant ?? 'text'],
        props.class,
      )}
      style={style()}
    />
  )
}
