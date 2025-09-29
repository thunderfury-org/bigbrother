package library

import "strings"

const (
	genreAnimation   = 16    // 动漫
	genreDocumentary = 99    // 记录
	genreReality     = 10764 // 真人秀
	genreTalk        = 10767 // 脱口秀
)

var subCatalog = map[string]string{}

func init() {
	m := map[string][]string{
		"国产": {"CN", "TW", "HK"},
		"日韩": {"JP", "KP", "KR", "TH", "IN", "SG"},
		"欧美": {"US", "FR", "GB", "DE", "ES", "IT", "NL", "PT", "RU", "UK"},
	}

	for k, v := range m {
		for _, c := range v {
			subCatalog[c] = k
		}
	}
}

func getCatalog(f *mediaFile) string {
	if f.Info.EpisodeNumber.IsNull() {
		return "电影"
	}
	for _, g := range f.Genres {
		switch g.ID {
		case genreAnimation:
			return "动漫"
		case genreDocumentary:
			return "纪录片"
		case genreReality:
		case genreTalk:
			return "综艺"
		}
	}
	return "电视剧"
}

func getSubCatalog(f *mediaFile) string {
	for _, c := range f.OriginCountry {
		c = strings.ToUpper(c)
		if k, ok := subCatalog[c]; ok {
			return k
		}
	}
	return "其它"
}
