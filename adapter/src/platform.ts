import type { Platform } from "@/runtime/platform/platform"
import type { ServerConnection } from "@/runtime/server/registry"
import type { DesktopZoomLevel } from "@/shell/commands/desktop-menu"
import { isMyGo, currentWindow } from "mygo-runtime"
import { fetch as mygoFetch } from "@mygo-plugins/fetch"
import { WebSocket as MyGoWebSocket } from "@mygo-plugins/websocket"
import type { AsyncStorage } from "@solid-primitives/storage"
import { createSignal } from "solid-js"
import { Desktop, events } from "./mygo"
import type { AttachmentOptions, Bootstrap, Profile } from "./mygo"

type ServerSeed = Extract<ServerConnection.Any, { type: "http" }>

export type ZocRuntime = {
  platform: Platform
  servers: ServerSeed[]
}

/* ---------------------------------------------------------------- origins */

const allowedOrigins = new Set<string>()

function normalizeOrigin(raw: string): string | undefined {
  let url: URL
  try {
    url = new URL(raw, "mygo://localhost/")
  } catch {
    return
  }
  if (url.protocol === "ws:") url.protocol = "http:"
  if (url.protocol === "wss:") url.protocol = "https:"
  if (url.protocol !== "http:" && url.protocol !== "https:") return
  const port = url.port || (url.protocol === "https:" ? "443" : "80")
  return `${url.protocol}//${url.hostname.toLowerCase()}:${port}`
}

function isServerRequest(raw: string): boolean {
  const origin = normalizeOrigin(raw)
  if (!origin) return false
  if (allowedOrigins.has(origin)) return true
  const url = new URL(raw, "mygo://localhost/")
  return url.pathname.startsWith("/api/") || url.pathname.startsWith("/global/")
}

function seedOrigins(profiles: Profile[]) {
  allowedOrigins.clear()
  for (const profile of profiles) {
    const origin = normalizeOrigin(profile.url)
    if (!origin) continue
    allowedOrigins.add(origin)
    // localhost, 127.0.0.1 and ::1 name the same loopback server on Linux.
    const url = new URL(profile.url)
    if (["localhost", "127.0.0.1", "[::1]", "::1"].includes(url.hostname.toLowerCase())) {
      const port = url.port || (url.protocol === "https:" ? "443" : "80")
      for (const host of ["localhost", "127.0.0.1", "[::1]"]) {
        allowedOrigins.add(`${url.protocol}//${host}:${port}`)
      }
    }
  }
}

const webFetch = globalThis.fetch.bind(globalThis)

/**
 * Server requests use MyGo's Go-backed fetch. App-local URLs (blob:, data:,
 * mygo:) and unrelated external requests stay on the WebView's fetch.
 * An unconfigured server is refused here rather than becoming a CORS
 * mystery; it must be added in Servers & Inbox first.
 */
export const zocFetch: typeof fetch = (input, init) => {
  const raw = input instanceof Request ? input.url : String(input)
  const origin = normalizeOrigin(raw)
  if (origin && isServerRequest(raw)) {
    if (!allowedOrigins.has(origin)) {
      return Promise.reject(
        new TypeError("ZOC: this server origin is not configured. Add it in Servers & Inbox before connecting."),
      )
    }
    return mygoFetch(input, init)
  }
  return webFetch(input, init)
}

/** The PTY WebSocket uses MyGo's Go-backed WebSocket, without an Origin header. */
export function openZocSocket(url: URL): WebSocket {
  if (!isMyGo()) return new WebSocket(url)
  const origin = normalizeOrigin(url.href)
  if (!origin || !allowedOrigins.has(origin)) {
    throw new Error("ZOC: this server origin is not configured.")
  }
  return new MyGoWebSocket(url) as unknown as WebSocket
}

/* ---------------------------------------------------------------- storage */

const storages = new Map<string, AsyncStorage>()

/**
 * Settings, server registry and prompt drafts live in ZOC's own user-data
 * directory through a typed Go boundary. Reads are cached in the page;
 * writes reach Go in call order.
 */
function storageFor(name = "default"): AsyncStorage {
  const cached = storages.get(name)
  if (cached) return cached

  const cache = new Map<string, string>()
  let ready: Promise<void> | undefined
  let writes: Promise<unknown> = Promise.resolve()

  const load = () => {
    ready ??= Desktop.storageItems(name).then((snapshot) => {
      for (const [key, value] of Object.entries(snapshot.items)) cache.set(key, value)
    })
    return ready
  }

  const flush = (insert: Record<string, string>, remove: string[]) => {
    const next = writes.then(() => Desktop.storageUpdate(name, insert, remove)).then(
      () => undefined,
      (error) => {
        console.error(`[zoc] storage write failed for ${name}`, error)
      },
    )
    writes = next
    return next
  }

  const storage: AsyncStorage = {
    getItem: async (key) => {
      await load()
      return cache.get(key) ?? null
    },
    setItem: async (key, value) => {
      await load()
      cache.set(key, value)
      await flush({ [key]: value }, [])
    },
    removeItem: async (key) => {
      await load()
      cache.delete(key)
      await flush({}, [key])
    },
    clear: async () => {
      await load()
      cache.clear()
      await Desktop.storageClear(name)
    },
    key: async (index: number) => {
      await load()
      return [...cache.keys()][index] ?? null
    },
    getLength: async () => {
      await load()
      return cache.size
    },
    get length() {
      return cache.size
    },
  }
  storages.set(name, storage)
  return storage
}

/* --------------------------------------------------------------- platform */

function base64Bytes(value: string): ArrayBuffer {
  const binary = atob(value)
  const buffer = new ArrayBuffer(binary.length)
  const bytes = new Uint8Array(buffer)
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i)
  return buffer
}

function pickerFilters(opts: { accept?: string[]; extensions?: string[] }) {
  const extensions = new Set(opts.extensions ?? [])
  const mimeTypes = new Set<string>()
  for (const accept of opts.accept ?? []) {
    if (accept.startsWith(".")) extensions.add(accept.slice(1))
    else if (accept.includes("/")) mimeTypes.add(accept)
    else extensions.add(accept)
  }
  return { extensions: [...extensions], mimeTypes: [...mimeTypes] }
}

function browserPlatform(version: string): ZocRuntime {
  const platform: Platform = {
    platform: "web",
    version,
    openExternal(value) {
      if (!URL.canParse(value)) return
      const url = new URL(value)
      if (!["http:", "https:", "mailto:"].includes(url.protocol)) return
      window.open(url.href, "_blank", "noopener,noreferrer")
    },
    restart: async () => window.location.reload(),
    async notify(title, description, onClick) {
      if (!("Notification" in window)) return
      const permission =
        Notification.permission === "default"
          ? await Notification.requestPermission().catch(() => "denied")
          : Notification.permission
      if (permission !== "granted") return
      if (document.visibilityState === "visible" && document.hasFocus()) return
      const notification = new Notification(title, { body: description ?? "" })
      notification.onclick = () => {
        window.focus()
        onClick?.()
        notification.close()
      }
    },
  }
  return { platform, servers: [] }
}

export async function createZocPlatform(version: string): Promise<ZocRuntime> {
  if (!isMyGo()) return browserPlatform(version)

  let bootstrap: Bootstrap
  try {
    bootstrap = await Desktop.bootstrap()
  } catch (error) {
    console.error("[zoc] bootstrap failed", error)
    return browserPlatform(version)
  }
  seedOrigins(bootstrap.servers)

  events.serverProfilesChanged.on((profiles) => seedOrigins(profiles))

  const servers: ServerSeed[] = bootstrap.servers.map((profile) => ({
    type: "http",
    displayName: profile.name,
    http: { url: profile.url },
  }))

  const [zoomLevel, setZoomLevelSignal] = createSignal<DesktopZoomLevel>((bootstrap.zoom || 1) as DesktopZoomLevel)
  const [fullscreen, setFullscreen] = createSignal(bootstrap.fullscreen ?? false)
  events.fullscreenChanged.on((value) => setFullscreen(value))

  const notificationActions = new Map<string, () => void>()
  events.notificationClicked.on((token) => {
    const action = notificationActions.get(token)
    notificationActions.delete(token)
    action?.()
  })

  const platform: Platform = {
    platform: "desktop",
    os: "linux",
    windowID: bootstrap.windowID || "main",
    version: bootstrap.version || version,
    fetch: zocFetch,

    openExternal(value) {
      void Desktop.openExternal(value).catch((error) => {
        console.warn("[zoc] openExternal failed", error)
        window.open(value, "_blank", "noopener,noreferrer")
      })
    },
    async openBrowser(value) {
      try {
        await Desktop.openExternal(value)
        return true
      } catch {
        return false
      }
    },
    async openPath(path) {
      await Desktop.openPath(path)
    },
    openLocalFile(url) {
      void Desktop.openLocalFile(url).catch((error) => console.warn("[zoc] openLocalFile failed", error))
    },
    async revealPath(path) {
      return Desktop.revealPath(path)
    },
    restart: async () => {
      await Desktop.restart()
    },
    async notify(title, description, onClick) {
      try {
        const token = onClick ? crypto.randomUUID() : ""
        if (onClick) {
          if (notificationActions.size > 100) notificationActions.clear()
          notificationActions.set(token, onClick)
        }
        await Desktop.notify(title, description ?? "", token)
      } catch (error) {
        console.warn("[zoc] notification failed", error)
      }
    },
    async openDirectoryPickerDialog(opts) {
      const paths = await Desktop.pickDirectory(opts?.title ?? "", opts?.multiple ?? false)
      return paths.length > 0 ? paths : null
    },
    async openAttachmentPickerDialog(opts, onFile) {
      const filters = pickerFilters(opts)
      const files = await Desktop.pickAttachmentFiles({
        title: opts.title,
        multiple: opts.multiple,
        extensions: filters.extensions,
        mimeTypes: filters.mimeTypes,
      })
      for (const picked of files) {
        const file = new File([base64Bytes(picked.data)], picked.name, {
          type: picked.mime,
          lastModified: Date.now(),
        })
        await onFile(file)
      }
    },
    async openFilePathPickerDialog(opts) {
      const paths = await Desktop.pickFilePaths({
        title: opts.title,
        multiple: opts.multiple,
        extensions: opts.extensions,
      })
      return paths.length > 0 ? paths : null
    },
    async saveFile(opts, content) {
      return Desktop.saveFile(opts.title ?? "", opts.defaultPath ?? "", content)
    },
    storage: (name) => storageFor(name),
    webviewZoom: zoomLevel,
    zoomLevel,
    async setZoomLevel(level) {
      await Desktop.setZoom(level)
      setZoomLevelSignal(level)
    },
    windowFullscreen: fullscreen,
    async readClipboardImage() {
      const data = await Desktop.readClipboardImage()
      if (!data) return null
      return new File([base64Bytes(data)], "clipboard.png", { type: "image/png" })
    },
    async writeClipboardText(text) {
      await Desktop.writeClipboardText(text)
    },
    async checkAppExists(name) {
      return Desktop.checkAppExists(name)
    },
  }

  return { platform, servers }
}

export { currentWindow }
