package desktop

import (
	"os"
	"path/filepath"
	"testing"
)

func TestStorageBatchesPersist(t *testing.T) {
	dir := t.TempDir()
	s := &Service{DataDir: dir}
	rev, err := s.StorageUpdate("opencode.global.dat", map[string]string{"a": "1", "b": "2"}, nil)
	if err != nil {
		t.Fatal(err)
	}
	if rev != 1 {
		t.Fatalf("revision = %d", rev)
	}
	snapshot, err := s.StorageItems("opencode.global.dat")
	if err != nil {
		t.Fatal(err)
	}
	if snapshot.Revision != 1 || snapshot.Items["a"] != "1" || snapshot.Items["b"] != "2" {
		t.Fatalf("snapshot = %+v", snapshot)
	}
	rev, err = s.StorageUpdate("opencode.global.dat", map[string]string{"a": "3"}, []string{"b"})
	if err != nil || rev != 2 {
		t.Fatalf("update = %d, %v", rev, err)
	}
	snapshot, _ = s.StorageItems("opencode.global.dat")
	if snapshot.Items["a"] != "3" || snapshot.Items["b"] != "" {
		t.Fatalf("items = %+v", snapshot.Items)
	}
	info, err := os.Stat(filepath.Join(dir, "storage", "opencode.global.dat.json"))
	if err != nil {
		t.Fatal(err)
	}
	if info.Mode().Perm() != 0o600 {
		t.Fatalf("mode = %o", info.Mode().Perm())
	}
}

func TestStorageRejectsTraversal(t *testing.T) {
	s := &Service{DataDir: t.TempDir()}
	for _, namespace := range []string{"../escape", "a/b", "", "."} {
		if _, err := s.StorageItems(namespace); err == nil {
			t.Fatalf("namespace %q was accepted", namespace)
		}
	}
}
