// Package networkpolicy restricts MyGo-native HTTP and WebSocket
// transports to the OpenCode Server origins this client has configured.
//
// The policy is deliberately narrow: it is not a general-purpose proxy.
// Callers may add origins only through the persisted server profile store.
package networkpolicy

import (
	"fmt"
	"net"
	"net/http"
	"net/url"
	"strings"
	"sync"
)

// Policy is a concurrent set of allowed HTTP origins.
type Policy struct {
	mu      sync.RWMutex
	origins map[string]struct{}
}

// New creates a policy that allows exactly the given HTTP(S) origins.
func New(rawURLs ...string) (*Policy, error) {
	p := &Policy{origins: make(map[string]struct{})}
	if err := p.Replace(rawURLs); err != nil {
		return nil, err
	}
	return p, nil
}

// Replace swaps the allowed origin set atomically.
func (p *Policy) Replace(rawURLs []string) error {
	next := make(map[string]struct{}, len(rawURLs))
	for _, raw := range rawURLs {
		origin, err := Origin(raw)
		if err != nil {
			return err
		}
		addOrigin(next, origin)
	}
	p.mu.Lock()
	p.origins = next
	p.mu.Unlock()
	return nil
}

// Allow adds one configured origin.
func (p *Policy) Allow(raw string) error {
	origin, err := Origin(raw)
	if err != nil {
		return err
	}
	p.mu.Lock()
	addOrigin(p.origins, origin)
	p.mu.Unlock()
	return nil
}

// addOrigin also admits the loopback spellings of a loopback origin:
// localhost, 127.0.0.1 and ::1 name the same local server. Other hosts are
// added exactly.
func addOrigin(set map[string]struct{}, origin string) {
	set[origin] = struct{}{}
	u, err := url.Parse(origin)
	if err != nil {
		return
	}
	host := strings.ToLower(u.Hostname())
	if host != "localhost" && host != "127.0.0.1" && host != "::1" {
		return
	}
	port := u.Port()
	for _, alias := range []string{"localhost", "127.0.0.1", "::1"} {
		set[u.Scheme+"://"+net.JoinHostPort(alias, port)] = struct{}{}
	}
}

// Allows reports whether raw is an HTTP(S) URL on an allowed origin.
func (p *Policy) Allows(raw string) bool {
	origin, err := Origin(raw)
	if err != nil {
		return false
	}
	p.mu.RLock()
	_, ok := p.origins[origin]
	p.mu.RUnlock()
	return ok
}

// AllowsRequest is the fetch plugin's Allow callback. It sees the initial
// request only; CheckRedirect below enforces the same policy on redirects.
func (p *Policy) AllowsRequest(r *http.Request) bool {
	return r != nil && r.URL != nil && p.Allows(r.URL.String())
}

// CheckRedirect enforces the policy on every redirect hop. Cross-origin
// redirects are rejected so credentials cannot be replayed to a different
// origin. At most five redirects are followed.
func (p *Policy) CheckRedirect(req *http.Request, via []*http.Request) error {
	if len(via) >= 5 {
		return fmt.Errorf("stopped after 5 redirects")
	}
	if req == nil || req.URL == nil {
		return fmt.Errorf("invalid redirect")
	}
	if !p.Allows(req.URL.String()) {
		return fmt.Errorf("redirect target is not a configured server origin")
	}
	if len(via) > 0 {
		from, err := Origin(via[0].URL.String())
		if err != nil {
			return err
		}
		to, err := Origin(req.URL.String())
		if err != nil {
			return err
		}
		if from != to {
			return fmt.Errorf("cross-origin redirect refused")
		}
	}
	return nil
}

// Origin canonicalizes an HTTP(S) or WS(S) URL to scheme://host:port.
// Userinfo is rejected so credentials never live in a URL. Query strings
// and fragments are ignored for origin comparison.
func Origin(raw string) (string, error) {
	u, err := url.Parse(strings.TrimSpace(raw))
	if err != nil {
		return "", fmt.Errorf("invalid server URL")
	}
	switch u.Scheme {
	case "http", "https":
	case "ws":
		u.Scheme = "http"
	case "wss":
		u.Scheme = "https"
	default:
		return "", fmt.Errorf("server URL must use http or https")
	}
	if u.User != nil {
		return "", fmt.Errorf("server URL must not contain credentials")
	}
	host := strings.ToLower(u.Hostname())
	if host == "" {
		return "", fmt.Errorf("server URL has no host")
	}
	port := u.Port()
	if port == "" {
		if u.Scheme == "https" {
			port = "443"
		} else {
			port = "80"
		}
	}
	if net.ParseIP(host) == nil && strings.ContainsAny(host, " /\\") {
		return "", fmt.Errorf("server URL has an invalid host")
	}
	return u.Scheme + "://" + net.JoinHostPort(host, port), nil
}

// ValidateServerURL checks a user-entered base URL. It permits a path, as
// some reverse proxies mount OpenCode below one, but no query, fragment or
// credentials.
func ValidateServerURL(raw string) (string, error) {
	raw = strings.TrimSpace(raw)
	u, err := url.Parse(raw)
	if err != nil {
		return "", fmt.Errorf("invalid server URL")
	}
	if u.Scheme != "http" && u.Scheme != "https" {
		return "", fmt.Errorf("server URL must use http or https")
	}
	if u.User != nil {
		return "", fmt.Errorf("server URL must not contain credentials")
	}
	if u.Hostname() == "" {
		return "", fmt.Errorf("server URL has no host")
	}
	if u.RawQuery != "" || u.Fragment != "" {
		return "", fmt.Errorf("server URL must not contain a query or fragment")
	}
	u.Path = strings.TrimRight(u.Path, "/")
	return u.String(), nil
}
