package cmd

import (
	"fmt"
	"strings"

	"github.com/spf13/cobra"
	"github.com/thunderfury-org/bigbrother/internal/media"
)

// parseCmd represents the parse command
var parseCmd = &cobra.Command{
	Use:   "parse <input>",
	Short: "Parse media info from input string",
	Args:  cobra.ExactArgs(1),
	Run: func(cmd *cobra.Command, args []string) {
		info := media.Parse(args[0])

		if info.FileType != "" {
			fmt.Printf("FileType: %s\n", info.FileType)
		}
		if info.Extension != "" {
			fmt.Printf("Extension: %s\n", info.Extension)
		}
		if info.TmdbID != "" {
			fmt.Printf("TmdbID: %s\n", info.TmdbID)
		}

		if len(info.Titles) > 0 {
			for _, t := range info.Titles {
				if t.Title == "" && t.Language == "" {
					continue
				}
				if t.Language == "" {
					fmt.Printf("Title: %s\n", t.Title)
				} else {
					fmt.Printf("Title: %s (%s)\n", t.Title, t.Language)
				}
			}
		}

		if info.Year != "" {
			fmt.Printf("Year: %s\n", info.Year)
		}
		if info.SeasonNumber != nil {
			fmt.Printf("SeasonNumber: %s\n", info.SeasonNumber.String())
		}
		if info.EpisodeNumber != nil {
			fmt.Printf("EpisodeNumber: %s\n", info.EpisodeNumber.String())
		}
		if info.SecondEpisodeNumber != nil {
			fmt.Printf("SecondEpisodeNumber: %s\n", info.SecondEpisodeNumber.String())
		}

		if info.Resolution != "" {
			fmt.Printf("Resolution: %s\n", info.Resolution)
		}
		if info.FrameRate != "" {
			fmt.Printf("FrameRate: %s\n", info.FrameRate)
		}
		if info.Quality != "" {
			fmt.Printf("Quality: %s\n", info.Quality)
		}
		if info.HDR != "" {
			fmt.Printf("HDR: %s\n", info.HDR)
		}
		if info.VideoCodec != "" {
			fmt.Printf("VideoCodec: %s\n", info.VideoCodec)
		}
		if info.AudioCodec != "" {
			fmt.Printf("AudioCodec: %s\n", info.AudioCodec)
		}
		if info.ReleaseGroup != "" {
			fmt.Printf("ReleaseGroup: %s\n", info.ReleaseGroup)
		}
		if len(info.Subtitles) > 0 {
			fmt.Printf("Subtitles: %s\n", strings.Join(info.Subtitles, ","))
		}
	},
}

func init() {
	rootCmd.AddCommand(parseCmd)
}
