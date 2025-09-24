package media

import (
	"regexp"
	"strings"

	"github.com/pemistahl/lingua-go"
)

func normalizeLanguage(l lingua.Language) string {
	switch l {
	case lingua.Chinese:
		return LanguageChinese
	case lingua.Japanese:
		return LanguageJapanese
	case lingua.English:
		return LanguageEnglish
	default:
		return l.String()
	}
}

func normalizeQuality(quality string) string {
	quality = strings.ToLower(strings.ReplaceAll(quality, ".", ""))
	if strings.Contains(quality, "remux") {
		return "Remux"
	}

	switch quality {
	case "web-dl", "webdl":
		return "WEB-DL"
	case "web-rip", "webrip":
		return "WEBRip"
	case "bluray", "blu-ray":
		return "BluRay"
	case "bdrip", "bd-rip":
		return "BDRip"
	case "brrip", "br-rip":
		return "BRRip"
	case "remux":
		return "Remux"
	default:
		return quality
	}
}

func normalizeVideoCodec(codec string) string {
	switch strings.ToLower(codec) {
	case "x264", "h264", "avc":
		return "H264"
	case "x265", "h265", "hevc":
		return "H265"
	default:
		return strings.ToUpper(codec)
	}
}

var audioNormalizeRe = regexp.MustCompile(`\d\.\d`)

func normalizeAudioCodec(codec string) string {
	codec = strings.ToUpper(codec)
	match := audioNormalizeRe.FindStringIndex(codec)
	if match == nil {
		return codec
	}

	part := strings.TrimSpace(strings.ReplaceAll(codec[:match[0]], ".", " "))
	if part == "TRUEHD" {
		part = "TrueHD"
	}
	return part + " " + codec[match[0]:]
}

func normalizeHDR(hdr string) string {
	hdr = strings.ToUpper(strings.ReplaceAll(hdr, "-", ""))
	switch {
	case strings.Contains(hdr, "DOLBY") || hdr == "DOVI":
		return "DV"
	default:
		return hdr
	}
}
