package profiles

import (
	"os"
	"path/filepath"
	"testing"
)

func TestOpenCreatesPrivateDefaultProfile(t *testing.T) {
	dir := t.TempDir()
	s, err := Open(dir)
	if err != nil {
		t.Fatal(err)
	}
	list := s.List()
	if len(list) != 1 || list[0].URL != defaultBaseURL {
		t.Fatalf("unexpected defaults: %+v", list)
	}
	info, err := os.Stat(filepath.Join(dir, fileName))
	if err != nil {
		t.Fatal(err)
	}
	if info.Mode().Perm() != 0o600 {
		t.Fatalf("profile mode = %o, want 600", info.Mode().Perm())
	}
}

func TestAddUpdateRemove(t *testing.T) {
	s, err := Open(t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	p, err := s.Add("Work", "https://opencode.example.com/")
	if err != nil {
		t.Fatal(err)
	}
	if p.URL != "https://opencode.example.com" {
		t.Fatalf("url = %q", p.URL)
	}
	if _, err := s.Add("Duplicate", "https://opencode.example.com/other"); err == nil {
		t.Fatal("duplicate origin was accepted")
	}
	p, err = s.Update(p.ID, "Work 2", "https://other.example.com")
	if err != nil {
		t.Fatal(err)
	}
	if p.Name != "Work 2" || p.URL != "https://other.example.com" {
		t.Fatalf("unexpected update: %+v", p)
	}
	if err := s.Remove(p.ID); err != nil {
		t.Fatal(err)
	}
	if _, ok := s.Get(p.ID); ok {
		t.Fatal("profile still present after removal")
	}
}

func TestRejectsCredentialsAndInvalidScheme(t *testing.T) {
	s, err := Open(t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	for _, raw := range []string{
		"http://user:secret@example.com",
		"ftp://example.com",
		"file:///etc/passwd",
		"https://example.com/?token=secret",
	} {
		if _, err := s.Add("bad", raw); err == nil {
			t.Fatalf("%q was accepted", raw)
		}
	}
}

func TestReopenPersists(t *testing.T) {
	dir := t.TempDir()
	s, err := Open(dir)
	if err != nil {
		t.Fatal(err)
	}
	p, err := s.Add("Remote", "http://192.0.2.10:4096")
	if err != nil {
		t.Fatal(err)
	}
	reopened, err := Open(dir)
	if err != nil {
		t.Fatal(err)
	}
	got, ok := reopened.Get(p.ID)
	if !ok || got.URL != p.URL || got.Name != "Remote" {
		t.Fatalf("reopened = %+v, want %+v", got, p)
	}
}
