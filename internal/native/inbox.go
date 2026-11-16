// Package native contains ZOC's Native UI routes. The inbox and server
// navigator is a complete native workflow: it lists configured OpenCode
// servers, projects and sessions, and opens a chosen session in the
// official Web UI. Every other workflow remains in the embedded Web UI.
package native

import (
	"context"
	"fmt"
	"sort"
	"strings"
	"sync"
	"time"

	"github.com/egoist/mygo"
	"github.com/egoist/mygo/ui"
	"github.com/zce/zoc/internal/desktop"
	"github.com/zce/zoc/internal/profiles"
)

// Backend is the narrow boundary Native UI uses. It has no server-side
// reducer and no second event stream: snapshots come from the OpenCode API
// on demand, and session semantics stay in the official Web UI.
type Backend interface {
	Profiles() []profiles.Profile
	AddProfile(input desktop.ProfileInput) (profiles.Profile, error)
	UpdateProfile(id, name, url string) (profiles.Profile, error)
	RemoveProfile(id string) error
	Snapshot(ctx context.Context, id, password string) (desktop.ServerSnapshot, error)
	TestServer(ctx context.Context, id, password string) (desktop.TestResult, error)
	OpenSession(serverID, sessionID string) error
	OpenMain(path string) error
	FocusMain()
}

type formMode int

const (
	formNone formMode = iota
	formAdd
	formEdit
)

// Inbox is the native Servers & Inbox window.
type Inbox struct {
	backend Backend

	mu       sync.Mutex
	win      *mygo.Window
	servers  []profiles.Profile
	selected string
	snapshot *desktop.ServerSnapshot
	loading  bool
	loadErr  string

	projectName string
	filter      string

	// passwords holds ephemeral in-memory credentials for native snapshots.
	// It is never written by the profile store.
	passwords map[string]string

	formMode     formMode
	formName     string
	formURL      string
	formPassword string
	formErr      string
	formStatus   string
	formTesting  bool

	testResult string
	testOK     bool
}

// NewInbox creates the native window controller. It does not open a window
// until Open is called.
func NewInbox(backend Backend) *Inbox {
	return &Inbox{backend: backend, passwords: map[string]string{}}
}

// Open shows the inbox, or focuses the existing window.
func (a *Inbox) Open() {
	a.mu.Lock()
	if a.win != nil {
		w := a.win
		a.mu.Unlock()
		w.Show()
		w.Focus()
		return
	}
	a.servers = a.backend.Profiles()
	if a.selected == "" && len(a.servers) > 0 {
		a.selected = a.servers[0].ID
	}
	a.mu.Unlock()

	w := mygo.NewWindow(mygo.WindowOptions{
		Title:     "ZOC — Servers & Inbox",
		Width:     980,
		Height:    680,
		MinWidth:  760,
		MinHeight: 480,
		StateKey:  "inbox",
		Content:   ui.View(a.view),
	})
	a.mu.Lock()
	a.win = w
	selected := a.selected
	a.mu.Unlock()
	if selected != "" {
		a.refresh()
	}
	w.OnClosed(func() {
		a.mu.Lock()
		if a.win == w {
			a.win = nil
		}
		a.mu.Unlock()
	})
}

// Window returns the native window, or nil before it is opened. It is used by
// smoke tests that capture the rendered native UI.
func (a *Inbox) Window() *mygo.Window {
	a.mu.Lock()
	defer a.mu.Unlock()
	return a.win
}

func (a *Inbox) invalidate() {
	a.mu.Lock()
	w := a.win
	a.mu.Unlock()
	if w != nil {
		w.Invalidate()
	}
}

func (a *Inbox) reloadServers() {
	a.mu.Lock()
	a.servers = a.backend.Profiles()
	a.mu.Unlock()
	a.invalidate()
}

func (a *Inbox) selectServer(id string) {
	a.mu.Lock()
	if a.selected == id {
		a.mu.Unlock()
		return
	}
	a.selected = id
	a.snapshot = nil
	a.projectName = ""
	a.filter = ""
	a.formMode = formNone
	a.formPassword = ""
	a.testResult = ""
	a.testOK = false
	a.loadErr = ""
	a.mu.Unlock()
	a.refresh()
}

func (a *Inbox) current() (profiles.Profile, bool) {
	a.mu.Lock()
	defer a.mu.Unlock()
	for _, p := range a.servers {
		if p.ID == a.selected {
			return p, true
		}
	}
	return profiles.Profile{}, false
}

func (a *Inbox) refresh() {
	profile, ok := a.current()
	if !ok {
		return
	}
	a.mu.Lock()
	password := a.passwords[profile.ID]
	serverID := profile.ID
	a.loading = true
	a.loadErr = ""
	a.mu.Unlock()
	a.invalidate()

	go func() {
		snapshot, err := a.backend.Snapshot(context.Background(), serverID, password)
		a.mu.Lock()
		if a.selected != serverID {
			a.mu.Unlock()
			return
		}
		a.loading = false
		if err != nil {
			a.loadErr = err.Error()
		} else {
			a.snapshot = &snapshot
			a.loadErr = ""
		}
		a.mu.Unlock()
		a.invalidate()
	}()
}

func (a *Inbox) startTest() {
	profile, ok := a.current()
	if !ok {
		a.mu.Lock()
		a.formErr = "Save the server first."
		a.mu.Unlock()
		return
	}
	a.mu.Lock()
	password := a.formPassword
	if password == "" {
		password = a.passwords[profile.ID]
	}
	a.formTesting = true
	a.testResult = ""
	a.mu.Unlock()
	a.invalidate()
	go func() {
		result, err := a.backend.TestServer(context.Background(), profile.ID, password)
		a.mu.Lock()
		a.formTesting = false
		if err != nil {
			a.testOK = false
			a.testResult = err.Error()
		} else if !result.OK {
			a.testOK = false
			a.testResult = result.Error
		} else {
			a.testOK = true
			a.testResult = fmt.Sprintf("Connected to OpenCode %s at %s", result.Version, result.Hostname)
		}
		a.mu.Unlock()
		a.invalidate()
	}()
}

func (a *Inbox) saveForm() {
	a.mu.Lock()
	mode := a.formMode
	name := strings.TrimSpace(a.formName)
	rawURL := strings.TrimSpace(a.formURL)
	id := a.selected
	a.formErr = ""
	a.mu.Unlock()

	if name == "" {
		name = rawURL
	}
	var (
		profile profiles.Profile
		err     error
	)
	switch mode {
	case formAdd:
		profile, err = a.backend.AddProfile(desktop.ProfileInput{Name: name, URL: rawURL})
	case formEdit:
		profile, err = a.backend.UpdateProfile(id, name, rawURL)
	default:
		return
	}
	if err != nil {
		a.mu.Lock()
		a.formErr = err.Error()
		a.mu.Unlock()
		a.invalidate()
		return
	}
	a.mu.Lock()
	a.formMode = formNone
	a.formStatus = "Saved"
	a.selected = profile.ID
	if a.formPassword != "" {
		a.passwords[profile.ID] = a.formPassword
	} else {
		delete(a.passwords, profile.ID)
	}
	a.snapshot = nil
	a.loadErr = ""
	a.projectName = ""
	a.filter = ""
	a.mu.Unlock()
	a.reloadServers()
	// The official registry keeps its own copy of provided servers. Reload
	// the Web UI so an added or re-pointed server appears there too.
	_ = a.backend.OpenMain("/")
	a.refresh()
}

func (a *Inbox) removeSelected() {
	profile, ok := a.current()
	if !ok {
		return
	}
	res, err := mygo.Dialog.Message(mygo.MessageOptions{
		Type:               mygo.MessageWarning,
		Message:            "Remove this server?",
		Detail:             profile.Name + "\n" + profile.URL,
		Buttons:            []string{"Cancel", "Remove"},
		CancelButton:       0,
		DestructiveButtons: []int{1},
	})
	if err != nil || res.Button != 1 {
		return
	}
	if err := a.backend.RemoveProfile(profile.ID); err != nil {
		a.mu.Lock()
		a.loadErr = err.Error()
		a.mu.Unlock()
		a.invalidate()
		return
	}
	a.mu.Lock()
	a.selected = ""
	a.snapshot = nil
	a.formMode = formNone
	delete(a.passwords, profile.ID)
	a.mu.Unlock()
	a.reloadServers()
}

func (a *Inbox) view(c *ui.Context) {
	t := c.Theme()
	a.mu.Lock()
	servers := append([]profiles.Profile(nil), a.servers...)
	selected := a.selected
	snapshot := a.snapshot
	loading := a.loading
	loadErr := a.loadErr
	formMode := a.formMode
	formName := a.formName
	formURL := a.formURL
	formPassword := a.formPassword
	formErr := a.formErr
	formStatus := a.formStatus
	formTesting := a.formTesting
	testResult := a.testResult
	testOK := a.testOK
	projectName := a.projectName
	filter := a.filter
	a.mu.Unlock()

	ui.Row(c).Fill().Children(func() {
		sidebar := ui.Column(c).Width(286).FillHeight().Background(t.Surface).Border(1, t.Border)
		sidebar.Children(func() {
			ui.Column(c).Padding(16, 16, 12, 16).Gap(2).Children(func() {
				ui.Text(c, "ZOC").FontSize(22).Bold()
				ui.Text(c, "Servers & Inbox").FontSize(12).TextColor(t.TextMuted)
			})
			ui.Row(c).Padding(16, 8).Gap(8).Children(func() {
				if ui.PrimaryButton(c, "Add server").Clicked() {
					a.mu.Lock()
					a.formMode = formAdd
					a.formName = ""
					a.formURL = "http://127.0.0.1:4096"
					a.formPassword = ""
					a.formErr = ""
					a.formStatus = ""
					a.testResult = ""
					a.testOK = false
					a.mu.Unlock()
					a.invalidate()
				}
				if ui.Button(c, "Official UI").Clicked() {
					_ = a.backend.OpenMain("/")
					a.backend.FocusMain()
				}
			})
			ui.Divider(c)
			if len(servers) == 0 {
				ui.Text(c, "No servers configured yet.").FontSize(12).TextColor(t.TextMuted).Padding(16)
			}
			ui.List(c, nil, len(servers), func(i int) {
				p := servers[i]
				bg := ui.Transparent
				if p.ID == selected {
					bg = t.SurfacePressed
				}
				ui.Column(c).Background(bg).Children(func() {
					name := ui.Text(c, p.Name).FontSize(13).SingleLine().FillWidth().Padding(10, 16, 2, 16)
					if p.ID == selected {
						name.Bold()
					}
					if name.RowClicked() {
						a.selectServer(p.ID)
					}
					ui.Text(c, p.URL).FontSize(11).TextColor(t.TextMuted).SingleLine().Padding(0, 16, 9, 16)
				})
			}).Grow(1)
			ui.Divider(c)
			ui.Text(c, "Unmigrated workflows stay in the official OpenCode Web UI.").
				FontSize(11).TextColor(t.TextMuted).Padding(12, 16)
		})

		ui.Column(c).Grow(1).FillHeight().Children(func() {
			if formMode != formNone {
				a.formView(c, formMode, formName, formURL, formPassword, formErr, formStatus, formTesting, testResult, testOK)
				return
			}
			profile, ok := profileByID(servers, selected)
			if !ok {
				ui.Column(c).Fill().Center().Gap(8).Children(func() {
					ui.Text(c, "Choose a server, or add one").FontSize(18).Bold()
					ui.Text(c, "ZOC connects to OpenCode Servers you already run.").TextColor(t.TextMuted)
				})
				return
			}
			a.detailView(c, profile, snapshot, loading, loadErr, projectName, filter)
		})
	})
}

func (a *Inbox) formView(c *ui.Context, mode formMode, name, rawURL, password, formErr, status string, testing bool, testResult string, testOK bool) {
	t := c.Theme()
	ui.Scroll(c).Grow(1).Padding(24).Children(func() {
		ui.Column(c).MaxWidth(620).Gap(14).Children(func() {
			title := "Add server"
			if mode == formEdit {
				title = "Edit server"
			}
			ui.Text(c, title).FontSize(22).Bold()
			ui.Text(c, "Only http and https URLs are accepted. Credentials never belong in a URL.").
				FontSize(12).TextColor(t.TextMuted)
			ui.TextInput(c, &a.formName).Label("Name").Placeholder("Work OpenCode")
			ui.TextInput(c, &a.formURL).Label("Server URL").Placeholder("http://127.0.0.1:4096")
			ui.TextInput(c, &a.formPassword).Label("Password (optional)").Password()
			ui.Text(c, "The password stays in this window's memory and is never saved.").
				FontSize(11).TextColor(t.TextMuted)
			if formErr != "" {
				ui.Text(c, formErr).FontSize(12).TextColor(t.Danger)
			}
			if testing {
				ui.Text(c, "Testing connection…").FontSize(12).TextColor(t.TextMuted)
			} else if testResult != "" {
				color := t.Danger
				if testOK {
					color = t.Success
				}
				ui.Text(c, testResult).FontSize(12).TextColor(color)
			}
			if status != "" {
				ui.Text(c, status).FontSize(12).TextColor(t.Success)
			}
			ui.Row(c).Gap(8).Children(func() {
				if ui.PrimaryButton(c, "Save").Clicked() {
					a.saveForm()
				}
				if ui.Button(c, "Save & test").Clicked() {
					a.saveForm()
					a.startTest()
				}
				if mode == formEdit {
					if ui.Button(c, "Test connection").Clicked() {
						a.startTest()
					}
				}
				if ui.Button(c, "Cancel").Clicked() {
					a.mu.Lock()
					a.formMode = formNone
					a.formErr = ""
					a.formPassword = ""
					a.mu.Unlock()
					a.invalidate()
				}
			})
		})
	})
}

func (a *Inbox) detailView(c *ui.Context, profile profiles.Profile, snapshot *desktop.ServerSnapshot, loading bool, loadErr string, projectName, filter string) {
	t := c.Theme()
	ui.Column(c).FillHeight().Children(func() {
		ui.Row(c).Padding(20, 24, 12, 24).Gap(10).Children(func() {
			ui.Column(c).Grow(1).Gap(2).Children(func() {
				ui.Text(c, profile.Name).FontSize(20).Bold().SingleLine()
				ui.Text(c, profile.URL).FontSize(12).TextColor(t.TextMuted).SingleLine()
			})
			if ui.Button(c, "Refresh").Clicked() {
				a.refresh()
			}
			if ui.Button(c, "Edit").Clicked() {
				a.mu.Lock()
				a.formMode = formEdit
				a.formName = profile.Name
				a.formURL = profile.URL
				a.formPassword = ""
				a.formErr = ""
				a.formStatus = ""
				a.testResult = ""
				a.testOK = false
				a.mu.Unlock()
				a.invalidate()
			}
			if ui.Button(c, "Remove").Clicked() {
				a.removeSelected()
			}
			if ui.PrimaryButton(c, "Open Web UI").Clicked() {
				_ = a.backend.OpenMain("/")
				a.backend.FocusMain()
			}
		})
		if loadErr != "" {
			ui.Text(c, loadErr).FontSize(12).TextColor(t.Danger).Padding(0, 24, 10, 24)
		} else if loading {
			ui.Text(c, "Loading projects and sessions…").FontSize(12).TextColor(t.TextMuted).Padding(0, 24, 10, 24)
		} else if snapshot != nil {
			ui.Text(c, fmt.Sprintf("OpenCode %s · %s", snapshot.Version, snapshot.Hostname)).
				FontSize(11).TextColor(t.TextMuted).Padding(0, 24, 8, 24)
		}
		ui.Divider(c)

		if snapshot == nil {
			ui.Column(c).Grow(1).Center().Children(func() {
				ui.Text(c, "No snapshot yet").TextColor(t.TextMuted)
			})
			return
		}

		projects := append([]desktop.NativeProject(nil), snapshot.Projects...)
		sort.SliceStable(projects, func(i, j int) bool {
			return strings.ToLower(projects[i].Name) < strings.ToLower(projects[j].Name)
		})
		names := []string{"All projects"}
		ids := []string{""}
		for _, project := range projects {
			names = append(names, project.Name)
			ids = append(ids, project.ID)
		}
		chosenID := ""
		for i, name := range names {
			if name == projectName {
				chosenID = ids[i]
			}
		}
		if projectName == "" {
			chosenID = ""
		}
		projectIndex := 0
		for i, id := range ids {
			if id == chosenID {
				projectIndex = i
			}
		}
		chosenName := names[projectIndex]

		ui.Row(c).Padding(12, 24).Gap(12).Children(func() {
			ui.Column(c).Width(240).Gap(4).Children(func() {
				ui.Text(c, "Project").FontSize(11).TextColor(t.TextMuted)
				if ui.Select(c, &chosenName, names).Changed() {
					a.mu.Lock()
					a.projectName = chosenName
					a.mu.Unlock()
					a.invalidate()
				}
			})
			ui.Column(c).Grow(1).Gap(4).Children(func() {
				ui.Text(c, "Find a session").FontSize(11).TextColor(t.TextMuted)
				ui.TextInput(c, &a.filter).Placeholder("Filter sessions")
			})
		})

		sessions := filteredSessions(snapshot.Sessions, chosenID, filter)
		ui.Row(c).Padding(0, 24, 6, 24).Children(func() {
			ui.Text(c, sessionCount(len(sessions))).FontSize(11).TextColor(t.TextMuted)
		})
		if len(sessions) == 0 {
			ui.Text(c, "No sessions match.").TextColor(t.TextMuted).Padding(12, 24)
			return
		}
		ui.List(c, nil, len(sessions), func(i int) {
			session := sessions[i]
			row := ui.Column(c).Padding(8, 24).Gap(3).FillWidth()
			if session.Busy {
				row.BorderStartWidth(2).BorderColor(t.Accent)
			}
			row.Children(func() {
				title := ui.Text(c, session.Title).FontSize(13).SingleLine().FillWidth().Padding(6, 24, 1, 24)
				if title.RowClicked() {
					if err := a.backend.OpenSession(profile.ID, session.ID); err != nil {
						a.mu.Lock()
						a.loadErr = err.Error()
						a.mu.Unlock()
						a.invalidate()
						return
					}
					a.backend.FocusMain()
				}
				meta := session.ProjectID
				for _, project := range projects {
					if project.ID == session.ProjectID {
						meta = project.Name
						break
					}
				}
				line := meta + " · " + relativeTime(session.Updated)
				if session.Busy {
					line += " · running"
				}
				ui.Text(c, line).FontSize(11).TextColor(t.TextMuted).SingleLine().Padding(0, 24, 7, 24)
			})
		}).Grow(1)
	})
}

func profileByID(list []profiles.Profile, id string) (profiles.Profile, bool) {
	for _, p := range list {
		if p.ID == id {
			return p, true
		}
	}
	return profiles.Profile{}, false
}

func filteredSessions(sessions []desktop.NativeSession, projectID, filter string) []desktop.NativeSession {
	filter = strings.ToLower(strings.TrimSpace(filter))
	out := make([]desktop.NativeSession, 0, len(sessions))
	for _, session := range sessions {
		if projectID != "" && session.ProjectID != projectID {
			continue
		}
		if filter != "" && !strings.Contains(strings.ToLower(session.Title), filter) {
			continue
		}
		out = append(out, session)
	}
	sort.SliceStable(out, func(i, j int) bool { return out[i].Updated > out[j].Updated })
	return out
}

func relativeTime(unixMillis int64) string {
	if unixMillis <= 0 {
		return "unknown"
	}
	d := time.Since(time.UnixMilli(unixMillis))
	switch {
	case d < time.Minute:
		return "now"
	case d < time.Hour:
		return fmt.Sprintf("%dm ago", int(d.Minutes()))
	case d < 24*time.Hour:
		return fmt.Sprintf("%dh ago", int(d.Hours()))
	default:
		return fmt.Sprintf("%dd ago", int(d.Hours()/24))
	}
}

func sessionCount(n int) string {
	if n == 1 {
		return "1 session"
	}
	return fmt.Sprintf("%d sessions", n)
}
