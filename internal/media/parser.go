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

	titleIndexEnd  int
	yearIndexStart int

	info *MediaInfo
}

func newParser(name string) *parser {
	return &parser{
		name:           strings.TrimSpace(name),
		titleIndexEnd:  -1,
		yearIndexStart: -1,
		info:           &MediaInfo{},
	}
}

func (p *parser) updateTitleIndexEnd(index int) {
	if p.titleIndexEnd < 0 {
		p.titleIndexEnd = index
	} else if index < p.titleIndexEnd {
		p.titleIndexEnd = index
	}
}

func (p *parser) updateNameAndIndex(match []int) {
	p.name = p.name[:match[0]+1] + p.name[match[1]-1:]
	p.updateTitleIndexEnd(match[0] + 1)
}

func (p *parser) parseValueFromName(re *regexp.Regexp, normalizer ...func(string) string) string {
	match := reFindLastIndex(re, p.name)
	if match == nil {
		return ""
	}

	value := getGroupFromMatch(tmdbRe, match, p.name, "value")
	p.updateNameAndIndex(match)

	for _, fn := range normalizer {
		value = fn(value)
	}
	return value
}

var (
	reBegin      = `(?i)[\. \-\[\{\(@]\s*`
	reEnd        = `\s*[\. \-\]\}\)@]`
	tmdbRe       = regexp.MustCompile(reBegin + `tmdb(?:id)?[-=](?P<value>\d+)` + reEnd)
	frameRateRe  = regexp.MustCompile(reBegin + `(?P<value>\d{2,3}fps)` + reEnd)
	qualityRe    = regexp.MustCompile(reBegin + `(?P<value>WEB-?DL|Blu-?Ray[\.\s-]?(?:Remux)?|Remux|WEB-?Rip|BR-?Rip|BD-?Rip)` + reEnd)
	hdrRe        = regexp.MustCompile(reBegin + `(?P<value>HDR(10\+?)?|Dolby[ -]?Vision|HLG|DV|DoVi)` + reEnd)
	videoCodecRe = regexp.MustCompile(reBegin + `(?P<value>[HX]\.?26[45]|AVC|HEVC|AV1|VP-9)` + reEnd)
	audioCodecRe = regexp.MustCompile(reBegin + `(?P<value>AAC([.\s]?\d\.\d)?|FLAC|Dolby[\.\s]?Digital|DDP?([\s\.]Atmos)?(?:[\s\.]?\d\.\d)?|DTS-HD[\. ](?:MA)?[\. ]?(?:DD[\. ]?)?\d\.\d|(?:TrueHD|DTS)[\. ]?\d\.\d)` + reEnd)
)

func (p *parser) parse() *MediaInfo {
	p.normalizeName()

	p.info.TmdbID = p.parseValueFromName(tmdbRe)
	p.info.FrameRate = p.parseValueFromName(frameRateRe, strings.ToLower)
	p.info.Quality = p.parseValueFromName(qualityRe, normalizeQuality)
	p.info.HDR = p.parseValueFromName(hdrRe, normalizeHDR)
	p.info.VideoCodec = p.parseValueFromName(videoCodecRe, normalizeVideoCodec)
	p.info.AudioCodec = p.parseValueFromName(audioCodecRe, normalizeAudioCodec)

	p.parseResolution()
	p.parseYear()
	p.parseSeasonEpisode()
	p.parseFileType()
	p.parseTitle()
	p.parseSubtitles()
	p.parseReleaseGroup()
	return p.info
}

var replaceRes = []*regexp.Regexp{
	regexp.MustCompile(`[_（）《》]`),
	regexp.MustCompile(`[\[★](\S{1,4}年)?\S{1,2}月新番[\]★]`),
	regexp.MustCompile(`(?i)\[\d+(\.\d+)G?\]`),
}

func (p *parser) normalizeName() {
	p.name = strings.ReplaceAll(p.name, "【", "[")
	p.name = strings.ReplaceAll(p.name, "】", "]")
	p.name = strings.ReplaceAll(p.name, "精校", ".")

	for _, re := range replaceRes {
		p.name = re.ReplaceAllLiteralString(p.name, ".")
	}
	p.name = " " + p.name + " "
}

var resolutionRe = regexp.MustCompile(reBegin + `((\d{3,4}x(?P<height>\d{3,4}))|(?P<resolution>\d{1,4}[pk]))` + reEnd)

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

	p.updateNameAndIndex(match)
}

var yearRe = regexp.MustCompile(reBegin + `(?P<year>19\d{2}|20\d{2})` + reEnd)

func (p *parser) parseYear() {
	match := reFindLastIndex(yearRe, p.name)
	if match == nil {
		return
	}

	if p.name[match[1]-1] != ')' {
		// matched .year. but maybe it's part of the title, e.g. "Movie.Title.2020.2021"
		// try to match another year
		newName := p.name[match[1]-1:]
		match2 := reFindLastIndex(yearRe, newName)
		if match2 != nil {
			p.info.Year = getGroupFromMatch(yearRe, match2, newName, "year")
			p.name = p.name[:match[1]] + "." + newName[match2[1]:]
			return
		}

		// not matched, keep the original match
	}

	p.info.Year = getGroupFromMatch(yearRe, match, p.name, "year")
	p.updateNameAndIndex(match)
	p.yearIndexStart = match[0]
}

var seasonEpisodeRe = regexp.MustCompile(`(?i)([\.\s\[]S(?:eason)?\s*(?P<season_number>\d{1,2})\s*\]?\s*)([E#-\[]\s*(?P<episode_number>\d{1,4})(-(?P<episode_number2>\d{1,4}))?)?`)
var episodeOnlyRe = regexp.MustCompile(reBegin + `[#第E]?\s*(?P<episode_number>\d{1,4})(-(?P<episode_number2>\d{1,4}))?\s*[集]?` + reEnd)

func (p *parser) parseSeasonEpisode() {
	re := seasonEpisodeRe
	match := reFindLastIndex(re, p.name)
	if match == nil {
		// not found season/episode info like S01E01
		// try match only episode info like 01 or - 01 or #01
		re = episodeOnlyRe
		match = reFindLastIndex(re, p.name)
		if match == nil || match[0] < p.yearIndexStart {
			// season/episode info not found
			// or episode is before year, maybe it's title info
			if p.titleIndexEnd < 0 {
				return
			}

			// try to split name and other info by title index
			p.name, p.other = p.name[:p.titleIndexEnd], p.name[p.titleIndexEnd:]
			return
		}
	}

	seasonNumber := getGroupFromMatch(re, match, p.name, "season_number")
	if seasonNumber != "" {
		p.info.SeasonNumber = mustAtoi(seasonNumber)
	}

	episodeNumber := getGroupFromMatch(re, match, p.name, "episode_number")
	if episodeNumber != "" {
		p.info.EpisodeNumber = mustAtoi(episodeNumber)
	}
	episodeNumber2 := getGroupFromMatch(re, match, p.name, "episode_number2")
	if episodeNumber2 != "" {
		p.info.SecondEpisodeNumber = mustAtoi(episodeNumber2)
	}

	p.name, p.other = p.name[:match[0]], "."+p.name[match[1]:]
}

func (p *parser) parseFileType() {
	// find from the last dot
	dotIndex := strings.LastIndex(p.other, ".")
	if dotIndex < 0 {
		// no extension
		return
	}

	extension := strings.TrimSpace(strings.ToLower(p.other[dotIndex:]))
	if len(extension) <= 1 {
		// no extension
		return
	}

	if _, ok := videoExtensions[extension]; ok {
		p.info.FileType = FileTypeVideo
	} else if _, ok := subtitleExtensions[extension]; ok {
		p.info.FileType = FileTypeSubtitle
	} else {
		// unknown file type
		return
	}
	p.info.Extension = extension

	p.other = p.other[:dotIndex]
}

var languageDetector = lingua.NewLanguageDetectorBuilder().
	FromLanguages(lingua.English, lingua.Chinese, lingua.Japanese).
	Build()
var titleRes = []struct {
	re *regexp.Regexp
	to string
}{
	{regexp.MustCompile(`[\.\[\]\{\}\(\)]`), " "},
	{regexp.MustCompile(`第[^\.\[\]]+季`), ""},
}
var digitRe = regexp.MustCompile(`^\d+$`)

func (p *parser) parseTitle() {
	index := strings.Index(p.name, "]")
	if index >= 0 {
		// remove [group] at the start of the name
		name := p.name[index+1:]
		for _, r := range titleRes {
			name = r.re.ReplaceAllString(name, r.to)
		}

		left := strings.TrimSpace(name)
		if left != "" {
			p.info.ReleaseGroup = strings.TrimSpace(strings.ReplaceAll(p.name[:index], "[", ""))
			p.name = left
		}
	}

	for _, r := range titleRes {
		p.name = r.re.ReplaceAllString(p.name, r.to)
	}

	for part := range strings.SplitSeq(p.name, "/") {
		part = strings.TrimSpace(part)
		if part == "" {
			continue
		}
		if digitRe.MatchString(part) {
			p.info.Titles = append(p.info.Titles, MediaTitle{
				Language: LanguageEnglish,
				Title:    part,
			})
			continue
		}

		for _, result := range languageDetector.DetectMultipleLanguagesOf(part) {
			p.info.Titles = append(p.info.Titles, MediaTitle{
				Language: normalizeLanguage(result.Language()),
				Title:    strings.TrimSpace(part[result.StartIndex():result.EndIndex()]),
			})
		}
	}
}

var subtitles = []struct {
	lang     string
	keywords []string
}{
	{LanguageChineseSimplified, []string{"简", "chs", "gb", "zh-hans"}},
	{LanguageChineseTraditional, []string{"繁", "cht", "big5", "zh-hant"}},
}

func (p *parser) parseSubtitles() {
	if len(p.other) == 0 {
		return
	}

	name := strings.ToLower(p.other)
	for _, subtitle := range subtitles {
		for _, keyword := range subtitle.keywords {
			if strings.Contains(name, keyword) {
				p.info.Subtitles = append(p.info.Subtitles, subtitle.lang)
				break
			}
		}
	}
}

var releaseGroupRe = regexp.MustCompile(`\[\s*(?P<value>[^\[\]]+)\s*\]`)

func (p *parser) parseReleaseGroup() {
	if len(p.info.ReleaseGroup) > 0 {
		return
	}

	match := reFindLastIndex(releaseGroupRe, p.other)
	if match != nil {
		value := strings.TrimSpace(getGroupFromMatch(releaseGroupRe, match, p.other, "value"))
		if strings.Contains(value, "-") {
			p.info.ReleaseGroup = value
			return
		}
	}

	index := strings.LastIndex(p.other, "-")
	if index < 0 {
		return
	}
	p.info.ReleaseGroup = strings.TrimSpace(p.other[index+1:])
}

func mustAtoi(s string) *NullableInt {
	n, err := strconv.Atoi(s)
	if err != nil {
		return nil
	}
	i := NullableInt(n)
	return &i
}

func Parse(name string) *MediaInfo {
	return newParser(name).parse()
}
