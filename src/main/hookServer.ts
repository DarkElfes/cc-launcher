import { randomBytes, timingSafeEqual } from 'node:crypto'
import { createServer, type Server } from 'node:http'

/**
 * Receives Claude Code HTTP hooks (https://code.claude.com/docs/en/hooks) from sessions this app
 * started. Listens on 127.0.0.1 only, on a random free port. Each app run makes a new secret,
 * passed to its claude processes as CCL_HOOK_TOKEN and sent back in the Authorization header,
 * so other local programs can't inject events. The tab id comes from the X-CCL-Tab header
 * (CCL_TAB_ID in the claude process env).
 *
 * Always answers 200 with an empty body: "success, no decision", so hooks never block or change
 * what Claude does.
 */

export type HookEvent = Record<string, unknown> & { hook_event_name?: string }

export const HOOK_PATH = '/ccl-hook'

export class HookServer {
  readonly token = randomBytes(24).toString('hex')
  private server: Server | null = null
  port = 0

  /**
   * onEvent may return a hook response (e.g. a PermissionRequest decision). The HTTP reply waits for
   * it; any other event is answered right away.
   */
  constructor(private onEvent: (tabId: string, event: HookEvent) => void | Promise<Record<string, unknown> | undefined>) {}

  start(): Promise<number> {
    return new Promise((resolve, reject) => {
      const expected = Buffer.from(`Bearer ${this.token}`)
      this.server = createServer((req, res) => {
        const done = (code = 200) => {
          res.writeHead(code)
          res.end()
        }
        if (req.method !== 'POST' || req.url !== HOOK_PATH) return done(404)
        const auth = Buffer.from(String(req.headers.authorization ?? ''))
        if (auth.length !== expected.length || !timingSafeEqual(auth, expected)) return done(401)
        const tabId = String(req.headers['x-ccl-tab'] ?? '')
        let size = 0
        const chunks: Buffer[] = []
        req.on('data', (c: Buffer) => {
          size += c.length
          if (size > 2 * 1024 * 1024) req.destroy() // ignore absurdly large payloads
          else chunks.push(c)
        })
        req.on('end', () => {
          let event: HookEvent
          try {
            event = JSON.parse(Buffer.concat(chunks).toString('utf8')) as HookEvent
          } catch {
            return done() // malformed: ignore
          }
          if (!tabId) return done()
          const result = this.onEvent(tabId, event)
          if (!(result instanceof Promise)) return done()
          result
            .then((body) => {
              if (!body) return done()
              res.writeHead(200, { 'Content-Type': 'application/json' })
              res.end(JSON.stringify(body))
            })
            .catch(() => done())
        })
      })
      this.server.on('error', reject)
      this.server.listen(0, '127.0.0.1', () => {
        const addr = this.server!.address()
        this.port = typeof addr === 'object' && addr ? addr.port : 0
        resolve(this.port)
      })
    })
  }

  stop(): void {
    this.server?.close()
  }
}
