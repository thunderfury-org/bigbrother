package server

import (
	"log/slog"

	"github.com/thunderfury-org/bigbrother/internal/config"
	"github.com/thunderfury-org/bigbrother/internal/openlist"
	"github.com/thunderfury-org/bigbrother/internal/tmdb"
)

func Run() {
	conf, err := config.Load("./data/config")
	if err != nil {
		slog.Error("Failed to load config", slog.Any("err", err))
		return
	}

	processor := &libraryProcessor{
		openlist: openlist.NewClient(conf.OpenList.BaseURL, conf.OpenList.Token),
		tmdb:     tmdb.NewClient(conf.Tmdb.ApiKey),
	}

	for _, library := range conf.Libraries {
		processor.Process(library)
	}
}
