// Package opencode is ZOC's narrow client for the OpenCode server API.
//
// It deliberately implements only the read surface Native UI needs:
// server info, projects and sessions. Session content, prompts,
// permissions and questions stay in the official Web UI until a native
// replacement is complete. Server responses are never cached as authority.
package opencode

import (
	"context"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"strconv"
	"strings"
	"time"

	"github.com/egoist/mygo/netproxy"
	"github.com/zce/zoc/internal/networkpolicy"
)

// ErrUnauthorized reports a 401/403 from the server.
var ErrUnauthorized = errors.New("opencode server rejected the credentials")

// Client talks to one configured server.
type Client struct {
	baseURL  string
	password string
	http     *http.Client
}

// New creates a client. The password stays in memory and is sent only as
// a Basic Authorization header.
func New(baseURL, password string, policy *networkpolicy.Policy) (*Client, error) {
	normalized, err := networkpolicy.ValidateServerURL(baseURL)
	if err != nil {
		return nil, err
	}
	if policy == nil || !policy.Allows(normalized) {
		return nil, fmt.Errorf("server origin is not configured")
	}
	return &Client{
		baseURL:  normalized,
		password: password,
		http: &http.Client{
			Timeout:       30 * time.Second,
			Transport:     netproxy.Transport(),
			CheckRedirect: policy.CheckRedirect,
		},
	}, nil
}

// Info is the response of GET /api/info.
type Info struct {
	Version  string `json:"version"`
	Hostname string `json:"hostname"`
}

// Project is one OpenCode project.
type Project struct {
	ID        string `json:"id"`
	Canonical string `json:"canonical"`
	Name      string `json:"name"`
	Time      struct {
		Created int64 `json:"created"`
		Updated int64 `json:"updated"`
	} `json:"time"`
}

// Session is the presentation subset of an OpenCode session.
type Session struct {
	ID        string `json:"id"`
	ParentID  string `json:"parentID"`
	ProjectID string `json:"projectID"`
	Title     string `json:"title"`
	Directory string `json:"directory"`
	Time      struct {
		Created  int64 `json:"created"`
		Updated  int64 `json:"updated"`
		Busy     int64 `json:"busy"`
		Idle     int64 `json:"idle"`
		Archived int64 `json:"archived"`
	} `json:"time"`
}

// Info probes the server.
func (c *Client) Info(ctx context.Context) (Info, error) {
	var out Info
	if err := c.do(ctx, http.MethodGet, "/api/info", nil, &out); err != nil {
		return Info{}, err
	}
	return out, nil
}

// Projects lists non-archived projects.
func (c *Client) Projects(ctx context.Context) ([]Project, error) {
	var out []Project
	if err := c.do(ctx, http.MethodGet, "/api/project", nil, &out); err != nil {
		return nil, err
	}
	return out, nil
}

// Sessions lists root sessions, newest first. projectID narrows the list
// when non-empty.
func (c *Client) Sessions(ctx context.Context, projectID string, limit int) ([]Session, error) {
	if limit <= 0 || limit > 200 {
		limit = 100
	}
	query := url.Values{}
	query.Set("limit", strconv.Itoa(limit))
	query.Set("order", "desc")
	query.Set("parentID", "null")
	if projectID != "" {
		query.Set("project", projectID)
	}
	var out struct {
		Data   []Session `json:"data"`
		Cursor struct {
			Next string `json:"next"`
		} `json:"cursor"`
	}
	if err := c.do(ctx, http.MethodGet, "/api/session", query, &out); err != nil {
		return nil, err
	}
	return out.Data, nil
}

func (c *Client) do(ctx context.Context, method, path string, query url.Values, out any) error {
	if ctx == nil {
		ctx = context.Background()
	}
	u := strings.TrimRight(c.baseURL, "/") + path
	if len(query) > 0 {
		u += "?" + query.Encode()
	}
	req, err := http.NewRequestWithContext(ctx, method, u, nil)
	if err != nil {
		return err
	}
	req.Header.Set("Accept", "application/json")
	if c.password != "" {
		token := base64.StdEncoding.EncodeToString([]byte("opencode:" + c.password))
		req.Header.Set("Authorization", "Basic "+token)
	}
	res, err := c.http.Do(req)
	if err != nil {
		return fmt.Errorf("opencode request failed: %w", err)
	}
	defer res.Body.Close()
	if res.StatusCode == http.StatusUnauthorized || res.StatusCode == http.StatusForbidden {
		io.Copy(io.Discard, io.LimitReader(res.Body, 4<<10))
		return ErrUnauthorized
	}
	if res.StatusCode < 200 || res.StatusCode >= 300 {
		io.Copy(io.Discard, io.LimitReader(res.Body, 4<<10))
		return fmt.Errorf("opencode %s %s returned HTTP %d", method, path, res.StatusCode)
	}
	if out == nil {
		io.Copy(io.Discard, io.LimitReader(res.Body, 1<<20))
		return nil
	}
	decoder := json.NewDecoder(io.LimitReader(res.Body, 8<<20))
	if err := decoder.Decode(out); err != nil {
		return fmt.Errorf("decode opencode response: %w", err)
	}
	return nil
}
