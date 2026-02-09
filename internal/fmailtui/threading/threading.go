package threading

import (
	"sort"
	"strings"
	"time"

	"github.com/tOgg1/forge/internal/fmail"
)

const maxDisplayDepth = 10

type Thread struct {
	Root         *fmail.Message // first message (or the one with no reply_to in the chain)
	Messages     []*ThreadNode  // all messages in chronological order
	Depth        int            // max nesting depth (clamped)
	Agents       []string       // unique participating agents
	LastActivity time.Time      // most recent message timestamp
}

type ThreadNode struct {
	Message  *fmail.Message
	Parent   *ThreadNode
	Children []*ThreadNode
	Depth    int // nesting level (0 = root, clamped)
}

// BuildThreads takes a flat list of messages and returns threaded conversations.
func BuildThreads(messages []fmail.Message) []*Thread {
	nodes := indexNodes(messages)
	linkParents(nodes)

	roots := make([]*ThreadNode, 0)
	for _, node := range nodes {
		if node.Parent == nil {
			roots = append(roots, node)
		}
	}

	// Root sort: earliest activity first (stable).
	sort.SliceStable(roots, func(i, j int) bool {
		return messageLess(*roots[i].Message, *roots[j].Message)
	})

	threads := make([]*Thread, 0, len(roots))
	for _, root := range roots {
		threads = append(threads, buildThread(root))
	}
	return threads
}

// BuildThread reconstructs a single thread from a root message ID.
func BuildThread(messages []fmail.Message, rootID string) *Thread {
	nodes := indexNodes(messages)
	linkParents(nodes)

	rootID = strings.TrimSpace(rootID)
	if rootID == "" {
		return nil
	}

	start := nodes[rootID]
	if start == nil {
		return nil
	}
	root := start
	seen := make(map[string]struct{}, 8)
	for root.Parent != nil {
		if root.Message == nil {
			break
		}
		if _, ok := seen[root.Message.ID]; ok {
			break
		}
		seen[root.Message.ID] = struct{}{}
		root = root.Parent
	}
	return buildThread(root)
}

// FlattenThread returns messages in display order (depth-first, chronological siblings).
func FlattenThread(thread *Thread) []*ThreadNode {
	if thread == nil || thread.Root == nil || len(thread.Messages) == 0 {
		return nil
	}

	var root *ThreadNode
	for _, node := range thread.Messages {
		if node == nil || node.Message == nil {
			continue
		}
		if node.Parent == nil && node.Message.ID == thread.Root.ID {
			root = node
			break
		}
	}
	if root == nil {
		root = thread.Messages[0]
	}

	out := make([]*ThreadNode, 0, len(thread.Messages))
	var walk func(n *ThreadNode)
	walk = func(n *ThreadNode) {
		if n == nil {
			return
		}
		out = append(out, n)
		if len(n.Children) == 0 {
			return
		}
		children := append([]*ThreadNode(nil), n.Children...)
		sort.SliceStable(children, func(i, j int) bool {
			if children[i] == nil || children[i].Message == nil {
				return false
			}
			if children[j] == nil || children[j].Message == nil {
				return true
			}
			return messageLess(*children[i].Message, *children[j].Message)
		})
		for _, child := range children {
			walk(child)
		}
	}
	walk(root)
	return out
}

func indexNodes(messages []fmail.Message) map[string]*ThreadNode {
	nodes := make(map[string]*ThreadNode, len(messages))
	for i := range messages {
		msg := messages[i]
		if strings.TrimSpace(msg.ID) == "" {
			continue
		}
		clone := msg
		nodes[msg.ID] = &ThreadNode{Message: &clone}
	}
	return nodes
}

func linkParents(nodes map[string]*ThreadNode) {
	if len(nodes) == 0 {
		return
	}

	// Deterministic: link in chronological order.
	ordered := make([]*ThreadNode, 0, len(nodes))
	for _, node := range nodes {
		ordered = append(ordered, node)
	}
	sort.SliceStable(ordered, func(i, j int) bool {
		if ordered[i] == nil || ordered[i].Message == nil {
			return false
		}
		if ordered[j] == nil || ordered[j].Message == nil {
			return true
		}
		return messageLess(*ordered[i].Message, *ordered[j].Message)
	})

	for _, node := range ordered {
		if node == nil || node.Message == nil {
			continue
		}
		replyTo := strings.TrimSpace(node.Message.ReplyTo)
		if replyTo == "" || replyTo == node.Message.ID {
			continue
		}
		parent := nodes[replyTo]
		if parent == nil || parent.Message == nil {
			continue
		}
		if wouldCreateCycle(node, parent) {
			continue
		}

		node.Parent = parent
		parent.Children = append(parent.Children, node)
	}

	// Assign depths (clamped) after parenting.
	for _, node := range ordered {
		if node == nil {
			continue
		}
		node.Depth = clampedDepth(node)
	}
}

func wouldCreateCycle(node, parent *ThreadNode) bool {
	if node == nil || parent == nil {
		return false
	}
	cur := parent
	for cur != nil {
		if cur == node {
			return true
		}
		cur = cur.Parent
	}
	return false
}

func clampedDepth(node *ThreadNode) int {
	depth := 0
	cur := node
	for cur != nil && cur.Parent != nil {
		depth++
		if depth >= maxDisplayDepth {
			return maxDisplayDepth
		}
		cur = cur.Parent
	}
	return depth
}

func buildThread(root *ThreadNode) *Thread {
	if root == nil || root.Message == nil {
		return nil
	}

	// Collect subtree.
	collected := make([]*ThreadNode, 0, 32)
	var stack []*ThreadNode
	stack = append(stack, root)
	seen := make(map[*ThreadNode]struct{}, 64)
	for len(stack) > 0 {
		n := stack[len(stack)-1]
		stack = stack[:len(stack)-1]
		if n == nil {
			continue
		}
		if _, ok := seen[n]; ok {
			continue
		}
		seen[n] = struct{}{}
		collected = append(collected, n)
		for i := range n.Children {
			stack = append(stack, n.Children[i])
		}
	}

	// Chronological within thread.
	sort.SliceStable(collected, func(i, j int) bool {
		if collected[i] == nil || collected[i].Message == nil {
			return false
		}
		if collected[j] == nil || collected[j].Message == nil {
			return true
		}
		return messageLess(*collected[i].Message, *collected[j].Message)
	})

	agentsSet := make(map[string]struct{}, 8)
	agents := make([]string, 0, 8)
	lastActivity := time.Time{}
	maxDepth := 0
	for _, node := range collected {
		if node == nil || node.Message == nil {
			continue
		}
		from := strings.TrimSpace(node.Message.From)
		if from != "" {
			if _, ok := agentsSet[from]; !ok {
				agentsSet[from] = struct{}{}
				agents = append(agents, from)
			}
		}
		if node.Message.Time.After(lastActivity) {
			lastActivity = node.Message.Time
		}
		if node.Depth > maxDepth {
			maxDepth = node.Depth
		}
	}
	sort.Strings(agents)

	return &Thread{
		Root:         root.Message,
		Messages:     collected,
		Depth:        maxDepth,
		Agents:       agents,
		LastActivity: lastActivity,
	}
}

func messageLess(a, b fmail.Message) bool {
	if !a.Time.IsZero() && !b.Time.IsZero() && !a.Time.Equal(b.Time) {
		return a.Time.Before(b.Time)
	}
	if a.ID != b.ID {
		return a.ID < b.ID
	}
	if a.From != b.From {
		return a.From < b.From
	}
	return a.To < b.To
}
