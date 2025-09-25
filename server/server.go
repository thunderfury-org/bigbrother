package server

import (
	"log/slog"

	"github.com/thunderfury-org/bigbrother/internal/config"
	"github.com/thunderfury-org/bigbrother/internal/library"
	"github.com/thunderfury-org/bigbrother/internal/openlist"
	"github.com/thunderfury-org/bigbrother/internal/tmdb"
)

func Run() {
	conf, err := config.Load("./data/config")
	if err != nil {
		slog.Error("Failed to load config", slog.Any("err", err))
		return
	}

	manager := library.NewManager(
		openlist.NewClient(conf.OpenList.BaseURL, conf.OpenList.Token),
		tmdb.NewClient(conf.Tmdb.ApiKey),
	)

	for _, lib := range conf.Libraries {
		if err := manager.AddLibrary(*lib); err != nil {
			slog.Error("Failed to add library", slog.String("name", lib.Name), slog.Any("err", err))
			return
		}
	}

	if err = manager.Start(); err != nil {
		slog.Error("Failed to start library manager", slog.Any("err", err))
		return
	}
}
