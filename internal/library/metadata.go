package library

import (
	"fmt"

	"github.com/thunderfury-org/bigbrother/internal/media"
	"github.com/thunderfury-org/bigbrother/internal/tmdb"
)

type metadataFetcher struct {
	tmdb *tmdb.Client
}

func (f *metadataFetcher) SearchMovie(titles []media.MediaTitle, year int) (*tmdb.MovieDetail, error) {
	if len(titles) == 0 {
		return nil, fmt.Errorf("no title to search")
	}

	movies, err := f.tmdb.SearchMovie(titles[0].Title, year)
	if err != nil {
		return nil, err
	}

	switch len(movies) {
	case 0:
		return nil, nil
	case 1:
		return f.tmdb.GetMovieDetail(movies[0].ID)
	default:
		for _, title := range titles {
			for _, m := range movies {
				if title.Title == m.Title || title.Title == m.OriginalTitle {
					return f.tmdb.GetMovieDetail(m.ID)
				}
			}
		}
	}

	return nil, nil
}

func (f *metadataFetcher) SearchTV(titles []media.MediaTitle, year int) (*tmdb.TVDetail, error) {
	if len(titles) == 0 {
		return nil, fmt.Errorf("no title to search")
	}

	tvs, err := f.tmdb.SearchTV(titles[0].Title, year)
	if err != nil {
		return nil, err
	}

	switch len(tvs) {
	case 0:
		return nil, nil
	case 1:
		return f.tmdb.GetTVDetail(tvs[0].ID)
	default:
		for _, title := range titles {
			for _, m := range tvs {
				if title.Title == m.Name || title.Title == m.OriginalName {
					return f.tmdb.GetTVDetail(m.ID)
				}
			}
		}
	}

	return nil, nil
}
