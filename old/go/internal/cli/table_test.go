package cli

import (
	"bytes"
	"testing"
)

func TestWriteTableAlignsWithANSI(t *testing.T) {
	var buf bytes.Buffer
	headers := []string{"ID", "NAME", "RUNS"}
	rows := [][]string{
		{"a", "alpha", "1"},
		{colorYellow + "bb" + colorReset, "beta", "22"},
	}

	if err := writeTable(&buf, headers, rows); err != nil {
		t.Fatalf("writeTable failed: %v", err)
	}

	got := stripANSI(buf.String())
	want := "" +
		"ID  NAME   RUNS\n" +
		"a   alpha  1\n" +
		"bb  beta   22\n"

	if got != want {
		t.Fatalf("unexpected table output:\nwant:\n%s\ngot:\n%s", want, got)
	}
}
