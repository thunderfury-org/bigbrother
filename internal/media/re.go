package media

import "regexp"

func getGroupFromMatch(r *regexp.Regexp, matchIndices []int, s string, group string) string {
	idx := r.SubexpIndex(group)
	if idx == -1 {
		return ""
	}
	startPos := matchIndices[2*idx]
	endPos := matchIndices[2*idx+1]
	if startPos != -1 && endPos != -1 {
		return s[startPos:endPos]
	}
	return ""
}

func reFindLastIndex(r *regexp.Regexp, s string) []int {
	// Find all matches in the string
	allMatches := r.FindAllStringSubmatchIndex(s, -1)
	if len(allMatches) == 0 {
		return nil
	}

	// Select the last match (closest to the end of the string)
	return allMatches[len(allMatches)-1]
}
