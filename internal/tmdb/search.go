package tmdb

import (
	"net/url"
	"strconv"
)

type SearchMovieResult struct {
	ID               int64  `json:"id"`
	Title            string `json:"title"`
	OriginalTitle    string `json:"original_title"`
	OriginalLanguage string `json:"original_language"`
	ReleaseDate      string `json:"release_date"`
}

type searchMovieResponse struct {
	Page         int                  `json:"page"`
	Results      []*SearchMovieResult `json:"results"`
	TotalPages   int                  `json:"total_pages"`
	TotalResults int                  `json:"total_results"`
}

func (c *Client) SearchMovie(query string, year string) ([]*SearchMovieResult, error) {
	params := url.Values{}
	params.Add("query", query)
	params.Add("primary_release_year", year)

	var result searchMovieResponse
	if err := c.get("/3/search/movie", params, &result); err != nil {
		return nil, err
	}
	return result.Results, nil
}

type SearchTVResult struct {
	ID               int64  `json:"id"`
	Name             string `json:"name"`
	OriginalName     string `json:"original_name"`
	OriginalLanguage string `json:"original_language"`
	FirstAirDate     string `json:"first_air_date"`
}

type searchTVResponse struct {
	Page         int               `json:"page"`
	Results      []*SearchTVResult `json:"results"`
	TotalPages   int               `json:"total_pages"`
	TotalResults int               `json:"total_results"`
}

func (c *Client) SearchTV(query string, year int) ([]*SearchTVResult, error) {
	params := url.Values{}
	params.Add("query", query)
	params.Add("first_air_date_year", strconv.Itoa(year))

	var result searchTVResponse
	if err := c.get("/3/search/tv", params, &result); err != nil {
		return nil, err
	}
	return result.Results, nil
}
