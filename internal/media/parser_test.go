package media_test

import (
	"os"
	"path/filepath"
	"reflect"
	"testing"

	"github.com/thunderfury-org/bigbrother/internal/media"

	"go.yaml.in/yaml/v3"
)

type TestCase struct {
	Input    string           `yaml:"input"`
	Expected *media.MediaInfo `yaml:"expected"`
}

func TestParse(t *testing.T) {
	filenames := []string{"dir.yaml", "tv_episode.yaml", "tv_season_episode.yaml", "movie.yaml", "anime.yaml"}
	for _, filename := range filenames {
		testParse(t, filename)
	}
}

func testParse(t *testing.T, filename string) {
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
		actual := media.Parse(tc.Input)
		if !reflect.DeepEqual(actual, tc.Expected) {
			t.Errorf("Parse(%q)\n got  %v\n want %v", tc.Input, actual, tc.Expected)
		}
	}
}
