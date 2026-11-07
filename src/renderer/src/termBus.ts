// Routes terminal output from the main process to the xterm instance for each id.
// Output that arrives before the terminal component mounts is buffered.

type Sink = (data: string) => void

const sinks = new Map<string, Sink>()
const pending = new Map<string, string[]>()

window.api.onTermData(({ id, data }) => {
  const sink = sinks.get(id)
  if (sink) sink(data)
  else {
    const buf = pending.get(id) ?? []
    buf.push(data)
    pending.set(id, buf)
  }
})

export function attachSink(id: string, sink: Sink): () => void {
  sinks.set(id, sink)
  for (const chunk of pending.get(id) ?? []) sink(chunk)
  pending.delete(id)
  return () => {
    if (sinks.get(id) === sink) sinks.delete(id)
  }
}

export function forget(id: string): void {
  sinks.delete(id)
  pending.delete(id)
  pasters.delete(id)
}

// Pasting into a terminal goes through xterm (bracketed paste), so Claude Code sees it as a paste.
// That's how it turns a pasted image path into an image attachment.
const pasters = new Map<string, (text: string) => void>()

export function registerPaster(id: string, paste: (text: string) => void): () => void {
  pasters.set(id, paste)
  return () => {
    if (pasters.get(id) === paste) pasters.delete(id)
  }
}

export function pasteInto(id: string, text: string): void {
  const paste = pasters.get(id)
  if (paste) paste(text)
  else window.api.writeTerminal(id, text)
}

/** Quoted image paths, ready to paste into a Claude prompt. */
export const imagePathsText = (paths: string[]) => paths.map((p) => `"${p}"`).join(' ') + ' '
