package media

import "regexp"

type reFindResutlt struct {
	start  int
	end    int
	groups map[string]string
}

func convertResults(r *regexp.Regexp, matchIndices []int, s string) *reFindResutlt {
	result := &reFindResutlt{
		start:  matchIndices[0],
		end:    matchIndices[1],
		groups: make(map[string]string),
	}

	subexpNames := r.SubexpNames()
	for i, name := range subexpNames {
		if i == 0 || name == "" {
			continue
		}
		startPos := matchIndices[2*i]
		endPos := matchIndices[2*i+1]
		if startPos != -1 && endPos != -1 {
			result.groups[name] = s[startPos:endPos]
		}
	}

	return result
}

func reFind(r *regexp.Regexp, s string) *reFindResutlt {
	matchIndices := r.FindStringSubmatchIndex(s)
	if matchIndices == nil {
		return nil
	}

	return convertResults(r, matchIndices, s)
}

func reFindLast(r *regexp.Regexp, s string) *reFindResutlt {
	// Find all matches in the string
	allMatches := r.FindAllStringSubmatchIndex(s, -1)
	if len(allMatches) == 0 {
		return nil
	}

	// Select the last match (closest to the end of the string)
	matchIndices := allMatches[len(allMatches)-1]

	return convertResults(r, matchIndices, s)
}
