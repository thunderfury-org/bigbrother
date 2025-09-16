package media

import (
	"os"
	"path/filepath"
	"reflect"
	"testing"

	"github.com/goccy/go-yaml"
)

type TestCase struct {
	Input    string     `yaml:"input"`
	Expected *MediaInfo `yaml:"expected"`
}

func TestParse(t *testing.T) {
	filenames := []string{"tv_episode.yaml", "tv_season_episode.yaml", "movie.yaml", "anime.yaml"}
	for _, filename := range filenames {
		testParse(t, filename, false)
	}
}

func TestParseDir(t *testing.T) {
	filenames := []string{"dir.yaml"}
	for _, filename := range filenames {
		testParse(t, filename, true)
	}
}

func testParse(t *testing.T, filename string, isDir bool) {
	data, err := os.ReadFile(filepath.Join("testdata", filename))
	if err != nil {
		t.Fatalf("Failed to read test data: %v", err)
	}

	var testCases []TestCase
	err = yaml.Unmarshal(data, &testCases)
	if err != nil {
		t.Fatalf("Failed to parse test data: %v", err)
	}

	for _, tc := range testCases {
		var actual *MediaInfo
		if isDir {
			actual = ParseDir(tc.Input)
		} else {
			actual = Parse(tc.Input)
		}

		if !reflect.DeepEqual(actual, tc.Expected) {
			t.Errorf("Parse(%q)\n got  %v\n want %v", tc.Input, actual, tc.Expected)
		}
	}
}
