package native

import (
	"context"
	"errors"
	"testing"
	"time"

	"github.com/egoist/mygo/ui"
	"github.com/zce/zoc/internal/desktop"
	"github.com/zce/zoc/internal/profiles"
)

type fakeBackend struct {
	servers     []profiles.Profile
	snapshot    desktop.ServerSnapshot
	openSession [2]string
	openPath    string
	added       []desktop.ProfileInput
}

func (f *fakeBackend) Profiles() []profiles.Profile {
	return append([]profiles.Profile(nil), f.servers...)
}
func (f *fakeBackend) AddProfile(input desktop.ProfileInput) (profiles.Profile, error) {
	p := profiles.Profile{ID: "new", Name: input.Name, URL: input.URL, CreatedAt: time.Now()}
	f.servers = append(f.servers, p)
	f.added = append(f.added, input)
	return p, nil
}
func (f *fakeBackend) UpdateProfile(id, name, url string) (profiles.Profile, error) {
	return profiles.Profile{}, errors.New("not implemented")
}
func (f *fakeBackend) RemoveProfile(id string) error { return nil }
func (f *fakeBackend) Snapshot(ctx context.Context, id, password string) (desktop.ServerSnapshot, error) {
	return f.snapshot, nil
}
func (f *fakeBackend) TestServer(ctx context.Context, id, password string) (desktop.TestResult, error) {
	return desktop.TestResult{OK: true, Version: "2.3.0", Hostname: "fedora"}, nil
}
func (f *fakeBackend) OpenSession(serverID, sessionID string) error {
	f.openSession = [2]string{serverID, sessionID}
	return nil
}
func (f *fakeBackend) OpenMain(path string) error { f.openPath = path; return nil }
func (f *fakeBackend) FocusMain()                 {}

func TestInboxShowsServersAndOpensSession(t *testing.T) {
	backend := &fakeBackend{
		servers: []profiles.Profile{{ID: "srv", Name: "Work", URL: "http://127.0.0.1:4096"}},
		snapshot: desktop.ServerSnapshot{
			Server:  profiles.Profile{ID: "srv", Name: "Work", URL: "http://127.0.0.1:4096"},
			Version: "2.3.0", Hostname: "fedora",
			Projects: []desktop.NativeProject{{ID: "p1", Name: "zoc"}},
			Sessions: []desktop.NativeSession{{ID: "ses-1", ProjectID: "p1", Title: "Fix login", Updated: time.Now().UnixMilli()}},
		},
	}
	app := &Inbox{
		backend: backend, servers: backend.servers, selected: "srv",
		snapshot: &backend.snapshot, passwords: map[string]string{},
	}
	tt := ui.NewTester(app.view, 980, 680)
	if !tt.HasText("Work") || !tt.HasText("Fix login") {
		t.Fatalf("inbox texts = %q", tt.Texts())
	}
	if err := tt.Click("Fix login"); err != nil {
		t.Fatal(err)
	}
	if backend.openSession != [2]string{"srv", "ses-1"} {
		t.Fatalf("opened session = %v", backend.openSession)
	}
}

func TestInboxAddServerFlow(t *testing.T) {
	backend := &fakeBackend{}
	app := NewInbox(backend)
	tt := ui.NewTester(app.view, 980, 680)
	if !tt.HasText("No servers configured yet.") {
		t.Fatalf("empty inbox texts = %q", tt.Texts())
	}
	if err := tt.Click("Add server"); err != nil {
		t.Fatal(err)
	}
	if err := tt.Click("Name"); err != nil {
		t.Fatal(err)
	}
	tt.Type("Remote")
	if err := tt.Click("Server URL"); err != nil {
		t.Fatal(err)
	}
	tt.Key(ui.Cmd, ui.KeyA)
	tt.Type("https://opencode.example.com")
	if err := tt.Click("Save"); err != nil {
		t.Fatal(err)
	}
	if len(backend.added) != 1 || backend.added[0].Name != "Remote" || backend.added[0].URL != "https://opencode.example.com" {
		t.Fatalf("added = %+v", backend.added)
	}
	tt.Frame()
	if !tt.HasText("Remote") {
		t.Fatalf("added server not shown: %q", tt.Texts())
	}
	if backend.openPath != "/" {
		t.Fatalf("web UI fallback path = %q", backend.openPath)
	}
}

func TestFilteredSessions(t *testing.T) {
	sessions := []desktop.NativeSession{
		{ID: "1", ProjectID: "a", Title: "Fix login", Updated: 1},
		{ID: "2", ProjectID: "b", Title: "Write docs", Updated: 2},
		{ID: "3", ProjectID: "a", Title: "Fix logout", Updated: 3},
	}
	got := filteredSessions(sessions, "a", "logout")
	if len(got) != 1 || got[0].ID != "3" {
		t.Fatalf("filtered = %+v", got)
	}
	got = filteredSessions(sessions, "", "")
	if len(got) != 3 || got[0].ID != "3" {
		t.Fatalf("sorted = %+v", got)
	}
}
