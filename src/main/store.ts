import { existsSync, mkdirSync, readFileSync, renameSync, writeFileSync } from 'node:fs'
import { dirname } from 'node:path'
import type { Project, Settings } from '../shared/types'

interface StoreData {
  version: 1
  projects: Project[]
  settings: Settings
}

const EMPTY: StoreData = { version: 1, projects: [], settings: {} }

/** App data (project list, settings) kept in one JSON file. Never holds credentials. */
export class Store {
  data: StoreData

  constructor(private file: string) {
    this.data = this.load()
  }

  private load(): StoreData {
    if (!existsSync(this.file)) return structuredClone(EMPTY)
    try {
      const raw = JSON.parse(readFileSync(this.file, 'utf8')) as Partial<StoreData>
      return {
        version: 1,
        projects: Array.isArray(raw.projects) ? raw.projects : [],
        settings: raw.settings ?? {}
      }
    } catch {
      // Keep the broken file for inspection and start fresh.
      renameSync(this.file, `${this.file}.broken-${Date.now()}`)
      return structuredClone(EMPTY)
    }
  }

  save(): void {
    mkdirSync(dirname(this.file), { recursive: true })
    const tmp = `${this.file}.tmp`
    writeFileSync(tmp, JSON.stringify(this.data, null, 2), 'utf8')
    renameSync(tmp, this.file)
  }
}
