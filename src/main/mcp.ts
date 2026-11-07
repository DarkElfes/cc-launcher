import { execFile } from 'node:child_process'
import type { McpServer } from '../shared/types'

/**
 * MCP servers / connectors available to a profile in a project, from `claude mcp list`
 * (https://code.claude.com/docs/en/mcp). It health-checks each server, so it can take a while; the
 * caller caches. The line format isn't documented, so parsing keys off the status markers.
 */

function statusOf(line: string): McpServer['status'] {
  if (/✔|connected/i.test(line) && !/fail|error/i.test(line)) return 'connected'
  if (/needs authentication|!\s*needs/i.test(line)) return 'needs-auth'
  if (/✘|failed|connection error/i.test(line)) return 'failed'
  if (/⏸|pending approval/i.test(line)) return 'pending'
  if (/⊘|disabled/i.test(line)) return 'disabled'
  return 'unknown'
}

export function listMcp(claude: string, cwd: string, env: Record<string, string>): Promise<McpServer[]> {
  return new Promise((resolve, reject) => {
    execFile(claude, ['mcp', 'list'], { cwd, env, windowsHide: true, timeout: 90_000, maxBuffer: 1024 * 1024 }, (err, stdout, stderr) => {
      const text = `${stdout}`.trim()
      if (err && !text) return reject(new Error(String(stderr || err.message).split('\n')[0]))
      const servers: McpServer[] = []
      for (const raw of text.split(/\r?\n/)) {
        const line = raw.trim()
        if (!line || /^checking mcp server health/i.test(line) || /^no mcp servers/i.test(line)) continue
        const m = line.match(/^([^:]+?):\s+(.*)$/)
        if (!m) continue
        const [, name, rest] = m
        const parts = rest.split(/\s+-\s+(?=[✔✘!⏸⊘]|Connected|Failed|Needs|Pending|Disabled)/i)
        servers.push({ name: name.trim(), detail: (parts[0] ?? rest).trim(), status: statusOf(parts[1] ?? rest) })
      }
      resolve(servers)
    })
  })
}
