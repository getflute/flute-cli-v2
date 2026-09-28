"""Walk the v1 CLI's --help tree and emit its full capability surface."""
import subprocess, re, sys, os

# This repository is public, so no contributor's absolute path belongs in it.
# `FLUTE_CLI_V1_BIN` names the binary outright; `FLUTE_CLI_V1` names the v1
# checkout; otherwise a sibling checkout is assumed, relative to this file
# rather than to the caller's working directory.
V1 = os.environ.get(
    "FLUTE_CLI_V1",
    os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "..", "flute-cli"),
)
BIN = os.environ.get("FLUTE_CLI_V1_BIN", os.path.join(V1, "target", "release", "flute"))

def help_for(path):
    r = subprocess.run([BIN] + path + ["--help"], capture_output=True, text=True)
    return r.stdout + r.stderr

def parse(text):
    subs, flags, section = [], [], None
    for line in text.splitlines():
        s = line.strip()
        if re.match(r'^(Commands|Subcommands):', s): section = 'cmd'; continue
        if re.match(r'^(Options|Arguments|Global options):', s): section = 'opt'; continue
        if not s: continue
        if section == 'cmd':
            m = re.match(r'^([a-z][a-z0-9-]*)(?:,\s*[a-z0-9-]+)?\s{2,}', s) or re.match(r'^([a-z][a-z0-9-]*)$', s)
            if m and m.group(1) != 'help': subs.append(m.group(1))
        elif section == 'opt':
            for f in re.findall(r'--[a-z0-9][a-z0-9-]*', s):
                if f not in ('--help', '--version'): flags.append(f)
    return subs, sorted(set(flags))

GLOBAL = set()
rows = []
def walk(path):
    subs, flags = parse(help_for(path))
    if not path:
        GLOBAL.update(flags)
    own = [f for f in flags if f not in GLOBAL]
    if path and not subs:
        rows.append((" ".join(path), own))
    elif path and subs and own:
        rows.append((" ".join(path), own))
    for s in subs:
        walk(path + [s])

walk([])
print("# v1 capability surface, generated from the v1 binary's --help tree.")
print("# One line per leaf command: `<command>\\t<flags>`. Regenerate with")
print("# docs/reference/snapshot-v1-capabilities.py against a v1 checkout.")
print(f"#globals\t{' '.join(sorted(GLOBAL))}")
for cmd, flags in sorted(rows):
    print(f"{cmd}\t{' '.join(flags)}")
