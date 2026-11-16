package opencode

import (
	"context"
	"encoding/base64"
	"errors"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/zce/zoc/internal/networkpolicy"
)

func testClient(t *testing.T, server *httptest.Server, password string) *Client {
	t.Helper()
	policy, err := networkpolicy.New(server.URL)
	if err != nil {
		t.Fatal(err)
	}
	c, err := New(server.URL, password, policy)
	if err != nil {
		t.Fatal(err)
	}
	return c
}

func TestInfoProjectsSessions(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/api/info":
			w.Header().Set("Content-Type", "application/json")
			w.Write([]byte(`{"version":"2.3.0","hostname":"fedora"}`))
		case "/api/project":
			w.Write([]byte(`[{"id":"p1","canonical":"/work/a","name":"A","time":{"created":1,"updated":2}}]`))
		case "/api/session":
			if r.URL.Query().Get("project") != "p1" || r.URL.Query().Get("order") != "desc" {
				t.Errorf("unexpected query: %s", r.URL.RawQuery)
			}
			w.Write([]byte(`{"data":[{"id":"s1","projectID":"p1","title":"Fix it","time":{"created":1,"updated":2}}],"cursor":{"next":null}}`))
		default:
			http.NotFound(w, r)
		}
	}))
	defer server.Close()

	c := testClient(t, server, "")
	info, err := c.Info(context.Background())
	if err != nil || info.Version != "2.3.0" {
		t.Fatalf("Info = %+v, %v", info, err)
	}
	projects, err := c.Projects(context.Background())
	if err != nil || len(projects) != 1 || projects[0].Name != "A" {
		t.Fatalf("Projects = %+v, %v", projects, err)
	}
	sessions, err := c.Sessions(context.Background(), "p1", 50)
	if err != nil || len(sessions) != 1 || sessions[0].Title != "Fix it" {
		t.Fatalf("Sessions = %+v, %v", sessions, err)
	}
}

func TestBasicAuthAndUnauthorized(t *testing.T) {
	want := "Basic " + base64.StdEncoding.EncodeToString([]byte("opencode:secret"))
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Header.Get("Authorization") != want {
			w.WriteHeader(http.StatusUnauthorized)
			return
		}
		w.Write([]byte(`{"version":"x"}`))
	}))
	defer server.Close()

	if _, err := testClient(t, server, "wrong").Info(context.Background()); !errors.Is(err, ErrUnauthorized) {
		t.Fatalf("wrong password error = %v", err)
	}
	if _, err := testClient(t, server, "secret").Info(context.Background()); err != nil {
		t.Fatalf("correct password: %v", err)
	}
}

func TestUnconfiguredOriginRefused(t *testing.T) {
	policy, err := networkpolicy.New("http://127.0.0.1:1")
	if err != nil {
		t.Fatal(err)
	}
	if _, err := New("http://127.0.0.1:2", "", policy); err == nil {
		t.Fatal("unconfigured origin was accepted")
	}
}
