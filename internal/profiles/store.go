// Package profiles persists the client's server profiles.
//
// Profiles are ZOC preferences, not OpenCode server data. They hold only a
// display name and a base URL. Passwords, tokens and pairing secrets never
// belong in this store; authentication stays in the official Web UI and in
// memory for native views.
package profiles

import (
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"sync"
	"time"

	"github.com/zce/zoc/internal/networkpolicy"
)

const (
	fileName       = "servers.json"
	schemaVersion  = 1
	defaultName    = "Local OpenCode"
	defaultBaseURL = "http://127.0.0.1:4096"
)

// Profile is one configured OpenCode Server.
type Profile struct {
	ID        string    `json:"id"`
	Name      string    `json:"name"`
	URL       string    `json:"url"`
	CreatedAt time.Time `json:"createdAt"`
}

// Store is a concurrent profile collection backed by one JSON file.
type Store struct {
	mu       sync.RWMutex
	path     string
	profiles []Profile
}

type file struct {
	Version  int       `json:"version"`
	Profiles []Profile `json:"profiles"`
}

// Open loads the store from dir, creating a local-server profile when the
// store does not exist yet. A missing directory is created.
func Open(dir string) (*Store, error) {
	if err := os.MkdirAll(dir, 0o700); err != nil {
		return nil, fmt.Errorf("create user data directory: %w", err)
	}
	s := &Store{path: filepath.Join(dir, fileName)}
	data, err := os.ReadFile(s.path)
	switch {
	case err == nil:
		var f file
		if err := json.Unmarshal(data, &f); err != nil {
			return nil, fmt.Errorf("read server profiles: %w", err)
		}
		if f.Version != schemaVersion {
			return nil, fmt.Errorf("unsupported server profile version %d", f.Version)
		}
		for _, p := range f.Profiles {
			normalized, err := normalize(p)
			if err != nil {
				return nil, fmt.Errorf("invalid stored server profile %q: %w", p.ID, err)
			}
			s.profiles = append(s.profiles, normalized)
		}
	case errors.Is(err, os.ErrNotExist):
		if raw := strings.TrimSpace(os.Getenv("ZOC_SERVER_URL")); raw != "" {
			s.profiles = []Profile{newProfile("OpenCode", raw)}
		} else {
			s.profiles = []Profile{newProfile(defaultName, defaultBaseURL)}
		}
		if err := s.saveLocked(); err != nil {
			return nil, err
		}
	default:
		return nil, fmt.Errorf("read server profiles: %w", err)
	}
	sort.SliceStable(s.profiles, func(i, j int) bool {
		return strings.ToLower(s.profiles[i].Name) < strings.ToLower(s.profiles[j].Name)
	})
	return s, nil
}

// Path is the backing file, for diagnostics that never print credentials.
func (s *Store) Path() string { return s.path }

// List returns a copy ordered by display name.
func (s *Store) List() []Profile {
	s.mu.RLock()
	defer s.mu.RUnlock()
	out := make([]Profile, len(s.profiles))
	copy(out, s.profiles)
	return out
}

// Get returns one profile.
func (s *Store) Get(id string) (Profile, bool) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	for _, p := range s.profiles {
		if p.ID == id {
			return p, true
		}
	}
	return Profile{}, false
}

// Origins returns the allowed origin of every profile.
func (s *Store) Origins() []string {
	s.mu.RLock()
	defer s.mu.RUnlock()
	out := make([]string, 0, len(s.profiles))
	for _, p := range s.profiles {
		out = append(out, p.URL)
	}
	return out
}

// Add creates a profile. Duplicate origins are rejected rather than
// silently merging two differently named entries for one server.
func (s *Store) Add(name, rawURL string) (Profile, error) {
	candidate := newProfile(strings.TrimSpace(name), rawURL)
	candidate, err := normalize(candidate)
	if err != nil {
		return Profile{}, err
	}
	if candidate.Name == "" {
		candidate.Name = defaultNameFor(candidate.URL)
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	origin, err := networkpolicy.Origin(candidate.URL)
	if err != nil {
		return Profile{}, err
	}
	for _, p := range s.profiles {
		existing, _ := networkpolicy.Origin(p.URL)
		if existing == origin {
			return Profile{}, fmt.Errorf("a server for %s already exists", origin)
		}
	}
	previous := append([]Profile(nil), s.profiles...)
	s.profiles = append(s.profiles, candidate)
	sort.SliceStable(s.profiles, func(i, j int) bool {
		return strings.ToLower(s.profiles[i].Name) < strings.ToLower(s.profiles[j].Name)
	})
	if err := s.saveLocked(); err != nil {
		s.profiles = previous
		return Profile{}, err
	}
	return candidate, nil
}

// Update changes a profile's name and URL.
func (s *Store) Update(id, name, rawURL string) (Profile, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	index := -1
	for i, p := range s.profiles {
		if p.ID == id {
			index = i
			break
		}
	}
	if index < 0 {
		return Profile{}, fmt.Errorf("server profile not found")
	}
	previous := append([]Profile(nil), s.profiles...)
	next := previous[index]
	next.Name = strings.TrimSpace(name)
	next.URL = rawURL
	next, err := normalize(next)
	if err != nil {
		return Profile{}, err
	}
	origin, _ := networkpolicy.Origin(next.URL)
	for i, p := range s.profiles {
		if i == index {
			continue
		}
		existing, _ := networkpolicy.Origin(p.URL)
		if existing == origin {
			return Profile{}, fmt.Errorf("a server for %s already exists", origin)
		}
	}
	s.profiles[index] = next
	sort.SliceStable(s.profiles, func(i, j int) bool {
		return strings.ToLower(s.profiles[i].Name) < strings.ToLower(s.profiles[j].Name)
	})
	if err := s.saveLocked(); err != nil {
		s.profiles = previous
		return Profile{}, err
	}
	return next, nil
}

// Remove deletes a profile.
func (s *Store) Remove(id string) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	for i, p := range s.profiles {
		if p.ID != id {
			continue
		}
		previous := append([]Profile(nil), s.profiles...)
		s.profiles = append(s.profiles[:i], s.profiles[i+1:]...)
		if err := s.saveLocked(); err != nil {
			s.profiles = previous
			return err
		}
		return nil
	}
	return fmt.Errorf("server profile not found")
}

func newProfile(name, rawURL string) Profile {
	return Profile{
		ID:        newID(),
		Name:      strings.TrimSpace(name),
		URL:       rawURL,
		CreatedAt: time.Now().UTC(),
	}
}

func newID() string {
	var b [8]byte
	if _, err := rand.Read(b[:]); err != nil {
		panic("zoc: crypto/rand unavailable: " + err.Error())
	}
	return hex.EncodeToString(b[:])
}

func normalize(p Profile) (Profile, error) {
	if p.ID == "" {
		return Profile{}, fmt.Errorf("missing id")
	}
	p.Name = strings.TrimSpace(p.Name)
	p.URL = strings.TrimSpace(p.URL)
	normalized, err := networkpolicy.ValidateServerURL(p.URL)
	if err != nil {
		return Profile{}, err
	}
	p.URL = normalized
	if p.Name == "" {
		p.Name = defaultNameFor(p.URL)
	}
	if len(p.Name) > 120 {
		return Profile{}, fmt.Errorf("name is too long")
	}
	if p.CreatedAt.IsZero() {
		p.CreatedAt = time.Now().UTC()
	}
	return p, nil
}

func defaultNameFor(rawURL string) string {
	origin, err := networkpolicy.Origin(rawURL)
	if err != nil {
		return "OpenCode"
	}
	return strings.TrimPrefix(strings.TrimPrefix(origin, "http://"), "https://")
}

// saveLocked writes a temporary sibling, fsyncs it and renames it into
// place. The file is private because it is client configuration.
func (s *Store) saveLocked() error {
	if err := os.MkdirAll(filepath.Dir(s.path), 0o700); err != nil {
		return err
	}
	data, err := json.MarshalIndent(file{Version: schemaVersion, Profiles: s.profiles}, "", "  ")
	if err != nil {
		return err
	}
	data = append(data, '\n')
	tmp, err := os.CreateTemp(filepath.Dir(s.path), ".servers-*.tmp")
	if err != nil {
		return err
	}
	tmpName := tmp.Name()
	defer os.Remove(tmpName)
	if err := tmp.Chmod(0o600); err != nil {
		tmp.Close()
		return err
	}
	if _, err := tmp.Write(data); err != nil {
		tmp.Close()
		return err
	}
	if err := tmp.Sync(); err != nil {
		tmp.Close()
		return err
	}
	if err := tmp.Close(); err != nil {
		return err
	}
	return os.Rename(tmpName, s.path)
}
