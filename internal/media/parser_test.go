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
	// Load test cases from parser.yaml
	data, err := os.ReadFile(filepath.Join("testdata", "parser.yaml"))
	if err != nil {
		t.Fatalf("Failed to read test data: %v", err)
	}

	var testCases []TestCase
	err = yaml.Unmarshal(data, &testCases)
	if err != nil {
		t.Fatalf("Failed to parse test data: %v", err)
	}

	for _, tc := range testCases {
		actual := Parse(tc.Input)
		if !reflect.DeepEqual(actual, tc.Expected) {
			t.Errorf("Parse(%q) = %v, want %v", tc.Input, actual, tc.Expected)
		}
	}
}
