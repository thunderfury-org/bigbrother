package library

import (
	"errors"
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
	slog.Info("Scan library watch path", "library", w.library.Name, "watch", w.library.WatchPath)
	return w.openlist.WalkDir(w.library.WatchPath, func(currentDir string, files []*openlist.File) error {
		if len(files) == 0 {
			// maybe need delete current dir
			return nil
		}

		// separate tv and movies, and process them
		// because tv can be processed in batch
		tvs := []*mediaFile{}
		movies := []*mediaFile{}

		for _, file := range files {
			f := &mediaFile{File: file, Info: media.Parse(file.Name)}
			if f.Info.EpisodeNumber.IsNull() {
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
		if f.Info.FileType == "" {
			continue
		}
		if len(f.Info.Titles) == 0 {
			slog.Warn("Skipping movie", slog.String("file", f.File.Name), slog.String("reason", "no title"))
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
	movie, err := w.meta.SearchMovie(f.Info.Titles, f.Info.Year)
	if err != nil {
		slog.Warn("Failed to get movie metadata", slog.String("file", f.File.Name), slog.Any("err", err))
		return nil
	}
	if movie == nil {
		// todo send notification
		slog.Warn("No movie metadata found", slog.String("file", f.File.Name), slog.String("title", f.Info.Titles[0].Title), slog.String("year", f.Info.Year))
		return nil
	}

	// update media info
	f.Genres = movie.Genres
	f.OriginCountry = movie.OriginCountry
	f.Info.Titles = []media.MediaTitle{
		{
			Language: media.LanguageChinese,
			Title:    movie.Title,
		},
	}
	f.Info.Year = movie.ReleaseDate[:4]
	f.Info.TmdbID = strconv.FormatInt(movie.ID, 10)

	filePathInLib, err := w.archiveMediaFile(currentDir, f)
	if err != nil {
		return err
	}

	switch f.Info.FileType {
	case media.FileTypeVideo:
		return w.generateStrm(filePathInLib)
	case media.FileTypeSubtitle:
		return w.downloadFile(filePathInLib)
	default:
		return nil
	}
}

func (w *innerWatcher) handleTvs(dir string, files []*mediaFile) error {
	if len(files) == 0 {
		return nil
	}

	for _, f := range files {
		slog.Info("handleTvs", "file", f.File.Name)
	}
	return nil
}

func (w *innerWatcher) generateStrm(filePathInLib string) error {
	if w.library.LocalPath == "" {
		return nil
	}
	if filePathInLib == "" {
		return fmt.Errorf("file path in library is empty")
	}

	url := fmt.Sprintf("%s/d%s", w.openlist.GetBaseURL(), filePathInLib)
	localFilePath := strings.Replace(filePathInLib, w.library.Path, w.library.LocalPath, 1)
	index := strings.LastIndex(localFilePath, ".")
	localFilePath = localFilePath[:index] + ".strm"
	slog.Info("Write strm file", "url", url, "localFilePath", localFilePath)

	fp, err := openLocalFile(localFilePath)
	if err != nil {
		return err
	}
	defer fp.Close()

	_, err = fp.WriteString(url)
	return err
}

func (w *innerWatcher) downloadFile(filePathInLib string) error {
	if w.library.LocalPath == "" {
		return nil
	}
	if filePathInLib == "" {
		return fmt.Errorf("file path in library is empty")
	}

	localFilePath := strings.Replace(filePathInLib, w.library.Path, w.library.LocalPath, 1)
	slog.Info("Download file", "filePathInLib", filePathInLib, "localFilePath", localFilePath)
	return downloadFile(w.openlist, filePathInLib, localFilePath)
}

// rename and move media file to library path
func (w *innerWatcher) archiveMediaFile(currentDir string, f *mediaFile) (string, error) {
	oldName := f.File.Name
	newName := f.File.Name
	newNamePrefix := fmt.Sprintf("%s.%s.", f.Info.Titles[0].Title, f.Info.Year)
	if !strings.HasPrefix(f.File.Name, newNamePrefix) {
		newName = generateFileName(f.Info)
		slog.Info("Rename movie file", slog.String("old", f.File.Name), slog.String("new", newName))
		err := w.openlist.BatchRename(currentDir, []*openlist.RenameObject{
			{
				SrcName: oldName,
				NewName: newName,
			},
		})
		if err != nil {
			return "", fmt.Errorf("rename movie file failed: %w", err)
		}
	}

	// get dest path in library
	destPath := w.generateDestPathInLibrary(f)

	// move file to library path
	slog.Info("Move file to library", slog.String("src", path.Join(currentDir, oldName)), slog.String("dest", path.Join(destPath, newName)))
	err := w.batchMoveFiles(currentDir, destPath, []string{newName})
	if err != nil {
		return "", fmt.Errorf("move files failed: %w", err)
	}

	return fmt.Sprintf("%s/%s", destPath, newName), nil
}

func (w *innerWatcher) batchMoveFiles(srcDir string, destDir string, names []string) error {
	err := w.openlist.BatchMove(srcDir, destDir, names)
	if err != nil && !errors.Is(err, openlist.ErrNotFound) {
		return err
	}

	// maybe dest dir not exists, create it
	err = w.openlist.Mkdir(destDir)
	if err != nil {
		return err
	}

	return w.openlist.BatchMove(srcDir, destDir, names)
}

func (w *innerWatcher) generateDestPathInLibrary(f *mediaFile) string {
	return fmt.Sprintf("%s/%s/%s/%s (%s) {tmdb-%s}",
		w.library.Path, getCatalog(f), getSubCatalog(f),
		f.Info.Titles[0].Title, f.Info.Year, f.Info.TmdbID)
}

func generateFileName(info *media.MediaInfo) string {
	var buf []string
	buf = append(buf, info.Titles[0].Title)
	buf = append(buf, info.Year)
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
