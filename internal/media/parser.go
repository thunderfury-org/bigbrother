package media

import (
	"regexp"
	"strconv"
	"strings"

	"github.com/pemistahl/lingua-go"
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
	isDir bool

	info *MediaInfo
}

func newParser(name string, isDir bool) *parser {
	return &parser{
		name:  strings.TrimSpace(name),
		isDir: isDir,
		info:  &MediaInfo{},
	}
}

func (p *parser) parse() *MediaInfo {
	if !p.isDir {
		p.parseFileType()
	}
	p.normalizeName()
	p.parseTmdbID()
	p.parseResolution()
	p.parseYear()
	p.parseSeasonEpisode()
	p.parseQuality()
	p.parseHDR()
	p.parseVideoCodec()
	p.parseTitle()
	return p.info
}

func (p *parser) parseFileType() {
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

var replaceRe = regexp.MustCompile(`[_（）【】]`)

func (p *parser) normalizeName() {
	// replace underscores and hyphens with spaces
	p.name = " " + replaceRe.ReplaceAllLiteralString(p.name, ".") + " "
}

var tmdbRe = regexp.MustCompile(`(?i){\s*tmdb-(?P<tmdb_id>\d+)\s*}`)

func (p *parser) parseTmdbID() {
	match := reFindLastIndex(tmdbRe, p.name)
	if match == nil {
		return
	}

	p.info.TmdbID = getGroupFromMatch(tmdbRe, match, p.name, "tmdb_id")
	p.name = p.name[:match[0]] + p.name[match[1]:]
}

var resolutionRe = regexp.MustCompile(`(?:\.|\[| )\s*((\d{3,4}x(?P<height>\d{3,4}))|(?i)(?P<resolution>\d{1,4}[pk]))\s*(?:\.|\]| )`)

func (p *parser) parseResolution() {
	match := reFindLastIndex(resolutionRe, p.name)
	if match == nil {
		return
	}

	height := getGroupFromMatch(resolutionRe, match, p.name, "height")
	if height != "" {
		// height from 1920x1080, 1280x720, etc.
		p.info.Resolution = height + "p"
	} else {
		resolution := getGroupFromMatch(resolutionRe, match, p.name, "resolution")
		if resolution != "" {
			// 720p, 1080p, 4k, etc.
			p.info.Resolution = strings.ToLower(resolution)
			if p.info.Resolution == "4k" {
				p.info.Resolution = "2160p"
			}
		}
	}

	p.name = p.name[:match[0]] + p.name[match[1]:]
}

var yearRe = regexp.MustCompile(`(?:\.|\()\s*(?P<year>19\d{2}|20\d{2})\s*(?:\.|\))`)

func (p *parser) parseYear() {
	match := reFindLastIndex(yearRe, p.name)
	if match == nil {
		return
	}

	p.info.Year = getGroupFromMatch(yearRe, match, p.name, "year")
	p.name = p.name[:match[0]] + p.name[match[1]:]
}

var seasonEpisodeRe = regexp.MustCompile(`(?i)(\[?S(?:eason)?\s*(?P<season_number>\d{1,2})\s*\]?\s*)([E#]\s*(?P<episode_number>\d{1,4})(-(?P<episode_number2>\d{1,4}))?)?`)

func (p *parser) parseSeasonEpisode() {
	match := reFindLastIndex(seasonEpisodeRe, p.name)
	if match == nil {
		return
	}

	seasonNumber := getGroupFromMatch(seasonEpisodeRe, match, p.name, "season_number")
	if seasonNumber != "" {
		p.info.SeasonNumber = mustAtoi(seasonNumber)
	}

	episodeNumber := getGroupFromMatch(seasonEpisodeRe, match, p.name, "episode_number")
	episodeNumber2 := getGroupFromMatch(seasonEpisodeRe, match, p.name, "episode_number2")
	if episodeNumber2 != "" {
		// episode range, e.g. 01-02, do not support yet
	} else if episodeNumber != "" {
		p.info.EpisodeNumber = mustAtoi(episodeNumber)
	}

	p.name, p.other = p.name[:match[0]], " "+p.name[match[1]:]
}

var qualityRe = regexp.MustCompile(`(?i)(?:\.| )(?P<quality>WEB-?DL|Blu-?Ray[\.\s-]?(?:Remux)?|Remux|WEB-?Rip|BR-?Rip|BD-?Rip)(?:\.| )`)

func (p *parser) parseQuality() {
	match := reFindLastIndex(qualityRe, p.other)
	if match == nil {
		return
	}

	p.info.Quality = getQuality(getGroupFromMatch(qualityRe, match, p.other, "quality"))
	p.other = p.other[:match[0]] + "." + p.other[match[1]:]
}

var hdrRe = regexp.MustCompile(`(?i)(?:\.| )(?P<hdr>HDR(?:10\+?)?|Dolby[ -]?Vision|HLG)(?:\.| )`)

func (p *parser) parseHDR() {
	match := reFindLastIndex(hdrRe, p.other)
	if match == nil {
		return
	}

	p.info.HDR = strings.ReplaceAll(getGroupFromMatch(hdrRe, match, p.other, "hdr"), "-", "")
	p.other = p.other[:match[0]] + "." + p.other[match[1]:]
}

var videoCodecRe = regexp.MustCompile(`(?i)(?:\.| )(?P<video_codec>[hx]\.?26[45]|avc|hevc)(?:\.| )`)

func (p *parser) parseVideoCodec() {
	match := reFindLastIndex(videoCodecRe, p.other)
	if match == nil {
		return
	}

	p.info.VideoCodec = getVideoCodec(getGroupFromMatch(videoCodecRe, match, p.other, "video_codec"))
	p.other = p.other[:match[0]] + "." + p.other[match[1]:]
}

var languageDetector = lingua.NewLanguageDetectorBuilder().
	FromLanguages(lingua.English, lingua.Chinese, lingua.Japanese).
	Build()
var titleRe = regexp.MustCompile(`[\.]`)

func (p *parser) parseTitle() {
	name := titleRe.ReplaceAllLiteralString(p.name, " ")
	for _, result := range languageDetector.DetectMultipleLanguagesOf(name) {
		p.info.Titles = append(p.info.Titles, MediaTitle{
			Language: getLanguage(result.Language()),
			Title:    strings.TrimSpace(name[result.StartIndex():result.EndIndex()]),
		})
	}
}

func mustAtoi(s string) *NullableInt {
	n, err := strconv.Atoi(s)
	if err != nil {
		panic(err)
	}
	i := NullableInt(n)
	return &i
}

func Parse(name string) *MediaInfo {
	return newParser(name, false).parse()
}

func ParseDir(name string) *MediaInfo {
	return newParser(name, true).parse()
}
