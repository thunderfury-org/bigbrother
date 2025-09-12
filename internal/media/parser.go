package media

import (
	"regexp"
	"strconv"
	"strings"
)

// video extensions (from https://en.wikipedia.org/wiki/Video_file_format)
var videoExtensions map[string]struct{} = make(map[string]struct{})
var subtitleExtensions map[string]struct{} = make(map[string]struct{})

func init() {
	extensions := []string{
		".3g2", ".3gp", ".3gp2", ".asf", ".avi", ".divx", ".flv",
		".iso", ".m4v", ".mk2", ".mk3d", ".mka", ".mkv", ".mov",
		".mp4", ".mp4a", ".mpeg", ".mpg", ".ogg", ".ogm", ".ogv",
		".qt", ".ra", ".ram", ".rm", ".ts", ".m2ts", ".vob", ".wav",
		".webm", ".wma", ".wmv",
	}
	for _, ext := range extensions {
		videoExtensions[ext] = struct{}{}
	}

	subExtensions := []string{".srt", ".sub", ".idx", ".ass", ".ssa"}
	for _, ext := range subExtensions {
		subtitleExtensions[ext] = struct{}{}
	}
}

type parser struct {
	name  string
	other string

	info *MediaInfo
}

func newParser(name string) *parser {
	return &parser{
		name: strings.TrimSpace(name),
		info: &MediaInfo{},
	}
}

func (p *parser) parse() *MediaInfo {
	p.parseFileType()
	p.parseResolution()
	p.parseSeasonEpisode()
	return p.info
}

func (p *parser) parseFileType() {
	p.info.FileType = FileTypeUnknown

	// find from the last dot
	dotIndex := strings.LastIndex(p.name, ".")
	if dotIndex < 0 {
		// no extension
		return
	}

	p.info.Extension = strings.ToLower(p.name[dotIndex:])
	if len(p.info.Extension) == 0 {
		// no extension
		return
	}

	if _, ok := videoExtensions[p.info.Extension]; ok {
		p.info.FileType = FileTypeVideo
	} else if _, ok := subtitleExtensions[p.info.Extension]; ok {
		p.info.FileType = FileTypeSubtitle
	}

	// remove the extension for further parsing
	p.name = p.name[:dotIndex]
}

var resolutionRe = regexp.MustCompile(`(\d{3,4}x(?P<height>\d{3,4}))|(?i)(?P<resolution>\d{1,4}[pk])`)

func (p *parser) parseResolution() {
	match := reFind(resolutionRe, p.name)
	if match == nil {
		return
	}

	height := match.groups["height"]
	if height != "" {
		// height from 1920x1080, 1280x720, etc.
		p.info.Resolution = height + "p"
	} else {
		resolution := match.groups["resolution"]
		if resolution != "" {
			// 720p, 1080p, 4k, etc.
			p.info.Resolution = strings.ToLower(resolution)
			if p.info.Resolution == "4k" {
				p.info.Resolution = "2160p"
			}
		}
	}

	p.name = p.name[:match.start] + p.name[match.end:]
}

var yearRe = regexp.MustCompile(`(?P<year>19\d{2}|20\d{2})`)

func (p *parser) parseYear() {

}

var seasonEpisodeRe = regexp.MustCompile(`(?i)(\[?S(eason)?\s*(?P<season_number>\d{1,2})\s*\]?\s*)?([\[|E]|(\-\s+)|(#\s*))(?P<episode_number>\d{1,4})(-(?P<episode_number2>\d{1,4}))?`)

func (p *parser) parseSeasonEpisode() {
	match := reFind(seasonEpisodeRe, p.name)
	if match == nil {
		return
	}

	seasonNumber := match.groups["season_number"]
	if seasonNumber != "" {
		p.info.SeasonNumber = mustAtoi(seasonNumber)
	}

	episodeNumber := match.groups["episode_number"]
	episodeNumber2 := match.groups["episode_number2"]
	if episodeNumber2 != "" {
		// episode range, e.g. 01-02, do not support yet
	} else if episodeNumber != "" {
		p.info.EpisodeNumber = mustAtoi(episodeNumber)
	}

	p.name, p.other = p.name[:match.start], p.name[match.end:]
}

func mustAtoi(s string) int {
	n, err := strconv.Atoi(s)
	if err != nil {
		panic(err)
	}
	return n
}

func Parse(name string) *MediaInfo {
	return newParser(name).parse()
}
