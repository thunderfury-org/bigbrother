package server

import (
	"context"
	"log"

	"github.com/thunderfury-org/bigbrother/internal/openlist"
)

func Run() {
	// Create openlist client
	// TODO: Get baseURL and apiKey from configuration
	client := openlist.NewClient("https://your-openlist-instance.com", "your-api-key")

	// read file list from openlist
	files, err := client.ListFiles(context.Background(), "/")
	if err != nil {
		log.Fatalf("Failed to read file list from openlist: %v", err)
	}

	// Process files
	for _, file := range files.Files {
		log.Printf("Processing file: %s (%s)", file.Name, file.Type)
		// parse media info from file names
		// query tmdb for metadata
		// move file to media library
		// send notification to telegram
	}
}