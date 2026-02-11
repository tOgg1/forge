package fmailtui

import (
	"sort"
	"strings"
	"time"

	"github.com/tOgg1/forge/internal/fmail"
	"github.com/tOgg1/forge/internal/fmailtui/threading"
)

type statsBucket struct {
	Label string
	Count int
	Pct   float64
}

type statsBar struct {
	Label string
	Count int
}

type statsThreadDist struct {
	Standalone int // 1 msg
	Small      int // 2-3
	Medium     int // 4-10
	Large      int // 10+
}

type statsSnapshot struct {
	TotalMessages int
	ActiveAgents  int
	ActiveTopics  int

	ReplySamples int
	AvgReply     time.Duration
	MedianReply  time.Duration

	LongestThreadMessages int
	MostRepliedID         string
	MostRepliedCount      int

	TopAgents    []statsBar
	TopicVolumes []statsBar

	OverTimeCounts   []int
	OverTimeStart    time.Time
	OverTimeInterval time.Duration

	ResponseLatency []statsBucket

	BusiestHourStart  time.Time
	BusiestHourCount  int
	QuietestHourStart time.Time
	QuietestHourCount int

	ThreadAvgMessages float64
	ThreadDist        statsThreadDist
}

func computeStats(messages []fmail.Message, windowStart, windowEnd time.Time) statsSnapshot {
	in := filterMessagesByTime(messages, windowStart, windowEnd)

	out := statsSnapshot{
		TotalMessages: len(in),
	}
	if len(in) == 0 {
		return out
	}

	// Active agents/topics.
	agents := make(map[string]struct{}, 16)
	topics := make(map[string]struct{}, 16)
	byAgent := make(map[string]int, 16)
	byTopic := make(map[string]int, 16)

	byID := make(map[string]fmail.Message, len(in))
	for i := range in {
		msg := in[i]
		if from := strings.TrimSpace(msg.From); from != "" {
			agents[from] = struct{}{}
			byAgent[from]++
		}
		if to := strings.TrimSpace(msg.To); to != "" {
			topics[to] = struct{}{}
			byTopic[to]++
		}
		if id := strings.TrimSpace(msg.ID); id != "" {
			byID[id] = msg
		}
	}
	out.ActiveAgents = len(agents)
	out.ActiveTopics = len(topics)

	out.TopAgents = topN(byAgent, 10)
	out.TopicVolumes = topN(byTopic, 10)

	// Reply latency + most replied-to message.
	replyCounts := make(map[string]int, 64)
	replyDeltas := make([]time.Duration, 0, 128)
	for i := range in {
		msg := in[i]
		parentID := strings.TrimSpace(msg.ReplyTo)
		if parentID == "" {
			continue
		}
		replyCounts[parentID]++
		parent, ok := byID[parentID]
		if !ok || parent.Time.IsZero() || msg.Time.IsZero() {
			continue
		}
		delta := msg.Time.Sub(parent.Time)
		if delta < 0 {
			continue
		}
		replyDeltas = append(replyDeltas, delta)
	}
	if len(replyCounts) > 0 {
		var bestID string
		bestCount := 0
		for id, count := range replyCounts {
			if count > bestCount {
				bestID = id
				bestCount = count
			}
		}
		out.MostRepliedID = bestID
		out.MostRepliedCount = bestCount
	}

	out.ReplySamples = len(replyDeltas)
	if len(replyDeltas) > 0 {
		var total time.Duration
		for _, d := range replyDeltas {
			total += d
		}
		out.AvgReply = total / time.Duration(len(replyDeltas))
		sort.Slice(replyDeltas, func(i, j int) bool { return replyDeltas[i] < replyDeltas[j] })
		mid := len(replyDeltas) / 2
		if len(replyDeltas)%2 == 1 {
			out.MedianReply = replyDeltas[mid]
		} else if len(replyDeltas) > 1 {
			out.MedianReply = (replyDeltas[mid-1] + replyDeltas[mid]) / 2
		} else {
			out.MedianReply = replyDeltas[0]
		}
	}

	out.ResponseLatency = latencyBuckets(replyDeltas)

	// Threads.
	threads := threading.BuildThreads(in)
	if len(threads) > 0 {
		var totalMsgs int
		for _, th := range threads {
			if th == nil {
				continue
			}
			n := len(th.Messages)
			totalMsgs += n
			if n > out.LongestThreadMessages {
				out.LongestThreadMessages = n
			}
			switch {
			case n <= 1:
				out.ThreadDist.Standalone++
			case n <= 3:
				out.ThreadDist.Small++
			case n <= 10:
				out.ThreadDist.Medium++
			default:
				out.ThreadDist.Large++
			}
		}
		out.ThreadAvgMessages = float64(totalMsgs) / float64(len(threads))
	}

	// Time buckets.
	out.OverTimeInterval = chooseBucketInterval(windowStart, windowEnd, 48)
	out.OverTimeStart = bucketStartTime(windowStart, out.OverTimeInterval)
	out.OverTimeCounts = bucketCounts(in, out.OverTimeStart, windowEnd, out.OverTimeInterval)

	// Busiest/quietest hour.
	busyStart, busyCount, quietStart, quietCount := busiestQuietestHour(in, windowStart, windowEnd)
	out.BusiestHourStart = busyStart
	out.BusiestHourCount = busyCount
	out.QuietestHourStart = quietStart
	out.QuietestHourCount = quietCount

	return out
}

func filterMessagesByTime(messages []fmail.Message, start, end time.Time) []fmail.Message {
	if len(messages) == 0 {
		return nil
	}
	out := make([]fmail.Message, 0, len(messages))
	for i := range messages {
		msg := messages[i]
		ts := msg.Time
		if !start.IsZero() && (ts.IsZero() || ts.Before(start)) {
			continue
		}
		if !end.IsZero() && (ts.IsZero() || !ts.Before(end)) {
			continue
		}
		out = append(out, msg)
	}
	return out
}

func topN(counts map[string]int, limit int) []statsBar {
	if len(counts) == 0 || limit <= 0 {
		return nil
	}
	out := make([]statsBar, 0, len(counts))
	for label, count := range counts {
		label = strings.TrimSpace(label)
		if label == "" || count <= 0 {
			continue
		}
		out = append(out, statsBar{Label: label, Count: count})
	}
	sort.SliceStable(out, func(i, j int) bool {
		if out[i].Count != out[j].Count {
			return out[i].Count > out[j].Count
		}
		return out[i].Label < out[j].Label
	})
	if len(out) > limit {
		out = out[:limit]
	}
	return out
}

func latencyBuckets(deltas []time.Duration) []statsBucket {
	total := len(deltas)
	buckets := []struct {
		label string
		min   time.Duration
		max   time.Duration
	}{
		{label: "<30s", min: 0, max: 30 * time.Second},
		{label: "30s-5m", min: 30 * time.Second, max: 5 * time.Minute},
		{label: "5m-30m", min: 5 * time.Minute, max: 30 * time.Minute},
		{label: "30m-2h", min: 30 * time.Minute, max: 2 * time.Hour},
		{label: ">2h", min: 2 * time.Hour, max: 0},
	}
	out := make([]statsBucket, 0, len(buckets))
	for _, b := range buckets {
		out = append(out, statsBucket{Label: b.label})
	}
	for _, d := range deltas {
		switch {
		case d < 30*time.Second:
			out[0].Count++
		case d < 5*time.Minute:
			out[1].Count++
		case d < 30*time.Minute:
			out[2].Count++
		case d < 2*time.Hour:
			out[3].Count++
		default:
			out[4].Count++
		}
	}
	if total > 0 {
		for i := range out {
			out[i].Pct = float64(out[i].Count) / float64(total) * 100.0
		}
	}
	return out
}

func chooseBucketInterval(start, end time.Time, maxBuckets int) time.Duration {
	if maxBuckets <= 0 {
		maxBuckets = 48
	}
	if start.IsZero() || end.IsZero() || !end.After(start) {
		return time.Hour
	}
	duration := end.Sub(start)
	candidates := []time.Duration{
		1 * time.Minute,
		5 * time.Minute,
		10 * time.Minute,
		15 * time.Minute,
		30 * time.Minute,
		1 * time.Hour,
		2 * time.Hour,
		4 * time.Hour,
		6 * time.Hour,
		12 * time.Hour,
		24 * time.Hour,
		48 * time.Hour,
		7 * 24 * time.Hour,
	}
	target := duration / time.Duration(maxBuckets)
	for _, cand := range candidates {
		if cand >= target {
			return cand
		}
	}
	return candidates[len(candidates)-1]
}

func bucketStartTime(start time.Time, interval time.Duration) time.Time {
	if start.IsZero() || interval <= 0 {
		return start
	}
	// Keep it simple: anchor to UTC to make sparkline stable across locales.
	start = start.UTC()
	switch {
	case interval >= 24*time.Hour:
		y, m, d := start.Date()
		return time.Date(y, m, d, 0, 0, 0, 0, time.UTC)
	case interval >= time.Hour:
		return start.Truncate(time.Hour)
	default:
		return start.Truncate(interval)
	}
}

func bucketCounts(messages []fmail.Message, start, end time.Time, interval time.Duration) []int {
	if interval <= 0 || start.IsZero() || end.IsZero() || !end.After(start) {
		return nil
	}
	n := int(end.Sub(start)/interval) + 1
	if n <= 0 {
		return nil
	}
	counts := make([]int, n)
	for i := range messages {
		ts := messages[i].Time
		if ts.IsZero() || ts.Before(start) || !ts.Before(end) {
			continue
		}
		idx := int(ts.Sub(start) / interval)
		if idx < 0 || idx >= len(counts) {
			continue
		}
		counts[idx]++
	}
	return counts
}

func busiestQuietestHour(messages []fmail.Message, start, end time.Time) (busyStart time.Time, busyCount int, quietStart time.Time, quietCount int) {
	if start.IsZero() || end.IsZero() || !end.After(start) {
		return time.Time{}, 0, time.Time{}, 0
	}
	start = start.UTC()
	end = end.UTC()

	counts := make(map[time.Time]int, 64)
	for i := range messages {
		ts := messages[i].Time
		if ts.IsZero() || ts.Before(start) || !ts.Before(end) {
			continue
		}
		h := ts.UTC().Truncate(time.Hour)
		counts[h]++
	}

	first := start.Truncate(time.Hour)
	last := end.Truncate(time.Hour)
	if last.Before(end) {
		last = last.Add(time.Hour)
	}

	busyCount = -1
	quietCount = -1
	for cur := first; cur.Before(last); cur = cur.Add(time.Hour) {
		c := counts[cur]
		if busyCount < 0 || c > busyCount {
			busyCount = c
			busyStart = cur
		}
		if quietCount < 0 || c < quietCount {
			quietCount = c
			quietStart = cur
		}
	}
	if busyCount < 0 {
		busyCount = 0
	}
	if quietCount < 0 {
		quietCount = 0
	}
	return busyStart, busyCount, quietStart, quietCount
}

