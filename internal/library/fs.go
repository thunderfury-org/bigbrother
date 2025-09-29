package library

import (
	"fmt"
	"io"
	"net/http"
	"os"
	"path/filepath"

	"github.com/thunderfury-org/bigbrother/internal/openlist"
)

func downloadFile(c *openlist.Client, path string, localPath string) error {
	f, err := c.GetFile(path)
	if err != nil {
		return err
	}

	resp, err := http.DefaultClient.Get(f.RawURL)
	if err != nil {
		return err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("failed to download file, status: %s", resp.Status)
	}

	fp, err := openLocalFile(localPath)
	if err != nil {
		return err
	}
	defer fp.Close()

	_, err = io.Copy(fp, resp.Body)
	return err
}

func openLocalFile(path string) (*os.File, error) {
	// create dir if not exists
	err := os.MkdirAll(filepath.Dir(path), 0755)
	if err != nil {
		return nil, err
	}
	return os.OpenFile(path, os.O_CREATE|os.O_RDWR, 0644)
}
