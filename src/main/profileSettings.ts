import { existsSync, readFileSync, renameSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import type { Settings } from '../shared/types'

/**
 * Applies app-level choices to a profile's Claude Code config, so they don't have to be set with
 * /config and /voice in every account:
 *
 * - settings.json `language`: Claude's reply language AND the voice-dictation language
 *   (https://code.claude.com/docs/en/voice-dictation). Untouched when the app setting is empty.
 * - settings.json `voice`: `{ enabled, mode }`. Untouched when voiceMode is unset.
 * - keybindings.json: binds Alt+K (`meta+k`) to `voice:pushToTalk` in the Chat context, so the app's
 *   mic button / Ctrl+Space can trigger dictation by sending that key. Space keeps working.
 *
 * Files that aren't valid JSON are left alone.
 */

export const DICTATE_KEY = 'meta+k'
/** What a terminal sends for Alt+K: ESC followed by "k". */
export const DICTATE_SEQUENCE = '\x1bk'

function readJson(path: string): Record<string, unknown> | null {
  if (!existsSync(path)) return {}
  try {
    const v = JSON.parse(readFileSync(path, 'utf8').replace(/^﻿/, ''))
    return v && typeof v === 'object' && !Array.isArray(v) ? (v as Record<string, unknown>) : null
  } catch {
    return null
  }
}

function writeJson(path: string, data: unknown): void {
  const tmp = `${path}.tmp`
  writeFileSync(tmp, JSON.stringify(data, null, 2), 'utf8')
  renameSync(tmp, path)
}

export function applyVoiceSettings(dir: string, s: Settings): void {
  if (!existsSync(dir)) return
  const settingsPath = join(dir, 'settings.json')
  const settings = readJson(settingsPath)
  if (settings) {
    const before = JSON.stringify(settings)
    if (s.claudeLanguage) settings.language = s.claudeLanguage
    if (s.voiceMode) {
      const voice = (typeof settings.voice === 'object' && settings.voice) || {}
      settings.voice =
        s.voiceMode === 'off' ? { ...voice, enabled: false } : { ...voice, enabled: true, mode: s.voiceMode }
    }
    if (JSON.stringify(settings) !== before) writeJson(settingsPath, settings)
  }
  if (s.voiceMode && s.voiceMode !== 'off') ensureDictateKey(dir)
}

function ensureDictateKey(dir: string): void {
  const path = join(dir, 'keybindings.json')
  const file = readJson(path)
  if (!file) return
  const bindings = Array.isArray(file.bindings) ? (file.bindings as Array<Record<string, unknown>>) : []
  const already = bindings.some(
    (b) => b.context === 'Chat' && (b.bindings as Record<string, unknown> | undefined)?.[DICTATE_KEY] === 'voice:pushToTalk'
  )
  if (already) return
  const chat = bindings.find((b) => b.context === 'Chat')
  if (chat) {
    const inner = (chat.bindings as Record<string, unknown> | undefined) ?? {}
    if (inner[DICTATE_KEY] !== undefined) return // user bound Alt+K to something else; respect it
    chat.bindings = { ...inner, [DICTATE_KEY]: 'voice:pushToTalk' }
  } else {
    bindings.push({ context: 'Chat', bindings: { [DICTATE_KEY]: 'voice:pushToTalk' } })
  }
  writeJson(path, { ...file, bindings })
}
