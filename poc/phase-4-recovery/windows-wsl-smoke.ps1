param(
    [string]$Distro = "Muxlane-E2E-CODX-R8M4QZ",
    [string]$DataRoot = "/var/tmp/muxlane-e2e/windows-R8M4QZ"
)

$ErrorActionPreference = "Stop"
if (-not $Distro.StartsWith("Muxlane-E2E-CODX-")) { throw "unsafe test distro" }
if (-not $DataRoot.StartsWith("/var/tmp/muxlane-e2e/windows-")) { throw "unsafe data root" }

function Invoke-Wsl([string[]]$Command) {
    $output = & wsl.exe -d $Distro --cd / -u root --exec @Command
    if ($LASTEXITCODE -ne 0) { throw "WSL command failed: $($Command -join ' ')" }
    return $output
}

function Invoke-Cli([string[]]$Arguments) {
    $command = @("/usr/bin/env", "MUXLANE_DATA_DIR=$DataRoot", "PATH=/opt/muxlane/bin:/usr/bin:/bin", "muxlane") + $Arguments
    return (Invoke-Wsl $command | ConvertFrom-Json)
}

function Start-Gateway {
    $start = [System.Diagnostics.ProcessStartInfo]::new()
    $start.FileName = "$env:SystemRoot\System32\wsl.exe"
    $start.Arguments = "-d $Distro --cd / -u root --exec /usr/bin/env MUXLANE_DATA_DIR=$DataRoot PATH=/opt/muxlane/bin:/usr/bin:/bin muxlaned terminal-gateway"
    $start.UseShellExecute = $false
    $start.RedirectStandardInput = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $start.CreateNoWindow = $true
    return [System.Diagnostics.Process]::Start($start)
}

function Send-Frame($Gateway, [uint64]$Id, $Request) {
    $frame = @{ id = $Id; request = $Request } | ConvertTo-Json -Compress -Depth 12
    $Gateway.StandardInput.WriteLine($frame)
    $Gateway.StandardInput.Flush()
}

function Read-Response($Gateway, [uint64]$Id, $Events) {
    while ($true) {
        $line = $Gateway.StandardOutput.ReadLine()
        if ($null -eq $line) { throw "Terminal Gateway disconnected" }
        $frame = $line | ConvertFrom-Json
        if ($frame.frame -eq "event") { $Events.Add($frame.event); continue }
        if ([uint64]$frame.id -ne $Id) { throw "unexpected Terminal response id" }
        if ($frame.result.status -eq "error") { throw "$($frame.result.error.code): $($frame.result.error.message)" }
        return $frame.result.response
    }
}

function Handshake($Gateway, $Events, [uint64]$Id) {
    Send-Frame $Gateway $Id @{ method = "terminal.handshake"; params = @{ protocol_major = 1; protocol_minor = 0; client_name = "windows_native_smoke" } }
    return Read-Response $Gateway $Id $Events
}

$daemon = $null
$gateway = $null
try {
    Invoke-Wsl @("/usr/bin/install", "-d", "-m", "0700", $DataRoot) | Out-Null
    $daemonStart = [System.Diagnostics.ProcessStartInfo]::new()
    $daemonStart.FileName = "$env:SystemRoot\System32\wsl.exe"
    $daemonStart.Arguments = "-d $Distro --cd / -u root --exec /usr/bin/env MUXLANE_DATA_DIR=$DataRoot PATH=/opt/muxlane/bin:/usr/bin:/bin MUXLANE_TEST_CODEX_MODE=wait muxlaned serve"
    $daemonStart.UseShellExecute = $false
    $daemonStart.CreateNoWindow = $true
    $daemon = [System.Diagnostics.Process]::Start($daemonStart)
    for ($attempt = 0; $attempt -lt 100; $attempt++) {
        try { Invoke-Cli @("status") | Out-Null; break } catch { Start-Sleep -Milliseconds 50 }
    }
    if ($attempt -eq 100) { throw "daemon did not become ready" }

    $account = (Invoke-Cli @("account", "import", "/var/tmp/muxlane-e2e/fixture-auth.json", "WindowsFixture")).result.data.account_id
    $projectSource = "/var/tmp/muxlane-project-windows-R8M4QZ"
    Invoke-Wsl @("/usr/bin/install", "-d", "-m", "0700", $projectSource) | Out-Null
    $project = (Invoke-Cli @("project", "register", $projectSource, "WindowsProject")).result.data.project_id
    Invoke-Cli @("launch", "start", $account, $project) | Out-Null
    $terminal = (Invoke-Cli @("terminal", "create", $project, "WindowsAux")).result.data
    $terminal2 = (Invoke-Cli @("terminal", "create", $project, "WindowsAuxTwo")).result.data

    $events = [System.Collections.Generic.List[object]]::new()
    $gateway = Start-Gateway
    $handshake = Handshake $gateway $events 1
    Send-Frame $gateway 2 @{ method = "terminal.attach"; params = @{ terminal_id = $terminal.terminal_id } }
    $stream = (Read-Response $gateway 2 $events).stream
    Send-Frame $gateway 3 @{ method = "terminal.stream.start"; params = @{ stream = $stream } }
    Read-Response $gateway 3 $events | Out-Null
    $input = [System.Text.Encoding]::UTF8.GetBytes("printf WINDOWS_FORMAL_LIVE`n")
    Send-Frame $gateway 4 @{ method = "terminal.input"; params = @{ stream = $stream; bytes = @($input) } }
    Read-Response $gateway 4 $events | Out-Null
    Send-Frame $gateway 5 @{ method = "terminal.resize"; params = @{ stream = $stream; columns = 112; rows = 34 } }
    Read-Response $gateway 5 $events | Out-Null
    Start-Sleep -Milliseconds 250
    Send-Frame $gateway 6 @{ method = "terminal.detach"; params = @{ stream = $stream } }
    Read-Response $gateway 6 $events | Out-Null
    $gateway.StandardInput.Close(); $gateway.WaitForExit(5000) | Out-Null; $gateway = $null

    $gateway2 = Start-Gateway
    $events2 = [System.Collections.Generic.List[object]]::new()
    Handshake $gateway2 $events2 1 | Out-Null
    Send-Frame $gateway2 2 @{ method = "terminal.attach"; params = @{ terminal_id = $terminal.terminal_id } }
    $stream2 = (Read-Response $gateway2 2 $events2).stream
    Send-Frame $gateway2 3 @{ method = "terminal.stream.start"; params = @{ stream = $stream2 } }
    Read-Response $gateway2 3 $events2 | Out-Null
    $historyText = ""
    foreach ($event in $events2) {
        if ($event.kind -eq "history") { $historyText += [System.Text.Encoding]::UTF8.GetString([byte[]]$event.bytes) }
    }
    if (-not $historyText.Contains("WINDOWS_FORMAL_LIVE")) { throw "reconnect history did not contain prior output" }
    Send-Frame $gateway2 4 @{ method = "terminal.switch"; params = @{ terminal_id = $terminal2.terminal_id } }
    $switched = (Read-Response $gateway2 4 $events2).stream
    Send-Frame $gateway2 5 @{ method = "terminal.stream.start"; params = @{ stream = $switched } }
    Read-Response $gateway2 5 $events2 | Out-Null
    Send-Frame $gateway2 6 @{ method = "terminal.close"; params = @{ terminal_id = $terminal.terminal_id } }
    Read-Response $gateway2 6 $events2 | Out-Null
    Send-Frame $gateway2 7 @{ method = "terminal.close"; params = @{ terminal_id = $terminal2.terminal_id } }
    Read-Response $gateway2 7 $events2 | Out-Null
    $gateway2.StandardInput.Close(); $gateway2.WaitForExit(5000) | Out-Null

    $session = "muxlane-" + $project.Substring(8, 24)
    Invoke-Wsl @("/usr/bin/tmux", "-L", "muxlane-runtime", "kill-session", "-t", $session) | Out-Null
    Invoke-Cli @("recover") | Out-Null
    $status = (Invoke-Cli @("status")).result.data
    $listeners = Get-NetTCPConnection -State Listen -ErrorAction SilentlyContinue | Where-Object OwningProcess -eq $daemon.Id
    if ($listeners) { throw "unexpected Windows TCP listener for WSL daemon process" }
    Invoke-Cli @("daemon", "stop") | Out-Null
    $daemon.WaitForExit(5000) | Out-Null
    [ordered]@{
        scenario = "windows_wsl_formal_control_and_terminal"
        status = "PASS"
        daemon_instance = $status.daemon_instance_id
        terminal_id = $terminal.terminal_id
        protocol_major = $handshake.protocol_major
        reconnect_history = "PASS"
        tcp_listener = "absent"
    } | ConvertTo-Json -Compress
}
finally {
    if ($gateway -and -not $gateway.HasExited) { $gateway.Kill() }
    if ($daemon -and -not $daemon.HasExited) {
        try { Invoke-Cli @("daemon", "stop") | Out-Null } catch { $daemon.Kill() }
    }
}
