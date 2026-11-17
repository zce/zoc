import { isMyGo } from "mygo-runtime"
import { Desktop, events } from "./mygo"

const ICONS = {
  minimize: '<svg viewBox="0 0 10 10" aria-hidden="true"><path d="M2 5h6" fill="none" stroke="currentColor" stroke-width="1"/></svg>',
  maximize:
    '<svg viewBox="0 0 10 10" aria-hidden="true"><rect x="2.5" y="2.5" width="5" height="5" fill="none" stroke="currentColor" stroke-width="1"/></svg>',
  restore:
    '<svg viewBox="0 0 10 10" aria-hidden="true"><path d="M3.5 2.5h4v4M2.5 3.5h4v4" fill="none" stroke="currentColor" stroke-width="1"/></svg>',
  close:
    '<svg viewBox="0 0 10 10" aria-hidden="true"><path d="M2 2l6 6M8 2l-6 6" fill="none" stroke="currentColor" stroke-width="1"/></svg>',
}

/** The page's window controls, installed only in the desktop WebView. */
export function installWindowControls(initialFullscreen = false) {
  if (!isMyGo() || typeof document === "undefined" || document.getElementById("zoc-window-controls")) return

  const bar = document.createElement("div")
  bar.id = "zoc-window-controls"
  bar.setAttribute("role", "group")
  bar.setAttribute("aria-label", "Window controls")

  const button = (action: "minimize" | "maximize" | "close", label: string, icon: string) => {
    const element = document.createElement("button")
    element.type = "button"
    element.dataset.action = action
    element.setAttribute("aria-label", label)
    element.title = label
    element.innerHTML = icon
    bar.append(element)
    return element
  }

  const minimize = button("minimize", "Minimize", ICONS.minimize)
  const maximize = button("maximize", "Maximize", ICONS.maximize)
  const close = button("close", "Close", ICONS.close)

  const refresh = async () => {
    const maximized = await Desktop.isMaximizedWindow().catch(() => false)
    if (maximized) document.documentElement.dataset.zocMaximized = "true"
    else delete document.documentElement.dataset.zocMaximized
    maximize.innerHTML = maximized ? ICONS.restore : ICONS.maximize
    const label = maximized ? "Restore" : "Maximize"
    maximize.setAttribute("aria-label", label)
    maximize.title = label
  }

  minimize.addEventListener("click", () => void Desktop.minimizeWindow())
  maximize.addEventListener("click", () => {
    void Desktop.toggleMaximizeWindow().finally(() => void refresh())
  })
  close.addEventListener("click", () => void Desktop.closeWindow())
  window.addEventListener("resize", () => void refresh())

  const mount = () => {
    document.body.append(bar)
    void refresh()
  }
  const syncFullscreen = (fullscreen: boolean) => {
    bar.style.display = fullscreen ? "none" : "flex"
    if (fullscreen) document.documentElement.dataset.zocFullscreen = "true"
    else delete document.documentElement.dataset.zocFullscreen
  }
  events.fullscreenChanged.on(syncFullscreen)
  syncFullscreen(initialFullscreen)
  if (document.body) mount()
  else document.addEventListener("DOMContentLoaded", mount, { once: true })
}
