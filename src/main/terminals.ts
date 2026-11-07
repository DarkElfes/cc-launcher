import { randomUUID } from 'node:crypto'
import * as pty from '@lydell/node-pty'

export interface SpawnOptions {
  file: string
  args: string[]
  cwd: string
  env: Record<string, string>
  cols: number
  rows: number
}

type Send = (channel: 'term:data' | 'term:exit', payload: unknown) => void

/** Pseudo-terminals running Claude Code, keyed by id. */
export class Terminals {
  private ptys = new Map<string, pty.IPty>()

  constructor(private send: Send) {}

  spawn(opts: SpawnOptions): string {
    const id = randomUUID()
    const p = pty.spawn(opts.file, opts.args, {
      name: 'xterm-256color',
      cols: Math.max(opts.cols, 20),
      rows: Math.max(opts.rows, 5),
      cwd: opts.cwd,
      env: opts.env
    })
    p.onData((data) => this.send('term:data', { id, data }))
    p.onExit(({ exitCode }) => {
      this.ptys.delete(id)
      this.send('term:exit', { id, exitCode })
    })
    this.ptys.set(id, p)
    return id
  }

  write(id: string, data: string): void {
    this.ptys.get(id)?.write(data)
  }

  resize(id: string, cols: number, rows: number): void {
    if (cols < 2 || rows < 2) return
    try {
      this.ptys.get(id)?.resize(cols, rows)
    } catch {
      // The process may have just exited.
    }
  }

  kill(id: string): void {
    const p = this.ptys.get(id)
    if (!p) return
    this.ptys.delete(id)
    try {
      p.kill()
    } catch {
      // Already gone.
    }
  }

  killAll(): void {
    for (const id of [...this.ptys.keys()]) this.kill(id)
  }
}
