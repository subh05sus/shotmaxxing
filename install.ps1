# Installs Shotmaxxing on Windows 10 (version 2004) or later, 64-bit. Paste this line into PowerShell:
#
#   irm https://raw.githubusercontent.com/subh05sus/shotmaxxing/main/install.ps1 | iex
#
# It downloads the setup program from the GitHub release named below, checks it against the SHA-256 written here,
# installs Shotmaxxing for this Windows user (no administrator rights needed) and opens it. Run the same line again
# to update.
#
# Why SmartScreen doesn't stop it: the setup program isn't signed yet, and Windows warns about an unsigned program
# when it carries the "downloaded from the internet" mark that browsers add. PowerShell's download doesn't add that
# mark. Nothing on the computer is switched off: the download is checked against the hash below instead.
#
# Optional setting:
#   $env:SHOTMAXXING_NO_OPEN = '1'   don't open the app at the end
#
# Each release changes $Version and $Sha256. GitHub shows the SHA-256 on the release page, next to the -setup.exe;
# the CI checks that this script, install.sh, the cask and the README name the same release.

# Everything runs in this block, so its settings don't stay in the PowerShell window, and an error ends the
# install without closing the window.
& {
  $ErrorActionPreference = 'Stop'
  # Windows PowerShell 5.1 downloads many times slower while it draws a progress bar.
  $ProgressPreference = 'SilentlyContinue'

  $Version = '0.2.0'
  $Sha256 = '36EB0F26AD8172DCC20EACE4F2F46B5C982DBEF1B9D112E38AE680FA376B9B93'
  $Url = "https://github.com/subh05sus/shotmaxxing/releases/download/v$Version/Shotmaxxing_${Version}_x64-setup.exe"

  $tmp = Join-Path ([IO.Path]::GetTempPath()) ('shotmaxxing-' + [Guid]::NewGuid().ToString('N'))
  try {
    if (-not [Environment]::Is64BitOperatingSystem) { throw 'it needs 64-bit Windows.' }
    $build = [Environment]::OSVersion.Version.Build
    if ($build -lt 19041) {
      throw "it needs Windows 10 version 2004 (build 19041) or later, and this computer has build $build."
    }
    # A silent install closes a running Shotmaxxing without asking, which would end a recording.
    if (Get-Process -Name 'shotmaxxing-desktop' -ErrorAction SilentlyContinue) {
      throw 'Shotmaxxing is open. Quit it first (Quit Shotmaxxing in its icon''s menu, by the clock), then run the line again.'
    }

    # GitHub takes TLS 1.2 or later, which Windows PowerShell 5.1 doesn't always offer by itself.
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

    New-Item -ItemType Directory -Path $tmp | Out-Null
    $setup = Join-Path $tmp "Shotmaxxing_${Version}_x64-setup.exe"
    Write-Host "Downloading Shotmaxxing $Version..."
    try {
      Invoke-WebRequest -Uri $Url -OutFile $setup -UseBasicParsing
    } catch {
      throw "the download didn't finish ($($_.Exception.Message)). Check the internet connection, then run the line again."
    }
    $got = (Get-FileHash -LiteralPath $setup -Algorithm SHA256).Hash
    if ($got -ne $Sha256) {
      throw "the download doesn't match its SHA-256 (expected $Sha256, got $got), so it was thrown away."
    }

    Write-Host 'Installing...'
    $run = Start-Process -FilePath $setup -ArgumentList '/S' -Wait -PassThru
    # Windows PowerShell can leave ExitCode empty for a program that has ended; the check below then tells.
    if ($null -ne $run.ExitCode -and $run.ExitCode -ne 0) {
      throw "the setup program stopped with code $($run.ExitCode)."
    }

    # Where the setup program says it put the app.
    $entry = Get-ItemProperty -Path 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Shotmaxxing' -ErrorAction SilentlyContinue
    if (-not $entry) { throw "the setup program finished, but Windows doesn't list Shotmaxxing as installed." }
    $dir = "$($entry.InstallLocation)".Trim('"')
    $main = if ($entry.MainBinaryName) { "$($entry.MainBinaryName)" } else { 'shotmaxxing-desktop.exe' }
    $exe = Join-Path $dir $main
    if (-not (Test-Path -LiteralPath $exe)) { throw "the setup program finished, but $exe isn't there." }

    Write-Host ''
    Write-Host "Shotmaxxing $Version is installed in $dir."
    $cli = Join-Path $dir 'shotmaxxing.exe'
    if (Test-Path -LiteralPath $cli) { Write-Host "The shotmaxxing command is $cli." }
    if ($env:SHOTMAXXING_NO_OPEN) {
      Write-Host 'To update it later, run the same line again.'
    } else {
      Write-Host 'Opening it now. To update it later, run the same line again.'
      Start-Process -FilePath $exe
    }
  } catch {
    Write-Host ''
    Write-Host "Shotmaxxing was not installed: $($_.Exception.Message)" -ForegroundColor Red
  } finally {
    Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue
  }
}
