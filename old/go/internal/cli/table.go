// Package cli provides table helpers for human-readable output.
package cli

import (
	"bufio"
	"io"
	"strings"

	"github.com/mattn/go-runewidth"
)

const tablePadding = 2

func writeTable(out io.Writer, headers []string, rows [][]string) error {
	colCount := len(headers)
	for _, row := range rows {
		if len(row) > colCount {
			colCount = len(row)
		}
	}
	if colCount == 0 {
		return nil
	}

	widths := make([]int, colCount)
	updateWidth := func(index int, value string) {
		if index >= colCount {
			return
		}
		displayWidth := runewidth.StringWidth(stripANSI(value))
		if displayWidth > widths[index] {
			widths[index] = displayWidth
		}
	}

	for idx, header := range headers {
		updateWidth(idx, header)
	}
	for _, row := range rows {
		for idx, cell := range row {
			updateWidth(idx, cell)
		}
	}

	writer := bufio.NewWriter(out)
	var writeErr error
	writeString := func(value string) {
		if writeErr != nil {
			return
		}
		_, writeErr = writer.WriteString(value)
	}
	writeRow := func(row []string) {
		if writeErr != nil {
			return
		}
		for idx := 0; idx < colCount; idx++ {
			cell := ""
			if idx < len(row) {
				cell = row[idx]
			}
			cellWidth := runewidth.StringWidth(stripANSI(cell))
			padding := widths[idx] - cellWidth
			if padding < 0 {
				padding = 0
			}
			writeString(cell)
			if idx < colCount-1 {
				writeString(strings.Repeat(" ", padding+tablePadding))
			}
		}
		writeString("\n")
	}

	if len(headers) > 0 {
		writeRow(headers)
	}
	for _, row := range rows {
		writeRow(row)
	}
	if writeErr != nil {
		return writeErr
	}
	return writer.Flush()
}

func formatYesNo(value bool) string {
	if value {
		return "yes"
	}
	return "no"
}

func stripANSI(value string) string {
	if value == "" {
		return value
	}
	var b strings.Builder
	b.Grow(len(value))
	for i := 0; i < len(value); i++ {
		if value[i] != 0x1b || i+1 >= len(value) || value[i+1] != '[' {
			b.WriteByte(value[i])
			continue
		}
		i += 2
		for i < len(value) {
			ch := value[i]
			if ch >= 0x40 && ch <= 0x7e {
				break
			}
			i++
		}
	}
	return b.String()
}
