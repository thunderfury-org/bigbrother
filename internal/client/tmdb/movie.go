package tmdb

import (
	"fmt"
)

type MovieDetail struct {
	ID               int64    `json:"id"`
	Title            string   `json:"title"`
	Adult            bool     `json:"adult"`
	Genres           []Genre  `json:"genres"`
	OriginalLanguage string   `json:"original_language"`
	OriginalTitle    string   `json:"original_title"`
	Overview         string   `json:"overview"`
	OriginCountry    []string `json:"origin_country"`
	ReleaseDate      string   `json:"release_date"`
}

func (c *Client) GetMovieDetail(id int64) (*MovieDetail, error) {
	var detail MovieDetail
	if err := c.get(fmt.Sprintf("/3/movie/%d", id), nil, &detail); err != nil {
		return nil, err
	}
	return &detail, nil
}
