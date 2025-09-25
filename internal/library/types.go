package library

import (
	"github.com/thunderfury-org/bigbrother/internal/media"
	"github.com/thunderfury-org/bigbrother/internal/openlist"
)

type mediaFile struct {
	file *openlist.File
	info *media.MediaInfo
}

type innerLibrary struct {
	name        string
	path        string
	watchPath   string
	invalidPath string
	localPath   string
}
