package library

import (
	"github.com/thunderfury-org/bigbrother/internal/media"
	"github.com/thunderfury-org/bigbrother/internal/openlist"
	"github.com/thunderfury-org/bigbrother/internal/tmdb"
)

type mediaFile struct {
	File *openlist.File
	Info *media.MediaInfo

	Genres        []tmdb.Genre
	OriginCountry []string
}

type innerLibrary struct {
	Name        string
	Path        string
	WatchPath   string
	InvalidPath string
	LocalPath   string
}
