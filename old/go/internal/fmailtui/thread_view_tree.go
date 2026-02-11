package fmailtui

import (
	"sort"
	"strings"

	"github.com/tOgg1/forge/internal/fmailtui/threading"
)

// prefixForNode builds a connector prefix using box-drawing characters, clamped to maxDepth.
// Returns true if the node depth exceeds maxDepth (meaning we should show an overflow indicator).
func prefixForNode(node *threading.ThreadNode, maxDepth int) (string, bool) {
	if node == nil || node.Message == nil || node.Parent == nil {
		return "", false
	}

	// Build path root..node for stable connector rendering.
	path := make([]*threading.ThreadNode, 0, 8)
	for cur := node; cur != nil; cur = cur.Parent {
		path = append(path, cur)
	}
	for i, j := 0, len(path)-1; i < j; i, j = i+1, j-1 {
		path[i], path[j] = path[j], path[i]
	}

	depth := len(path) - 1
	if depth <= 0 {
		return "", false
	}

	clamped := maxDepth > 0 && depth > maxDepth
	visibleDepth := depth
	if maxDepth > 0 && visibleDepth > maxDepth {
		visibleDepth = maxDepth
	}
	start := depth - visibleDepth

	hasNextSibling := func(parent, child *threading.ThreadNode) bool {
		if parent == nil || child == nil || parent.Message == nil || child.Message == nil {
			return false
		}
		siblings := sortedChildren(parent.Children)
		if len(siblings) == 0 {
			return false
		}
		last := siblings[len(siblings)-1]
		if last == nil || last.Message == nil {
			return false
		}
		return last.Message.ID != child.Message.ID
	}

	parts := make([]string, 0, visibleDepth)
	for i := 0; i < visibleDepth; i++ {
		parent := path[start+i]
		child := path[start+i+1]
		if i == visibleDepth-1 {
			if hasNextSibling(parent, child) {
				parts = append(parts, "├─ ")
			} else {
				parts = append(parts, "└─ ")
			}
			continue
		}
		if hasNextSibling(parent, child) {
			parts = append(parts, "│  ")
		} else {
			parts = append(parts, "   ")
		}
	}

	return strings.Join(parts, ""), clamped
}

func sortedChildren(children []*threading.ThreadNode) []*threading.ThreadNode {
	cloned := append([]*threading.ThreadNode(nil), children...)
	sort.SliceStable(cloned, func(i, j int) bool {
		if cloned[i] == nil || cloned[i].Message == nil {
			return false
		}
		if cloned[j] == nil || cloned[j].Message == nil {
			return true
		}
		left := *cloned[i].Message
		right := *cloned[j].Message
		if !left.Time.Equal(right.Time) {
			return left.Time.Before(right.Time)
		}
		return left.ID < right.ID
	})
	return cloned
}
