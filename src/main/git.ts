import { execFile } from 'node:child_process'
import { existsSync } from 'node:fs'
import type { GitChange, GitInfo } from '../shared/types'

/** Git state of a project folder for the side panel. Read-only: never changes the repo. */

function gitBinary(): string {
  const bundled = 'C:\\Program Files\\Git\\cmd\\git.exe'
  return process.platform === 'win32' && existsSync(bundled) ? bundled : 'git'
}

function git(cwd: string, args: string[]): Promise<string> {
  return new Promise((resolve, reject) => {
    execFile(gitBinary(), ['-C', cwd, ...args], { windowsHide: true, timeout: 15_000, maxBuffer: 8 * 1024 * 1024 }, (err, stdout) => {
      if (err) reject(err)
      else resolve(String(stdout))
    })
  })
}

const STATUS: Record<string, GitChange['status']> = {
  M: 'modified',
  A: 'added',
  D: 'deleted',
  R: 'renamed',
  C: 'added',
  U: 'conflict',
  '?': 'untracked'
}

export async function getGitInfo(cwd: string): Promise<GitInfo> {
  try {
    await git(cwd, ['rev-parse', '--is-inside-work-tree'])
  } catch {
    return { isRepo: false, files: [], added: 0, removed: 0 }
  }

  const status = await git(cwd, ['status', '--porcelain=v1', '--branch', '--untracked-files=all'])
  const lines = status.split('\n').filter(Boolean)
  let branch: string | undefined
  let ahead = 0
  let behind = 0
  const files = new Map<string, GitChange>()
  for (const line of lines) {
    if (line.startsWith('## ')) {
      const head = line.slice(3)
      branch = head.split('...')[0].replace(/^No commits yet on /, '').split(' ')[0]
      ahead = Number(head.match(/ahead (\d+)/)?.[1] ?? 0)
      behind = Number(head.match(/behind (\d+)/)?.[1] ?? 0)
      continue
    }
    const xy = line.slice(0, 2)
    let path = line.slice(3)
    if (path.includes(' -> ')) path = path.split(' -> ')[1]
    path = path.replace(/^"|"$/g, '')
    const code = xy === '??' ? '?' : (xy.trim()[0] ?? 'M')
    files.set(path, { path, status: STATUS[code] ?? 'modified', staged: xy[0] !== ' ' && xy[0] !== '?' })
  }

  // Line counts vs HEAD (staged + unstaged). Fails in a repo without commits; that's fine.
  let added = 0
  let removed = 0
  try {
    const numstat = await git(cwd, ['diff', '--numstat', 'HEAD'])
    for (const l of numstat.split('\n').filter(Boolean)) {
      const [a, r, ...rest] = l.split('\t')
      const p = rest.join('\t').replace(/^.*=> /, '').replace(/[{}]/g, '')
      const f = files.get(p)
      const ai = a === '-' ? 0 : Number(a)
      const ri = r === '-' ? 0 : Number(r)
      if (f) Object.assign(f, { added: ai, removed: ri })
      added += ai
      removed += ri
    }
  } catch {
    // no HEAD yet
  }

  let lastCommit: GitInfo['lastCommit']
  try {
    const [hash, subject, ts] = (await git(cwd, ['log', '-1', '--format=%h%x09%s%x09%ct'])).trim().split('\t')
    if (hash) lastCommit = { hash, subject, at: Number(ts) * 1000 }
  } catch {
    // no commits
  }

  return { isRepo: true, branch, ahead, behind, files: [...files.values()], added, removed, lastCommit }
}
