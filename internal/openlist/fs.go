package openlist

type ListFilesRequest struct {
	Path    string `json:"path"`
	Refresh bool   `json:"refresh"`
	Page    int    `json:"page"`
	PerPage int    `json:"per_page"`
}

type ListFilesResponse struct {
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
	Modified string            `json:"modified"`
	Created  string            `json:"created"`
	Sign     string            `json:"sign"`
	Thumb    string            `json:"thumb"`
	Type     int               `json:"type"`
	HashInfo map[string]string `json:"hash_info"`
}

func (c *Client) ListFiles(path string, refresh bool) ([]*File, error) {
	request := ListFilesRequest{
		Path:    path,
		Refresh: refresh,
		Page:    1,
		PerPage: 0,
	}
	var result ListFilesResponse
	if err := c.post("/api/fs/list", request, &result); err != nil {
		return nil, err
	}
	return result.Content, nil
}
