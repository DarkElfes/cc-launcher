import { useEffect, useRef } from 'react'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { attachSink } from './termBus'

interface Props {
  id: string
  active: boolean
  exitCode?: number
}

const theme = {
  background: '#0a0a0a', // matches the shadcn dark --background
  foreground: '#e4e4e7',
  cursor: '#e4e4e7',
  selectionBackground: '#3f3f46',
  black: '#18181b',
  brightBlack: '#52525b',
  red: '#f87171',
  brightRed: '#fca5a5',
  green: '#4ade80',
  brightGreen: '#86efac',
  yellow: '#facc15',
  brightYellow: '#fde047',
  blue: '#60a5fa',
  brightBlue: '#93c5fd',
  magenta: '#c084fc',
  brightMagenta: '#d8b4fe',
  cyan: '#22d3ee',
  brightCyan: '#67e8f9',
  white: '#d4d4d8',
  brightWhite: '#fafafa'
}

/** One xterm.js terminal bound to a Claude Code process. Stays mounted while hidden so its scrollback survives. */
export function TerminalPane({ id, active, exitCode }: Props) {
  const host = useRef<HTMLDivElement>(null)
  const term = useRef<Terminal | null>(null)
  const fit = useRef<FitAddon | null>(null)
  const activeRef = useRef(active)
  activeRef.current = active

  useEffect(() => {
    const t = new Terminal({
      fontFamily: '"Cascadia Mono", "Cascadia Code", Consolas, monospace',
      fontSize: 13,
      cursorBlink: true,
      scrollback: 10000,
      allowProposedApi: true,
      theme
    })
    const f = new FitAddon()
    t.loadAddon(f)
    t.open(host.current!)
    term.current = t
    fit.current = f

    // Ctrl+C copies when text is selected; Ctrl+V and Ctrl+Shift+C/V use the clipboard.
    t.attachCustomKeyEventHandler((e) => {
      if (e.type !== 'keydown' || !e.ctrlKey) return true
      const key = e.key.toLowerCase()
      if (key === 'c' && (e.shiftKey || t.hasSelection())) {
        void navigator.clipboard.writeText(t.getSelection())
        t.clearSelection()
        return false
      }
      if (key === 'v') return false // let the browser fire a paste event, which xterm handles
      return true
    })

    const detach = attachSink(id, (data) => t.write(data))
    const input = t.onData((data) => window.api.writeTerminal(id, data))

    const refit = () => {
      if (!activeRef.current || !host.current?.offsetParent) return
      try {
        f.fit()
        window.api.resizeTerminal(id, t.cols, t.rows)
      } catch {
        // Not laid out yet.
      }
    }
    const ro = new ResizeObserver(refit)
    ro.observe(host.current!)
    refit()

    return () => {
      ro.disconnect()
      input.dispose()
      detach()
      t.dispose()
      term.current = null
    }
  }, [id])

  useEffect(() => {
    if (!active || !term.current || !fit.current) return
    const t = term.current
    requestAnimationFrame(() => {
      try {
        fit.current?.fit()
        window.api.resizeTerminal(id, t.cols, t.rows)
      } catch {
        // ignore
      }
      t.focus()
    })
  }, [active, id])

  useEffect(() => {
    if (exitCode === undefined) return
    term.current?.write(`\r\n\x1b[90m[Claude Code exited with code ${exitCode}. Close this tab or open a new one.]\x1b[0m\r\n`)
  }, [exitCode])

  return <div className="terminal-host" ref={host} style={{ display: active ? 'block' : 'none' }} />
}
