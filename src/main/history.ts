import { closeSync, existsSync, openSync, readdirSync, readSync, statSync } from 'node:fs'
import { join } from 'node:path'
import type { HistorySession } from '../shared/types'

/**
 * Past Claude Code sessions of a project, per profile. Transcripts live in
 * <profile>/projects/<project>/<session-id>.jsonl, where <project> is the working directory with
 * every non-alphanumeric character replaced by "-" (https://code.claude.com/docs/en/sessions).
 *
 * Only file names, times and the first part of each file are read, to find a title. The entry
 * format is internal to Claude Code, so title detection is best-effort and falls back to the date.
 * Resuming is done by Claude Code itself (`claude --resume <id>`).
 */

export function projectDirName(cwd: string): string {
  return cwd.replace(/[^a-zA-Z0-9]/g, '-')
}

function readHead(path: string, bytes = 96 * 1024): string {
  const fd = openSync(path, 'r')
  try {
    const buf = Buffer.alloc(bytes)
    const n = readSync(fd, buf, 0, bytes, 0)
    return buf.subarray(0, n).toString('utf8')
  } finally {
    closeSync(fd)
  }
}

function textOf(content: unknown): string | undefined {
  if (typeof content === 'string') return content
  if (Array.isArray(content)) {
    for (const part of content) {
      const p = part as { type?: string; text?: string }
      if (p?.type === 'text' && typeof p.text === 'string') return p.text
    }
  }
  return undefined
}

function titleOf(path: string): string | undefined {
  let firstPrompt: string | undefined
  for (const line of readHead(path).split('\n')) {
    if (!line.trim()) continue
    let e: Record<string, unknown>
    try {
      e = JSON.parse(line)
    } catch {
      continue // probably the cut-off last line
    }
    const custom = e.customTitle ?? e.title
    if (typeof custom === 'string' && custom.trim()) return custom.trim()
    if (e.type === 'summary' && typeof e.summary === 'string') return e.summary
    if (!firstPrompt && e.type === 'user' && !e.isMeta) {
      const t = textOf((e.message as { content?: unknown } | undefined)?.content)?.trim()
      if (t && !t.startsWith('<')) firstPrompt = t
    }
  }
  return firstPrompt
}

export function listHistory(cwd: string, profiles: Array<{ name: string; dir: string }>, limit = 40): HistorySession[] {
  const name = projectDirName(cwd)
  if (name.length > 200) return [] // Claude hashes long names; not worth guessing
  const out: HistorySession[] = []
  for (const p of profiles) {
    const folder = join(p.dir, 'projects', name)
    if (!existsSync(folder)) continue
    for (const f of readdirSync(folder)) {
      if (!f.endsWith('.jsonl')) continue
      const file = join(folder, f)
      try {
        const st = statSync(file)
        if (!st.isFile() || st.size === 0) continue
        const title = titleOf(file)
        out.push({
          id: f.slice(0, -'.jsonl'.length),
          profile: p.name,
          title: title ? title.replace(/\s+/g, ' ').slice(0, 120) : '',
          modifiedAt: st.mtimeMs,
          sizeKb: Math.round(st.size / 1024)
        })
      } catch {
        // locked or vanished
      }
    }
  }
  return out.sort((a, b) => b.modifiedAt - a.modifiedAt).slice(0, limit)
}
