package cmd

import (
	"context"
	"fmt"
	"log"
	"os"

	"github.com/spf13/cobra"

	"github.com/thunderfury-org/bigbrother/internal/openlist"
)

// openlistCmd represents the openlist command
var openlistCmd = &cobra.Command{
	Use:   "openlist",
	Short: "Interact with openlist API",
}

var listCmd = &cobra.Command{
	Use:   "list [path]",
	Short: "List files in a directory",
	Args:  cobra.MaximumNArgs(1),
	Run: func(cmd *cobra.Command, args []string) {
		path := "/"
		if len(args) > 0 {
			path = args[0]
		}

		baseURL := os.Getenv("OPENLIST_BASE_URL")
		apiKey := os.Getenv("OPENLIST_API_KEY")

		if baseURL == "" {
			log.Fatal("OPENLIST_BASE_URL environment variable is required")
		}

		client := openlist.NewClient(baseURL, apiKey)
		resp, err := client.ListFiles(context.Background(), path)
		if err != nil {
			log.Fatalf("Failed to list files: %v", err)
		}

		for _, file := range resp.Files {
			fmt.Printf("%s\t%s\t%d\n", file.Type, file.Name, file.Size)
		}
	},
}

var infoCmd = &cobra.Command{
	Use:   "info [path]",
	Short: "Get information about a file",
	Args:  cobra.ExactArgs(1),
	Run: func(cmd *cobra.Command, args []string) {
		path := args[0]

		baseURL := os.Getenv("OPENLIST_BASE_URL")
		apiKey := os.Getenv("OPENLIST_API_KEY")

		if baseURL == "" {
			log.Fatal("OPENLIST_BASE_URL environment variable is required")
		}

		client := openlist.NewClient(baseURL, apiKey)
		resp, err := client.GetFileInfo(context.Background(), path)
		if err != nil {
			log.Fatalf("Failed to get file info: %v", err)
		}

		file := resp.File
		fmt.Printf("Name: %s\n", file.Name)
		fmt.Printf("Type: %s\n", file.Type)
		fmt.Printf("Size: %d\n", file.Size)
		fmt.Printf("Modified: %d\n", file.Modified)
		fmt.Printf("Path: %s\n", file.Path)
	},
}

func init() {
	rootCmd.AddCommand(openlistCmd)
	openlistCmd.AddCommand(listCmd)
	openlistCmd.AddCommand(infoCmd)
}