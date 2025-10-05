package server

import (
	"log/slog"
	"path"

	"gopkg.in/natefinch/lumberjack.v2"

	"github.com/thunderfury-org/bigbrother/internal/client/openlist"
	"github.com/thunderfury-org/bigbrother/internal/client/telegram"
	"github.com/thunderfury-org/bigbrother/internal/client/tmdb"
	"github.com/thunderfury-org/bigbrother/internal/config"
	"github.com/thunderfury-org/bigbrother/internal/library"
)

func Run(dataDir string) {
	// log to file with daily rotation
	logger := &lumberjack.Logger{
		Filename:   path.Join(dataDir, "logs", "bigbrother.log"),
		MaxSize:    10, // megabytes
		MaxBackups: 3,  // number of backups to keep
		Compress:   false,
	}
	slog.SetDefault(slog.New(slog.NewTextHandler(logger, nil)))

	conf, err := config.Load(path.Join(dataDir, "config"))
	if err != nil {
		slog.Error("Failed to load config", slog.Any("err", err))
		return
	}

	manager := library.NewManager(
		openlist.NewClient(conf.OpenList.BaseURL, conf.OpenList.Token),
		tmdb.NewClient(conf.Tmdb.ApiKey),
		telegram.NewClient(conf.Telegram.Token, conf.Telegram.ChatId),
	)

	for _, lib := range conf.Libraries {
		if err = manager.AddLibrary(*lib); err != nil {
			slog.Error("Failed to add library", slog.String("name", lib.Name), slog.Any("err", err))
			return
		}
	}

	if err = manager.Start(); err != nil {
		slog.Error("Failed to start library manager", slog.Any("err", err))
		return
	}
}
