package main

import (
	"encoding/json"
	"fmt"
	"log"
	"net/url"
	"os"
	"runtime"
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

	policy, err := networkpolicy.New()
	if err != nil {
		log.Fatal(err)
	}
	service := &desktop.Service{
		Version:  version,
		WindowID: "main",
		Policy:   policy,
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
		if err := policy.Replace(store.Origins()); err != nil {
			log.Fatal(err)
		}

		win := mygo.NewWindow(mygo.WindowOptions{
			Title:           "ZOC",
			URL:             "/",
			Width:           1280,
			Height:          850,
			MinWidth:        760,
			MinHeight:       520,
			StateKey:        "main",
			BackgroundColor: "light-dark(#fafafa, #191515)",
		})
		service.SetMainWindow(win)
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
				if err != nil {
					result["error"] = err.Error()
				} else {
					result["eval"] = raw
					if s, ok := raw.(string); ok {
						result["ok"] = strings.Contains(s, `"mounted":true`) &&
							strings.Contains(s, `"mygo":true`) &&
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
