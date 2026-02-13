# PAR-101 Logs Corpus Pack

Task: `forge-5s3`
Project: `prj-v5pc07bf`

## Deliverables

- Added sanitized real-transcript fixtures for all target harnesses:
  - `crates/forge-cli/testdata/log_highlighting_corpus/codex_real_transcript.log`
  - `crates/forge-cli/testdata/log_highlighting_corpus/claude_real_transcript.log`
  - `crates/forge-cli/testdata/log_highlighting_corpus/opencode_real_transcript.log`
  - `crates/forge-cli/testdata/log_highlighting_corpus/pi_real_transcript.log`
- Added line-span token expectations:
  - `crates/forge-cli/testdata/log_highlighting_corpus/token_spans.json`
- Added baseline scanner + acceptance gate test:
  - `crates/forge-cli/tests/log_highlighting_corpus_test.rs`

## Coverage intent

Corpus includes representative success/failure flows and tokenization anchors across:

- code fences
- diff blocks (header/hunk/add/del)
- stack frames/path+line traces
- tool output and command lines
- approval/warning/failure states

Acceptance gate enforces:

- distinct normalized pattern count `>= 50`
- no `unknown` token class in baseline scan
- required classes present (`success`, `failure`, `code_fence`, `diff_add`, `diff_del`, `stack_frame`, `tool_output`)

## Verification command

```bash
cargo test -p forge-cli --test log_highlighting_corpus_test
```
