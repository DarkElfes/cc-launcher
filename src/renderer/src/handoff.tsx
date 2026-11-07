import { useEffect, useState } from 'react'
import { ArrowsLeftRightIcon } from '@phosphor-icons/react'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group'
import { cn } from '@/lib/utils'
import type { ProfileInfo, SessionInfo } from '../../shared/types'
import { ProfileTag, UsageInline } from './ui'

/**
 * Moving work to another account. Chat history belongs to each account, so either:
 * - "summary" (default): Claude writes the state into docs/STATUS.md (and commits it), then a new
 *   session on the other account starts by reading it. Small, cheap, and saved in git.
 * - "full": the other account resumes this exact conversation file (`claude --resume <path>`).
 *   It remembers everything, but the whole conversation is sent to the model again.
 */

export type HandoffMode = 'summary' | 'full'

export const HANDOFF_PROMPT =
  'We are handing this work over to another Claude session on a different account. ' +
  'Update docs/STATUS.md (create it if missing) so a fresh session can continue without this chat: ' +
  'what was done in this session, what is in progress (with file paths), the exact next steps, and known problems. ' +
  'Be concise and specific. Then commit only docs/STATUS.md with the message "Handoff: update STATUS.md" ' +
  '(skip the commit if this folder is not a git repository). Do not change anything else.'

export const CONTINUE_PROMPT =
  'This work was handed over from another session. Read CLAUDE.md (if present) and docs/STATUS.md, ' +
  'check git status for uncommitted work, then briefly summarize where things stand and continue with the next step.'

const pct = (p: ProfileInfo) =>
  Math.max(
    p.usage?.fiveHour && !p.usage.fiveHour.stale ? p.usage.fiveHour.usedPercent : 0,
    p.usage?.week && !p.usage.week.stale ? p.usage.week.usedPercent : 0
  )

export function HandoffDialog(props: {
  open: boolean
  onOpenChange: (open: boolean) => void
  from: string
  session?: SessionInfo
  profiles: ProfileInfo[]
  onConfirm: (to: string, mode: HandoffMode) => void
}) {
  const targets = props.profiles.filter((p) => p.name !== props.from)
  // Default: the signed-in account with the most room left.
  const best = [...targets].filter((p) => p.signedIn).sort((a, b) => pct(a) - pct(b))[0]?.name ?? targets[0]?.name
  const [to, setTo] = useState(best)
  const [mode, setMode] = useState<HandoffMode>('summary')
  useEffect(() => {
    if (props.open) {
      setTo(best)
      setMode('summary')
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [props.open])

  const busy = props.session?.activity === 'working'
  const canFull = !!props.session?.transcriptPath
  const tokens = props.session?.context?.tokens

  return (
    <Dialog open={props.open} onOpenChange={props.onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <ArrowsLeftRightIcon /> Hand off to another account
          </DialogTitle>
          <DialogDescription>Continue this project's work with a different Claude account.</DialogDescription>
        </DialogHeader>

        {targets.length === 0 ? (
          <p className="text-xs text-amber-400">Add another account under Accounts first.</p>
        ) : (
          <div className="flex flex-col gap-4">
            <div className="flex flex-col gap-2">
              <span className="text-xs font-medium">To account</span>
              <RadioGroup value={to} onValueChange={setTo} className="gap-1">
                {targets.map((p) => (
                  <label
                    key={p.name}
                    className={cn('flex cursor-pointer items-center gap-3 border px-3 py-2', to === p.name && 'border-primary')}
                  >
                    <RadioGroupItem value={p.name} />
                    <span className="flex min-w-0 flex-1 flex-col">
                      <ProfileTag name={p.name} />
                      <span className="truncate text-[11px] text-muted-foreground">
                        {p.account?.email ?? (p.signedIn ? '' : 'not signed in')}
                      </span>
                    </span>
                    <UsageInline usage={p.usage} />
                  </label>
                ))}
              </RadioGroup>
            </div>

            <div className="flex flex-col gap-2">
              <span className="text-xs font-medium">How</span>
              <RadioGroup value={mode} onValueChange={(v) => setMode(v as HandoffMode)} className="gap-1">
                <label className={cn('flex cursor-pointer gap-3 border px-3 py-2', mode === 'summary' && 'border-primary')}>
                  <RadioGroupItem value="summary" className="mt-0.5" />
                  <span className="flex flex-col gap-0.5">
                    <span className="text-xs">Summary via docs/STATUS.md (recommended)</span>
                    <span className="text-[11px] text-muted-foreground">
                      Claude here writes the current state to docs/STATUS.md and commits it. Then a new session on the other
                      account reads it and continues. Small and cheap, and the handover is saved in git.
                    </span>
                  </span>
                </label>
                <label
                  className={cn(
                    'flex gap-3 border px-3 py-2',
                    canFull ? 'cursor-pointer' : 'cursor-not-allowed opacity-50',
                    mode === 'full' && 'border-primary'
                  )}
                >
                  <RadioGroupItem value="full" disabled={!canFull} className="mt-0.5" />
                  <span className="flex flex-col gap-0.5">
                    <span className="text-xs">Continue the full conversation</span>
                    <span className="text-[11px] text-muted-foreground">
                      The other account opens this exact conversation and remembers everything. But the whole conversation
                      {tokens ? ` (about ${Math.round(tokens / 1000)}k tokens)` : ''} is sent again with its next message,
                      which uses more of its limit.
                      {!canFull && ' Not available yet: send at least one message in this session first.'}
                    </span>
                  </span>
                </label>
              </RadioGroup>
            </div>

            {mode === 'summary' && busy && (
              <p className="text-[11px] text-amber-400">Claude is still working in this tab. The handoff starts when it finishes.</p>
            )}
          </div>
        )}

        <DialogFooter>
          <Button variant="outline" onClick={() => props.onOpenChange(false)}>
            Cancel
          </Button>
          <Button
            disabled={!to || targets.length === 0}
            onClick={() => {
              props.onConfirm(to!, mode)
              props.onOpenChange(false)
            }}
          >
            Hand off to {to}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
