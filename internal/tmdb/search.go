package tmdb

import (
	"net/url"
)

type SearchMovieResult struct {
	ID               int64   `json:"id"`
	Title            string  `json:"title"`
	OriginalTitle    string  `json:"original_title"`
	OriginalLanguage string  `json:"original_language"`
	Overview         string  `json:"overview"`
	ReleaseDate      string  `json:"release_date"`
	PosterPath       string  `json:"poster_path"`
	BackdropPath     string  `json:"backdrop_path"`
	GenreIDs         []int64 `json:"genre_ids"`
	Adult            bool    `json:"adult"`
}

type searchMovieResponse struct {
	Page         int                  `json:"page"`
	Results      []*SearchMovieResult `json:"results"`
	TotalPages   int64                `json:"total_pages"`
	TotalResults int64                `json:"total_results"`
}

func (c *Client) SearchMovie(query string, year string) ([]*SearchMovieResult, error) {
	params := url.Values{}
	params.Add("query", query)
	params.Add("year", year)

	var result searchMovieResponse
	if err := c.get("/3/search/movie", params, &result); err != nil {
		return nil, err
	}
	return result.Results, nil
}
