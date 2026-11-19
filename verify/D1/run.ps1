# D1 - role and current state of acaprino/shinatama and acaprino/daodan (GitHub API, public data).
# Read-only network fetch. Rerunnable without edits.
param([string]$Date = (Get-Date -Format 'yyyy-MM-dd'))
$out = Join-Path $PSScriptRoot "out\$Date"
New-Item -ItemType Directory -Force -Path $out | Out-Null
foreach ($r in 'shinatama','daodan') {
  $j = Invoke-RestMethod -Uri "https://api.github.com/repos/acaprino/$r" -Headers @{ 'User-Agent' = 'shinatama-verification' }
  $j | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $out "$r-repo.json") -Encoding utf8
  "{0}: pushed_at={1} created_at={2} branch={3} size={4}KB archived={5} license={6}" -f $r, $j.pushed_at, $j.created_at, $j.default_branch, $j.size, $j.archived, $j.license.spdx_id
}
