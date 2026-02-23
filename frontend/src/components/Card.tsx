import type { Component, JSX } from 'solid-js'
import { cn } from '@/lib/utils'

export interface CardProps extends JSX.HTMLAttributes<HTMLDivElement> {
  variant?: 'elevated' | 'outlined' | 'filled' | 'glass'
  interactive?: boolean
}

const variantStyles: Record<NonNullable<CardProps['variant']>, string> = {
  elevated: 'bg-surface-elevated border border-border/40 shadow-lg hover:shadow-xl hover:border-border/60',
  outlined: 'bg-surface-elevated border border-border/60 hover:border-border-hover shadow-sm hover:shadow-md',
  filled: 'bg-surface-secondary hover:bg-surface-tertiary shadow-sm',
  glass: 'bg-surface/80 backdrop-blur-sm border border-border/40 shadow-lg hover:shadow-xl',
}

export const Card: Component<CardProps> = (props) => {
  return (
    <div
      {...props}
      class={cn(
        'rounded-2xl transition-all duration-200 ease-[cubic-bezier(0.34,1.56,0.64,1)]',
        variantStyles[props.variant ?? 'elevated'],
        props.interactive && 'cursor-pointer active:scale-[0.97] active:shadow-sm',
        props.class,
      )}
    >
      {props.children}
    </div>
  )
}

export interface CardHeaderProps extends JSX.HTMLAttributes<HTMLDivElement> {}

export const CardHeader: Component<CardHeaderProps> = (props) => {
  return (
    <div
      {...props}
      class={cn('px-6 py-4 border-b border-border', props.class)}
    >
      {props.children}
    </div>
  )
}

export interface CardBodyProps extends JSX.HTMLAttributes<HTMLDivElement> {}

export const CardBody: Component<CardBodyProps> = (props) => {
  return (
    <div {...props} class={cn('px-6 py-4', props.class)}>
      {props.children}
    </div>
  )
}

export interface CardFooterProps extends JSX.HTMLAttributes<HTMLDivElement> {}

export const CardFooter: Component<CardFooterProps> = (props) => {
  return (
    <div
      {...props}
      class={cn('px-6 py-4 border-t border-border', props.class)}
    >
      {props.children}
    </div>
  )
}
