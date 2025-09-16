package media

import "strconv"

const (
	FileTypeVideo    = "video"
	FileTypeSubtitle = "subtitle"
)

const (
	LanguageChinese  = "zh"
	LanguageJapanese = "ja"
	LanguageEnglish  = "en"

	LanguageChineseSimplified  = "zh-CN"
	LanguageChineseTraditional = "zh-TW"
)

type NullableInt int

func (n *NullableInt) IsNull() bool {
	return n == nil
}

func (n *NullableInt) String() string {
	if n == nil {
		return "nil"
	}
	return strconv.Itoa(int(*n))
}

type MediaTitle struct {
	Title    string `yaml:"title,omitempty"`
	Language string `yaml:"language,omitempty"`
}

// MediaInfo represents metadata information about a media file.
type MediaInfo struct {
	FileType  string `yaml:"file_type,omitempty"` // File type based on file extension
	Extension string `yaml:"extension,omitempty"` // File extension (e.g: .mkv, .mp4, .srt)

	TmdbID string `yaml:"tmdb_id,omitempty"`

	Titles        []MediaTitle `yaml:"titles,omitempty"`         // Movie or TV Show title
	Year          string       `yaml:"year,omitempty"`           // Release year
	SeasonNumber  *NullableInt `yaml:"season_number,omitempty"`  // Season number for TV shows
	EpisodeNumber *NullableInt `yaml:"episode_number,omitempty"` // Episode number for TV shows

	Resolution string `yaml:"resolution,omitempty"`  // Video resolution (e.g: 2160p, 1080p, 720p)
	Quality    string `yaml:"quality,omitempty"`     // Quality of the media (e.g: BluRay, WEB-DL)
	HDR        string `yaml:"hdr,omitempty"`         // HDR type (e.g: HDR10, HDR10+, DolbyVision, HLG)
	VideoCodec string `yaml:"video_codec,omitempty"` // Video codec (e.g: H.264, H.265)
	AudioCodec string `yaml:"audio_codec,omitempty"` // Audio codec (e.g: AAC, DTS)

	ReleaseGroup string   `yaml:"release_group,omitempty"` // Release group name
	Subtitles    []string `yaml:"subtitles,omitempty"`     // Subtitle language (e.g: en, fr, es)
}
