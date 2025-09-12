package media

import "regexp"

type reFindResutlt struct {
	start  int
	end    int
	groups map[string]string
}

func reFind(r *regexp.Regexp, s string) *reFindResutlt {
	matchIndices := r.FindStringSubmatchIndex(s)
	if matchIndices == nil {
		return nil
	}

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
