package library

import (
	"fmt"
	"log/slog"
	"path"

	"github.com/thunderfury-org/bigbrother/internal/config"
	"github.com/thunderfury-org/bigbrother/internal/openlist"
	"github.com/thunderfury-org/bigbrother/internal/tmdb"
)

type Manager struct {
	openlist *openlist.Client
	meta     *metadataFetcher

	watchers map[string]*innerWatcher
}

func NewManager(openlist *openlist.Client, tmdb *tmdb.Client) *Manager {
	return &Manager{
		openlist: openlist,
		meta:     &metadataFetcher{tmdb: tmdb},
		watchers: map[string]*innerWatcher{},
	}
}

func (m *Manager) AddLibrary(lib config.LibraryConfig) error {
	// todo check lib valid
	if lib.Name == "" {
		return fmt.Errorf("library name must not be empty")
	}
	if lib.Path == "" {
		return fmt.Errorf("library path must not be empty")
	}

	watchPath := lib.WatchPath
	if watchPath == "" {
		watchPath = path.Join(lib.Path, ".watch")
	}

	invalidPath := lib.InvalidPath
	if invalidPath == "" {
		invalidPath = path.Join(lib.Path, ".invalid")
	}

	if _, exists := m.watchers[lib.Name]; exists {
		return fmt.Errorf("library %s already exists", lib.Name)
	}

	m.watchers[lib.Name] = &innerWatcher{
		openlist: m.openlist,
		meta:     m.meta,
		library: innerLibrary{
			Name:        lib.Name,
			Path:        lib.Path,
			WatchPath:   watchPath,
			InvalidPath: invalidPath,
			LocalPath:   lib.LocalPath,
		},
	}

	return nil
}

func (m *Manager) Start() error {
	for name, watcher := range m.watchers {
		err := watcher.Start()
		if err != nil {
			slog.Error(fmt.Sprintf("Failed to start watcher for library %s: %s", name, err))
		}
	}
	return nil
}

func (m *Manager) Stop() error {
	for name, watcher := range m.watchers {
		err := watcher.Stop()
		if err != nil {
			return fmt.Errorf("failed to stop watcher for library %s: %w", name, err)
		}
	}
	return nil
}
