package main

import (
	"encoding/json"
	"fmt"
	"log"
	"net/url"
	"os"
	"runtime"
	"strconv"
	"strings"
	"sync"
	"sync/atomic"
	"time"

	"github.com/egoist/mygo"
	"github.com/egoist/mygo/plugins/fetch"
	"github.com/egoist/mygo/plugins/websocket"
	"github.com/zce/zoc/internal/desktop"
	"github.com/zce/zoc/internal/native"
	"github.com/zce/zoc/internal/networkpolicy"
	"github.com/zce/zoc/internal/profiles"
)

const version = "0.1.0"

func main() {
	startedAt := time.Now()
	if os.Getenv("MYGO_GENERATE") == "" {
		if !mygo.App.RequestSingleInstanceLock() {
			return
		}
	}

	if raw := strings.TrimSpace(os.Getenv("ZOC_USER_DATA")); raw != "" {
		mygo.App.SetPath(mygo.PathUserData, raw)
	}

	configureInputModule()

	// Let WebKitGTK use its DMA-BUF renderer where the host can draw with
	// it. MyGo keeps it off by default because it blanks windows on NVIDIA,
	// in VMs and in containers; ZOC's policy enables it on the Wayland +
	// recent NVIDIA combination verified here, and elsewhere leaves MyGo's
	// auto probe to decide. ZOC_DMABUF=on|off|auto overrides the policy.
	mygo.App.SetDMABUFRenderer(dmabufRenderer())

	policy, err := networkpolicy.New()
	if err != nil {
		log.Fatal(err)
	}
	service := &desktop.Service{
		Version:       version,
		WindowID:      "main",
		Policy:        policy,
		ReduceEffects: reduceEffectsDefault(),
	}
	inbox := native.NewInbox(service)
	service.OpenInbox = inbox.Open

	// Native transports are restricted to configured server origins. The
	// policy is filled from the profile store once the app is ready, before
	// the first page can make a request.
	client := &httpClient{policy: policy}
	mygo.Use(fetch.New(fetch.Options{
		Client: client.httpClient(),
		Allow:  policy.AllowsRequest,
	}))
	mygo.Use(websocket.New(websocket.Options{
		Client: client.httpClient(),
		Allow:  policy.AllowsRequest,
	}))
	mygo.BindAs("Desktop", desktop.NewBindings(service))

	var pendingLinks []string
	var pendingMu sync.Mutex
	var linksReady atomic.Bool
	drainLinks := func() {
		pendingMu.Lock()
		links := pendingLinks
		pendingLinks = nil
		pendingMu.Unlock()
		for _, link := range links {
			handleDeepLink(service, link)
		}
	}
	mygo.App.OnOpenURL(func(raw string) {
		pendingMu.Lock()
		pendingLinks = append(pendingLinks, raw)
		pendingMu.Unlock()
		if linksReady.Load() {
			drainLinks()
		}
	})

	mygo.App.WhenReady(func() {
		if err := mygo.App.RegisterURLScheme("zoc"); err != nil {
			log.Printf("zoc: register URL scheme: %v", err)
		}
		dir, err := mygo.App.Path(mygo.PathUserData)
		if err != nil {
			log.Fatal(err)
		}
		store, err := profiles.Open(dir)
		if err != nil {
			log.Fatal(err)
		}
		service.Store = store
		service.DataDir = dir
		if v := mygo.App.Version(); v != "" {
			service.Version = v
		}
		if runtime.GOOS == "linux" {
			dmabuf := "enabled"
			if os.Getenv("WEBKIT_DISABLE_DMABUF_RENDERER") == "1" {
				dmabuf = "disabled (CPU)"
			}
			log.Printf(
				"zoc: dmabuf=%s session=%s gdk=%s software_gl=%s",
				dmabuf,
				os.Getenv("XDG_SESSION_TYPE"),
				os.Getenv("GDK_BACKEND"),
				os.Getenv("LIBGL_ALWAYS_SOFTWARE"),
			)
		}
		if service.ReduceEffects {
			log.Printf("zoc: reduced compositing effects (ZOC_REDUCE_EFFECTS=0 restores animations)")
		}
		if err := policy.Replace(store.Origins()); err != nil {
			log.Fatal(err)
		}

		opts := mygo.WindowOptions{
			Title:           "ZOC",
			URL:             "/",
			Width:           1280,
			Height:          850,
			MinWidth:        760,
			MinHeight:       520,
			StateKey:        "main",
			BackgroundColor: "light-dark(transparent, transparent)",
			// ZOC is a developer client; keep the Web Inspector available
			// from View → Toggle Developer Tools for diagnosing the embedded
			// official UI.
			Page: mygo.PageOptions{DevTools: mygo.DevToolsEnabled},
		}
		if runtime.GOOS == "linux" && os.Getenv("ZOC_NATIVE_TITLEBAR") != "1" {
			// The official Web UI draws its own 36px title bar. Hide GTK's
			// title bar and its buttons; the page draws the window controls
			// in the title bar's trailing space and calls the native
			// minimize, maximize and close bindings.
			opts.TitleBarStyle = mygo.TitleBarHidden
			opts.TitleBarHeight = 36
			opts.HideWindowButtons = true
		}
		win := mygo.NewWindow(opts)
		service.SetMainWindow(win)
		applyWindowChrome()
		mygo.Theme.OnUpdated(applyWindowChrome)
		service.InstallContextMenu(win)
		menu := buildMenu(service)
		if runtime.GOOS == "darwin" {
			mygo.App.SetMenu(menu)
		} else {
			win.SetMenu(menu)
		}

		if out := os.Getenv("ZOC_SMOKE_OUT"); out != "" {
			installSmokeProbe(win, out, startedAt, inbox)
		}

		linksReady.Store(true)
		drainLinks()
	})

	mygo.App.OnSecondInstance(func(args []string, workingDir string) {
		service.FocusMain()
	})

	if err := mygo.App.Run(); err != nil {
		log.Fatal(err)
	}
}

// applyWindowChrome paints the GTK CSD background with the same color as
// the Web UI. The webview is transparent so the CSD rounded corners show
// through the page's clipped corners; without this the theme's default
// window background appears as a gray corner around a dark page.
//
// Adwaita's GTK 3 theme rounds only the decoration's top corners
// (`decoration { border-radius: 8px 8px 0 0 }`); GTK 4 apps, which the
// desktop's native apps use, round all four. ZOC runs on GTK 3, so it
// rounds the decoration and the window background itself, and keeps the
// square shape the theme gives maximized, full-screen and tiled windows.
func applyWindowChrome() {
	background := "#fafafa"
	if mygo.Theme.IsDark() {
		background = "#191515"
	}
	radius := "8"
	if value := strings.TrimSpace(os.Getenv("ZOC_WINDOW_RADIUS")); value != "" {
		radius = value
	}
	// Round only normal windows: the theme already squares decorated
	// popups and maximized, full-screen and tiled windows. Excluding those
	// states here lets the theme's own rules win without !important, which
	// GTK 3's CSS parser does not accept.
	state := ":not(.popup):not(.tooltip):not(.maximized):not(.fullscreen)" +
		":not(.tiled):not(.tiled-top):not(.tiled-right):not(.tiled-bottom):not(.tiled-left)"
	css := fmt.Sprintf(
		`window.csd, window.solid-csd { background-color: %[1]s; }
window.csd%[2]s, window.solid-csd%[2]s { border-radius: %[3]spx; }
window.csd%[2]s decoration, window.solid-csd%[2]s decoration { border-radius: %[3]spx; }`,
		background,
		state,
		radius,
	)
	if err := mygo.Theme.SetGTKCSS(css); err != nil {
		log.Printf("zoc: window background CSS: %v", err)
	}
}

// configureInputModule uses IBus's GTK module when the session runs IBus
// but GTK_IM_MODULE is unset. On Wayland the default text-input module can
// place the first IBus candidate before WebKit's cursor area reaches the
// panel; the IBus module follows the context sooner. An explicit
// GTK_IM_MODULE always wins.
func configureInputModule() {
	if runtime.GOOS != "linux" || os.Getenv("GTK_IM_MODULE") != "" {
		return
	}
	ibus := strings.Contains(os.Getenv("XMODIFIERS"), "@im=ibus") ||
		strings.Contains(os.Getenv("QT_IM_MODULE"), "ibus")
	if !ibus {
		return
	}
	if err := os.Setenv("GTK_IM_MODULE", "ibus"); err == nil {
		log.Printf("zoc: GTK_IM_MODULE=ibus for the IBus session")
	}
}

// dmabufRenderer chooses WebKitGTK's renderer before App.Run. MyGo's auto
// probe disables DMA-BUF on every NVIDIA host because older drivers blank
// the window; the verified WebKitGTK 2.54 + NVIDIA 600-series combination
// draws correctly and scrolls much faster with it. Other hosts keep MyGo's
// safer probe.
func dmabufRenderer() mygo.DMABUFRenderer {
	switch strings.ToLower(strings.TrimSpace(os.Getenv("ZOC_DMABUF"))) {
	case "on":
		return mygo.DMABUFRendererOn
	case "off":
		return mygo.DMABUFRendererOff
	case "auto":
		return mygo.DMABUFRendererAuto
	}
	if runtime.GOOS == "linux" && os.Getenv("WAYLAND_DISPLAY") != "" && nvidiaDriverMajor() >= 600 {
		return mygo.DMABUFRendererOn
	}
	return mygo.DMABUFRendererAuto
}

// reduceEffectsDefault keeps WebKitGTK's accelerated compositing workload
// down on the Wayland/NVIDIA combination where its backdrop filters and
// continuous CSS animations are expensive. The page is still rendered on
// the GPU; only the effects that trigger repeated compositing are removed.
// ZOC_REDUCE_EFFECTS=0 restores full effects, and =1 forces the reduced mode
// on other hosts.
func reduceEffectsDefault() bool {
	switch strings.ToLower(strings.TrimSpace(os.Getenv("ZOC_REDUCE_EFFECTS"))) {
	case "1", "true", "on", "reduced":
		return true
	case "0", "false", "off", "full":
		return false
	}
	return runtime.GOOS == "linux" && os.Getenv("WAYLAND_DISPLAY") != "" && nvidiaDriverMajor() >= 600
}

// nvidiaDriverMajor returns the major version in /proc/driver/nvidia/version,
// or 0 when the NVIDIA driver is not loaded.
func nvidiaDriverMajor() int {
	data, err := os.ReadFile("/proc/driver/nvidia/version")
	if err != nil {
		return 0
	}
	for _, field := range strings.Fields(string(data)) {
		major, _, ok := strings.Cut(field, ".")
		if !ok {
			continue
		}
		if n, err := strconv.Atoi(major); err == nil && n > 0 {
			return n
		}
	}
	return 0
}

// installSmokeProbe lets CI verify that the embedded official Web UI
// actually mounted under WebKitGTK, not merely that the process started. It
// writes a JSON result and quits. It is inert unless ZOC_SMOKE_OUT is set.
func installSmokeProbe(win *mygo.Window, out string, startedAt time.Time, inbox *native.Inbox) {
	var once sync.Once
	win.Page().OnDidFinishLoad(func() {
		once.Do(func() {
			loadedAt := time.Now()
			go func() {
				time.Sleep(4 * time.Second)
				raw, err := win.Page().Eval(`const bootstrap = await window.mygo.call("Desktop.Bootstrap")
await window.mygo.call("Desktop.StorageUpdate", "smoke", { key: "value" }, [])
const storage = await window.mygo.call("Desktop.StorageItems", "smoke")
await window.mygo.call("Desktop.WriteClipboardText", "zoc-smoke")
const clipboard = await window.mygo.call("Desktop.ReadClipboardText")
await window.mygo.call("Desktop.NotifyImeCursor", 8, 8, 1, 16)
const wsURL = new URL(bootstrap.servers[0].url)
wsURL.protocol = wsURL.protocol === "https:" ? "wss:" : "ws:"
wsURL.pathname = "/api/experimental/persistent-pty/pty-fixture/connect"
wsURL.search = "?ticket=fixture-ticket"
const wsEvents = window.mygo.channel()
void window.mygo.call("plugin:websocket.Connect", {
  id: "smoke-ws", url: wsURL.href, protocols: [], headers: []
}, wsEvents)
const wsIterator = wsEvents[Symbol.asyncIterator]()
const wsFirst = await Promise.race([
  wsIterator.next(),
  new Promise((_, reject) => setTimeout(() => reject(new Error("websocket timeout")), 5000))
])
const wsSecond = await Promise.race([
  wsIterator.next(),
  new Promise((_, reject) => setTimeout(() => reject(new Error("websocket timeout")), 5000))
])
return JSON.stringify({
  title: document.title,
  root: !!document.querySelector("#root"),
  mounted: !!document.querySelector("#root > *"),
  mygo: typeof window.mygo === "object",
  zocOs: document.documentElement.dataset.zocOs ?? "",
  windowControls: !!document.querySelector("#zoc-window-controls"),
  version: bootstrap.version,
  servers: bootstrap.servers.length,
  storage: storage.items.key ?? null,
  clipboard,
  bash: await window.mygo.call("Desktop.CheckAppExists", "bash"),
  wsEvents: [wsFirst.value, wsSecond.value],
  text: (document.body?.innerText ?? "").slice(0, 4000)
})`)
				if os.Getenv("ZOC_SMOKE_INBOX") != "" {
					inbox.Open()
					time.Sleep(3 * time.Second)
					if nativeWindow := inbox.Window(); nativeWindow != nil {
						if png, err := nativeWindow.CapturePage(); err != nil {
							result := map[string]any{"ok": false, "error": "native inbox capture: " + err.Error()}
							data, _ := json.MarshalIndent(result, "", "  ")
							_ = os.WriteFile(out, data, 0o644)
							mygo.App.Quit()
							return
						} else {
							_ = os.WriteFile(strings.TrimSuffix(out, ".json")+"-inbox.png", png, 0o644)
						}
					}
				}
				rss, hwm := processMemory()
				result := map[string]any{
					"ok":        false,
					"startupMs": loadedAt.Sub(startedAt).Milliseconds(),
					"probeMs":   time.Since(startedAt).Milliseconds(),
					"rssKb":     rss,
					"hwmKb":     hwm,
				}
				if png, err := win.CapturePage(); err == nil {
					_ = os.WriteFile(strings.TrimSuffix(out, ".json")+"-page.png", png, 0o644)
				}
				if err != nil {
					result["error"] = err.Error()
				} else {
					result["eval"] = raw
					if s, ok := raw.(string); ok {
						result["ok"] = strings.Contains(s, `"mounted":true`) &&
							strings.Contains(s, `"mygo":true`) &&
							strings.Contains(s, `"zocOs":"linux"`) &&
							strings.Contains(s, `"windowControls":true`) &&
							strings.Contains(s, `"storage":"value"`) &&
							strings.Contains(s, `"clipboard":"zoc-smoke"`) &&
							strings.Contains(s, `"bash":true`) &&
							strings.Contains(s, `"servers":1`) &&
							strings.Contains(s, `"type":"open"`) &&
							strings.Contains(s, `"type":"text"`) &&
							strings.Contains(s, `attached`) &&
							(os.Getenv("ZOC_SMOKE_INBOX") == "" || func() bool {
								info, err := os.Stat(strings.TrimSuffix(out, ".json") + "-inbox.png")
								return err == nil && info.Size() > 20000
							}())
					}
				}
				data, _ := json.MarshalIndent(result, "", "  ")
				_ = os.WriteFile(out, data, 0o644)
				if raw := os.Getenv("ZOC_SMOKE_HOLD_MS"); raw != "" {
					if ms, err := time.ParseDuration(raw + "ms"); err == nil {
						time.Sleep(ms)
					}
				}
				mygo.App.Quit()
			}()
		})
	})
}

func buildMenu(service *desktop.Service) *mygo.Menu {
	return mygo.NewMenu([]*mygo.MenuItem{
		{Role: mygo.RoleAppMenu},
		{Label: "File", Submenu: []*mygo.MenuItem{
			{Label: "Servers & Inbox…", Accelerator: "CmdOrCtrl+Shift+I", Click: func(*mygo.MenuItem, *mygo.Window) {
				service.OpenInboxWindow()
			}},
			{Label: "Open Server Home", Accelerator: "CmdOrCtrl+Shift+H", Click: func(*mygo.MenuItem, *mygo.Window) {
				_ = service.OpenMain("/")
			}},
			mygo.Separator(),
			{Role: mygo.RoleClose},
			{Role: mygo.RoleQuit},
		}},
		{Role: mygo.RoleEditMenu},
		{Label: "View", Submenu: []*mygo.MenuItem{
			{Role: mygo.RoleReload},
			{Role: mygo.RoleForceReload},
			{Role: mygo.RoleToggleDevTools},
			mygo.Separator(),
			{Role: mygo.RoleResetZoom},
			{Role: mygo.RoleZoomIn},
			{Role: mygo.RoleZoomOut},
			mygo.Separator(),
			{Role: mygo.RoleToggleFullScreen},
		}},
		{Role: mygo.RoleWindowMenu},
		{Label: "Help", Submenu: []*mygo.MenuItem{
			{Label: "OpenCode Documentation", Click: func(*mygo.MenuItem, *mygo.Window) {
				_ = mygo.Shell.OpenExternal("https://opencode.ai/docs")
			}},
			{Label: "ZOC Documentation", Click: func(*mygo.MenuItem, *mygo.Window) {
				_ = mygo.Shell.OpenExternal("https://github.com/zce/zoc")
			}},
		}},
	})
}

// handleDeepLink supports client-only links:
//
//	zoc://add?url=http%3A%2F%2Fhost%3A4096&name=Work
//	zoc://open?url=http%3A%2F%2Fhost%3A4096
//
// Credentials are rejected by the profile store and never belong in a URL.
func handleDeepLink(service *desktop.Service, raw string) {
	u, err := url.Parse(raw)
	if err != nil || u.Scheme != "zoc" {
		return
	}
	serverURL := u.Query().Get("url")
	switch u.Host {
	case "add", "open":
		if serverURL == "" {
			return
		}
		list := service.Store.List()
		var found bool
		for _, p := range list {
			if p.URL == serverURL || strings.TrimRight(p.URL, "/") == strings.TrimRight(serverURL, "/") {
				found = true
				break
			}
		}
		if !found {
			if _, err := service.AddProfile(desktop.ProfileInput{Name: u.Query().Get("name"), URL: serverURL}); err != nil {
				log.Printf("zoc: deep link: %v", err)
				return
			}
		}
		if u.Host == "open" {
			_ = service.OpenMain("/")
		}
		service.FocusMain()
	}
}

// processMemory reads the current resident and peak resident set sizes in KiB.
func processMemory() (int64, int64) {
	data, err := os.ReadFile("/proc/self/status")
	if err != nil {
		return 0, 0
	}
	var rss, hwm int64
	for _, line := range strings.Split(string(data), "\n") {
		var value int64
		if _, err := fmt.Sscanf(line, "VmRSS: %d kB", &value); err == nil {
			rss = value
		}
		if _, err := fmt.Sscanf(line, "VmHWM: %d kB", &value); err == nil {
			hwm = value
		}
	}
	return rss, hwm
}
