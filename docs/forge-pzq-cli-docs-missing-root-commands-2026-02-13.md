# forge-pzq - CLI docs parity for missing root commands (2026-02-13)

## Scope shipped
- Added missing command sections in `docs/cli.md` for:
  - `forge audit`
  - `forge doctor`
  - `forge explain`
  - `forge export`
- Normalized TUI heading to explicit `forge tui` and documented plain `forge` as equivalent entrypoint.
- Verified docs coverage parity against root `forge --help` command list (allowing known excluded infra command families).

## Validation
```bash
python3 - <<'PY'
import re,subprocess
help_txt=subprocess.check_output(['cargo','run','-q','-p','forge-cli','--bin','forge-cli','--','--help'],text=True)
cmds=[]; in_cmd=False
for ln in help_txt.splitlines():
    if ln.startswith('Commands:'): in_cmd=True; continue
    if ln.startswith('Global Flags:'): break
    if in_cmd:
        m=re.match(r'\\s{2}([a-z0-9_-]+)\\s',ln)
        if m: cmds.append(m.group(1))
sections=open('docs/cli.md').read().splitlines()
doc=set()
for ln in sections:
    m=re.match(r'### `forge ([^`]+)`',ln)
    if m:
        name=m.group(1).strip().split()[0].replace('/', '').strip()
        if name=='loop':
            doc.update(['up','ps','logs','msg','stop','kill','resume','rm','clean','scale','queue','run'])
        elif name:
            doc.add(name)
missing=[c for c in cmds if c not in doc and c not in {'completion','context','hook','inject','lock','mail','migrate','send','skills','status','use'}]
print('Missing-doc-candidates:',missing)
PY
cargo run -q -p forge-cli --bin forge-cli -- export --help
cargo check -p forge-cli
```
