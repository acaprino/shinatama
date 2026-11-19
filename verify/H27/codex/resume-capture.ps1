# H27 codex leg - resume an EXISTING session of the scratch CODEX_HOME with one prompt and capture
# what Codex sends to the model. Resets nothing (it is also run on sessions written by txcript); it
# only writes config.toml if the scratch home has none. Runs:
#   codex exec resume --json --skip-git-repo-check --dangerously-bypass-approvals-and-sandbox <id> <prompt>
# from the session's own cwd (session_meta.cwd) when that folder exists, else from the scratch work dir.
# Writes verify\H27\out\<date>\codex\resume\<Tag>.capture.jsonl plus <Tag>.stdout.jsonl, .stderr.txt,
# .exit.txt, .cmd.txt and .egress.jsonl. Example:
#   powershell -NoProfile -ExecutionPolicy Bypass -File verify\H27\codex\resume-capture.ps1 `
#     -SessionId 01a17dfa-... -Prompt "H27-RESUME what did we do?" -Tag native
param(
    [Parameter(Mandatory)][string]$SessionId,
    [Parameter(Mandatory)][string]$Prompt,
    [Parameter(Mandatory)][string]$Tag,
    [int]$TimeoutSec = 300
)
. (Join-Path $PSScriptRoot 'env.ps1')

$cfg = Write-H27Config
$dir = Join-Path $H27Out 'resume'
New-Item -ItemType Directory -Force -Path $dir | Out-Null
$prefix  = Join-Path $dir $Tag
$capture = "$prefix.capture.jsonl"
$egress  = "$prefix.egress.jsonl"
Remove-Item "$prefix.*" -ErrorAction SilentlyContinue

$rollout = Find-H27Rollout $SessionId
$cwd = $H27Work
if ($rollout) {
    $meta = (Get-Content $rollout.FullName -TotalCount 1 | ConvertFrom-Json).payload
    if ($meta.cwd -and (Test-Path $meta.cwd)) { $cwd = $meta.cwd }
    "rollout: $($rollout.FullName)"
} else {
    "rollout: none named *$SessionId.jsonl under $H27Home\sessions (codex resolves the id itself)"
}
"CODEX_HOME: $env:CODEX_HOME (config $cfg)"
"cwd: $cwd"

Start-H27Mock -Capture $capture -EgressLog $egress
try {
    $r = Invoke-H27Codex -ArgList @('exec', 'resume', '--json', '--skip-git-repo-check',
        '--dangerously-bypass-approvals-and-sandbox', $SessionId, $Prompt) -OutPrefix $prefix -Cwd $cwd -TimeoutSec $TimeoutSec
} finally {
    Stop-H27Mock
}

"exit: $($r.Exit)"
if (Test-Path $capture) {
    & node -e "const l=require('fs').readFileSync(process.argv[1],'utf8').trim().split('\n').map(JSON.parse);console.log('requests captured: '+l.length);for(const e of l)console.log('  #'+e.n+' '+e.method+' '+e.url+(Array.isArray(e.body&&e.body.input)?' input items: '+e.body.input.length:''))" $capture
} else { "requests captured: 0" }
"egress attempts refused by the sink: $(if (Test-Path $egress) { @(Get-Content $egress).Count } else { 0 })"
"wrote: $capture"
"       $prefix.stdout.jsonl, $prefix.stderr.txt, $prefix.exit.txt"
