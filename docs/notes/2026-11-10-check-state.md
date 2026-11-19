# F03P privacy and integrity check (2026-11-10)

`shinatama check-state` at the commit recorded in progress.json.

- Unit tests: state `privacy` (counts never values, integrity failure on a corrupt file, only
  text-like files scanned) and CLI `check_state` (detector, exit codes, no value in JSON).
- Real read-only run on the owner's profile (2026-11-10): `shinatama.db` integrity ok; search
  index, hop journal and message store absent; no credential-like text; exit 0.

Open: selective forget of companion records (no companion records exist before F10), the SEC
second-reader review, outbound-traffic capture checks (X4/X5), and a CLI rebuild of the search
index (the desktop Search panel already rebuilds it).
