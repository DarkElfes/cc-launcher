import { contextBridge, ipcRenderer, webUtils, type IpcRendererEvent } from 'electron'
import type {
  Api,
  ChatEventMsg,
  ChatExitMsg,
  ChatPermissionRequest,
  SessionInfo,
  TermDataEvent,
  TermExitEvent,
  UpdateStatus
} from '../shared/types'

function listen<T>(channel: string, cb: (payload: T) => void): () => void {
  const handler = (_e: IpcRendererEvent, payload: T) => cb(payload)
  ipcRenderer.on(channel, handler)
  return () => ipcRenderer.removeListener(channel, handler)
}

const api: Api = {
  getState: () => ipcRenderer.invoke('state:get'),
  addProject: () => ipcRenderer.invoke('project:add'),
  removeProject: (id) => ipcRenderer.invoke('project:remove', id),
  addProfile: (name) => ipcRenderer.invoke('profile:add', name),
  removeProfile: (name) => ipcRenderer.invoke('profile:remove', name),
  saveSettings: (settings) => ipcRenderer.invoke('settings:save', settings),
  openFolder: (path) => ipcRenderer.invoke('shell:openFolder', path),
  openInEditor: (path) => ipcRenderer.invoke('shell:openInEditor', path),
  spawnTerminal: (req) => ipcRenderer.invoke('term:spawn', req),
  writeTerminal: (id, data) => ipcRenderer.send('term:write', id, data),
  dictate: (id) => ipcRenderer.send('term:dictate', id),
  saveClipboardImage: () => ipcRenderer.invoke('clipboard:saveImage'),
  pickImages: () => ipcRenderer.invoke('dialog:pickImages'),
  imageThumbnail: (path) => ipcRenderer.invoke('image:thumbnail', path),
  openImage: (path) => ipcRenderer.invoke('image:open', path),
  pathForFile: (file) => webUtils.getPathForFile(file),
  resizeTerminal: (id, cols, rows) => ipcRenderer.send('term:resize', id, cols, rows),
  killTerminal: (id) => ipcRenderer.send('term:kill', id),
  onTermData: (cb) => listen<TermDataEvent>('term:data', cb),
  onTermExit: (cb) => listen<TermExitEvent>('term:exit', cb),
  chatStart: (req) => ipcRenderer.invoke('chat:start', req),
  chatSend: (tabId, text, images) => ipcRenderer.invoke('chat:send', tabId, text, images),
  chatStop: (tabId) => ipcRenderer.send('chat:stop', tabId),
  chatClose: (tabId) => ipcRenderer.send('chat:close', tabId),
  chatSetPermissionMode: (tabId, mode) => ipcRenderer.send('chat:setMode', tabId, mode),
  chatPermissionReply: (tabId, requestId, decision) => ipcRenderer.send('chat:permissionReply', tabId, requestId, decision),
  onChatEvent: (cb) => listen<ChatEventMsg>('chat:event', cb),
  onChatExit: (cb) => listen<ChatExitMsg>('chat:exit', cb),
  onChatPermission: (cb) => listen<ChatPermissionRequest>('chat:permission', cb),
  onChatPermissionDone: (cb) => listen<{ tabId: string; requestId: string; decision: string }>('chat:permission-done', cb),
  getSessions: () => ipcRenderer.invoke('sessions:get'),
  onSessionUpdate: (cb) => listen<SessionInfo>('session:update', cb),
  notify: (n) => ipcRenderer.send('notify', n),
  onNotificationClick: (cb) => listen<string>('notify:click', cb),
  gitInfo: (projectId) => ipcRenderer.invoke('git:info', projectId),
  listHistory: (projectId) => ipcRenderer.invoke('history:list', projectId),
  listMcp: (profile, projectId) => ipcRenderer.invoke('mcp:list', profile, projectId),
  refreshAccounts: (profile) => ipcRenderer.invoke('accounts:refresh', profile),
  onStateChanged: (cb) => listen<null>('state:changed', () => cb()),
  checkForUpdates: () => ipcRenderer.invoke('update:check'),
  installUpdate: () => ipcRenderer.send('update:install'),
  onUpdateStatus: (cb) => listen<UpdateStatus>('update:status', cb)
}

contextBridge.exposeInMainWorld('api', api)
