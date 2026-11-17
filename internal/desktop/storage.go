package desktop

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"sync"
)

// storageFile is one namespace's persisted key-value document.
type storageFile struct {
	Revision int64             `json:"revision"`
	Items    map[string]string `json:"items"`
}

var namespacePattern = regexp.MustCompile(`^[A-Za-z0-9._-]{1,80}$`)

// storageLocks serializes writes per namespace.
var storageLocks sync.Map

// StorageSnapshot is a namespace snapshot.
type StorageSnapshot struct {
	Items    map[string]string `json:"items"`
	Revision int64             `json:"revision"`
}

// StorageItems returns a namespace snapshot and its revision.
func (s *Service) StorageItems(namespace string) (StorageSnapshot, error) {
	if err := s.validateNamespace(namespace); err != nil {
		return StorageSnapshot{}, err
	}
	lock := storageLock(namespace)
	lock.Lock()
	defer lock.Unlock()
	doc, err := s.readStorage(namespace)
	if err != nil {
		return StorageSnapshot{}, err
	}
	out := make(map[string]string, len(doc.Items))
	for k, v := range doc.Items {
		out[k] = v
	}
	return StorageSnapshot{Items: out, Revision: doc.Revision}, nil
}

// StorageUpdate applies one batch and returns the new revision.
func (s *Service) StorageUpdate(namespace string, insert map[string]string, remove []string) (int64, error) {
	if err := s.validateNamespace(namespace); err != nil {
		return 0, err
	}
	lock := storageLock(namespace)
	lock.Lock()
	defer lock.Unlock()
	doc, err := s.readStorage(namespace)
	if err != nil {
		return 0, err
	}
	if doc.Items == nil {
		doc.Items = make(map[string]string)
	}
	for _, key := range remove {
		delete(doc.Items, key)
	}
	for key, value := range insert {
		doc.Items[key] = value
	}
	doc.Revision++
	if err := s.writeStorage(namespace, doc); err != nil {
		return 0, err
	}
	return doc.Revision, nil
}

// StorageClear empties a namespace.
func (s *Service) StorageClear(namespace string) error {
	if err := s.validateNamespace(namespace); err != nil {
		return err
	}
	lock := storageLock(namespace)
	lock.Lock()
	defer lock.Unlock()
	return s.writeStorage(namespace, storageFile{Revision: 1, Items: map[string]string{}})
}

func (s *Service) validateNamespace(namespace string) error {
	if !namespacePattern.MatchString(namespace) || namespace == "." || namespace == ".." {
		return fmt.Errorf("invalid storage namespace")
	}
	if s.DataDir == "" {
		return fmt.Errorf("data directory is not configured")
	}
	return nil
}

func storageLock(namespace string) *sync.Mutex {
	actual, _ := storageLocks.LoadOrStore(namespace, &sync.Mutex{})
	return actual.(*sync.Mutex)
}

func (s *Service) storagePath(namespace string) string {
	return filepath.Join(s.DataDir, "storage", namespace+".json")
}

func (s *Service) readStorage(namespace string) (storageFile, error) {
	data, err := os.ReadFile(s.storagePath(namespace))
	if os.IsNotExist(err) {
		return storageFile{Items: map[string]string{}}, nil
	}
	if err != nil {
		return storageFile{}, err
	}
	var doc storageFile
	if err := json.Unmarshal(data, &doc); err != nil {
		return storageFile{}, fmt.Errorf("read storage %s: %w", namespace, err)
	}
	if doc.Items == nil {
		doc.Items = map[string]string{}
	}
	return doc, nil
}

func (s *Service) writeStorage(namespace string, doc storageFile) error {
	dir := filepath.Join(s.DataDir, "storage")
	if err := os.MkdirAll(dir, 0o700); err != nil {
		return err
	}
	data, err := json.Marshal(doc)
	if err != nil {
		return err
	}
	tmp, err := os.CreateTemp(dir, "."+namespace+"-*.tmp")
	if err != nil {
		return err
	}
	name := tmp.Name()
	defer os.Remove(name)
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
	return os.Rename(name, s.storagePath(namespace))
}
