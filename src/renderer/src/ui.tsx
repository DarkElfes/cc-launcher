import { toast } from 'sonner'
import { Badge } from '@/components/ui/badge'
import type { ProfileInfo } from '../../shared/types'

export const showError = (e: string) => toast.error(e)

/** Stable color per profile name so each account is easy to recognise. */
export function profileColor(name: string): string {
  if (name === 'default') return 'hsl(220 10% 62%)'
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
