$ErrorActionPreference='Stop'
$ProgressPreference='SilentlyContinue'
[Console]::OutputEncoding=[System.Text.Encoding]::UTF8
$listeners=@(Get-NetTCPConnection -LocalPort ([int]$env:HSP_PORT) -State Listen)
if ($listeners.Count -ne 1 -or $listeners[0].LocalAddress -ne '127.0.0.1') { throw 'Listener mismatch' }
$main=Get-CimInstance Win32_Process -Filter ('ProcessId='+$listeners[0].OwningProcess)
$expected=(Resolve-Path -LiteralPath $env:HSP_EXE).ProviderPath
if (-not [string]::Equals($main.ExecutablePath,$expected,[StringComparison]::OrdinalIgnoreCase)) { throw 'Executable mismatch' }
$archive=Join-Path (Split-Path $main.ExecutablePath) 'resources/app.asar'
$hashStream=[IO.File]::OpenRead($archive)
try { $digest=[BitConverter]::ToString([Security.Cryptography.SHA256]::Create().ComputeHash($hashStream)).Replace('-','') } finally { $hashStream.Dispose() }
if ($digest -ne '983CA71114E6DFD353FC79AF5A1F9481A250EE64C2A3C757673029B811B23BC2') { throw 'Unsupported Harness build' }
$entry=$archive+'\dsh\node_modules\@deepseek-ai\dsh-desktop-host\lib\index.js'
$children=@(Get-CimInstance Win32_Process -Filter ('ParentProcessId='+$main.ProcessId) | Where-Object { $_.CommandLine.Contains($entry) })
if ($children.Count -ne 1) { throw 'Host mismatch' }
$parts=@([regex]::Matches($children[0].CommandLine,'"([^"]*)"|(\S+)') | ForEach-Object { if($_.Groups[1].Success){$_.Groups[1].Value}else{$_.Groups[2].Value} })
$position=[Array]::IndexOf($parts,$entry)
if ($position -lt 0 -or $parts.Count -le $position+2) { throw 'Profile unavailable' }
$profile=(Resolve-Path -LiteralPath $parts[$position+2]).ProviderPath
if (-not $profile.EndsWith('\profiles\desktop',[StringComparison]::OrdinalIgnoreCase)) { throw 'Unsupported profile' }
@{profile=$profile.ToLowerInvariant();hostPid=[int]$children[0].ProcessId;mainPid=[int]$main.ProcessId;adapter='0.2.0-rc.2'} | ConvertTo-Json -Compress
