package library

import (
	"fmt"
	"log/slog"
	"path"
	"strconv"
	"strings"

	"github.com/thunderfury-org/bigbrother/internal/media"
	"github.com/thunderfury-org/bigbrother/internal/openlist"
)

type innerWatcher struct {
	openlist *openlist.Client
	meta     *metadataFetcher

	library innerLibrary
}

func (w *innerWatcher) Start() error {
	return w.processLibrary()
}

func (w *innerWatcher) Stop() error {
	return nil
}

func (w *innerWatcher) processLibrary() error {
	return w.openlist.WalkDir(w.library.watchPath, func(currentDir string, files []*openlist.File) error {
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

		err := w.handleMovies(currentDir, movies)
		if err != nil {
			return err
		}

		err = w.handleTvs(currentDir, tvs)
		if err != nil {
			return err
		}

		return nil
	})
}

func (w *innerWatcher) handleMovies(currentDir string, files []*mediaFile) error {
	if len(files) == 0 {
		return nil
	}

	for _, f := range files {
		if f.info.FileType == "" {
			continue
		}
		if len(f.info.Titles) == 0 {
			slog.Warn("Skipping movie", slog.String("file", f.file.Name), slog.String("reason", "no title"))
			continue
		}

		err := w.processOneMovieFile(currentDir, f)
		if err != nil {
			return err
		}
	}

	return nil
}

func (w *innerWatcher) processOneMovieFile(currentDir string, f *mediaFile) error {
	// get metadata
	movie, err := w.meta.SearchMovie(f.info.Titles, f.info.Year)
	if err != nil {
		slog.Warn("Failed to get movie metadata", slog.String("file", f.file.Name), slog.Any("err", err))
		return nil
	}
	if movie == nil {
		slog.Warn("No movie metadata found", slog.String("file", f.file.Name), slog.String("title", f.info.Titles[0].Title), slog.Int("year", f.info.Year))
		return nil
	}

	// update media info
	f.info.Titles = []media.MediaTitle{
		{
			Language: media.LanguageChinese,
			Title:    movie.Title,
		},
	}
	f.info.Year, _ = strconv.Atoi(movie.ReleaseDate[:4])
	f.info.TmdbID = strconv.FormatInt(movie.ID, 10)

	oldName := f.file.Name
	newName := f.file.Name
	newNamePrefix := fmt.Sprintf("%s.%d.", movie.Title, f.info.Year)
	if !strings.HasPrefix(f.file.Name, newNamePrefix) {
		newName = generateFileName(f.info)
		slog.Info("Renaming movie", slog.String("old", f.file.Name), slog.String("new", newName))
	}

	// get dest path in library
	destPath := fmt.Sprintf("%s/%s (%s) {tmdb-%d}", w.library.path, movie.Title, movie.ReleaseDate[:4], movie.ID)

	// move file to library path
	slog.Info("Moving movie file to library", slog.String("src", path.Join(currentDir, oldName)), slog.String("dest", path.Join(destPath, newName)))

	// generate strm file in local path
	if w.library.localPath != "" {
		slog.Info("Generating strm file", slog.String("name", path.Join(w.library.localPath, fmt.Sprintf("%s.strm", movie.Title))))
	}
	return nil
}

func (w *innerWatcher) handleTvs(dir string, files []*mediaFile) error {
	if len(files) == 0 {
		return nil
	}
	return nil
}

func generateFileName(info *media.MediaInfo) string {
	var buf []string
	buf = append(buf, info.Titles[0].Title)
	buf = append(buf, strconv.Itoa(info.Year))
	if !info.SeasonNumber.IsNull() {
		buf = append(buf, fmt.Sprintf("S%02dE%02d", info.SeasonNumber.Int(), info.EpisodeNumber.Int()))
	}
	if info.Resolution != "" {
		buf = append(buf, info.Resolution)
	}
	if info.FrameRate != "" {
		buf = append(buf, info.FrameRate)
	}
	if info.Quality != "" {
		buf = append(buf, info.Quality)
	}
	if info.HDR != "" {
		buf = append(buf, info.HDR)
	}
	if info.VideoCodec != "" {
		buf = append(buf, info.VideoCodec)
	}
	if info.AudioCodec != "" {
		buf = append(buf, info.AudioCodec)
	}

	return strings.Join(buf, ".") + info.Extension
}
