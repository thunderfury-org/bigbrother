package server

import (
	"container/list"
	"log/slog"
	"path"

	"github.com/thunderfury-org/bigbrother/internal/config"
	"github.com/thunderfury-org/bigbrother/internal/media"
	"github.com/thunderfury-org/bigbrother/internal/openlist"
)

func Run() {
	conf, err := config.Load("./data/config")
	if err != nil {
		slog.Error("Failed to load config", slog.Any("err", err))
		return
	}

	// Create openlist client
	client := openlist.NewClient(conf.OpenList.BaseURL, conf.OpenList.Token)

	fileList := list.New()
	fileList.PushBack("/123pan/inbox/tgto123")

	for e := fileList.Front(); e != nil; e = e.Next() {
		currentPath := e.Value.(string)
		slog.Info("Processing path", slog.String("path", currentPath))
		files, err := client.ListFiles(currentPath, false)
		if err != nil {
			slog.Error("Failed to read file list from openlist", slog.Any("err", err))
			return
		}

		// Process files
		for _, file := range files {
			if file.IsDir {
				fileList.PushBack(path.Join(currentPath, file.Name))
			} else {
				info := media.Parse(file.Name)
				slog.Info("Processing file", slog.String("name", file.Name), slog.Any("file", info))
			}
		}
	}
}
