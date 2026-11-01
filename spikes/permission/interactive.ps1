# Spike: starts an interactive claude session in the throwaway project with one hook mode.
param([string]$Mode, [string]$Root)
Set-Location $Root
Set-Content mode.txt $Mode
Remove-Item marker.txt -ErrorAction SilentlyContinue
Remove-Item env:CLAUDECODE, env:CLAUDE_CODE_ENTRYPOINT -ErrorAction SilentlyContinue
claude --model haiku --permission-mode default "Use the Bash tool to run exactly this command and nothing else: node -e `"require('fs').writeFileSync('marker.txt','x')`""
