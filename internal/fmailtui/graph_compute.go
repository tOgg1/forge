package fmailtui

import (
	"sort"
	"strings"

	"github.com/tOgg1/forge/internal/fmail"
)

const graphMaxNodesDefault = 12

type graphNode struct {
	Name  string
	Sent  int
	Recv  int
	Total int
}

type graphEdge struct {
	From  string
	To    string
	Count int
}

type graphSnapshot struct {
	Messages int
	Nodes    []graphNode
	Edges    []graphEdge
	Topics          []graphTopic
	AgentTopicEdges []graphEdge
}

type graphEdgeKey struct {
	from string
	to   string
}

type graphTopic struct {
	Name         string
	MessageCount int
	Participants int
}

func buildGraphSnapshot(messages []fmail.Message, maxNodes int) graphSnapshot {
	if maxNodes <= 0 {
		maxNodes = graphMaxNodesDefault
	}

	// Phase 1: group topic messages so we can treat topics as shared broadcast channels.
	topicParticipants := make(map[string]map[string]struct{}, 32)
	topicMsgs := make(map[string][]fmail.Message, 64)
	topicCounts := make(map[string]int, 64)
	agentTopic := make(map[graphEdgeKey]int, 256) // from agent -> topic

	dmEdges := make(map[graphEdgeKey]int, 64)

	for i := range messages {
		msg := messages[i]
		from := strings.TrimSpace(msg.From)
		to := strings.TrimSpace(msg.To)
		if from == "" || to == "" {
			continue
		}

		if strings.HasPrefix(to, "@") {
			peer := strings.TrimPrefix(to, "@")
			peer = strings.TrimSpace(peer)
			if peer == "" || peer == from {
				continue
			}
			dmEdges[graphEdgeKey{from: from, to: peer}]++
			continue
		}

		// Topic message.
		topic := to
		parts := topicParticipants[topic]
		if parts == nil {
			parts = make(map[string]struct{}, 8)
			topicParticipants[topic] = parts
		}
		parts[from] = struct{}{}
		topicMsgs[topic] = append(topicMsgs[topic], msg)
		topicCounts[topic]++
		agentTopic[graphEdgeKey{from: from, to: topic}]++
	}

	// Phase 2: build directed agent->agent edges.
	edges := make(map[graphEdgeKey]int, 256)

	for k, v := range dmEdges {
		edges[k] += v
	}

	for topic, msgs := range topicMsgs {
		parts := topicParticipants[topic]
		if len(parts) <= 1 {
			continue
		}
		peers := make([]string, 0, len(parts))
		for agent := range parts {
			peers = append(peers, agent)
		}
		for i := range msgs {
			from := strings.TrimSpace(msgs[i].From)
			if from == "" {
				continue
			}
			for _, peer := range peers {
				if peer == from {
					continue
				}
				edges[graphEdgeKey{from: from, to: peer}]++
			}
		}
	}

	// Phase 3: compute node totals.
	nodeMap := make(map[string]*graphNode, 64)
	addNode := func(name string) *graphNode {
		n := nodeMap[name]
		if n != nil {
			return n
		}
		n = &graphNode{Name: name}
		nodeMap[name] = n
		return n
	}

	for k, count := range edges {
		if strings.TrimSpace(k.from) == "" || strings.TrimSpace(k.to) == "" || k.from == k.to || count <= 0 {
			continue
		}
		from := addNode(k.from)
		to := addNode(k.to)
		from.Sent += count
		to.Recv += count
	}
	nodes := make([]graphNode, 0, len(nodeMap))
	for _, node := range nodeMap {
		node.Total = node.Sent + node.Recv
		nodes = append(nodes, *node)
	}
	sort.Slice(nodes, func(i, j int) bool {
		if nodes[i].Total != nodes[j].Total {
			return nodes[i].Total > nodes[j].Total
		}
		return nodes[i].Name < nodes[j].Name
	})

	// Phase 4: collapse to max nodes (including "others").
	keep := make(map[string]struct{}, len(nodes))
	others := ""
	if len(nodes) > maxNodes {
		others = "others"
		limit := maxNodes - 1
		if limit < 1 {
			limit = 1
		}
		for i := 0; i < len(nodes) && i < limit; i++ {
			keep[nodes[i].Name] = struct{}{}
		}
		keep[others] = struct{}{}
	}

	mapNode := func(name string) string {
		if others == "" {
			return name
		}
		if _, ok := keep[name]; ok {
			return name
		}
		return others
	}

	aggEdges := make(map[graphEdgeKey]int, len(edges))
	for k, count := range edges {
		from := mapNode(k.from)
		to := mapNode(k.to)
		if from == to || count <= 0 {
			continue
		}
		aggEdges[graphEdgeKey{from: from, to: to}] += count
	}

	nodeMap = make(map[string]*graphNode, len(keep))
	for k, count := range aggEdges {
		from := addNodeForGraph(nodeMap, k.from)
		to := addNodeForGraph(nodeMap, k.to)
		from.Sent += count
		to.Recv += count
	}

	finalNodes := make([]graphNode, 0, len(nodeMap))
	for _, node := range nodeMap {
		node.Total = node.Sent + node.Recv
		finalNodes = append(finalNodes, *node)
	}
	sort.Slice(finalNodes, func(i, j int) bool {
		if finalNodes[i].Total != finalNodes[j].Total {
			return finalNodes[i].Total > finalNodes[j].Total
		}
		return finalNodes[i].Name < finalNodes[j].Name
	})

	finalEdges := make([]graphEdge, 0, len(aggEdges))
	for k, count := range aggEdges {
		finalEdges = append(finalEdges, graphEdge{From: k.from, To: k.to, Count: count})
	}
	sort.Slice(finalEdges, func(i, j int) bool {
		if finalEdges[i].Count != finalEdges[j].Count {
			return finalEdges[i].Count > finalEdges[j].Count
		}
		if finalEdges[i].From != finalEdges[j].From {
			return finalEdges[i].From < finalEdges[j].From
		}
		return finalEdges[i].To < finalEdges[j].To
	})

	// Phase 5: topic overlay data (top topics + agent->topic edges).
	topics := make([]graphTopic, 0, len(topicCounts))
	for topic, count := range topicCounts {
		parts := 0
		if ps := topicParticipants[topic]; ps != nil {
			parts = len(ps)
		}
		topics = append(topics, graphTopic{Name: topic, MessageCount: count, Participants: parts})
	}
	sort.Slice(topics, func(i, j int) bool {
		if topics[i].MessageCount != topics[j].MessageCount {
			return topics[i].MessageCount > topics[j].MessageCount
		}
		return topics[i].Name < topics[j].Name
	})
	const maxTopics = 10
	if len(topics) > maxTopics {
		topics = topics[:maxTopics]
	}
	keepTopics := make(map[string]struct{}, len(topics))
	for i := range topics {
		keepTopics[topics[i].Name] = struct{}{}
	}

	agentTopicEdges := make(map[graphEdgeKey]int, len(agentTopic))
	for k, count := range agentTopic {
		if _, ok := keepTopics[k.to]; !ok {
			continue
		}
		from := mapNode(k.from)
		agentTopicEdges[graphEdgeKey{from: from, to: k.to}] += count
	}
	finalAgentTopic := make([]graphEdge, 0, len(agentTopicEdges))
	for k, count := range agentTopicEdges {
		if strings.TrimSpace(k.from) == "" || strings.TrimSpace(k.to) == "" || count <= 0 {
			continue
		}
		finalAgentTopic = append(finalAgentTopic, graphEdge{From: k.from, To: k.to, Count: count})
	}
	sort.Slice(finalAgentTopic, func(i, j int) bool {
		if finalAgentTopic[i].Count != finalAgentTopic[j].Count {
			return finalAgentTopic[i].Count > finalAgentTopic[j].Count
		}
		if finalAgentTopic[i].From != finalAgentTopic[j].From {
			return finalAgentTopic[i].From < finalAgentTopic[j].From
		}
		return finalAgentTopic[i].To < finalAgentTopic[j].To
	})

	return graphSnapshot{
		Messages:        len(messages),
		Nodes:           finalNodes,
		Edges:           finalEdges,
		Topics:          topics,
		AgentTopicEdges: finalAgentTopic,
	}
}

func addNodeForGraph(m map[string]*graphNode, name string) *graphNode {
	n := m[name]
	if n != nil {
		return n
	}
	n = &graphNode{Name: name}
	m[name] = n
	return n
}
