package desktop

import (
	"context"
	"encoding/base64"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/zce/zoc/internal/networkpolicy"
	"github.com/zce/zoc/internal/profiles"
)

func serviceFor(t *testing.T, serverURL string) *Service {
	t.Helper()
	store, err := profiles.Open(t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	// Replace the default profile with the test server.
	for _, p := range store.List() {
		_ = store.Remove(p.ID)
	}
	if _, err := store.Add("Fixture", serverURL); err != nil {
		t.Fatal(err)
	}
	policy, err := networkpolicy.New(store.Origins()...)
	if err != nil {
		t.Fatal(err)
	}
	return &Service{Store: store, Policy: policy, DataDir: t.TempDir()}
}

func TestSnapshotAndTestServer(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		switch r.URL.Path {
		case "/api/info":
			w.Write([]byte(`{"version":"2.3.0","hostname":"fixture"}`))
		case "/api/project":
			w.Write([]byte(`[{"id":"p1","canonical":"/work/zoc","name":"zoc","time":{"created":1,"updated":2}}]`))
		case "/api/session":
			w.Write([]byte(`{"data":[{"id":"ses-1","projectID":"p1","title":"Fix login","time":{"created":1,"updated":2,"busy":3,"idle":2}}],"cursor":{"previous":null,"next":null}}`))
		default:
			http.NotFound(w, r)
		}
	}))
	defer server.Close()

	s := serviceFor(t, server.URL)
	result, err := s.TestServer(context.Background(), s.Store.List()[0].ID, "")
	if err != nil || !result.OK || result.Version != "2.3.0" {
		t.Fatalf("TestServer = %+v, %v", result, err)
	}
	snapshot, err := s.Snapshot(context.Background(), s.Store.List()[0].ID, "")
	if err != nil {
		t.Fatal(err)
	}
	if len(snapshot.Projects) != 1 || snapshot.Projects[0].Name != "zoc" {
		t.Fatalf("projects = %+v", snapshot.Projects)
	}
	if len(snapshot.Sessions) != 1 || !snapshot.Sessions[0].Busy {
		t.Fatalf("sessions = %+v", snapshot.Sessions)
	}
}

func TestSessionRouteMatchesOfficialEncoding(t *testing.T) {
	route := sessionRoute("https://example.com:4096", "ses/1")
	want := "/server/" + base64.RawURLEncoding.EncodeToString([]byte("https://example.com:4096")) + "/session/ses%2F1"
	if route != want {
		t.Fatalf("route = %q, want %q", route, want)
	}
}
