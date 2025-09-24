package server

import (
	"container/list"
	"log/slog"
	"path"

	"github.com/thunderfury-org/bigbrother/internal/config"
	"github.com/thunderfury-org/bigbrother/internal/media"
	"github.com/thunderfury-org/bigbrother/internal/openlist"
	"github.com/thunderfury-org/bigbrother/internal/tmdb"
)

type libraryProcessor struct {
	openlist *openlist.Client
	tmdb     *tmdb.Client
}

type mediaFile struct {
	file *openlist.File
	info *media.MediaInfo
}

func (p *libraryProcessor) Process(library *config.LibraryConfig) error {
	return p.walk(library.MonitorPath, func(dir string, files []*openlist.File) error {
		tvs := []*mediaFile{}
		movies := []*mediaFile{}

		for _, file := range files {
			f := &mediaFile{file: file, info: media.Parse(file.Name)}
			if f.info.EpisodeNumber.IsNull() {
				movies = append(movies, f)
			} else {
				tvs = append(tvs, f)
			}
		}

		err := p.handleMovies(dir, movies)
		if err != nil {
			return err
		}

		err = p.handleTvs(dir, tvs)
		if err != nil {
			return err
		}

		return nil
	})
}

func (p *libraryProcessor) handleMovies(dir string, files []*mediaFile) error {
	if len(files) == 0 {
		return nil
	}

	for _, f := range files {
		if len(f.info.Titles) == 0 {
			slog.Warn("Skipping movie", slog.String("file", f.file.Name), slog.String("reason", "no title"))
			continue
		}

		// query tmdb and get metadata
		metadata, err := p.tmdb.SearchMovie(f.info.Titles[0].Title, f.info.Year)
		if err != nil {
			slog.Warn("Failed to get movie metadata", slog.String("file", f.file.Name), slog.Any("err", err))
			continue
		}
		if len(metadata) == 0 {
			slog.Warn("No movie metadata found", slog.String("file", f.file.Name), slog.String("title", f.info.Titles[0].Title), slog.String("year", f.info.Year))
			continue
		}

		for _, m := range metadata {
			slog.Info("Found movie metadata", slog.String("file", f.file.Name), slog.String("title", m.Title), slog.String("release_date", m.ReleaseDate), slog.Int64("id", m.ID))
		}

		// format new name

		// rename file if necessary

		// get dest path in library

		// move file to library path

		// generate strm file in local path
	}

	return nil
}

func (p *libraryProcessor) handleTvs(dir string, files []*mediaFile) error {
	if len(files) == 0 {
		return nil
	}

	for _, f := range files {
		if len(f.info.Titles) == 0 {
			slog.Warn("Skipping TV show", slog.String("file", f.file.Name), slog.String("reason", "no title"))
			continue
		}

		// query tmdb and get metadata

		// format new name

		// rename file if necessary

		// get dest path in library

		// move file to library path

		// generate strm file in local path
	}

	return nil
}

func (p *libraryProcessor) walk(dir string, handler func(string, []*openlist.File) error) error {
	dirs := list.New()
	dirs.PushBack(dir)

	for e := dirs.Front(); e != nil; e = e.Next() {
		currentDir := e.Value.(string)
		slog.Info("Scanning directory", slog.String("dir", currentDir))
		files, err := p.openlist.ListFiles(currentDir, true)
		if err != nil {
			return err
		}

		onlyFiles := []*openlist.File{}
		for _, file := range files {
			if file.IsDir {
				slog.Info("Found subdirectory", slog.String("dir", file.Name), slog.String("parent", currentDir))
				dirs.PushBack(path.Join(currentDir, file.Name))
			} else {
				onlyFiles = append(onlyFiles, file)
			}
		}

		if len(onlyFiles) > 0 {
			err := handler(currentDir, onlyFiles)
			if err != nil {
				return err
			}
		}
	}
	return nil
}
