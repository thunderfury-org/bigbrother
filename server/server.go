package server

import (
	"fmt"
	"log"

	"github.com/thunderfury-org/bigbrother/internal/media"
	"github.com/thunderfury-org/bigbrother/internal/openlist"

	"github.com/spf13/viper"
)

func Run() {
	viper.SetConfigName("config")
	viper.SetConfigType("yaml")
	viper.AddConfigPath("./data/config")
	err := viper.ReadInConfig() // Find and read the config file
	if err != nil {             // Handle errors reading the config file
		panic(fmt.Errorf("fatal error config file: %w", err))
	}

	// Create openlist client
	client := openlist.NewClient(viper.GetString("openlist.host"), viper.GetString("openlist.token"))

	// read file list from openlist
	files, err := client.ListFiles("/123pan/inbox/tgto123", false)
	if err != nil {
		log.Fatalf("Failed to read file list from openlist: %v", err)
	}

	// Process files
	for _, file := range files {
		info := media.Parse(file.Name)
		log.Printf("Processing file: %v", info)
	}
}
