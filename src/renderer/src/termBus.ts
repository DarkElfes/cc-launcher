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
}
