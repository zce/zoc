import { isMyGo } from "mygo-runtime"
import { Desktop } from "./mygo"

/**
 * WebKitGTK positions the input method popup with document coordinates, so
 * after the page scrolls the candidate window can drift off the window.
 * Report the focused editor's caret in viewport coordinates; the Go side
 * passes them to webkit_input_method_context_notify_cursor_area.
 */
let refresh = () => {}
let warned = false
let composing = false
let compositionData = ""
let compositionTarget: HTMLElement | undefined
let lastCaret: { element: HTMLElement; rect: DOMRect } | undefined
let measureContext: CanvasRenderingContext2D | null | undefined

type TerminalLike = {
  cols?: number
  rows?: number
  buffer?: { active?: { cursorX?: number; cursorY?: number } }
}

export function installImeCursorTracking(getZoom: () => number, offset = { x: 0, y: 0 }) {
  if (!isMyGo() || typeof document === "undefined") return

  let frame = 0
  let compositionTimer: number | undefined
  let last = ""
  let lastElement: HTMLElement | undefined
  const schedule = () => {
    if (frame) return
    frame = requestAnimationFrame(() => {
      frame = 0
      update(getZoom())
    })
  }
  const updateNow = () => update(getZoom())
  refresh = schedule

  const stopCompositionTimer = () => {
    if (compositionTimer !== undefined) {
      window.clearInterval(compositionTimer)
      compositionTimer = undefined
    }
  }
  const startCompositionTimer = () => {
    stopCompositionTimer()
    // The IME can show its first candidate before WebKit's selection has
    // settled. Refresh while composing; the dedupe in update keeps this
    // from sending an unchanged rectangle on every tick.
    compositionTimer = window.setInterval(updateNow, 50)
  }

  const update = (zoom: number) => {
    const element = activeEditable()
    if (!element) return
    const rect = caretRect(element)
    if (!rect) return

    if (element !== lastElement) {
      lastElement = element
      last = ""
    }

    const scale = zoom > 0 ? zoom : 1
    const x = Math.round(rect.x * scale + offset.x)
    const y = Math.round(rect.y * scale + offset.y)
    const width = Math.max(1, Math.round(rect.width * scale))
    const height = Math.max(1, Math.round(rect.height * scale))

    if (x + width < 0 || y + height < 0 || x > innerWidth || y > innerHeight) return
    const key = `${x},${y},${width},${height}`
    if (key === last) return
    last = key
    void Desktop.notifyImeCursor(x, y, width, height).catch((error) => {
      if (warned) return
      warned = true
      console.warn("[zoc] IME cursor update failed", error)
    })
  }

  const listen = (target: EventTarget, type: string, capture = false) => {
    target.addEventListener(type, schedule, { capture, passive: true })
  }

  listen(document, "selectionchange")
  listen(document, "input")
  listen(document, "keyup")
  listen(document, "pointerup")
  listen(window, "resize")

  // Prime the input method context before the IME opens its first
  // candidate window. compositionstart alone can be too late there.
  document.addEventListener("keydown", () => updateNow(), { capture: true, passive: true })
  document.addEventListener("beforeinput", () => updateNow(), { capture: true, passive: true })

  document.addEventListener(
    "focusin",
    () => {
      updateNow()
      schedule()
    },
    { passive: true },
  )
  document.addEventListener(
    "focusout",
    () => {
      composing = false
      stopCompositionTimer()
      schedule()
    },
    { passive: true },
  )

  // A candidate window only exists while composing. Updating the native
  // cursor area on every scroll frame otherwise adds bridge and GTK work
  // to ordinary scrolling without moving anything visible.
  document.addEventListener(
    "compositionstart",
    () => {
      composing = true
      compositionData = ""
      compositionTarget = activeEditable()
      updateNow()
      startCompositionTimer()
      schedule()
    },
    { passive: true },
  )
  document.addEventListener(
    "compositionupdate",
    (event) => {
      compositionData = (event as CompositionEvent).data ?? ""
      updateNow()
    },
    { passive: true },
  )
  document.addEventListener(
    "compositionend",
    () => {
      composing = false
      compositionData = ""
      compositionTarget = undefined
      stopCompositionTimer()
      updateNow()
      schedule()
    },
    { passive: true },
  )
  document.addEventListener(
    "scroll",
    () => {
      if (composing) updateNow()
    },
    { capture: true, passive: true },
  )

  schedule()
}

export function refreshImeCursor() {
  refresh()
}

function activeEditable(): HTMLElement | undefined {
  const active = document.activeElement
  if (!(active instanceof HTMLElement)) return
  if (active.isContentEditable) return active
  if (active instanceof HTMLTextAreaElement) return active
  if (active instanceof HTMLInputElement) {
    return ["text", "search", "email", "url", "tel", "password", "number"].includes(active.type) ? active : undefined
  }
  return
}

function caretRect(element: HTMLElement): DOMRect | undefined {
  if (element instanceof HTMLTextAreaElement) {
    // The terminal's textarea is a clipped 1×1 input at the top-left; its
    // own rectangle is never the visible cursor. If the terminal instance
    // is not available yet, skip rather than falling back to that corner.
    if (element.closest('[data-component="terminal"]')) return terminalCursorRect(element)
    return textControlRect(element)
  }

  if (element instanceof HTMLInputElement) return textControlRect(element)

  if (element.isContentEditable) {
    const editor = element.getBoundingClientRect()
    const usable = (rect: DOMRect) => {
      if (!rect.width && !rect.height) return false
      // A collapsed caret in a transient DOM state can report the whole
      // editor. Sending that makes the candidate window jump from the
      // caret to the editor's top-left; keep the last real caret instead.
      return !(
        Math.abs(rect.left - editor.left) < 2 &&
        Math.abs(rect.top - editor.top) < 2 &&
        Math.abs(rect.width - editor.width) < 2 &&
        Math.abs(rect.height - editor.height) < 2
      )
    }

    const selection = window.getSelection()
    if (selection && selection.rangeCount > 0) {
      if (composing) {
        const range = selection.getRangeAt(0).getBoundingClientRect()
        if (usable(range) && range.width > 1) {
          lastCaret = { element, rect: range }
          return range
        }
      }

      const node = selection.focusNode
      if (node && element.contains(node)) {
        const range = document.createRange()
        range.setStart(node, selection.focusOffset)
        range.collapse(true)
        const caret = range.getBoundingClientRect()
        if (usable(caret)) {
          const adjusted = compositionCaretRect(element, caret, selection)
          lastCaret = { element, rect: adjusted }
          return adjusted
        }
      }

      const range = selection.getRangeAt(0).getBoundingClientRect()
      if (usable(range)) {
        lastCaret = { element, rect: range }
        return range
      }
    }

    // While WebKitGTK rebuilds the composition selection, keep the candidate
    // window at the last real caret instead of jumping to the editor rect.
    if (lastCaret?.element === element) return lastCaret.rect
    return undefined
  }

  const rect = element.getBoundingClientRect()
  return rect.width || rect.height ? rect : undefined
}

/**
 * WebKitGTK can leave a contenteditable's collapsed selection at the start
 * of the composition while the preedit is painted after it. If the preedit
 * is not reflected at the caret, shift the reported caret by its width.
 */
function compositionCaretRect(
  element: HTMLElement,
  caret: DOMRect,
  selection: Selection,
): DOMRect {
  if (!composing || !compositionData) return caret
  const node = selection.focusNode
  if (node && node.nodeType === Node.TEXT_NODE) {
    const text = node.textContent ?? ""
    const offset = selection.focusOffset
    if (text.slice(offset - compositionData.length, offset) === compositionData) return caret
    if (text.slice(offset, offset + compositionData.length) !== compositionData) {
      // The preedit is outside the DOM text at this caret; assume it follows.
    }
  }
  const style = getComputedStyle(element)
  const font = [style.fontStyle, style.fontVariant, style.fontWeight, style.fontSize, style.fontFamily].join(" ")
  const width = textWidth(compositionData, font)
  if (!width) return caret
  return new DOMRect(caret.x + width, caret.y, Math.max(1, caret.width), caret.height)
}

/**
 * The caret rectangle of an input or textarea. A single-line input is
 * measured with the canvas; a textarea uses the current line. Sending the
 * whole control rectangle makes the IME window sit at its top-left and
 * jump whenever the control resizes.
 */
function textControlRect(element: HTMLInputElement | HTMLTextAreaElement): DOMRect | undefined {
  const rect = element.getBoundingClientRect()
  if (!rect.width || !rect.height) return
  const style = getComputedStyle(element)
  const value = element.value
  const index = element.selectionStart ?? value.length
  const preedit = compositionTarget === element ? compositionData : ""
  let startText = value.slice(0, index)
  let endText = startText
  if (preedit) {
    const after = value.slice(index)
    if (after.startsWith(preedit)) {
      // WebKit put the composition in the input at the selection.
      endText = value.slice(0, index + preedit.length)
    } else if (index >= preedit.length && value.slice(index - preedit.length, index) === preedit) {
      // The selection already sits after a composition held in the value.
      startText = value.slice(0, index - preedit.length)
      endText = value.slice(0, index)
    } else {
      // WebKit keeps the composition outside value/selectionStart while
      // composing; the visible composition follows the current text.
      endText = startText + preedit
    }
  }
  const lineHeight = number(style.lineHeight, number(style.fontSize, 14) * 1.2)
  const borderTop = number(style.borderTopWidth, 0)
  const borderBottom = number(style.borderBottomWidth, 0)
  const borderLeft = number(style.borderLeftWidth, 0)
  const borderRight = number(style.borderRightWidth, 0)
  const paddingTop = number(style.paddingTop, 0)
  const paddingBottom = number(style.paddingBottom, 0)
  const paddingLeft = number(style.paddingLeft, 0)
  const paddingRight = number(style.paddingRight, 0)
  const contentTop = rect.top + borderTop + paddingTop
  const contentHeight = Math.max(0, rect.height - borderTop - borderBottom - paddingTop - paddingBottom)
  const font = [style.fontStyle, style.fontVariant, style.fontWeight, style.fontSize, style.fontFamily].join(" ")

  if (element instanceof HTMLInputElement) {
    const startWidth = textWidth(startText, font)
    const endWidth = textWidth(endText, font)
    const width = Math.max(1, endWidth - startWidth)
    const rtl = style.direction === "rtl"
    const x = rtl
      ? rect.right - borderRight - paddingRight - endWidth + element.scrollLeft
      : rect.left + borderLeft + paddingLeft + startWidth - element.scrollLeft
    const y = contentTop + Math.max(0, (contentHeight - lineHeight) / 2)
    return new DOMRect(x, y, width, Math.max(1, lineHeight))
  }

  const startLines = startText.split("\n")
  const endLines = endText.split("\n")
  const startLine = startLines[startLines.length - 1] ?? ""
  const endLine = endLines[endLines.length - 1] ?? ""
  const line = Math.max(0, endLines.length - 1)
  const startWidth = textWidth(startLine, font)
  const endWidth = textWidth(endLine, font)
  const sameLine = startLines.length === endLines.length
  const width = sameLine ? Math.max(1, endWidth - startWidth) : 1
  const widthForX = sameLine ? startWidth : endWidth
  const rtl = style.direction === "rtl"
  const x = rtl
    ? rect.right - borderRight - paddingRight - widthForX + element.scrollLeft
    : rect.left + borderLeft + paddingLeft + widthForX - element.scrollLeft
  const y = contentTop + line * lineHeight - element.scrollTop
  return new DOMRect(x, y, width, Math.max(1, lineHeight))
}

function number(value: string, fallback: number): number {
  const parsed = Number.parseFloat(value)
  return Number.isFinite(parsed) ? parsed : fallback
}

function textWidth(text: string, font: string): number {
  if (measureContext === undefined) {
    measureContext = typeof document === "undefined" ? null : document.createElement("canvas").getContext("2d")
  }
  if (!measureContext) return 0
  measureContext.font = font
  return measureContext.measureText(text).width
}

/**
 * ghostty-web keeps keyboard focus in a 1×1, clipped textarea at the
 * terminal's top-left. Its rectangle is not the visible cursor, so use the
 * terminal buffer's viewport cursor and the canvas cell metrics instead.
 */
function terminalCursorRect(element: HTMLElement): DOMRect | undefined {
  const container = element.closest<HTMLElement>('[data-component="terminal"]')
  if (!container) return
  const terminal = (container as HTMLElement & { __zocTerminal?: TerminalLike }).__zocTerminal
  const cursor = terminal?.buffer?.active
  const canvas = container.querySelector("canvas")
  const cols = terminal?.cols ?? 0
  const rows = terminal?.rows ?? 0
  if (!terminal || !cursor || !(canvas instanceof HTMLCanvasElement) || cols <= 0 || rows <= 0) return

  const rect = canvas.getBoundingClientRect()
  const width = rect.width / cols
  const height = rect.height / rows
  if (!width || !height) return
  return new DOMRect(
    rect.left + (cursor.cursorX ?? 0) * width,
    rect.top + (cursor.cursorY ?? 0) * height,
    width,
    height,
  )
}
