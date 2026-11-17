// Package desktop implements the native desktop capabilities the official
// OpenCode Web UI expects, using MyGo APIs rather than Electron IPC.
//
// The service is bound to pages, so only ZOC's own trusted pages can call
// it. It never persists credentials and never forwards them to logs.
package desktop

import (
	"context"
	"encoding/base64"
	"errors"
	"fmt"
	"mime"
	"net/url"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"github.com/egoist/mygo"
	"github.com/zce/zoc/internal/networkpolicy"
	"github.com/zce/zoc/internal/opencode"
	"github.com/zce/zoc/internal/profiles"
)

// maxAttachmentBytes bounds one file read over the typed bridge. Larger
// files can still be opened by the server through a path when the upstream
// Web UI supports that workflow.
const maxAttachmentBytes = 128 << 20

// ServerProfilesChanged is emitted after the profile list changes.
var ServerProfilesChanged = mygo.NewEvent[[]profiles.Profile]("server-profiles-changed")

// FullscreenChanged is emitted when the main window enters or leaves full
// screen.
var FullscreenChanged = mygo.NewEvent[bool]("fullscreen-changed")

// NotificationClicked is emitted with the token of a clicked notification.
var NotificationClicked = mygo.NewEvent[string]("notification-clicked")

// Service is the bound desktop API.
type Service struct {
	Version   string
	WindowID  string
	DataDir   string
	Store     *profiles.Store
	Policy    *networkpolicy.Policy
	OpenInbox func()
	// ReduceEffects starts the page with backdrop filters and continuous
	// animations disabled, for hosts where WebKitGTK compositing is expensive.
	ReduceEffects bool

	mu     sync.Mutex
	main   *mygo.Window
	zoom   float64
	unsubs []func()
}

// Bootstrap is what the Web UI needs at document start.
type Bootstrap struct {
	Version       string             `json:"version"`
	Platform      string             `json:"platform"`
	WindowID      string             `json:"windowID"`
	Servers       []profiles.Profile `json:"servers"`
	Zoom          float64            `json:"zoom"`
	Fullscreen    bool               `json:"fullscreen"`
	ReduceEffects bool               `json:"reduceEffects,omitempty"`
	Info          map[string]string  `json:"info,omitempty"`
	Limits        map[string]int64   `json:"limits,omitempty"`
}

// ProfileInput is one user-entered server profile.
type ProfileInput struct {
	Name string `json:"name"`
	URL  string `json:"url"`
}

// TestResult is the outcome of a connection probe.
type TestResult struct {
	OK       bool   `json:"ok"`
	Version  string `json:"version,omitempty"`
	Hostname string `json:"hostname,omitempty"`
	Error    string `json:"error,omitempty"`
}

// PickedFile is an attachment read through the native dialog.
type PickedFile struct {
	Name string `json:"name"`
	MIME string `json:"mime"`
	Data []byte `json:"data"`
}

// AttachmentOptions narrows the native picker.
type AttachmentOptions struct {
	Title       string   `json:"title,omitempty"`
	DefaultPath string   `json:"defaultPath,omitempty"`
	Multiple    bool     `json:"multiple,omitempty"`
	Extensions  []string `json:"extensions,omitempty"`
	MIMETypes   []string `json:"mimeTypes,omitempty"`
}

// NativeProject is the presentation subset of a project for Native UI.
type NativeProject struct {
	ID        string `json:"id"`
	Name      string `json:"name"`
	Canonical string `json:"canonical"`
	Updated   int64  `json:"updated"`
}

// NativeSession is the presentation subset of a session for Native UI.
type NativeSession struct {
	ID        string `json:"id"`
	ProjectID string `json:"projectID"`
	Title     string `json:"title"`
	Updated   int64  `json:"updated"`
	Created   int64  `json:"created"`
	Busy      bool   `json:"busy"`
}

// ServerSnapshot is the read-only data the native inbox displays. OpenCode
// remains the source of truth; ZOC caches nothing across calls.
type ServerSnapshot struct {
	Server       profiles.Profile `json:"server"`
	Version      string           `json:"version,omitempty"`
	Hostname     string           `json:"hostname,omitempty"`
	Projects     []NativeProject  `json:"projects"`
	Sessions     []NativeSession  `json:"sessions"`
	SessionsNext string           `json:"sessionsNext,omitempty"`
}

// NativeSessionPage is one page of root sessions for Native UI.
type NativeSessionPage struct {
	Sessions []NativeSession `json:"sessions"`
	Next     string          `json:"next,omitempty"`
}

// SetMainWindow records the Web UI window and wires fullscreen events.
func (s *Service) SetMainWindow(w *mygo.Window) {
	s.mu.Lock()
	s.main = w
	s.mu.Unlock()
	s.unsubs = append(s.unsubs,
		w.OnEnterFullScreen(func() { _ = FullscreenChanged.Emit(w, true) }),
		w.OnLeaveFullScreen(func() { _ = FullscreenChanged.Emit(w, false) }),
	)
}

// MainWindow returns the Web UI window, or nil before it exists.
func (s *Service) MainWindow() *mygo.Window {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.main
}

// Bootstrap returns server profiles and identity for the Web UI. It never
// returns passwords or tokens.
func (s *Service) Bootstrap() Bootstrap {
	zoom := s.GetZoom()
	info := map[string]string{}
	for key, env := range map[string]string{
		"imeOffsetX":     "ZOC_IME_OFFSET_X",
		"imeOffsetY":     "ZOC_IME_OFFSET_Y",
		"imeNative":      "ZOC_IME_NATIVE",
		"imeCalc":        "ZOC_IME_CALC",
		"nativeTitleBar": "ZOC_NATIVE_TITLEBAR",
		"windowRadius":   "ZOC_WINDOW_RADIUS",
	} {
		if value := os.Getenv(env); value != "" {
			info[key] = value
		}
	}
	return Bootstrap{
		Version:       s.Version,
		Platform:      "desktop",
		WindowID:      s.WindowID,
		Servers:       s.Store.List(),
		Zoom:          zoom,
		Fullscreen:    s.IsFullscreen(),
		ReduceEffects: s.ReduceEffects,
		Info:          info,
		Limits:        map[string]int64{"attachmentBytes": maxAttachmentBytes},
	}
}

// Profiles lists configured servers.
func (s *Service) Profiles() []profiles.Profile { return s.Store.List() }

// AddProfile adds a configured server and widens the transport policy.
func (s *Service) AddProfile(input ProfileInput) (profiles.Profile, error) {
	p, err := s.Store.Add(input.Name, input.URL)
	if err != nil {
		return profiles.Profile{}, err
	}
	if err := s.refreshPolicy(); err != nil {
		_ = s.Store.Remove(p.ID)
		_ = s.refreshPolicy()
		return profiles.Profile{}, err
	}
	ServerProfilesChanged.Emit(s.MainWindow(), s.Store.List())
	return p, nil
}

// UpdateProfile edits a configured server.
func (s *Service) UpdateProfile(id, name, rawURL string) (profiles.Profile, error) {
	previous, ok := s.Store.Get(id)
	if !ok {
		return profiles.Profile{}, fmt.Errorf("server profile not found")
	}
	p, err := s.Store.Update(id, name, rawURL)
	if err != nil {
		return profiles.Profile{}, err
	}
	if err := s.refreshPolicy(); err != nil {
		_, _ = s.Store.Update(id, previous.Name, previous.URL)
		_ = s.refreshPolicy()
		return profiles.Profile{}, err
	}
	ServerProfilesChanged.Emit(s.MainWindow(), s.Store.List())
	return p, nil
}

// RemoveProfile removes a configured server.
func (s *Service) RemoveProfile(id string) error {
	if err := s.Store.Remove(id); err != nil {
		return err
	}
	if err := s.refreshPolicy(); err != nil {
		return err
	}
	ServerProfilesChanged.Emit(s.MainWindow(), s.Store.List())
	return nil
}

func (s *Service) refreshPolicy() error {
	return s.Policy.Replace(s.Store.Origins())
}

// TestServer probes one configured server. The password is used for this
// call only and is never persisted or logged.
func (s *Service) TestServer(ctx context.Context, id, password string) (TestResult, error) {
	p, ok := s.Store.Get(id)
	if !ok {
		return TestResult{}, fmt.Errorf("server profile not found")
	}
	client, err := opencode.New(p.URL, password, s.Policy)
	if err != nil {
		return TestResult{OK: false, Error: err.Error()}, nil
	}
	probeCtx, cancel := context.WithTimeout(ctx, 15*time.Second)
	defer cancel()
	info, err := client.Info(probeCtx)
	if err != nil {
		return TestResult{OK: false, Error: friendlyProbeError(err)}, nil
	}
	return TestResult{OK: true, Version: info.Version, Hostname: info.Hostname}, nil
}

// Snapshot reads projects and root sessions for Native UI.
func (s *Service) Snapshot(ctx context.Context, id, password string) (ServerSnapshot, error) {
	p, ok := s.Store.Get(id)
	if !ok {
		return ServerSnapshot{}, fmt.Errorf("server profile not found")
	}
	client, err := opencode.New(p.URL, password, s.Policy)
	if err != nil {
		return ServerSnapshot{}, err
	}
	ctx, cancel := context.WithTimeout(ctx, 20*time.Second)
	defer cancel()
	info, err := client.Info(ctx)
	if err != nil {
		return ServerSnapshot{}, err
	}
	projects, err := client.Projects(ctx)
	if err != nil {
		return ServerSnapshot{}, err
	}
	page, err := client.SessionsPage(ctx, "", 100, "")
	if err != nil {
		return ServerSnapshot{}, err
	}
	out := ServerSnapshot{Server: p, Version: info.Version, Hostname: info.Hostname, SessionsNext: page.Next}
	for _, project := range projects {
		name := project.Name
		if name == "" {
			name = filepath.Base(project.Canonical)
		}
		out.Projects = append(out.Projects, NativeProject{
			ID: project.ID, Name: name, Canonical: project.Canonical, Updated: project.Time.Updated,
		})
	}
	out.Sessions = nativeSessions(page.Sessions)
	return out, nil
}

// SessionsPage reads one page of root sessions for Native UI. The cursor is
// opaque and comes from the previous page; no local session state is kept.
func (s *Service) SessionsPage(ctx context.Context, id, password, cursor string) (NativeSessionPage, error) {
	p, ok := s.Store.Get(id)
	if !ok {
		return NativeSessionPage{}, fmt.Errorf("server profile not found")
	}
	client, err := opencode.New(p.URL, password, s.Policy)
	if err != nil {
		return NativeSessionPage{}, err
	}
	ctx, cancel := context.WithTimeout(ctx, 20*time.Second)
	defer cancel()
	page, err := client.SessionsPage(ctx, "", 100, cursor)
	if err != nil {
		return NativeSessionPage{}, err
	}
	return NativeSessionPage{Sessions: nativeSessions(page.Sessions), Next: page.Next}, nil
}

func nativeSessions(sessions []opencode.Session) []NativeSession {
	out := make([]NativeSession, 0, len(sessions))
	for _, session := range sessions {
		out = append(out, NativeSession{
			ID: session.ID, ProjectID: session.ProjectID, Title: session.Title,
			Updated: session.Time.Updated, Created: session.Time.Created,
			Busy: session.Time.Busy > session.Time.Idle,
		})
	}
	return out
}

// OpenSession navigates the official Web UI to a session.
func (s *Service) OpenSession(serverID, sessionID string) error {
	p, ok := s.Store.Get(serverID)
	if !ok {
		return fmt.Errorf("server profile not found")
	}
	return s.OpenMain(sessionRoute(p.URL, sessionID))
}

func sessionRoute(serverURL, sessionID string) string {
	return "/server/" + base64.RawURLEncoding.EncodeToString([]byte(serverURL)) + "/session/" + url.PathEscape(sessionID)
}

// OpenMain loads a path of the embedded official Web UI.
func (s *Service) OpenMain(path string) error {
	w := s.MainWindow()
	if w == nil {
		return fmt.Errorf("main window is not ready")
	}
	if !strings.HasPrefix(path, "/") || strings.HasPrefix(path, "//") {
		return fmt.Errorf("path must be an app-relative path")
	}
	return w.Page().LoadURL(path)
}

// OpenInboxWindow opens the native inbox/servers window.
func (s *Service) OpenInboxWindow() {
	if s.OpenInbox != nil {
		s.OpenInbox()
	}
}

// Restart relaunches the application.
func (s *Service) Restart() { mygo.App.Relaunch() }

// OpenExternal opens an http, https or mailto URL in the system browser.
func (s *Service) OpenExternal(raw string) error {
	u, err := url.Parse(raw)
	if err != nil {
		return fmt.Errorf("invalid URL")
	}
	switch u.Scheme {
	case "http", "https", "mailto":
	default:
		return fmt.Errorf("unsupported URL scheme")
	}
	return mygo.Shell.OpenExternal(u.String())
}

// OpenPath opens a local path in its default application.
func (s *Service) OpenPath(path string) error {
	if strings.TrimSpace(path) == "" {
		return fmt.Errorf("empty path")
	}
	return mygo.Shell.OpenPath(path)
}

// OpenLocalFile opens a file:// URL.
func (s *Service) OpenLocalFile(raw string) error {
	u, err := url.Parse(raw)
	if err != nil || u.Scheme != "file" {
		return fmt.Errorf("not a file URL")
	}
	path, err := url.PathUnescape(u.Path)
	if err != nil {
		return fmt.Errorf("invalid file URL")
	}
	return mygo.Shell.OpenPath(path)
}

// RevealPath shows a local path in the file manager.
func (s *Service) RevealPath(path string) (bool, error) {
	if _, err := os.Stat(path); err != nil {
		if errors.Is(err, os.ErrNotExist) {
			return false, nil
		}
		return false, err
	}
	mygo.Shell.ShowItemInFolder(path)
	return true, nil
}

// Notify shows a system notification; clicking it focuses the main window
// and emits NotificationClicked with token so the page can run its action.
func (s *Service) Notify(title, body, token string) error {
	n := mygo.NewNotification(mygo.NotificationOptions{ID: token, Title: title, Body: body})
	n.OnClick(func() {
		if w := s.MainWindow(); w != nil {
			w.Show()
			w.Focus()
		}
		if token != "" {
			NotificationClicked.Emit(s.MainWindow(), token)
		}
	})
	return n.Show()
}

// WriteClipboardText writes text to the native clipboard.
func (s *Service) WriteClipboardText(text string) error {
	mygo.Clipboard.WriteText(text)
	return nil
}

// ReadClipboardText reads text from the native clipboard.
func (s *Service) ReadClipboardText() string { return mygo.Clipboard.ReadText() }

// ReadClipboardImage reads a PNG image from the native clipboard, base64
// encoded for the bridge. It returns an empty string when there is none.
func (s *Service) ReadClipboardImage() string {
	return base64.StdEncoding.EncodeToString(mygo.Clipboard.ReadImage())
}

// PickAttachmentFiles shows the native file picker and reads the selected
// files for upload through the official Web UI. The data is returned once;
// ZOC keeps no copy.
func (s *Service) PickAttachmentFiles(ctx context.Context, opts AttachmentOptions) ([]PickedFile, error) {
	paths, err := mygo.Dialog.Open(mygo.OpenDialogOptions{
		Parent:      mygo.CallerWindow(ctx),
		Title:       opts.Title,
		DefaultPath: opts.DefaultPath,
		Multiple:    opts.Multiple,
		Filters:     []mygo.FileFilter{{Name: "Files", Extensions: opts.Extensions, MIMETypes: opts.MIMETypes}},
	})
	if err != nil || len(paths) == 0 {
		return nil, err
	}
	out := make([]PickedFile, 0, len(paths))
	for _, path := range paths {
		info, err := os.Stat(path)
		if err != nil {
			return nil, err
		}
		if info.Size() > maxAttachmentBytes {
			return nil, fmt.Errorf("%s is larger than the 128 MiB attachment limit", filepath.Base(path))
		}
		data, err := os.ReadFile(path)
		if err != nil {
			return nil, err
		}
		mimeType := mime.TypeByExtension(filepath.Ext(path))
		if mimeType == "" {
			mimeType = "application/octet-stream"
		}
		out = append(out, PickedFile{Name: filepath.Base(path), MIME: mimeType, Data: data})
	}
	return out, nil
}

// PickFilePaths shows the native file picker and returns paths without
// reading their contents.
func (s *Service) PickFilePaths(ctx context.Context, opts AttachmentOptions) ([]string, error) {
	return mygo.Dialog.Open(mygo.OpenDialogOptions{
		Parent:      mygo.CallerWindow(ctx),
		Title:       opts.Title,
		DefaultPath: opts.DefaultPath,
		Multiple:    opts.Multiple,
		Filters:     []mygo.FileFilter{{Name: "Files", Extensions: opts.Extensions, MIMETypes: opts.MIMETypes}},
	})
}

// PickDirectory shows the native directory picker.
func (s *Service) PickDirectory(ctx context.Context, title string, multiple bool) ([]string, error) {
	return mygo.Dialog.Open(mygo.OpenDialogOptions{
		Parent:    mygo.CallerWindow(ctx),
		Title:     title,
		Directory: true,
		Multiple:  multiple,
	})
}

// SaveFile shows the native save dialog and writes content.
func (s *Service) SaveFile(ctx context.Context, title, defaultPath, content string) (bool, error) {
	path, err := mygo.Dialog.Save(mygo.SaveDialogOptions{
		Parent:      mygo.CallerWindow(ctx),
		Title:       title,
		DefaultPath: defaultPath,
	})
	if err != nil || path == "" {
		return false, err
	}
	if err := os.WriteFile(path, []byte(content), 0o600); err != nil {
		return false, err
	}
	return true, nil
}

// CheckAppExists reports whether an executable is on PATH.
func (s *Service) CheckAppExists(name string) bool {
	if strings.TrimSpace(name) == "" {
		return false
	}
	_, err := exec.LookPath(name)
	return err == nil
}

// GetZoom returns the Web UI zoom factor.
func (s *Service) GetZoom() float64 {
	if w := s.MainWindow(); w != nil {
		if page := w.Page(); page != nil {
			if zoom := page.ZoomFactor(); zoom > 0 {
				return zoom
			}
		}
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.zoom == 0 {
		return 1
	}
	return s.zoom
}

// SetZoom applies a zoom factor to the Web UI.
func (s *Service) SetZoom(zoom float64) error {
	if zoom < 0.5 || zoom > 3 {
		return fmt.Errorf("zoom out of range")
	}
	w := s.MainWindow()
	if w == nil {
		return fmt.Errorf("main window is not ready")
	}
	w.Page().SetZoomFactor(zoom)
	s.mu.Lock()
	s.zoom = zoom
	s.mu.Unlock()
	return nil
}

// ToggleFullscreen toggles the main window's fullscreen state.
func (s *Service) ToggleFullscreen() {
	if w := s.MainWindow(); w != nil {
		w.ToggleFullScreen()
	}
}

// IsFullscreen reports the main window's fullscreen state.
func (s *Service) IsFullscreen() bool {
	if w := s.MainWindow(); w != nil {
		return w.IsFullScreen()
	}
	return false
}

// FocusMain brings the Web UI window to the front.
func (s *Service) FocusMain() {
	if w := s.MainWindow(); w != nil {
		w.Show()
		w.Focus()
	}
}

// MinimizeWindow minimizes the Web UI window.
func (s *Service) MinimizeWindow() {
	if w := s.MainWindow(); w != nil {
		w.Minimize()
	}
}

// ToggleMaximizeWindow maximizes the Web UI window, or restores it.
func (s *Service) ToggleMaximizeWindow() {
	if w := s.MainWindow(); w != nil {
		w.ToggleMaximize()
	}
}

// CloseWindow closes the Web UI window.
func (s *Service) CloseWindow() {
	if w := s.MainWindow(); w != nil {
		w.Close()
	}
}

// ReloadWindow reloads the Web UI page.
func (s *Service) ReloadWindow() {
	if w := s.MainWindow(); w != nil {
		if page := w.Page(); page != nil {
			page.Reload()
		}
	}
}

// ToggleDevTools opens or closes the Web UI's developer tools.
func (s *Service) ToggleDevTools() {
	if w := s.MainWindow(); w != nil {
		if page := w.Page(); page != nil {
			page.ToggleDevTools()
		}
	}
}

// IsMaximizedWindow reports whether the Web UI window is maximized.
func (s *Service) IsMaximizedWindow() bool {
	if w := s.MainWindow(); w != nil {
		return w.IsMaximized()
	}
	return false
}

// NotifyImeCursor tells WebKitGTK where the focused editor's caret is, in
// CSS pixels of the page viewport. WebKitGTK otherwise uses document
// coordinates for the input method popup after a page scrolls, which puts
// the candidate window outside the window.
func (s *Service) NotifyImeCursor(x, y, width, height float64) {
	notifyImeCursor(s.MainWindow(), x, y, width, height)
}

func friendlyProbeError(err error) string {
	switch {
	case errors.Is(err, opencode.ErrUnauthorized):
		return "The server rejected the credentials."
	case errors.Is(err, context.DeadlineExceeded):
		return "The server did not answer in time."
	default:
		return err.Error()
	}
}
