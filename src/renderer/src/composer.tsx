import { useEffect, useRef, useState, type ClipboardEvent, type DragEvent, type KeyboardEvent } from 'react'
import { ImageIcon, InfoIcon, PaperPlaneRightIcon, WarningCircleIcon, XIcon } from '@phosphor-icons/react'
import { Button } from '@/components/ui/button'
import { Textarea } from '@/components/ui/textarea'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
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

/** "Screenshot 23:41" for pasted images (their file names are generated), else the file name. */
function imageLabel(path: string): string {
  const m = baseName(path).match(/^paste-(\d{4}-\d{2}-\d{2})T(\d{2})-(\d{2})-(\d{2})-(\d{3})Z\./)
  if (!m) return baseName(path)
  const d = new Date(`${m[1]}T${m[2]}:${m[3]}:${m[4]}.${m[5]}Z`)
  return `Screenshot ${d.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' })}`
}

const HELP: Array<[string, string]> = [
  ['Enter', 'Send to Claude'],
  ['Shift+Enter', 'New line'],
  ['↑ / ↓', 'Previous messages (in an empty box)'],
  ['Ctrl+V', 'Paste text, or a screenshot as an image'],
  ['Drag & drop', 'Attach image files'],
  ['🖼 button', 'Pick images from disk'],
  ['Thumbnail', 'Click to open, × to remove'],
  ['Ctrl+Space', 'Voice dictation (if turned on in Settings)'],
  ['Esc', 'Go to the terminal'],
  ['Terminal', "Answer Claude's questions and menus there. The box waits until you do."]
]

/** ⓘ next to the box: what it can do. */
function ComposerHelp() {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button variant="ghost" size="icon" aria-label="What this box can do">
          <InfoIcon />
        </Button>
      </TooltipTrigger>
      <TooltipContent side="top" align="end" className="max-w-sm p-3">
        <div className="flex flex-col gap-2">
          <p className="font-semibold">Message box</p>
          <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1">
            {HELP.map(([k, v]) => (
              <div key={k} className="contents">
                <dt className="font-medium whitespace-nowrap">{k}</dt>
                <dd className="opacity-80">{v}</dd>
              </div>
            ))}
          </dl>
        </div>
      </TooltipContent>
    </Tooltip>
  )
}

const thumbs = new Map<string, string | null>()

/** Attached image: a real preview, readable label, click to open, × to remove. */
function ImageChip({ path, onRemove }: { path: string; onRemove: () => void }) {
  const [src, setSrc] = useState<string | null | undefined>(thumbs.get(path))
  useEffect(() => {
    if (thumbs.has(path)) return
    let alive = true
    void window.api.imageThumbnail(path).then((url) => {
      thumbs.set(path, url)
      if (alive) setSrc(url)
    })
    return () => {
      alive = false
    }
  }, [path])
  return (
    <div className="group relative flex w-32 flex-col gap-1" title={`${path}\nClick to open`}>
      <button
        className="flex h-20 w-32 items-center justify-center overflow-hidden border bg-background hover:border-ring"
        onClick={() => void window.api.openImage(path)}
      >
        {src ? (
          <img src={src} alt="" className="size-full object-cover" draggable={false} />
        ) : (
          <ImageIcon className="size-5 text-muted-foreground" />
        )}
      </button>
      <span className="truncate text-[10px] text-muted-foreground">{imageLabel(path)}</span>
      <button
        className="absolute -top-1.5 -right-1.5 flex size-4 items-center justify-center border bg-background text-muted-foreground opacity-0 group-hover:opacity-100 hover:text-foreground"
        title="Remove"
        onClick={onRemove}
      >
        <XIcon className="size-3" />
      </button>
    </div>
  )
}

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
        <div className="mb-2 flex flex-wrap gap-2 pt-1.5 pr-1.5">
          {images.map((p) => (
            <ImageChip key={p} path={p} onRemove={() => setImages((c) => c.filter((x) => x !== p))} />
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
          aria-label="Message Claude"
          data-composer
          className={cn('max-h-48 min-h-9 resize-none overflow-y-auto bg-background text-sm', waiting && 'opacity-70')}
        />
        <ComposerHelp />
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
