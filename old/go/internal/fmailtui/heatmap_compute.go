package fmailtui

import (
	"sort"
	"strings"
	"time"

	"github.com/tOgg1/forge/internal/fmail"
)

type heatmapMode int

const (
	heatmapModeAgents heatmapMode = iota
	heatmapModeTopics
)

type heatmapSort int

const (
	heatmapSortTotal heatmapSort = iota
	heatmapSortName
	heatmapSortPeak
	heatmapSortRecency
)

type heatmapRow struct {
	Label   string
	Counts  []int
	Total   int
	PeakIdx int
	Last    time.Time
}

type heatmapMatrix struct {
	Start     time.Time
	End       time.Time
	Bucket    time.Duration
	Cols      int
	Rows      []heatmapRow
	MaxCell   int
	Threshold [3]int // <=t0 ░, <=t1 ▒, <=t2 ▓, >t2 █
}

func buildHeatmapMatrix(messages []fmail.Message, start, end time.Time, bucket time.Duration, mode heatmapMode) heatmapMatrix {
	out := heatmapMatrix{Start: start, End: end, Bucket: bucket}
	if bucket <= 0 || start.IsZero() || end.IsZero() || !end.After(start) {
		return out
	}
	cols := int(end.Sub(start) / bucket)
	if cols <= 0 {
		return out
	}
	out.Cols = cols

	type agg struct {
		counts []int
		total  int
		peak   int
		last   time.Time
	}

	byLabel := make(map[string]*agg, 32)
	nonZero := make([]int, 0, 128)
	maxCell := 0

	for i := range messages {
		msg := messages[i]
		ts := msg.Time
		if ts.IsZero() || ts.Before(start) || !ts.Before(end) {
			continue
		}
		label := ""
		switch mode {
		case heatmapModeTopics:
			label = strings.TrimSpace(msg.To)
		default:
			label = strings.TrimSpace(msg.From)
		}
		if label == "" {
			continue
		}
		col := int(ts.Sub(start) / bucket)
		if col < 0 || col >= cols {
			continue
		}

		a := byLabel[label]
		if a == nil {
			a = &agg{counts: make([]int, cols)}
			byLabel[label] = a
		}
		a.counts[col]++
		a.total++
		if a.counts[col] > a.counts[a.peak] {
			a.peak = col
		}
		if a.last.IsZero() || ts.After(a.last) {
			a.last = ts
		}
	}

	rows := make([]heatmapRow, 0, len(byLabel))
	for label, a := range byLabel {
		total := a.total
		peak := a.peak
		last := a.last
		for _, c := range a.counts {
			if c <= 0 {
				continue
			}
			nonZero = append(nonZero, c)
			if c > maxCell {
				maxCell = c
			}
		}
		rows = append(rows, heatmapRow{
			Label:   label,
			Counts: a.counts,
			Total:   total,
			PeakIdx: peak,
			Last:    last,
		})
	}

	out.Rows = rows
	out.MaxCell = maxCell
	out.Threshold = heatmapThresholds(nonZero)
	return out
}

func (m *heatmapMatrix) sortRows(mode heatmapSort) {
	if m == nil || len(m.Rows) == 0 {
		return
	}
	sort.SliceStable(m.Rows, func(i, j int) bool {
		a := m.Rows[i]
		b := m.Rows[j]
		switch mode {
		case heatmapSortName:
			la := strings.ToLower(strings.TrimSpace(a.Label))
			lb := strings.ToLower(strings.TrimSpace(b.Label))
			if la != lb {
				return la < lb
			}
		case heatmapSortPeak:
			if a.PeakIdx != b.PeakIdx {
				return a.PeakIdx < b.PeakIdx
			}
			if a.Total != b.Total {
				return a.Total > b.Total
			}
		case heatmapSortRecency:
			if !a.Last.Equal(b.Last) {
				return a.Last.After(b.Last)
			}
			if a.Total != b.Total {
				return a.Total > b.Total
			}
		default: // total
			if a.Total != b.Total {
				return a.Total > b.Total
			}
		}
		return strings.TrimSpace(a.Label) < strings.TrimSpace(b.Label)
	})
}

func heatmapThresholds(nonZero []int) [3]int {
	// Default fixed thresholds.
	if len(nonZero) < 8 {
		return [3]int{5, 15, 30}
	}
	vals := append([]int(nil), nonZero...)
	sort.Ints(vals)
	p25 := percentileInt(vals, 0.25)
	p50 := percentileInt(vals, 0.50)
	p75 := percentileInt(vals, 0.75)
	// Ensure strictly increasing thresholds to avoid flat rendering on small ranges.
	if p25 < 1 {
		p25 = 1
	}
	if p50 < p25 {
		p50 = p25
	}
	if p75 < p50 {
		p75 = p50
	}
	if p50 == p25 {
		p50 = p25 + 1
	}
	if p75 == p50 {
		p75 = p50 + 1
	}
	return [3]int{p25, p50, p75}
}

func percentileInt(sorted []int, p float64) int {
	if len(sorted) == 0 {
		return 0
	}
	if p <= 0 {
		return sorted[0]
	}
	if p >= 1 {
		return sorted[len(sorted)-1]
	}
	idx := int(float64(len(sorted)-1) * p)
	if idx < 0 {
		idx = 0
	}
	if idx >= len(sorted) {
		idx = len(sorted) - 1
	}
	return sorted[idx]
}

