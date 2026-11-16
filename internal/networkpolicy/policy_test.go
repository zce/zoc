package networkpolicy

import (
	"net/http"
	"testing"
)

func TestOrigin(t *testing.T) {
	cases := []struct {
		in, want string
		ok       bool
	}{
		{"http://127.0.0.1:4096", "http://127.0.0.1:4096", true},
		{"https://Example.COM/", "https://example.com:443", true},
		{"http://localhost:4096/api?x=1", "http://localhost:4096", true},
		{"ws://localhost:4096/pty", "http://localhost:4096", true},
		{"wss://example.com/pty", "https://example.com:443", true},
		{"ftp://example.com", "", false},
		{"http://user:pass@example.com", "", false},
		{"file:///etc/passwd", "", false},
	}
	for _, tc := range cases {
		got, err := Origin(tc.in)
		if tc.ok && err != nil {
			t.Fatalf("Origin(%q): %v", tc.in, err)
		}
		if !tc.ok {
			if err == nil {
				t.Fatalf("Origin(%q) unexpectedly succeeded: %q", tc.in, got)
			}
			continue
		}
		if got != tc.want {
			t.Fatalf("Origin(%q) = %q, want %q", tc.in, got, tc.want)
		}
	}
}

func TestPolicyAllowsOnlyConfiguredOrigins(t *testing.T) {
	p, err := New("http://127.0.0.1:4096")
	if err != nil {
		t.Fatal(err)
	}
	if !p.Allows("http://127.0.0.1:4096/api/session") {
		t.Fatal("configured origin was refused")
	}
	if !p.Allows("ws://127.0.0.1:4096/api/experimental/persistent-pty/x/connect?ticket=secret") {
		t.Fatal("websocket on configured origin was refused")
	}
	if p.Allows("http://127.0.0.1:4097/api/session") {
		t.Fatal("unconfigured port was allowed")
	}
	if p.Allows("https://evil.example/") {
		t.Fatal("unconfigured host was allowed")
	}
	for _, alias := range []string{"http://localhost:4096/api", "http://[::1]:4096/api"} {
		if !p.Allows(alias) {
			t.Fatalf("loopback alias %s was refused", alias)
		}
	}
}

func TestCheckRedirectRefusesCrossOrigin(t *testing.T) {
	p, err := New("http://a.example:4096", "http://b.example:4096")
	if err != nil {
		t.Fatal(err)
	}
	from := mustRequest(t, "http://a.example:4096/start")
	next := mustRequest(t, "http://b.example:4096/other")
	if err := p.CheckRedirect(next, []*http.Request{from}); err == nil {
		t.Fatal("cross-origin redirect was allowed")
	}
	same := mustRequest(t, "http://a.example:4096/next")
	if err := p.CheckRedirect(same, []*http.Request{from}); err != nil {
		t.Fatalf("same-origin redirect refused: %v", err)
	}
	outside := mustRequest(t, "http://c.example:4096/")
	if err := p.CheckRedirect(outside, []*http.Request{from}); err == nil {
		t.Fatal("redirect outside policy was allowed")
	}
}

func mustRequest(t *testing.T, raw string) *http.Request {
	t.Helper()
	r, err := http.NewRequest(http.MethodGet, raw, nil)
	if err != nil {
		t.Fatal(err)
	}
	return r
}
