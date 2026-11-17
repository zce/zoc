package native

import (
	"context"
	"errors"
	"sync/atomic"
	"testing"
	"time"

	"github.com/egoist/mygo/ui"
	"github.com/zce/zoc/internal/desktop"
	"github.com/zce/zoc/internal/profiles"
)

type fakeBackend struct {
	servers     []profiles.Profile
	snapshot    desktop.ServerSnapshot
	snapshotFn  func(ctx context.Context, id, password string) (desktop.ServerSnapshot, error)
	page        desktop.NativeSessionPage
	moreCalled  chan struct{}
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
	if f.snapshotFn != nil {
		return f.snapshotFn(ctx, id, password)
	}
	return f.snapshot, nil
}
func (f *fakeBackend) SessionsPage(ctx context.Context, id, password, cursor string) (desktop.NativeSessionPage, error) {
	if f.moreCalled != nil {
		f.moreCalled <- struct{}{}
	}
	return f.page, nil
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

func waitFor(t *testing.T, condition func() bool) {
	t.Helper()
	deadline := time.Now().Add(2 * time.Second)
	for time.Now().Before(deadline) {
		if condition() {
			return
		}
		time.Sleep(5 * time.Millisecond)
	}
	t.Fatal("condition not reached")
}

func TestInboxLoadsMoreSessions(t *testing.T) {
	backend := &fakeBackend{
		servers: []profiles.Profile{{ID: "srv", Name: "Work", URL: "http://127.0.0.1:4096"}},
		snapshot: desktop.ServerSnapshot{
			Server:       profiles.Profile{ID: "srv", Name: "Work", URL: "http://127.0.0.1:4096"},
			Sessions:     []desktop.NativeSession{{ID: "ses-1", ProjectID: "p1", Title: "First", Updated: 2}},
			SessionsNext: "page-2",
		},
		page: desktop.NativeSessionPage{
			Sessions: []desktop.NativeSession{{ID: "ses-2", ProjectID: "p1", Title: "Second", Updated: 1}},
		},
		moreCalled: make(chan struct{}, 1),
	}
	app := &Inbox{
		backend: backend, servers: backend.servers, selected: "srv",
		snapshot: &backend.snapshot, sessionsNext: "page-2", passwords: map[string]string{},
	}
	tt := ui.NewTester(app.view, 980, 680)
	if !tt.HasText("more available") || !tt.HasText("First") {
		t.Fatalf("texts = %q", tt.Texts())
	}
	if err := tt.Click("Load more sessions"); err != nil {
		t.Fatal(err)
	}
	<-backend.moreCalled
	waitFor(t, func() bool { return app.snapshot != nil && len(app.snapshot.Sessions) == 2 })
	tt.Frame()
	if !tt.HasText("Second") {
		t.Fatalf("loaded page not shown: %q", tt.Texts())
	}
	if app.sessionsNext != "" {
		t.Fatalf("next cursor = %q", app.sessionsNext)
	}
}

func TestInboxShowsLoadMoreWhenFilterMatchesNoLoadedSessions(t *testing.T) {
	backend := &fakeBackend{
		servers: []profiles.Profile{{ID: "srv", Name: "Work", URL: "http://127.0.0.1:4096"}},
		snapshot: desktop.ServerSnapshot{
			Server:       profiles.Profile{ID: "srv", Name: "Work", URL: "http://127.0.0.1:4096"},
			Projects:     []desktop.NativeProject{{ID: "p1", Name: "zoc"}},
			Sessions:     []desktop.NativeSession{{ID: "ses-1", ProjectID: "p1", Title: "Unrelated session", Updated: 2}},
			SessionsNext: "page-2",
		},
		page: desktop.NativeSessionPage{
			Sessions: []desktop.NativeSession{{ID: "ses-2", ProjectID: "p1", Title: "Needle session", Updated: 1}},
		},
		moreCalled: make(chan struct{}, 1),
	}
	app := &Inbox{
		backend: backend, servers: backend.servers, selected: "srv",
		snapshot: &backend.snapshot, sessionsNext: "page-2", passwords: map[string]string{},
		filter: "needle",
	}
	tt := ui.NewTester(app.view, 980, 680)
	if !tt.HasText("No loaded sessions match") || !tt.HasText("Load more sessions") {
		t.Fatalf("empty filtered view lost pagination: %q", tt.Texts())
	}
	if err := tt.Click("Load more sessions"); err != nil {
		t.Fatal(err)
	}
	<-backend.moreCalled
	waitFor(t, func() bool { return app.snapshot != nil && len(app.snapshot.Sessions) == 2 })
	tt.Frame()
	if !tt.HasText("Needle session") || !tt.HasText("1 matching loaded sessions") {
		t.Fatalf("later page match not shown: %q", tt.Texts())
	}
}

func TestStaleRefreshCannotOverwriteNewerResult(t *testing.T) {
	release := []chan struct{}{make(chan struct{}), make(chan struct{})}
	started := make(chan int, 2)
	var calls atomic.Int32
	backend := &fakeBackend{
		servers: []profiles.Profile{{ID: "srv", Name: "Work", URL: "http://127.0.0.1:4096"}},
		snapshotFn: func(ctx context.Context, id, password string) (desktop.ServerSnapshot, error) {
			n := int(calls.Add(1))
			started <- n
			<-release[n-1]
			return desktop.ServerSnapshot{
				Server:   profiles.Profile{ID: id, Name: "Work", URL: "http://127.0.0.1:4096"},
				Sessions: []desktop.NativeSession{{ID: "ses", Title: []string{"old", "new"}[n-1]}},
			}, nil
		},
	}
	app := &Inbox{backend: backend, servers: backend.servers, selected: "srv", passwords: map[string]string{}}
	app.refresh()
	if n := <-started; n != 1 {
		t.Fatalf("first call = %d", n)
	}
	app.refresh()
	if n := <-started; n != 2 {
		t.Fatalf("second call = %d", n)
	}
	close(release[1])
	waitFor(t, func() bool {
		app.mu.Lock()
		defer app.mu.Unlock()
		return app.snapshot != nil && len(app.snapshot.Sessions) == 1 && app.snapshot.Sessions[0].Title == "new"
	})
	close(release[0])
	time.Sleep(30 * time.Millisecond)
	app.mu.Lock()
	defer app.mu.Unlock()
	if app.snapshot == nil || len(app.snapshot.Sessions) != 1 || app.snapshot.Sessions[0].Title != "new" {
		t.Fatalf("stale result overwrote newer: %+v", app.snapshot)
	}
}

func TestStaleRefreshErrorCannotOverwriteNewerResult(t *testing.T) {
	release := []chan struct{}{make(chan struct{}), make(chan struct{})}
	started := make(chan int, 2)
	var calls atomic.Int32
	backend := &fakeBackend{
		servers: []profiles.Profile{{ID: "srv", Name: "Work", URL: "http://127.0.0.1:4096"}},
		snapshotFn: func(ctx context.Context, id, password string) (desktop.ServerSnapshot, error) {
			n := int(calls.Add(1))
			started <- n
			<-release[n-1]
			if n == 1 {
				return desktop.ServerSnapshot{}, errors.New("stale failure")
			}
			return desktop.ServerSnapshot{
				Server:   profiles.Profile{ID: id, Name: "Work", URL: "http://127.0.0.1:4096"},
				Sessions: []desktop.NativeSession{{ID: "ses", Title: "new"}},
			}, nil
		},
	}
	app := &Inbox{backend: backend, servers: backend.servers, selected: "srv", passwords: map[string]string{}}
	app.refresh()
	if n := <-started; n != 1 {
		t.Fatalf("first call = %d", n)
	}
	app.refresh()
	if n := <-started; n != 2 {
		t.Fatalf("second call = %d", n)
	}
	close(release[1])
	waitFor(t, func() bool {
		app.mu.Lock()
		defer app.mu.Unlock()
		return app.snapshot != nil && app.snapshot.Sessions[0].Title == "new" && app.loadErr == ""
	})
	close(release[0])
	time.Sleep(30 * time.Millisecond)
	app.mu.Lock()
	defer app.mu.Unlock()
	if app.loadErr != "" {
		t.Fatalf("stale error = %q", app.loadErr)
	}
	if app.snapshot == nil || app.snapshot.Sessions[0].Title != "new" {
		t.Fatalf("stale error replaced newer result: %+v", app.snapshot)
	}
}

func TestMergeSessionsDeduplicates(t *testing.T) {
	got := mergeSessions(
		[]desktop.NativeSession{{ID: "a", Title: "A"}, {ID: "b", Title: "B"}},
		[]desktop.NativeSession{{ID: "b", Title: "B2"}, {ID: "c", Title: "C"}},
	)
	if len(got) != 3 || got[2].ID != "c" || got[1].Title != "B" {
		t.Fatalf("merged = %+v", got)
	}
}

func TestSwitchServerIgnoresOlderRefresh(t *testing.T) {
	releaseA := make(chan struct{})
	releaseB := make(chan struct{})
	started := make(chan string, 2)
	backend := &fakeBackend{
		servers: []profiles.Profile{
			{ID: "a", Name: "A", URL: "http://127.0.0.1:4096"},
			{ID: "b", Name: "B", URL: "http://127.0.0.1:4097"},
		},
		snapshotFn: func(ctx context.Context, id, password string) (desktop.ServerSnapshot, error) {
			started <- id
			if id == "a" {
				<-releaseA
			} else {
				<-releaseB
			}
			return desktop.ServerSnapshot{
				Server:   profiles.Profile{ID: id, Name: id, URL: "http://127.0.0.1:4096"},
				Sessions: []desktop.NativeSession{{ID: "ses", Title: id}},
			}, nil
		},
	}
	app := &Inbox{backend: backend, servers: backend.servers, selected: "a", passwords: map[string]string{}}
	app.refresh()
	if id := <-started; id != "a" {
		t.Fatalf("first server = %s", id)
	}
	app.selectServer("b")
	if id := <-started; id != "b" {
		t.Fatalf("second server = %s", id)
	}
	close(releaseB)
	waitFor(t, func() bool {
		app.mu.Lock()
		defer app.mu.Unlock()
		return app.selected == "b" && app.snapshot != nil && app.snapshot.Sessions[0].Title == "b"
	})
	close(releaseA)
	time.Sleep(30 * time.Millisecond)
	app.mu.Lock()
	defer app.mu.Unlock()
	if app.selected != "b" || app.snapshot == nil || app.snapshot.Sessions[0].Title != "b" {
		t.Fatalf("server switch was overwritten: %+v", app.snapshot)
	}
}
