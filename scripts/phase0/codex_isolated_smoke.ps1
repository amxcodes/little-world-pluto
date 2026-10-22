$ErrorActionPreference = 'Stop'

function Read-ServerLine {
    param(
        [Parameter(Mandatory)]
        [System.IO.StreamReader] $Reader,

        [Parameter(Mandatory)]
        [int] $TimeoutMs
    )

    $read = $Reader.ReadLineAsync()
    if (-not $read.Wait($TimeoutMs)) {
        throw "Timed out waiting for app-server output after $TimeoutMs ms."
    }

    $read.Result
}

function Read-CorrelatedResponse {
    param(
        [Parameter(Mandatory)]
        [System.IO.StreamReader] $Reader,

        [Parameter(Mandatory)]
        [int] $RequestId,

        [Parameter(Mandatory)]
        [int] $TimeoutMs
    )

    $deadline = [DateTime]::UtcNow.AddMilliseconds($TimeoutMs)
    while ([DateTime]::UtcNow -lt $deadline) {
        $remaining = [int][Math]::Max(1, ($deadline - [DateTime]::UtcNow).TotalMilliseconds)
        $line = Read-ServerLine -Reader $Reader -TimeoutMs $remaining
        if ($null -eq $line) {
            break
        }

        $message = $line | ConvertFrom-Json
        $hasResponseBody = $null -ne $message.PSObject.Properties['result'] -or
            $null -ne $message.PSObject.Properties['error']
        if ($null -ne $message.PSObject.Properties['id'] -and
            $null -eq $message.PSObject.Properties['method'] -and
            $hasResponseBody -and
            [string]$message.id -eq [string]$RequestId) {
            return $message
        }

        # Notifications may contain machine-specific identifiers; discard their payloads.
    }

    throw "Timed out waiting for app-server response id $RequestId."
}

function Read-CorrelatedResponses {
    param(
        [Parameter(Mandatory)]
        [System.IO.StreamReader] $Reader,

        [Parameter(Mandatory)]
        [string[]] $RequestIds,

        [Parameter(Mandatory)]
        [int] $TimeoutMs
    )

    $pending = [System.Collections.Generic.List[string]]::new()
    foreach ($requestId in $RequestIds) {
        $pending.Add($requestId)
    }
    $responses = @{}
    $deadline = [DateTime]::UtcNow.AddMilliseconds($TimeoutMs)

    while ($pending.Count -gt 0 -and [DateTime]::UtcNow -lt $deadline) {
        $remaining = [int][Math]::Max(1, ($deadline - [DateTime]::UtcNow).TotalMilliseconds)
        $line = Read-ServerLine -Reader $Reader -TimeoutMs $remaining
        if ($null -eq $line) {
            break
        }

        $message = $line | ConvertFrom-Json
        $hasResponseBody = $null -ne $message.PSObject.Properties['result'] -or
            $null -ne $message.PSObject.Properties['error']
        if ($null -eq $message.PSObject.Properties['id'] -or
            $null -ne $message.PSObject.Properties['method'] -or
            -not $hasResponseBody) {
            # Notifications and server-initiated requests are not client responses.
            continue
        }

        $requestId = [string]$message.id
        if ($pending.Contains($requestId)) {
            $responses[$requestId] = $message
            $pending.Remove($requestId) | Out-Null
        }
    }

    if ($pending.Count -gt 0) {
        throw "Timed out waiting for app-server response ids: $($pending -join ', ')."
    }

    $responses
}

function Send-ServerMessage {
    param(
        [Parameter(Mandatory)]
        [System.Diagnostics.Process] $Process,

        [Parameter(Mandatory)]
        [hashtable] $Message
    )

    $Process.StandardInput.WriteLine(($Message | ConvertTo-Json -Depth 16 -Compress))
    $Process.StandardInput.Flush()
}

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$targetRoot = Join-Path $repoRoot 'target'
New-Item -ItemType Directory -Path $targetRoot -Force | Out-Null
$profilePath = Join-Path $targetRoot ('codex-isolated-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $profilePath | Out-Null

$codex = Get-Command codex -ErrorAction Stop
$process = [System.Diagnostics.Process]::new()
$process.StartInfo = [System.Diagnostics.ProcessStartInfo]::new()
$process.StartInfo.FileName = $codex.Source
$process.StartInfo.Arguments = 'app-server --listen stdio://'
$process.StartInfo.UseShellExecute = $false
$process.StartInfo.RedirectStandardInput = $true
$process.StartInfo.RedirectStandardOutput = $true
$process.StartInfo.RedirectStandardError = $true
$process.StartInfo.CreateNoWindow = $true
$process.StartInfo.EnvironmentVariables['CODEX_HOME'] = $profilePath
$started = $false

try {
    if (-not $process.Start()) {
        throw 'Could not start the Codex app-server.'
    }
    $started = $true

    $stderr = $process.StandardError.ReadToEndAsync()
    Send-ServerMessage -Process $process -Message @{
        method = 'initialize'
        id = 0
        params = @{
            clientInfo = @{
                name = 'island-desktop-phase0'
                title = 'Island Phase 0 isolated smoke'
                version = '0.1.0'
            }
            capabilities = @{ experimentalApi = $false }
        }
    }

    $line = Read-ServerLine -Reader $process.StandardOutput -TimeoutMs 15000
    if ($null -eq $line) {
        $process.WaitForExit(1000) | Out-Null
        $stderrText = if ($stderr.IsCompleted) { $stderr.Result.Trim() } else { '' }
        throw "The app-server exited before initialize (exit code $($process.ExitCode)). $stderrText"
    }
    $initialize = $line | ConvertFrom-Json
    if ($initialize.id -ne 0 -or -not $initialize.result) {
        throw 'Initialize did not return a success response for request id 0.'
    }
    if ([IO.Path]::GetFullPath($initialize.result.codexHome) -ne [IO.Path]::GetFullPath($profilePath)) {
        throw 'The app-server did not use the isolated CODEX_HOME created for this smoke test.'
    }

    Send-ServerMessage -Process $process -Message @{ method = 'initialized'; params = @{} }
    Send-ServerMessage -Process $process -Message @{
        method = 'island/phase0UnknownMethod'
        id = 11
        params = @{}
    }
    $unknownMethod = Read-CorrelatedResponse -Reader $process.StandardOutput -RequestId 11 -TimeoutMs 15000
    if ($null -eq $unknownMethod.error) {
        throw 'The isolated app-server did not reject the unknown method with an error response.'
    }
    Send-ServerMessage -Process $process -Message @{
        method = 'thread/list'
        id = 12
        params = @{ limit = 1; useStateDbOnly = $true }
    }
    $afterUnknown = Read-CorrelatedResponse -Reader $process.StandardOutput -RequestId 12 -TimeoutMs 15000
    if ($afterUnknown.error) {
        throw 'The isolated app-server did not remain usable after an unknown-method error.'
    }
    Write-Output 'PASS protocol error recovery: unknown method returned an error; next request succeeded'

    Send-ServerMessage -Process $process -Message @{
        method = 'thread/list'
        id = 1
        params = @{ limit = 1; useStateDbOnly = $true }
    }

    $threadList = Read-CorrelatedResponse -Reader $process.StandardOutput -RequestId 1 -TimeoutMs 15000
    if ($threadList.error) {
        throw 'The isolated thread/list request returned a protocol error.'
    }

    $count = @($threadList.result.data).Count
    Write-Output "PASS initialize: $($initialize.result.userAgent)"
    Write-Output "PASS thread/list: isolated sessions=$count"

    Send-ServerMessage -Process $process -Message @{
        method = 'thread/start'
        id = 2
        params = @{ cwd = $repoRoot }
    }
    $threadStart = Read-CorrelatedResponse -Reader $process.StandardOutput -RequestId 2 -TimeoutMs 15000
    if ($threadStart.error -or -not $threadStart.result.thread.id) {
        throw 'The isolated thread/start request did not return a thread id.'
    }

    $threadId = [string]$threadStart.result.thread.id
    Send-ServerMessage -Process $process -Message @{
        method = 'thread/read'
        id = 3
        params = @{ threadId = $threadId; includeTurns = $false }
    }
    $threadRead = Read-CorrelatedResponse -Reader $process.StandardOutput -RequestId 3 -TimeoutMs 15000
    if ($threadRead.error -or [string]$threadRead.result.thread.id -ne $threadId) {
        throw 'The isolated thread/read request did not return the started thread.'
    }

    $threadName = 'Island Phase 0 smoke'
    Send-ServerMessage -Process $process -Message @{
        method = 'thread/name/set'
        id = 16
        params = @{ threadId = $threadId; name = $threadName }
    }
    $threadNameSet = Read-CorrelatedResponse -Reader $process.StandardOutput -RequestId 16 -TimeoutMs 15000
    if ($threadNameSet.error) {
        throw 'The isolated thread/name/set request failed for the disposable thread.'
    }
    Send-ServerMessage -Process $process -Message @{
        method = 'thread/read'
        id = 17
        params = @{ threadId = $threadId; includeTurns = $false }
    }
    $threadNameRead = Read-CorrelatedResponse -Reader $process.StandardOutput -RequestId 17 -TimeoutMs 15000
    if ($threadNameRead.error -or [string]$threadNameRead.result.thread.name -ne $threadName) {
        throw 'The isolated thread/read request did not reflect the updated thread name.'
    }

    Write-Output 'PASS thread/start + thread/read + thread/name/set: isolated identity and renamed title retained'

    Send-ServerMessage -Process $process -Message @{
        method = 'thread/resume'
        id = 13
        params = @{ threadId = $threadId; excludeTurns = $true }
    }
    $threadResume = Read-CorrelatedResponse -Reader $process.StandardOutput -RequestId 13 -TimeoutMs 15000
    if ($threadResume.error) {
        if ([int]$threadResume.error.code -ne -32600 -or [string]$threadResume.error.message -notmatch 'rollout') {
            throw 'The isolated thread/resume request failed for a reason other than the missing rollout.'
        }
        Write-Output 'PASS thread/resume empty-thread path: app-server reports no saved rollout'
    }
    elseif ([string]$threadResume.result.thread.id -eq $threadId) {
        Write-Output 'PASS thread/resume empty-thread path: app-server resumed the same thread without a model turn'
    }
    else {
        throw 'The isolated thread/resume request returned neither the expected rollout error nor the same thread.'
    }

    Send-ServerMessage -Process $process -Message @{
        method = 'thread/fork'
        id = 14
        params = @{ threadId = $threadId; cwd = $repoRoot }
    }
    $threadFork = Read-CorrelatedResponse -Reader $process.StandardOutput -RequestId 14 -TimeoutMs 15000
    if ($threadFork.error) {
        if ([int]$threadFork.error.code -ne -32600 -or [string]$threadFork.error.message -notmatch 'rollout') {
            throw 'The isolated thread/fork request failed for a reason other than the missing rollout.'
        }
        Write-Output 'PASS thread/fork empty-thread path: app-server reports no saved rollout'
    }
    elseif ($threadFork.result.thread.id -and [string]$threadFork.result.thread.id -ne $threadId) {
        $forkedThreadId = [string]$threadFork.result.thread.id
        Send-ServerMessage -Process $process -Message @{
            method = 'thread/archive'
            id = 15
            params = @{ threadId = $forkedThreadId }
        }
        $forkArchive = Read-CorrelatedResponse -Reader $process.StandardOutput -RequestId 15 -TimeoutMs 15000
        if ($forkArchive.error) {
            throw 'The isolated fork thread was created but could not be archived.'
        }
        Write-Output 'PASS thread/fork empty-thread path: a distinct thread was created and archived without a model turn'
    }
    else {
        throw 'The isolated thread/fork request returned neither the expected rollout error nor a distinct thread.'
    }

    Send-ServerMessage -Process $process -Message @{
        method = 'thread/archive'
        id = 4
        params = @{ threadId = $threadId }
    }
    $threadArchive = Read-CorrelatedResponse -Reader $process.StandardOutput -RequestId 4 -TimeoutMs 15000
    if ($threadArchive.error) {
        throw 'The isolated thread/archive request failed for the disposable thread.'
    }

    Send-ServerMessage -Process $process -Message @{
        method = 'thread/unarchive'
        id = 18
        params = @{ threadId = $threadId }
    }
    $threadUnarchive = Read-CorrelatedResponse -Reader $process.StandardOutput -RequestId 18 -TimeoutMs 15000
    if ($threadUnarchive.error -or [string]$threadUnarchive.result.thread.id -ne $threadId) {
        throw 'The isolated thread/unarchive request did not restore the same thread.'
    }
    Send-ServerMessage -Process $process -Message @{
        method = 'thread/read'
        id = 19
        params = @{ threadId = $threadId; includeTurns = $false }
    }
    $threadUnarchiveRead = Read-CorrelatedResponse -Reader $process.StandardOutput -RequestId 19 -TimeoutMs 15000
    if ($threadUnarchiveRead.error -or [string]$threadUnarchiveRead.result.thread.id -ne $threadId -or
        [string]$threadUnarchiveRead.result.thread.name -ne $threadName) {
        throw 'The isolated thread/read request did not preserve identity and name after unarchive.'
    }
    Send-ServerMessage -Process $process -Message @{
        method = 'thread/archive'
        id = 20
        params = @{ threadId = $threadId }
    }
    $threadRearchive = Read-CorrelatedResponse -Reader $process.StandardOutput -RequestId 20 -TimeoutMs 15000
    if ($threadRearchive.error) {
        throw 'The isolated thread could not be re-archived after the unarchive check.'
    }

    Write-Output 'PASS thread/archive + thread/unarchive + thread/read: archived state restored and same identity retained'

    foreach ($requestId in @(5, 6)) {
        Send-ServerMessage -Process $process -Message @{
            method = 'thread/start'
            id = $requestId
            params = @{ cwd = $repoRoot }
        }
    }
    $concurrentStarts = Read-CorrelatedResponses -Reader $process.StandardOutput -RequestIds @('5', '6') -TimeoutMs 15000
    if ($concurrentStarts['5'].error -or $concurrentStarts['6'].error -or
        -not $concurrentStarts['5'].result.thread.id -or
        -not $concurrentStarts['6'].result.thread.id) {
        throw 'Concurrent thread/start requests did not both return thread identities.'
    }
    $threadA = [string]$concurrentStarts['5'].result.thread.id
    $threadB = [string]$concurrentStarts['6'].result.thread.id
    if ($threadA -eq $threadB) {
        throw 'Concurrent thread/start requests returned the same thread identity.'
    }

    Send-ServerMessage -Process $process -Message @{
        method = 'thread/read'
        id = 7
        params = @{ threadId = $threadA; includeTurns = $false }
    }
    Send-ServerMessage -Process $process -Message @{
        method = 'thread/read'
        id = 8
        params = @{ threadId = $threadB; includeTurns = $false }
    }
    $concurrentReads = Read-CorrelatedResponses -Reader $process.StandardOutput -RequestIds @('7', '8') -TimeoutMs 15000
    if ($concurrentReads['7'].error -or $concurrentReads['8'].error -or
        [string]$concurrentReads['7'].result.thread.id -ne $threadA -or
        [string]$concurrentReads['8'].result.thread.id -ne $threadB) {
        throw 'Concurrent thread/read responses did not preserve request-to-thread identity.'
    }

    Send-ServerMessage -Process $process -Message @{
        method = 'thread/archive'
        id = 9
        params = @{ threadId = $threadA }
    }
    Send-ServerMessage -Process $process -Message @{
        method = 'thread/archive'
        id = 10
        params = @{ threadId = $threadB }
    }
    $concurrentArchives = Read-CorrelatedResponses -Reader $process.StandardOutput -RequestIds @('9', '10') -TimeoutMs 15000
    if ($concurrentArchives['9'].error -or $concurrentArchives['10'].error) {
        throw 'A disposable concurrently created thread could not be archived.'
    }

    Write-Output 'PASS concurrent requests: two isolated threads started, read by correlated ID, and archived'
    Write-Output 'No model turn was started; the smoke remains isolated in its disposable CODEX_HOME.'

    $process.StandardInput.Close()
    if (-not $process.WaitForExit(10000)) {
        throw 'The app-server did not exit after stdin was closed.'
    }
    if ($process.ExitCode -ne 0) {
        throw "The app-server exited with code $($process.ExitCode)."
    }

    Write-Output 'PASS shutdown: app-server exited cleanly after stdin closed.'
}
finally {
    if ($started -and -not $process.HasExited) {
        $process.Kill()
        $process.WaitForExit()
    }
    if ($process) {
        $process.Dispose()
    }

    if (Test-Path -LiteralPath $profilePath) {
        $resolvedTarget = (Resolve-Path -LiteralPath $targetRoot).Path.TrimEnd('\')
        $resolvedProfile = (Resolve-Path -LiteralPath $profilePath).Path
        $profileParent = [IO.Path]::GetDirectoryName($resolvedProfile)
        $profileName = [IO.Path]::GetFileName($resolvedProfile)
        if ($profileParent -ne $resolvedTarget -or $profileName -notmatch '^codex-isolated-[0-9a-f]{32}$') {
            throw 'Refusing to remove an isolated Codex profile outside the expected target directory.'
        }
        $cleanupAttempt = 0
        while ($true) {
            try {
                Remove-Item -LiteralPath $resolvedProfile -Recurse -Force
                break
            }
            catch {
                $cleanupAttempt++
                if ($cleanupAttempt -ge 45) {
                    throw
                }
                Start-Sleep -Milliseconds ([Math]::Min(1000, 500 * $cleanupAttempt))
            }
        }
    }
}
