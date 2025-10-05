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
	codec = strings.ToUpper(codec)
	switch codec {
	case "X264", "H.264", "AVC":
		return "H264"
	case "X265", "H.265", "HEVC":
		return "H265"
	default:
		return codec
	}
}

var audioNormalizeRe = regexp.MustCompile(`\d\.\d`)

func normalizeAudioCodec(codec string) string {
	codec = strings.ToUpper(codec)
	match := audioNormalizeRe.FindStringIndex(codec)
	if match == nil {
		return parseAudioCodec(codec)
	}

	parts := []string{
		parseAudioCodec(codec[:match[0]]),
		codec[match[0]:match[1]],
	}
	left := parseAudioCodec(codec[match[1]:])
	if left != "" {
		parts = append(parts, left)
	}

	return strings.Join(parts, ".")
}

func parseAudioCodec(codec string) string {
	if codec == "" {
		return codec
	}

	parts := []string{}
	val := strings.ReplaceAll(codec, " ", ".")
	for _, p := range strings.Split(val, ".") {
		p = strings.TrimSpace(p)
		if p == "" {
			continue
		}

		switch p {
		case "TRUEHD":
			p = "TrueHD"
		case "ATMOS":
			p = "Atmos"
		case "DTSHD":
			p = "DTS-HD"
		}
		parts = append(parts, p)
	}

	return strings.Join(parts, ".")
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
