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
			if f.Info.FileType == "" {
				continue
			}

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
		return w.generateStrm(filePathInLib, f.File.Sign)
	case media.FileTypeSubtitle:
		return w.downloadFile(filePathInLib)
	default:
		return nil
	}
}

func (w *innerWatcher) handleTvs(currentDir string, files []*mediaFile) error {
	if len(files) == 0 {
		return nil
	}

	dirInfo, err := w.parseMediaInfoFromPath(currentDir)
	if err != nil {
		return fmt.Errorf("failed to parse media info from path: %w", err)
	}

	// 按照 name - year 分组
	tvGroups := make(map[string][]*mediaFile)
	for _, f := range files {
		var title string
		if len(f.Info.Titles) > 0 {
			title = f.Info.Titles[0].Title
		} else if dirInfo != nil && len(dirInfo.Titles) > 0 {
			f.Info.Titles = dirInfo.Titles
			title = dirInfo.Titles[0].Title
		} else {
			slog.Info("Skipping TV file", slog.String("file", f.File.Name), slog.String("reason", "no title found"))
			continue
		}

		if f.Info.Year == "" {
			// 如果年份为空，尝试从目录信息中获取
			if dirInfo != nil && dirInfo.Year != "" {
				f.Info.Year = dirInfo.Year
			}
		} else if dirInfo != nil && dirInfo.Year != f.Info.Year {
			//  文件年份和目录年份不一致， 以目录年份为准
			f.Info.Year = dirInfo.Year
		}

		if f.Info.SeasonNumber.IsNull() {
			f.Info.SeasonNumber = dirInfo.SeasonNumber
		}

		key := fmt.Sprintf("%s.%s", title, f.Info.Year)
		tvGroups[key] = append(tvGroups[key], f)
	}

	// 批量处理每个组的文件
	for _, groupFiles := range tvGroups {
		err = w.processOneTvGroup(currentDir, groupFiles)
		if err != nil {
			return err
		}
	}

	return nil
}

func (w *innerWatcher) processOneTvGroup(currentDir string, files []*mediaFile) error {
	tvInfo, err := w.meta.SearchTV(files[0].Info.Titles, files[0].Info.Year)
	if err != nil {
		slog.Info("Failed to get TV metadata", slog.String("title", files[0].Info.Titles[0].Title), slog.Any("err", err))
		return nil
	}
	if tvInfo == nil {
		slog.Warn("No TV metadata found", slog.String("title", files[0].Info.Titles[0].Title), slog.String("year", files[0].Info.Year))
		return nil
	}

	slog.Info("TV metadata found", slog.String("title", tvInfo.Name), slog.String("year", tvInfo.FirstAirDate))

	var seasonEpisodeNumberMap = make(map[int]int)
	for _, season := range tvInfo.Seasons {
		seasonEpisodeNumberMap[season.SeasonNumber] = season.EpisodeCount
	}

	var renameObjects []*openlist.RenameObject
	moveFiles := make(map[string][]*mediaFile)

	for _, f := range files {
		// 更新媒体信息
		f.Genres = tvInfo.Genres
		f.OriginCountry = tvInfo.OriginCountry
		f.Info.Titles = []media.MediaTitle{
			{
				Language: media.LanguageChinese,
				Title:    tvInfo.Name,
			},
		}
		f.Info.Year = tvInfo.FirstAirDate[:4]
		f.Info.TmdbID = strconv.FormatInt(tvInfo.ID, 10)
		if f.Info.SeasonNumber.IsNull() {
			if tvInfo.NumberOfSeasons == 1 {
				seasonNumber := media.NullableInt(1)
				f.Info.SeasonNumber = &seasonNumber
			} else {
				slog.Warn("No season number found", slog.String("file", f.File.Name), slog.String("title", f.Info.Titles[0].Title), slog.String("year", f.Info.Year))
				continue
			}
		}

		// 准备重命名对象
		oldName := f.File.Name
		newNamePrefix := fmt.Sprintf("%s.%s.S%02dE%02d", f.Info.Titles[0].Title, f.Info.Year, f.Info.SeasonNumber.Int(), f.Info.EpisodeNumber.Int())
		if !strings.HasPrefix(f.File.Name, newNamePrefix) {
			newName := generateFileName(f.Info)
			slog.Info("Rename TV file", slog.String("old", f.File.Name), slog.String("new", newName))
			renameObjects = append(renameObjects, &openlist.RenameObject{
				SrcName: oldName,
				NewName: newName,
			})
			f.File.Name = newName
		}

		destPath := w.generateDestPathInLibrary(f)
		moveFiles[destPath] = append(moveFiles[destPath], f)
	}

	// 批量重命名文件
	if len(renameObjects) > 0 {
		err := w.openlist.BatchRename(currentDir, renameObjects)
		if err != nil {
			slog.Error("Batch rename failed", slog.String("dir", currentDir), slog.Any("err", err))
			return err
		}
	}

	for destPath, files := range moveFiles {
		// 批量移动文件
		moveFiles := make([]string, len(files))
		for i, f := range files {
			moveFiles[i] = f.File.Name
		}
		err = w.batchMoveFiles(currentDir, destPath, moveFiles)
		if err != nil {
			slog.Error("Batch move failed", slog.String("src", currentDir), slog.String("dest", destPath), slog.Any("err", err))
			return err
		}

		mediaFileMap := make(map[string]*mediaFile)
		for _, f := range files {
			mediaFileMap[f.File.Name] = f
		}

		// 移动文件后重新获取文件信息, 用于获取 sign
		newFiles, err := w.openlist.ListFiles(destPath, true)
		if err != nil {
			slog.Error("Failed to list files", slog.String("dir", destPath), slog.Any("err", err))
			return err
		}

		// 处理 .strm 文件和下载字幕文件
		for _, f := range newFiles {
			mf := mediaFileMap[f.Name]
			if mf == nil {
				slog.Warn("No media file found", slog.String("file", f.Name))
				continue
			}

			filePathInLib := fmt.Sprintf("%s/%s", destPath, f.Name)
			switch mf.Info.FileType {
			case media.FileTypeVideo:
				err := w.generateStrm(filePathInLib, f.Sign)
				if err != nil {
					slog.Error("Failed to generate strm", slog.String("file", filePathInLib), slog.Any("err", err))
					return err
				}
			case media.FileTypeSubtitle:
				err := w.downloadFile(filePathInLib)
				if err != nil {
					slog.Error("Failed to download file", slog.String("file", filePathInLib), slog.Any("err", err))
					return err
				}
			}
		}
	}

	return nil
}

// 从目录路径中获取电视剧名称信息
func (w *innerWatcher) parseMediaInfoFromPath(currentDir string) (*media.MediaInfo, error) {
	if strings.HasPrefix(w.library.WatchPath, currentDir) {
		// 不是 library watch path 的子目录，直接返回 nil
		return nil, nil
	}

	// 提取相对于 library path 的路径部分
	subPath := currentDir[len(w.library.WatchPath):]
	if subPath == "" {
		return nil, fmt.Errorf("empty sub path, currentDir: %s", currentDir)
	}

	// 分割路径为各个部分
	pathParts := strings.Split(strings.TrimPrefix(subPath, "/"), "/")
	if len(pathParts) == 0 {
		return nil, fmt.Errorf("empty path parts, currentDir: %s", currentDir)
	}

	// 对目录名进行解析，尝试提取信息
	dirInfo := media.Parse(pathParts[len(pathParts)-1])
	if len(dirInfo.Titles) > 0 && dirInfo.Year != "" {
		return dirInfo, nil
	}

	if len(pathParts) == 1 {
		return dirInfo, nil
	}

	// 继续解析上级目录
	parentDirInfo := media.Parse(pathParts[len(pathParts)-2])
	if len(parentDirInfo.Titles) == 0 {
		return dirInfo, nil
	}

	if parentDirInfo.Year == "" {
		parentDirInfo.Year = dirInfo.Year
	}
	if parentDirInfo.TmdbID == "" {
		parentDirInfo.TmdbID = dirInfo.TmdbID
	}
	if parentDirInfo.SeasonNumber.IsNull() {
		parentDirInfo.SeasonNumber = dirInfo.SeasonNumber
	}
	return parentDirInfo, nil
}

func (w *innerWatcher) generateStrm(filePathInLib string, sign string) error {
	if w.library.LocalPath == "" {
		return nil
	}
	if filePathInLib == "" {
		return fmt.Errorf("file path in library is empty")
	}

	url := fmt.Sprintf("%s/d%s?sign=%s", w.openlist.GetBaseURL(), filePathInLib, sign)
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
	if len(names) == 0 {
		return nil
	}

	// maybe dest dir not exists, create it
	err := w.openlist.Mkdir(destDir)
	if err != nil {
		return err
	}

	return w.openlist.BatchMove(srcDir, destDir, names)
}

func (w *innerWatcher) generateDestPathInLibrary(f *mediaFile) string {
	if f.Info.SeasonNumber.IsNull() {
		return fmt.Sprintf("%s/%s/%s/%s (%s) {tmdb-%s}",
			w.library.Path, getCatalog(f), getSubCatalog(f),
			f.Info.Titles[0].Title, f.Info.Year, f.Info.TmdbID)
	}

	return fmt.Sprintf("%s/%s/%s/%s (%s) {tmdb-%s}/Season %02d",
		w.library.Path, getCatalog(f), getSubCatalog(f),
		f.Info.Titles[0].Title, f.Info.Year, f.Info.TmdbID, f.Info.SeasonNumber.Int())
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
