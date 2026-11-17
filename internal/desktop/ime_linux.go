//go:build linux

package desktop

import (
	"log"
	"math"
	"os"
	"sync"
	"sync/atomic"

	"github.com/ebitengine/purego"
	"github.com/egoist/mygo"
)

// WebKitGTK's input method context keeps the caret's document coordinates
// after the page scrolls, so the candidate window drifts away from the
// focused editor. The page reports the caret's viewport rectangle, which
// we pass to webkit_input_method_context_notify_cursor_area on the GTK
// thread. This mirrors the workaround used by Tauri/wry on Linux.
var (
	imeOnce          sync.Once
	imeReady         bool
	imeDebug         = os.Getenv("ZOC_DEBUG_IME") == "1"
	imeWebView       atomic.Uint64
	gtkWindowGet     func(uintptr) uintptr
	gtkWidgetParent  func(uintptr) uintptr
	gtkGetChildren   func(uintptr) uintptr
	gtkContainerType func() uintptr
	gListLength      func(uintptr) uint32
	gListNthData     func(uintptr, uint32) uintptr
	gListFree        func(uintptr)
	typeIsA          func(uintptr, uintptr) bool
	webViewType      func() uintptr
	webViewIME       func(uintptr) uintptr
	imeNotifyArea    func(uintptr, int32, int32, int32, int32)
)

func bindIME(lib uintptr, name string, fn any) bool {
	addr, err := purego.Dlsym(lib, name)
	if err != nil || addr == 0 {
		return false
	}
	purego.RegisterFunc(fn, addr)
	return true
}

func loadIME() {
	gtk, err := purego.Dlopen("libgtk-3.so.0", purego.RTLD_LAZY|purego.RTLD_GLOBAL)
	if err != nil {
		return
	}
	gobject, err := purego.Dlopen("libgobject-2.0.so.0", purego.RTLD_LAZY|purego.RTLD_GLOBAL)
	if err != nil {
		return
	}
	glib, err := purego.Dlopen("libglib-2.0.so.0", purego.RTLD_LAZY|purego.RTLD_GLOBAL)
	if err != nil {
		return
	}
	webkit, err := purego.Dlopen("libwebkit2gtk-4.1.so.0", purego.RTLD_LAZY|purego.RTLD_GLOBAL)
	if err != nil {
		webkit, err = purego.Dlopen("libwebkit2gtk-4.0.so.37", purego.RTLD_LAZY|purego.RTLD_GLOBAL)
	}
	if err != nil {
		return
	}
	imeReady = bindIME(gtk, "gtk_window_get_focus", &gtkWindowGet) &&
		bindIME(gtk, "gtk_widget_get_parent", &gtkWidgetParent) &&
		bindIME(gtk, "gtk_container_get_children", &gtkGetChildren) &&
		bindIME(gtk, "gtk_container_get_type", &gtkContainerType) &&
		bindIME(glib, "g_list_length", &gListLength) &&
		bindIME(glib, "g_list_nth_data", &gListNthData) &&
		bindIME(glib, "g_list_free", &gListFree) &&
		bindIME(gobject, "g_type_check_instance_is_a", &typeIsA) &&
		bindIME(webkit, "webkit_web_view_get_type", &webViewType) &&
		bindIME(webkit, "webkit_web_view_get_input_method_context", &webViewIME) &&
		bindIME(webkit, "webkit_input_method_context_notify_cursor_area", &imeNotifyArea)
}

func isWebView(w uintptr) bool {
	return w != 0 && typeIsA(w, webViewType())
}

func isContainer(w uintptr) bool {
	return w != 0 && typeIsA(w, gtkContainerType())
}

func findWebViewIn(w uintptr, depth int) uintptr {
	if w == 0 || depth > 12 {
		return 0
	}
	if isWebView(w) {
		return w
	}
	if !isContainer(w) {
		return 0
	}
	list := gtkGetChildren(w)
	if list == 0 {
		return 0
	}
	defer gListFree(list)
	for i, n := uint32(0), gListLength(list); i < n; i++ {
		if found := findWebViewIn(gListNthData(list, i), depth+1); found != 0 {
			return found
		}
	}
	return 0
}

func findWebView(win uintptr) uintptr {
	if win == 0 {
		return 0
	}
	if focus := gtkWindowGet(win); focus != 0 {
		for w := focus; w != 0; w = gtkWidgetParent(w) {
			if isWebView(w) {
				return w
			}
		}
	}
	return findWebViewIn(win, 0)
}

func imeNotify(win uintptr, x, y, width, height float64) {
	imeOnce.Do(loadIME)
	if !imeReady || win == 0 {
		if imeDebug {
			log.Printf("zoc: IME bridge unavailable (ready=%v win=%#x)", imeReady, win)
		}
		return
	}
	web := uintptr(imeWebView.Load())
	if web == 0 || !isWebView(web) {
		web = findWebView(win)
		if web == 0 {
			if imeDebug {
				log.Printf("zoc: IME web view not found (win=%#x)", win)
			}
			return
		}
		imeWebView.Store(uint64(web))
	}
	context := webViewIME(web)
	if context == 0 {
		if imeDebug {
			log.Printf("zoc: IME context unavailable (web=%#x)", web)
		}
		return
	}
	ix, iy := int32(math.Round(x)), int32(math.Round(y))
	iw, ih := int32(math.Round(width)), int32(math.Round(height))
	if iw < 1 {
		iw = 1
	}
	if ih < 1 {
		ih = 1
	}
	if imeDebug {
		log.Printf("zoc: IME cursor web=%#x ctx=%#x at %d,%d %dx%d", web, context, ix, iy, iw, ih)
	}
	imeNotifyArea(context, ix, iy, iw, ih)
}

// notifyImeCursor runs the WebKitGTK call on the GTK main thread.
func notifyImeCursor(w *mygo.Window, x, y, width, height float64) {
	if w == nil {
		return
	}
	mygo.RunOnMain(func() {
		imeNotify(w.NativeHandle(), x, y, width, height)
	})
}
