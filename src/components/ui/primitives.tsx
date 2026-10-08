import * as React from 'react'
import * as Collapsible from '@radix-ui/react-collapsible'
import * as SelectPrimitive from '@radix-ui/react-select'
import * as SwitchPrimitive from '@radix-ui/react-switch'
import * as TooltipPrimitive from '@radix-ui/react-tooltip'
import { Check, ChevronDown, Info } from 'lucide-react'
import { cn } from '@/lib/utils'

/* 一组最小的 UI 原语（shadcn/ui 风格：源码在仓库里，不做成库）。 */

export function Button({
  variant = 'primary',
  size = 'md',
  className,
  ...props
}: React.ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: 'primary' | 'secondary' | 'ghost' | 'danger'
  size?: 'sm' | 'md' | 'lg'
}) {
  const variants = {
    primary: 'bg-[var(--color-accent)] text-white hover:brightness-110 shadow-sm',
    secondary: 'bg-white text-[var(--color-ink)] border border-[var(--color-line)] hover:bg-[var(--color-canvas)]',
    ghost: 'text-[var(--color-ink-soft)] hover:bg-[var(--color-canvas)]',
    danger: 'bg-[var(--color-bad)] text-white hover:brightness-110',
  }
  const sizes = { sm: 'h-8 px-3 text-[13px]', md: 'h-10 px-4', lg: 'h-12 px-6 text-[16px]' }
  return (
    <button
      className={cn(
        'inline-flex items-center justify-center gap-2 rounded-lg font-medium transition',
        'disabled:opacity-40 disabled:pointer-events-none focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--color-accent)]',
        variants[variant],
        sizes[size],
        className,
      )}
      {...props}
    />
  )
}

export function Card({ className, ...props }: React.HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      className={cn(
        'rounded-[var(--radius-card)] border border-[var(--color-line)] bg-[var(--color-surface)]',
        className,
      )}
      {...props}
    />
  )
}

export function Badge({
  tone = 'neutral',
  className,
  ...props
}: React.HTMLAttributes<HTMLSpanElement> & { tone?: 'neutral' | 'good' | 'warn' | 'bad' | 'accent' }) {
  const tones = {
    neutral: 'bg-[var(--color-canvas)] text-[var(--color-ink-soft)] border-[var(--color-line)]',
    good: 'bg-[var(--color-good-soft)] text-[var(--color-good)] border-transparent',
    warn: 'bg-[var(--color-warn-soft)] text-[oklch(0.45_0.12_70)] border-transparent',
    bad: 'bg-[var(--color-bad-soft)] text-[var(--color-bad)] border-transparent',
    accent: 'bg-[var(--color-accent-soft)] text-[var(--color-accent)] border-transparent',
  }
  return (
    <span
      className={cn(
        'inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-[12px] font-medium',
        tones[tone],
        className,
      )}
      {...props}
    />
  )
}

export function Progress({ value, className }: { value: number; className?: string }) {
  return (
    <div className={cn('h-2 w-full overflow-hidden rounded-full bg-[var(--color-line)]', className)}>
      <div
        className="h-full rounded-full bg-[var(--color-accent)] transition-[width] duration-300"
        style={{ width: `${Math.max(0, Math.min(100, value))}%` }}
      />
    </div>
  )
}

/** 高级设置：默认收起、视觉弱化，不打断新手的主线。 */
export function Advanced({
  label,
  hint,
  children,
  defaultOpen = false,
}: {
  label: string
  hint?: string
  children: React.ReactNode
  defaultOpen?: boolean
}) {
  const [open, setOpen] = React.useState(defaultOpen)
  return (
    <Collapsible.Root open={open} onOpenChange={setOpen} className="mt-4">
      <Collapsible.Trigger
        className={cn(
          'group inline-flex items-center gap-1.5 rounded-md px-2 py-1 -ml-2 text-[13px] text-[var(--color-ink-faint)]',
          'hover:text-[var(--color-ink-soft)] hover:bg-[var(--color-canvas)] transition',
        )}
      >
        <ChevronDown className={cn('h-3.5 w-3.5 transition-transform', open && 'rotate-180')} />
        {label}
      </Collapsible.Trigger>
      <Collapsible.Content className="fade-in">
        <div className="mt-3 rounded-xl border border-dashed border-[var(--color-line)] bg-[var(--color-canvas)] p-4">
          {hint && <p className="mb-3 text-[13px] text-[var(--color-ink-faint)]">{hint}</p>}
          {children}
        </div>
      </Collapsible.Content>
    </Collapsible.Root>
  )
}

export function Field({
  label,
  help,
  children,
}: {
  label: string
  help?: string
  children: React.ReactNode
}) {
  return (
    <label className="flex items-center justify-between gap-6 py-2">
      <span className="text-[13px]">
        <span className="text-[var(--color-ink)]">{label}</span>
        {help && <span className="ml-2 text-[var(--color-ink-faint)]">{help}</span>}
      </span>
      <span className="shrink-0">{children}</span>
    </label>
  )
}

export function NumberInput({
  value,
  onChange,
  step = 1,
  min,
  max,
  width = 'w-28',
}: {
  value: number
  onChange: (v: number) => void
  step?: number
  min?: number
  max?: number
  width?: string
}) {
  return (
    <input
      type="number"
      className={cn(
        'tnum h-9 rounded-lg border border-[var(--color-line)] bg-white px-2 text-right text-[13px]',
        'focus:outline-2 focus:outline-offset-0 focus:outline-[var(--color-accent)]',
        width,
      )}
      value={value}
      step={step}
      min={min}
      max={max}
      onChange={(e) => onChange(Number(e.target.value))}
    />
  )
}

export function Toggle({
  checked,
  onChange,
  label,
}: {
  checked: boolean
  onChange: (v: boolean) => void
  label?: string
}) {
  return (
    <span className="inline-flex items-center gap-2">
      {label && <span className="text-[13px] text-[var(--color-ink-soft)]">{label}</span>}
      <SwitchPrimitive.Root
        checked={checked}
        onCheckedChange={onChange}
        className={cn(
          'relative h-6 w-10 rounded-full border border-transparent transition',
          checked ? 'bg-[var(--color-accent)]' : 'bg-[var(--color-line)]',
        )}
      >
        <SwitchPrimitive.Thumb className="block h-5 w-5 translate-x-0.5 rounded-full bg-white shadow transition-transform data-[state=checked]:translate-x-[18px]" />
      </SwitchPrimitive.Root>
    </span>
  )
}

export function Select<T extends string>({
  value,
  onChange,
  options,
  className,
}: {
  value: T
  onChange: (v: T) => void
  options: { value: T; label: string }[]
  className?: string
}) {
  return (
    <SelectPrimitive.Root value={value} onValueChange={(v) => onChange(v as T)}>
      <SelectPrimitive.Trigger
        className={cn(
          'inline-flex h-9 items-center justify-between gap-2 rounded-lg border border-[var(--color-line)] bg-white px-3 text-[13px]',
          'focus:outline-2 focus:outline-offset-0 focus:outline-[var(--color-accent)]',
          className,
        )}
      >
        <SelectPrimitive.Value />
        <SelectPrimitive.Icon>
          <ChevronDown className="h-3.5 w-3.5 text-[var(--color-ink-faint)]" />
        </SelectPrimitive.Icon>
      </SelectPrimitive.Trigger>
      <SelectPrimitive.Portal>
        <SelectPrimitive.Content
          position="popper"
          sideOffset={6}
          className="z-50 overflow-hidden rounded-xl border border-[var(--color-line)] bg-white shadow-lg"
        >
          <SelectPrimitive.Viewport className="p-1">
            {options.map((o) => (
              <SelectPrimitive.Item
                key={o.value}
                value={o.value}
                className="flex cursor-default items-center gap-2 rounded-lg px-3 py-2 text-[13px] data-[highlighted]:bg-[var(--color-canvas)] data-[highlighted]:outline-none"
              >
                <SelectPrimitive.ItemText>{o.label}</SelectPrimitive.ItemText>
                <SelectPrimitive.ItemIndicator className="ml-auto">
                  <Check className="h-3.5 w-3.5 text-[var(--color-accent)]" />
                </SelectPrimitive.ItemIndicator>
              </SelectPrimitive.Item>
            ))}
          </SelectPrimitive.Viewport>
        </SelectPrimitive.Content>
      </SelectPrimitive.Portal>
    </SelectPrimitive.Root>
  )
}

/** 名词解释气泡：初学者点一下就懂，不占版面。 */
export function Explain({ children }: { children: React.ReactNode }) {
  return (
    <TooltipPrimitive.Provider delayDuration={120}>
      <TooltipPrimitive.Root>
        <TooltipPrimitive.Trigger asChild>
          <button
            type="button"
            className="inline-flex h-4 w-4 items-center justify-center rounded-full text-[var(--color-ink-faint)] hover:text-[var(--color-accent)]"
          >
            <Info className="h-3.5 w-3.5" />
          </button>
        </TooltipPrimitive.Trigger>
        <TooltipPrimitive.Portal>
          <TooltipPrimitive.Content
            sideOffset={6}
            className="z-50 max-w-xs rounded-lg bg-[var(--color-ink)] px-3 py-2 text-[12.5px] leading-relaxed text-white shadow-lg"
          >
            {children}
            <TooltipPrimitive.Arrow className="fill-[var(--color-ink)]" />
          </TooltipPrimitive.Content>
        </TooltipPrimitive.Portal>
      </TooltipPrimitive.Root>
    </TooltipPrimitive.Provider>
  )
}

export function Note({
  tone = 'neutral',
  title,
  children,
}: {
  tone?: 'neutral' | 'good' | 'warn' | 'bad' | 'accent'
  title?: string
  children?: React.ReactNode
}) {
  const tones = {
    neutral: 'bg-[var(--color-canvas)] border-[var(--color-line)]',
    good: 'bg-[var(--color-good-soft)] border-transparent',
    warn: 'bg-[var(--color-warn-soft)] border-transparent',
    bad: 'bg-[var(--color-bad-soft)] border-transparent',
    accent: 'bg-[var(--color-accent-soft)] border-transparent',
  }
  return (
    <div className={cn('rounded-xl border p-4 text-[13.5px] leading-relaxed', tones[tone])}>
      {title && <div className="mb-1 font-semibold">{title}</div>}
      <div className="text-[var(--color-ink-soft)]">{children}</div>
    </div>
  )
}
