package openlist

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"strings"
)

var (
	ErrNotFound = fmt.Errorf("object not found")
)

type Client struct {
	baseURL string
	token   string
}

func NewClient(baseURL, token string) *Client {
	return &Client{
		baseURL: baseURL,
		token:   token,
	}
}

type apiResponse struct {
	Code    int             `json:"code"`
	Message string          `json:"message"`
	Data    json.RawMessage `json:"data"`
}

func (c *Client) GetBaseURL() string {
	return c.baseURL
}

func (c *Client) post(path string, payload any, respPayload any) error {
	body, err := json.Marshal(payload)
	if err != nil {
		return fmt.Errorf("failed to marshal request payload: %w", err)
	}

	url := fmt.Sprintf("%s%s", c.baseURL, path)
	req, err := http.NewRequest(http.MethodPost, url, bytes.NewBuffer(body))
	if err != nil {
		return fmt.Errorf("failed to create request: %w", err)
	}

	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Authorization", c.token)

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

	var result apiResponse
	if err := json.Unmarshal(respBody, &result); err != nil {
		return fmt.Errorf("failed to decode response: %w, payload: %s", err, string(respBody))
	}

	if result.Code != 200 {
		if strings.Contains(result.Message, "object not found") {
			return ErrNotFound
		}
		return fmt.Errorf("http request failed, url %s, msg: %s", url, result.Message)
	}

	if respPayload != nil {
		if err := json.Unmarshal(result.Data, respPayload); err != nil {
			return fmt.Errorf("failed to decode response payload: %w, payload: %s", err, string(result.Data))
		}
	}
	return nil
}
