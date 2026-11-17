package desktop

import "github.com/egoist/mygo"

// InstallContextMenu replaces the navigation items WebKitGTK adds to the
// Web UI's context menu with the actions the desktop client owns. Editing,
// spelling and Inspect Element remain WebKitGTK's own items.
func (s *Service) InstallContextMenu(w *mygo.Window) {
	if w == nil || w.Page() == nil {
		return
	}
	w.Page().OnContextMenu(func(e *mygo.ContextMenuEvent) {
		if e.Menu == nil {
			return
		}
		e.Menu.Remove(
			mygo.ContextMenuBack,
			mygo.ContextMenuForward,
			mygo.ContextMenuReload,
			mygo.ContextMenuStop,
			mygo.ContextMenuOpenLink,
			mygo.ContextMenuOpenLinkInNewWindow,
			mygo.ContextMenuCopyLink,
			mygo.ContextMenuOpenImageInNewWindow,
			mygo.ContextMenuCopyImageURL,
			mygo.ContextMenuOpenMediaInNewWindow,
			mygo.ContextMenuCopyMediaURL,
		)

		copyText := func(text string) func(*mygo.MenuItem, *mygo.Window) {
			return func(*mygo.MenuItem, *mygo.Window) {
				_ = s.WriteClipboardText(text)
			}
		}
		openExternal := func(raw string) func(*mygo.MenuItem, *mygo.Window) {
			return func(*mygo.MenuItem, *mygo.Window) {
				_ = s.OpenExternal(raw)
			}
		}

		if e.LinkURL != "" {
			e.Menu.Append(
				mygo.Separator(),
				&mygo.MenuItem{Label: "Open Link in Browser", Click: openExternal(e.LinkURL)},
				&mygo.MenuItem{Label: "Copy Link Address", Click: copyText(e.LinkURL)},
			)
		}
		if e.ImageURL != "" && e.LinkURL == "" {
			e.Menu.Append(
				mygo.Separator(),
				&mygo.MenuItem{Label: "Open Image in Browser", Click: openExternal(e.ImageURL)},
				&mygo.MenuItem{Label: "Copy Image Address", Click: copyText(e.ImageURL)},
			)
		}
		if e.MediaURL != "" && e.LinkURL == "" && e.ImageURL == "" {
			e.Menu.Append(
				mygo.Separator(),
				&mygo.MenuItem{Label: "Open Media in Browser", Click: openExternal(e.MediaURL)},
				&mygo.MenuItem{Label: "Copy Media Address", Click: copyText(e.MediaURL)},
			)
		}
	})
}
