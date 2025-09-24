package tmdb

import (
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
)

const defaultBaseURL = "https://api.themoviedb.org"

type Client struct {
	apiKey string
}

func NewClient(apiKey string) *Client {
	return &Client{apiKey: apiKey}
}

func (c *Client) get(path string, params url.Values, respPayload any) error {
	params.Add("language", "zh-CN")
	params.Add("include_adult", "true")
	params.Add("api_key", c.apiKey)

	url := fmt.Sprintf("%s%s?%s", defaultBaseURL, path, params.Encode())
	req, err := http.NewRequest(http.MethodGet, url, nil)
	if err != nil {
		return fmt.Errorf("failed to create request: %w", err)
	}

	req.Header.Add("accept", "application/json")

	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		return fmt.Errorf("failed to send request: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("http request failed, url: %s, status code: %d", url, resp.StatusCode)
	}

	respBody, err := io.ReadAll(resp.Body)
	if err != nil {
		return fmt.Errorf("failed to read response body: %w", err)
	}

	if respPayload != nil {
		if err := json.Unmarshal(respBody, respPayload); err != nil {
			return fmt.Errorf("failed to decode response payload: %w, payload: %s", err, string(respBody))
		}
	}
	return nil
}
