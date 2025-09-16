package media

import (
	"strings"

	"github.com/pemistahl/lingua-go"
)

func getLanguage(l lingua.Language) string {
	switch l {
	case lingua.Chinese:
		return LanguageChinese
	case lingua.Japanese:
		return LanguageJapanese
	case lingua.English:
		return LanguageEnglish
	default:
		return ""
	}
}

func getQuality(quality string) string {
	quality = strings.ToLower(quality)
	if strings.Contains(quality, "remux") {
		return "Remux"
	}

	switch quality {
	case "web-dl", "webdl":
		return "WEB-DL"
	case "web-rip", "webrip":
		return "WEB-Rip"
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

func getVideoCodec(codec string) string {
	switch strings.ToLower(codec) {
	case "x264", "h264", "avc":
		return "H.264"
	case "x265", "h265", "hevc":
		return "H.265"
	default:
		return strings.ToUpper(codec)
	}
}
