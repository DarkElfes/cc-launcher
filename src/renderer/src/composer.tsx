import { useEffect, useRef, useState, type ClipboardEvent, type DragEvent, type KeyboardEvent } from 'react'
import { ImageIcon, PaperPlaneRightIcon, WarningCircleIcon, XIcon } from '@phosphor-icons/react'
import { Button } from '@/components/ui/button'
import { Textarea } from '@/components/ui/textarea'
import { cn } from '@/lib/utils'
import type { Activity } from '../../shared/types'
import { pasteInto } from './termBus'

/**
 * A normal text box under the Claude terminal: real text editing (select words, Ctrl+A/Z, mouse),
 * multi-line, image attachments. On send it pastes into Claude Code's prompt (bracketed paste, so
 * newlines stay newlines and image paths become [Image #N]) and presses Enter.
 * The terminal stays fully usable for Claude's menus (permissions, choices).
 */

// Per-tab drafts and sent history survive switching tabs.
const drafts = new Map<string, { text: string; images: string[] }>()
const history = new Map<string, string[]>()

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms))
const baseName = (p: string) => p.split(/[\\/]/).pop() ?? p

export function Composer(props: { tabId: string; activity?: Activity }) {
  const { tabId } = props
  const [text, setText] = useState(() => drafts.get(tabId)?.text ?? '')
  const [images, setImages] = useState<string[]>(() => drafts.get(tabId)?.images ?? [])
  const [histIndex, setHistIndex] = useState(-1)
  const [sending, setSending] = useState(false)
  const box = useRef<HTMLTextAreaElement>(null)

  // Switch drafts with the tab.
  useEffect(() => {
    const d = drafts.get(tabId)
    setText(d?.text ?? '')
    setImages(d?.images ?? [])
    setHistIndex(-1)
  }, [tabId])
  useEffect(() => {
    drafts.set(tabId, { text, images })
  }, [tabId, text, images])

  const waiting = props.activity === 'needs-you'
  const canSend = !sending && !waiting && (text.trim().length > 0 || images.length > 0)

  const addImages = (paths: string[]) => setImages((cur) => [...cur, ...paths.filter((p) => !cur.includes(p))])

  const send = async () => {
    if (!canSend) return
    setSending(true)
    try {
      // Images first, one paste each, so Claude Code turns every path into an attachment.
      for (const p of images) {
        pasteInto(tabId, `"${p}" `)
        await sleep(120)
      }
      const msg = text.replace(/\s+$/, '')
      if (msg) pasteInto(tabId, msg)
      await sleep(150)
      window.api.writeTerminal(tabId, '\r')
      if (msg) history.set(tabId, [msg, ...(history.get(tabId) ?? []).filter((h) => h !== msg)].slice(0, 50))
      setText('')
      setImages([])
      setHistIndex(-1)
    } finally {
      setSending(false)
      box.current?.focus()
    }
  }

  const onKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.nativeEvent.isComposing) return
    if (e.key === 'Enter' && !e.shiftKey && !e.ctrlKey && !e.altKey) {
      e.preventDefault()
      void send()
      return
    }
    if (e.key === 'Escape') {
      // Back to the terminal (e.g. to answer one of Claude's menus).
      ;(document.querySelector('.terminal-host[style*="block"] textarea') as HTMLTextAreaElement | null)?.focus()
      return
    }
    // ↑ / ↓ in an empty (or history) box walks previously sent messages.
    const past = history.get(tabId) ?? []
    if (e.key === 'ArrowUp' && past.length && (text === '' || histIndex >= 0) && !text.includes('\n')) {
      e.preventDefault()
      const i = Math.min(histIndex + 1, past.length - 1)
      setHistIndex(i)
      setText(past[i])
    } else if (e.key === 'ArrowDown' && histIndex >= 0) {
      e.preventDefault()
      const i = histIndex - 1
      setHistIndex(i)
      setText(i >= 0 ? past[i] : '')
    }
  }

  const onPaste = async (e: ClipboardEvent<HTMLTextAreaElement>) => {
    const hasImage = [...e.clipboardData.items].some((i) => i.kind === 'file' && i.type.startsWith('image/'))
    if (!hasImage) return // normal text paste
    e.preventDefault()
    const path = await window.api.saveClipboardImage()
    if (path) addImages([path])
  }

  const onDrop = (e: DragEvent<HTMLDivElement>) => {
    const files = [...e.dataTransfer.files]
    if (!files.length) return
    e.preventDefault()
    addImages(files.map((f) => window.api.pathForFile(f)).filter(Boolean))
  }

  return (
    <div
      className="shrink-0 border-t bg-sidebar/60 p-2"
      onDragOver={(e) => e.dataTransfer.types.includes('Files') && e.preventDefault()}
      onDrop={onDrop}
    >
      {waiting && (
        <p className="mb-1.5 flex items-center gap-1.5 text-[11px] text-amber-300">
          <WarningCircleIcon className="size-3.5" />
          Claude is asking something in the terminal above (permission or a choice). Answer there first.
        </p>
      )}
      {images.length > 0 && (
        <div className="mb-1.5 flex flex-wrap gap-1">
          {images.map((p) => (
            <span key={p} className="flex items-center gap-1 border bg-background px-1.5 py-0.5 text-[11px]" title={p}>
              <ImageIcon className="size-3" />
              <span className="max-w-48 truncate">{baseName(p)}</span>
              <button className="text-muted-foreground hover:text-foreground" onClick={() => setImages((c) => c.filter((x) => x !== p))}>
                <XIcon className="size-3" />
              </button>
            </span>
          ))}
        </div>
      )}
      <div className="flex items-end gap-2">
        <Textarea
          ref={box}
          value={text}
          onChange={(e) => {
            setText(e.target.value)
            setHistIndex(-1)
          }}
          onKeyDown={onKeyDown}
          onPaste={(e) => void onPaste(e)}
          rows={1}
          spellCheck
          placeholder="Message Claude… Enter to send, Shift+Enter for a new line, Ctrl+V to paste a screenshot"
          className={cn('max-h-48 min-h-9 resize-none overflow-y-auto bg-background text-sm', waiting && 'opacity-70')}
        />
        <Button
          variant="ghost"
          size="icon"
          title="Attach images"
          onClick={async () => addImages(await window.api.pickImages())}
        >
          <ImageIcon />
        </Button>
        <Button size="icon" title={waiting ? 'Answer Claude in the terminal first' : 'Send (Enter)'} disabled={!canSend} onClick={() => void send()}>
          <PaperPlaneRightIcon />
        </Button>
      </div>
    </div>
  )
}
