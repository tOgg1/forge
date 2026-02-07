package cli

import (
	"fmt"
	"os"
	"strings"
)

func defaultLoopRef() (string, bool) {
	if v := strings.TrimSpace(os.Getenv("FORGE_LOOP_ID")); v != "" {
		return v, true
	}
	if v := strings.TrimSpace(os.Getenv("FORGE_LOOP_NAME")); v != "" {
		return v, true
	}
	return "", false
}

func requireLoopRef(explicit string) (string, error) {
	if strings.TrimSpace(explicit) != "" {
		return strings.TrimSpace(explicit), nil
	}
	if v, ok := defaultLoopRef(); ok {
		return v, nil
	}
	return "", fmt.Errorf("loop required (pass --loop or set FORGE_LOOP_ID)")
}
