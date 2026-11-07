import { CircleIcon, SpinnerIcon } from '@phosphor-icons/react'
import { Badge } from '@/components/ui/badge'
import { cn } from '@/lib/utils'
import type { Activity } from '../../shared/types'

/**
 * What a tab/project should signal:
 * - needs-you: Claude waits for a permission/answer/plan approval (always shown)
 * - error: the turn failed (rate limit, overload, …)
 * - done: Claude finished and you haven't looked at that tab yet
 * - working: Claude is busy
 * Nothing is shown for idle tabs or finished work you've already seen.
 */
export type Signal = 'needs-you' | 'error' | 'done' | 'working' | null

const RANK: Record<Exclude<Signal, null>, number> = { 'needs-you': 4, error: 3, done: 2, working: 1 }

export function signalOf(activity: Activity | undefined, unseen: boolean): Signal {
  if (activity === 'needs-you') return 'needs-you'
  if (activity === 'error') return unseen ? 'error' : null
  if (activity === 'done') return unseen ? 'done' : null
  if (activity === 'working') return 'working'
  return null
}

export function strongest(signals: Signal[]): Signal {
  return signals.reduce<Signal>((best, s) => (s && (!best || RANK[s] > RANK[best]) ? s : best), null)
}

const LABEL: Record<Exclude<Signal, null>, string> = {
  'needs-you': 'Claude is waiting for you',
  error: 'Claude stopped with an error',
  done: 'Claude finished. Not viewed yet.',
  working: 'Claude is working'
}

export function SignalDot({ signal, className }: { signal: Signal; className?: string }) {
  if (!signal) return null
  if (signal === 'working') return <SpinnerIcon className={cn('size-3 shrink-0 animate-spin text-sky-400', className)} aria-label={LABEL[signal]} />
  return (
    <span title={LABEL[signal]} className={cn('relative flex size-2.5 shrink-0', className)}>
      {signal === 'needs-you' && <span className="absolute inline-flex size-full animate-ping bg-amber-400 opacity-60" />}
      <CircleIcon
        weight="fill"
        className={cn(
          'relative size-2.5',
          signal === 'needs-you' ? 'text-amber-400' : signal === 'error' ? 'text-red-500' : 'text-emerald-400'
        )}
      />
    </span>
  )
}

const BADGE: Record<Activity, [string, string]> = {
  starting: ['starting', 'text-muted-foreground'],
  idle: ['ready', 'text-muted-foreground'],
  working: ['working', 'border-sky-500/40 text-sky-300'],
  'needs-you': ['needs you', 'border-amber-500/50 text-amber-300'],
  done: ['finished', 'border-emerald-500/40 text-emerald-300'],
  error: ['error', 'border-red-500/50 text-red-300'],
  ended: ['ended', 'text-muted-foreground']
}

export function ActivityBadge({ activity }: { activity: Activity }) {
  const [label, cls] = BADGE[activity]
  return (
    <Badge variant="outline" className={cn('gap-1', cls)}>
      {activity === 'working' && <SpinnerIcon className="size-3 animate-spin" />}
      {label}
    </Badge>
  )
}
