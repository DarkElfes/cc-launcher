import { toast } from 'sonner'
import { Badge } from '@/components/ui/badge'
import { Progress } from '@/components/ui/progress'
import { cn } from '@/lib/utils'
import type { ProfileInfo, ProfileUsage, UsageWindow } from '../../shared/types'

export const showError = (e: string) => toast.error(e)

/** Stable color per profile name so each account is easy to recognise. */
export function profileColor(name: string): string {
  let h = 0
  for (const ch of name) h = (h * 31 + ch.charCodeAt(0)) % 360
  return `hsl(${h} 70% 62%)`
}

export function ProfileTag({ name }: { name: string }) {
  const color = profileColor(name)
  return (
    <span className="inline-flex items-center gap-1.5 text-xs font-medium" style={{ color }}>
      <span className="size-2 shrink-0" style={{ background: color }} />
      {name}
    </span>
  )
}

export function SignedIn({ profile }: { profile: ProfileInfo }) {
  return profile.signedIn ? (
    <Badge variant="outline" className="border-emerald-500/40 text-emerald-400">
      signed in
    </Badge>
  ) : (
    <Badge variant="outline" className="border-amber-500/40 text-amber-400">
      not signed in
    </Badge>
  )
}

/** "2h 13m", "3d 4h", "5m" until the given epoch ms. */
export function timeUntil(ts: number): string {
  const m = Math.max(0, Math.round((ts - Date.now()) / 60000))
  if (m < 60) return `${m}m`
  const h = Math.floor(m / 60)
  if (h < 24) return `${h}h ${m % 60}m`
  return `${Math.floor(h / 24)}d ${h % 24}h`
}

function barColor(pct: number): string {
  if (pct >= 90) return '[&>div]:bg-red-500'
  if (pct >= 70) return '[&>div]:bg-amber-400'
  return '[&>div]:bg-emerald-500'
}

const timeFmt = new Intl.DateTimeFormat(undefined, { hour: 'numeric', minute: '2-digit' })
const dayTimeFmt = new Intl.DateTimeFormat(undefined, { weekday: 'short', hour: 'numeric', minute: '2-digit' })
const dateTimeFmt = new Intl.DateTimeFormat(undefined, { month: 'short', day: 'numeric', hour: 'numeric', minute: '2-digit' })

/** "Resets at 11:00 PM · in 2h 7m", "Resets Wed 6:00 PM · in 3d 4h", like the Claude app. */
export function resetText(w: UsageWindow): string {
  if (w.stale) return 'Window restarted. New usage shows after the next refresh.'
  if (w.resetsAt !== null) {
    const d = new Date(w.resetsAt)
    const today = new Date()
    const days = (w.resetsAt - Date.now()) / 86_400_000
    const when = d.toDateString() === today.toDateString() ? `at ${timeFmt.format(d)}` : days < 6.5 ? dayTimeFmt.format(d) : dateTimeFmt.format(d)
    return `Resets ${when} · in ${timeUntil(w.resetsAt)}`
  }
  if (w.resetsLabel) return `Resets ${w.resetsLabel}`
  return w.usedPercent === 0 ? 'Not started. The window begins with your next message.' : ''
}

/** One usage window: label, percent, bar and reset time. */
export function UsageMeter({ label, w }: { label: string; w?: UsageWindow }) {
  if (!w) return <span className="text-xs text-muted-foreground">{label} —</span>
  const pct = Math.round(w.usedPercent)
  return (
    <div className="flex min-w-44 flex-col gap-1">
      <div className="flex justify-between gap-3 text-xs">
        <span className="text-muted-foreground">{label}</span>
        <span>{w.stale ? '0%' : `${pct}%`}</span>
      </div>
      <Progress value={w.stale ? 0 : pct} className={cn('h-1.5', barColor(pct))} />
      <span className="text-[10px] text-muted-foreground">{resetText(w)}</span>
    </div>
  )
}

/** Email + plan under a profile name. */
export function AccountLine({ profile }: { profile: ProfileInfo }) {
  const a = profile.account
  if (!a) return <span className="text-[10px] text-muted-foreground">checking account…</span>
  if (!a.loggedIn) return <span className="text-[10px] text-amber-400">not signed in</span>
  return (
    <span className="flex items-center gap-1.5 text-[11px] text-muted-foreground">
      <span className="truncate">{a.email ?? 'unknown email'}</span>
      {a.plan && (
        <Badge variant="outline" className="h-4 px-1 text-[10px] uppercase">
          {a.plan}
        </Badge>
      )}
    </span>
  )
}

/** Compact "5h 23% · wk 41%" for buttons. */
export function UsageInline({ usage }: { usage?: ProfileUsage }) {
  if (!usage) return null
  const f = (w?: UsageWindow) => (w ? (w.stale ? '0%' : `${Math.round(w.usedPercent)}%`) : '–')
  const hot = Math.max(usage.fiveHour?.stale ? 0 : (usage.fiveHour?.usedPercent ?? 0), usage.week?.stale ? 0 : (usage.week?.usedPercent ?? 0))
  return (
    <span className={cn('text-[10px]', hot >= 90 ? 'text-red-400' : hot >= 70 ? 'text-amber-400' : 'text-muted-foreground')}>
      5h {f(usage.fiveHour)} · wk {f(usage.week)}
    </span>
  )
}

export function timeAgo(ts?: number): string {
  if (!ts) return 'never'
  const s = Math.round((Date.now() - ts) / 1000)
  if (s < 60) return 'just now'
  const m = Math.round(s / 60)
  if (m < 60) return `${m} min ago`
  const h = Math.round(m / 60)
  if (h < 24) return `${h} h ago`
  return `${Math.round(h / 24)} d ago`
}
