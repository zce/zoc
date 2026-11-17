package desktop

import "context"

// Bindings is the page-facing desktop API. It is deliberately narrower than
// Service: internal lifecycle and native-inbox methods are not callable from
// the Web UI, so a page cannot replace the main-window reference or open
// native windows without a user action.
type Bindings struct {
	s *Service
}

// NewBindings exposes the page-facing subset of a Service.
func NewBindings(s *Service) *Bindings { return &Bindings{s: s} }

// Bootstrap returns server profiles and identity for the Web UI.
func (b *Bindings) Bootstrap() Bootstrap { return b.s.Bootstrap() }

// Restart relaunches the application.
func (b *Bindings) Restart() { b.s.Restart() }

// OpenExternal opens an http, https or mailto URL in the system browser.
func (b *Bindings) OpenExternal(raw string) error { return b.s.OpenExternal(raw) }

// OpenPath opens a local path in its default application.
func (b *Bindings) OpenPath(path string) error { return b.s.OpenPath(path) }

// OpenLocalFile opens a file:// URL.
func (b *Bindings) OpenLocalFile(raw string) error { return b.s.OpenLocalFile(raw) }

// RevealPath shows a local path in the file manager.
func (b *Bindings) RevealPath(path string) (bool, error) { return b.s.RevealPath(path) }

// Notify shows a system notification.
func (b *Bindings) Notify(title, body, token string) error { return b.s.Notify(title, body, token) }

// WriteClipboardText writes text to the native clipboard.
func (b *Bindings) WriteClipboardText(text string) error { return b.s.WriteClipboardText(text) }

// ReadClipboardText reads text from the native clipboard.
func (b *Bindings) ReadClipboardText() string { return b.s.ReadClipboardText() }

// ReadClipboardImage reads a PNG image as base64.
func (b *Bindings) ReadClipboardImage() string { return b.s.ReadClipboardImage() }

// PickAttachmentFiles shows the native file picker and reads selected files.
func (b *Bindings) PickAttachmentFiles(ctx context.Context, opts AttachmentOptions) ([]PickedFile, error) {
	return b.s.PickAttachmentFiles(ctx, opts)
}

// PickFilePaths shows the native file picker and returns paths.
func (b *Bindings) PickFilePaths(ctx context.Context, opts AttachmentOptions) ([]string, error) {
	return b.s.PickFilePaths(ctx, opts)
}

// PickDirectory shows the native directory picker.
func (b *Bindings) PickDirectory(ctx context.Context, title string, multiple bool) ([]string, error) {
	return b.s.PickDirectory(ctx, title, multiple)
}

// SaveFile shows the native save dialog and writes content.
func (b *Bindings) SaveFile(ctx context.Context, title, defaultPath, content string) (bool, error) {
	return b.s.SaveFile(ctx, title, defaultPath, content)
}

// CheckAppExists reports whether an executable is on PATH.
func (b *Bindings) CheckAppExists(name string) bool { return b.s.CheckAppExists(name) }

// GetZoom returns the Web UI zoom factor.
func (b *Bindings) GetZoom() float64 { return b.s.GetZoom() }

// SetZoom applies a zoom factor to the Web UI.
func (b *Bindings) SetZoom(zoom float64) error { return b.s.SetZoom(zoom) }

// MinimizeWindow minimizes the Web UI window.
func (b *Bindings) MinimizeWindow() { b.s.MinimizeWindow() }

// ToggleMaximizeWindow maximizes the Web UI window, or restores it.
func (b *Bindings) ToggleMaximizeWindow() { b.s.ToggleMaximizeWindow() }

// CloseWindow closes the Web UI window.
func (b *Bindings) CloseWindow() { b.s.CloseWindow() }

// ToggleFullscreen toggles the Web UI window's fullscreen state.
func (b *Bindings) ToggleFullscreen() { b.s.ToggleFullscreen() }

// ReloadWindow reloads the Web UI page.
func (b *Bindings) ReloadWindow() { b.s.ReloadWindow() }

// ToggleDevTools opens or closes the Web UI's developer tools.
func (b *Bindings) ToggleDevTools() { b.s.ToggleDevTools() }

// IsMaximizedWindow reports whether the Web UI window is maximized.
func (b *Bindings) IsMaximizedWindow() bool { return b.s.IsMaximizedWindow() }

// StorageItems returns a storage namespace snapshot.
func (b *Bindings) StorageItems(namespace string) (StorageSnapshot, error) {
	return b.s.StorageItems(namespace)
}

// StorageUpdate applies one storage batch.
func (b *Bindings) StorageUpdate(namespace string, insert map[string]string, remove []string) (int64, error) {
	return b.s.StorageUpdate(namespace, insert, remove)
}

// StorageClear empties a storage namespace.
func (b *Bindings) StorageClear(namespace string) error { return b.s.StorageClear(namespace) }
