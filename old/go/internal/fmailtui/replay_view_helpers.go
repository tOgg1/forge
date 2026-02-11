package fmailtui

import (
	"sort"
	"time"

	"github.com/tOgg1/forge/internal/fmail"
)

var replaySpeedPresets = []float64{1, 5, 10, 50}

func replayMessageTime(msg fmail.Message) time.Time {
	if !msg.Time.IsZero() {
		return msg.Time.UTC()
	}
	if len(msg.ID) >= 15 {
		// Message IDs are sortable: YYYYMMDD-HHMMSS-NNNN
		if ts, err := time.Parse("20060102-150405", msg.ID[:15]); err == nil {
			return ts.UTC()
		}
	}
	return time.Time{}
}

func replaySeekIndexBeforeOrAt(times []time.Time, target time.Time) int {
	if len(times) == 0 {
		return 0
	}
	i := sort.Search(len(times), func(i int) bool {
		return !times[i].Before(target)
	})
	if i == 0 {
		return 0
	}
	if i >= len(times) {
		return len(times) - 1
	}
	if times[i].After(target) {
		return i - 1
	}
	return i
}

func replayNextInterval(curr, next time.Time, speed float64) time.Duration {
	if speed <= 0 {
		speed = 1
	}
	delta := next.Sub(curr)
	if delta <= 0 {
		return 50 * time.Millisecond
	}
	scaled := time.Duration(float64(delta) / speed)
	if scaled < 10*time.Millisecond {
		scaled = 10 * time.Millisecond
	}
	// Fast-forward large gaps.
	if scaled > 200*time.Millisecond || delta > 30*time.Second {
		scaled = 200 * time.Millisecond
	}
	return scaled
}
