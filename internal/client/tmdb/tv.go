package tmdb

import "fmt"

type TVDetail struct {
	ID               int64     `json:"id"`
	Name             string    `json:"name"`
	FirstAirDate     string    `json:"first_air_date"`
	Genres           []Genre   `json:"genres"`
	NumberOfEpisodes int       `json:"number_of_episodes"`
	NumberOfSeasons  int       `json:"number_of_seasons"`
	OriginCountry    []string  `json:"origin_country"`
	OriginalLanguage string    `json:"original_language"`
	OriginalName     string    `json:"original_name"`
	Seasons          []*Season `json:"seasons"`
}

type Season struct {
	ID           int64  `json:"id"`
	Name         string `json:"name"`
	EpisodeCount int    `json:"episode_count"`
	AirDate      string `json:"air_date"`
	SeasonNumber int    `json:"season_number"`
}

func (c *Client) GetTVDetail(id int64) (*TVDetail, error) {
	var detail TVDetail
	if err := c.get(fmt.Sprintf("/3/tv/%d", id), nil, &detail); err != nil {
		return nil, err
	}
	return &detail, nil
}
