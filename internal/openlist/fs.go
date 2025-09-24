package openlist

import "time"

type listFilesRequest struct {
	Path    string `json:"path"`
	Refresh bool   `json:"refresh"`
	Page    int    `json:"page"`
	PerPage int    `json:"per_page"`
}

type listFilesResponse struct {
	Content []*File `json:"content"`
	Total   int     `json:"total"`
	Readme  string  `json:"readme"` // 说明
	Header  string  `json:"header"`
	Write   bool    `json:"write"` // 是否可写
}

type File struct {
	Id       string            `json:"id"`
	Name     string            `json:"name"`
	Size     int64             `json:"size"`
	IsDir    bool              `json:"is_dir"`
	Modified time.Time         `json:"modified"`
	Created  time.Time         `json:"created"`
	Sign     string            `json:"sign"`
	Thumb    string            `json:"thumb"`
	Type     int               `json:"type"`
	HashInfo map[string]string `json:"hash_info"`
}

func (c *Client) ListFiles(path string, refresh bool) ([]*File, error) {
	request := listFilesRequest{
		Path:    path,
		Refresh: refresh,
		Page:    1,
		PerPage: 0,
	}
	var result listFilesResponse
	if err := c.post("/api/fs/list", request, &result); err != nil {
		return nil, err
	}
	return result.Content, nil
}

type RenameObject struct {
	SrcName string `json:"src_name"`
	NewName string `json:"new_name"`
}

type batchRenameRequest struct {
	SrcDir        string          `json:"src_dir"`
	RenameObjects []*RenameObject `json:"rename_objects"`
}

func (c *Client) BatchRename(path string, objects []*RenameObject) error {
	if len(objects) == 0 {
		return nil
	}

	request := batchRenameRequest{
		SrcDir:        path,
		RenameObjects: objects,
	}
	return c.post("/api/fs/batch_rename", request, nil)
}

type batchMoveRequest struct {
	SrcDir string   `json:"src_dir"`
	DstDir string   `json:"dst_dir"`
	Names  []string `json:"names"`
}

func (c *Client) BatchMove(srcDir string, dstDir string, names []string) error {
	if len(names) == 0 {
		return nil
	}

	request := batchMoveRequest{
		SrcDir: srcDir,
		DstDir: dstDir,
		Names:  names,
	}
	return c.post("/api/fs/move", request, nil)
}
