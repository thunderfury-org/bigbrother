package config

import (
	"os"
	"path"

	"go.yaml.in/yaml/v3"
)

type Config struct {
	OpenList  OpenListConfig   `yaml:"openlist"`
	Tmdb      TmdbConfig       `yaml:"tmdb"`
	Libraries []*LibraryConfig `yaml:"libraries"`
}

type OpenListConfig struct {
	BaseURL string `yaml:"base_url"`
	Token   string `yaml:"token"`
}

type TmdbConfig struct {
	ApiKey string `yaml:"api_key"`
}

type LibraryConfig struct {
	Name string `yaml:"name"`

	// Library path in openlist
	//
	// Must not be the same as other library path and must not be a subdirectory of other library path
	Path string `yaml:"path"`

	// Path in openlist to monitor for new files
	//
	// Will be renamed to Path after processing automatically.
	//
	// Must not be the same as Path and must not be a subdirectory of Path.
	MonitorPath string `yaml:"monitor_path"`

	// Local storage path to generate strm files
	LocalPath string `yaml:"local_path"`
}

func Load(configDir string) (*Config, error) {
	if configDir != "" {
		err := os.MkdirAll(configDir, 0755)
		if err != nil {
			return nil, err
		}
	}

	path := path.Join(configDir, "config.yaml")
	if _, err := os.Stat(path); os.IsNotExist(err) {
		// Create empty config file, if not exist
		err = os.WriteFile(path, []byte{}, 0644)
		if err != nil {
			return nil, err
		}
	}

	data, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}

	var config Config
	if err := yaml.Unmarshal(data, &config); err != nil {
		return nil, err
	}

	return &config, nil
}
