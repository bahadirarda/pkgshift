[CmdletBinding()]
param(
  [string] $Version,
  [string] $To,
  [string] $DataDir
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$script:PkgshiftRepository = "bahadirarda/pkgshift"
[System.Net.ServicePointManager]::SecurityProtocol = `
  [System.Net.ServicePointManager]::SecurityProtocol -bor [System.Net.SecurityProtocolType]::Tls12

function Write-PkgshiftMessage {
  param([Parameter(Mandatory)][string] $Message)
  Write-Output "pkgshift: $Message"
}

function Throw-PkgshiftError {
  param([Parameter(Mandatory)][string] $Message)
  throw "pkgshift: error: $Message"
}

function Invoke-PkgshiftDownload {
  param(
    [Parameter(Mandatory)][string] $Uri,
    [Parameter(Mandatory)][string] $OutFile
  )
  for ($attempt = 1; $attempt -le 3; $attempt++) {
    try {
      Invoke-WebRequest -Uri $Uri -OutFile $OutFile -UseBasicParsing
      return
    } catch {
      if ($attempt -eq 3) {
        throw
      }
      Start-Sleep -Seconds $attempt
    }
  }
}

function Resolve-PkgshiftLatestTag {
  $headers = @{
    Accept = "application/vnd.github+json"
    "User-Agent" = "pkgshift-installer"
    "X-GitHub-Api-Version" = "2022-11-28"
  }
  $release = Invoke-RestMethod `
    -Uri "https://api.github.com/repos/$script:PkgshiftRepository/releases/latest" `
    -Headers $headers
  return [string] $release.tag_name
}

function Test-PkgshiftReparsePoint {
  param([Parameter(Mandatory)][System.IO.FileSystemInfo] $Item)
  return ($Item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0
}

function Assert-PkgshiftDestination {
  param(
    [Parameter(Mandatory)][string] $Path,
    [Parameter(Mandatory)][bool] $Directory
  )
  $item = Get-Item -LiteralPath $Path -Force -ErrorAction SilentlyContinue
  if ($null -eq $item) {
    return
  }
  if ((Test-PkgshiftReparsePoint -Item $item) -or ($item.PSIsContainer -ne $Directory)) {
    Throw-PkgshiftError "unsafe installation destination: $Path"
  }
}

function Remove-PkgshiftTemporaryPath {
  param([AllowNull()][string] $Path)
  if ($Path -and (Test-Path -LiteralPath $Path)) {
    try {
      Remove-Item -LiteralPath $Path -Recurse -Force
    } catch {
      Write-Warning "pkgshift: could not remove temporary path: $Path"
    }
  }
}

function Invoke-PkgshiftInstaller {
  [CmdletBinding()]
  param(
    [string] $RequestedVersion,
    [string] $InstallDirectory,
    [string] $SharedDataDirectory
  )

  if ([System.Environment]::OSVersion.Platform -ne [System.PlatformID]::Win32NT) {
    Throw-PkgshiftError "this installer supports Windows; use install.sh on Linux or macOS"
  }
  $architecture = if ($env:PROCESSOR_ARCHITEW6432) {
    $env:PROCESSOR_ARCHITEW6432
  } else {
    $env:PROCESSOR_ARCHITECTURE
  }
  if ($architecture -notin @("AMD64", "x86_64")) {
    Throw-PkgshiftError "unsupported Windows architecture: $architecture"
  }

  if (-not $RequestedVersion) {
    $RequestedVersion = if ($env:PKGSHIFT_VERSION) { $env:PKGSHIFT_VERSION } else { "latest" }
  }
  $releaseTag = if ($RequestedVersion -eq "latest") {
    Resolve-PkgshiftLatestTag
  } elseif ($RequestedVersion.StartsWith("v")) {
    $RequestedVersion
  } else {
    "v$RequestedVersion"
  }
  if ($releaseTag -notmatch '^v[0-9]+\.[0-9]+\.[0-9]+$') {
    Throw-PkgshiftError "release version must match vX.Y.Z: $releaseTag"
  }

  if (-not $InstallDirectory) {
    $InstallDirectory = if ($env:PKGSHIFT_INSTALL_DIR) {
      $env:PKGSHIFT_INSTALL_DIR
    } elseif ($env:LOCALAPPDATA) {
      Join-Path $env:LOCALAPPDATA "pkgshift\bin"
    } else {
      Throw-PkgshiftError "LOCALAPPDATA is not set; pass -To or PKGSHIFT_INSTALL_DIR"
    }
  }
  if (-not $SharedDataDirectory) {
    $SharedDataDirectory = if ($env:PKGSHIFT_DATA_DIR) {
      $env:PKGSHIFT_DATA_DIR
    } elseif ($env:LOCALAPPDATA) {
      Join-Path $env:LOCALAPPDATA "pkgshift"
    } else {
      Throw-PkgshiftError "LOCALAPPDATA is not set; pass -DataDir or PKGSHIFT_DATA_DIR"
    }
  }
  $InstallDirectory = [System.IO.Path]::GetFullPath($InstallDirectory)
  $SharedDataDirectory = [System.IO.Path]::GetFullPath($SharedDataDirectory)

  $target = "x86_64-pc-windows-msvc"
  $archive = "pkgshift-$releaseTag-$target.zip"
  $downloadRoot = "https://github.com/$script:PkgshiftRepository/releases/download/$releaseTag"
  $temporaryRoot = Join-Path ([System.IO.Path]::GetTempPath()) "pkgshift-install-$([guid]::NewGuid().ToString('N'))"
  $skillTemporary = $null
  $skillBackup = $null
  $binaryTemporary = $null
  $binaryBackup = $null
  $skillActivated = $false
  $binaryActivated = $false
  $skillDestination = $null
  $destination = $null

  try {
    New-Item -ItemType Directory -Path $temporaryRoot | Out-Null
    $archivePath = Join-Path $temporaryRoot $archive
    $checksumPath = Join-Path $temporaryRoot "SHA256SUMS"
    Write-PkgshiftMessage "downloading $releaseTag for $target"
    Invoke-PkgshiftDownload -Uri "$downloadRoot/$archive" -OutFile $archivePath
    Invoke-PkgshiftDownload -Uri "$downloadRoot/SHA256SUMS" -OutFile $checksumPath

    $matchingHashes = @()
    foreach ($line in Get-Content -LiteralPath $checksumPath) {
      if ($line -match '^(?<Hash>[0-9a-fA-F]{64})\s+\*?(?<Name>.+)$' -and $Matches.Name -eq $archive) {
        $matchingHashes += $Matches.Hash.ToLowerInvariant()
      }
    }
    if ($matchingHashes.Count -ne 1) {
      Throw-PkgshiftError "release checksum is missing or ambiguous for $archive"
    }
    Write-PkgshiftMessage "verifying SHA-256 checksum"
    $actualHash = (Get-FileHash -LiteralPath $archivePath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actualHash -ne $matchingHashes[0]) {
      Throw-PkgshiftError "checksum verification failed"
    }

    Expand-Archive -LiteralPath $archivePath -DestinationPath $temporaryRoot
    $stagingName = "pkgshift-$releaseTag-$target"
    $staging = Join-Path $temporaryRoot $stagingName
    $sourceBinary = Join-Path $staging "pkgshift.exe"
    $sourceSkill = Join-Path $staging "skills\pkgshift"
    $sourceMetadata = Join-Path $staging "release.json"
    if (-not (Test-Path -LiteralPath $sourceBinary -PathType Leaf)) {
      Throw-PkgshiftError "release archive does not contain pkgshift.exe"
    }
    if (-not (Test-Path -LiteralPath (Join-Path $sourceSkill "SKILL.md") -PathType Leaf)) {
      Throw-PkgshiftError "release archive does not contain the portable Agent Skill"
    }
    if (-not (Test-Path -LiteralPath $sourceMetadata -PathType Leaf)) {
      Throw-PkgshiftError "release archive does not contain release.json"
    }
    $metadata = Get-Content -LiteralPath $sourceMetadata -Raw | ConvertFrom-Json
    if (
      $metadata.name -ne "pkgshift" -or
      $metadata.version -ne $releaseTag.Substring(1) -or
      $metadata.tag -ne $releaseTag -or
      $metadata.target -ne $target
    ) {
      Throw-PkgshiftError "release metadata does not match the requested artifact"
    }

    $skillParent = Join-Path $SharedDataDirectory "skills"
    $skillDestination = Join-Path $skillParent "pkgshift"
    New-Item -ItemType Directory -Force -Path $skillParent | Out-Null
    Assert-PkgshiftDestination -Path $skillDestination -Directory $true
    $skillNonce = [guid]::NewGuid().ToString('N')
    $skillTemporary = Join-Path $skillParent ".pkgshift.$skillNonce.tmp"
    $skillBackup = Join-Path $skillParent ".pkgshift.$skillNonce.backup"
    Copy-Item -LiteralPath $sourceSkill -Destination $skillTemporary -Recurse

    New-Item -ItemType Directory -Force -Path $InstallDirectory | Out-Null
    $destination = Join-Path $InstallDirectory "pkgshift.exe"
    Assert-PkgshiftDestination -Path $destination -Directory $false
    $binaryNonce = [guid]::NewGuid().ToString('N')
    $binaryTemporary = Join-Path $InstallDirectory ".pkgshift.$binaryNonce.tmp.exe"
    $binaryBackup = Join-Path $InstallDirectory ".pkgshift.$binaryNonce.backup.exe"
    Copy-Item -LiteralPath $sourceBinary -Destination $binaryTemporary

    if (Test-Path -LiteralPath $skillDestination) {
      Move-Item -LiteralPath $skillDestination -Destination $skillBackup
    }
    Move-Item -LiteralPath $skillTemporary -Destination $skillDestination
    $skillTemporary = $null
    $skillActivated = $true

    if (Test-Path -LiteralPath $destination) {
      Move-Item -LiteralPath $destination -Destination $binaryBackup
    }
    Move-Item -LiteralPath $binaryTemporary -Destination $destination
    $binaryTemporary = $null
    $binaryActivated = $true

    $previousDataDirectory = $env:PKGSHIFT_DATA_DIR
    try {
      $env:PKGSHIFT_DATA_DIR = $SharedDataDirectory
      & $destination --version | Out-Null
      if ($LASTEXITCODE -ne 0) {
        Throw-PkgshiftError "installed executable failed its version check"
      }
      & $destination skill status --scope project --client codex --cwd $temporaryRoot --json --non-interactive | Out-Null
      if ($LASTEXITCODE -ne 0) {
        Throw-PkgshiftError "installed executable could not resolve its portable Agent Skill"
      }
    } finally {
      $env:PKGSHIFT_DATA_DIR = $previousDataDirectory
    }

    $binaryActivated = $false
    $skillActivated = $false
    Remove-PkgshiftTemporaryPath -Path $binaryBackup
    $binaryBackup = $null
    Remove-PkgshiftTemporaryPath -Path $skillBackup
    $skillBackup = $null
    Write-PkgshiftMessage "installed $releaseTag to $destination"
    Write-PkgshiftMessage "installed portable Agent Skill data to $skillDestination"
    $pathEntries = @($env:PATH -split ';' | ForEach-Object { $_.TrimEnd('\') })
    if ($pathEntries -notcontains $InstallDirectory.TrimEnd('\')) {
      Write-PkgshiftMessage "add $InstallDirectory to PATH before running pkgshift"
    }
  } catch {
    if ($binaryActivated -and (Test-Path -LiteralPath $destination)) {
      Remove-Item -LiteralPath $destination -Force
    }
    if ($binaryBackup -and (Test-Path -LiteralPath $binaryBackup)) {
      Move-Item -LiteralPath $binaryBackup -Destination $destination
      $binaryBackup = $null
    }
    if ($skillActivated -and (Test-Path -LiteralPath $skillDestination)) {
      Remove-Item -LiteralPath $skillDestination -Recurse -Force
    }
    if ($skillBackup -and (Test-Path -LiteralPath $skillBackup)) {
      Move-Item -LiteralPath $skillBackup -Destination $skillDestination
      $skillBackup = $null
    }
    throw
  } finally {
    Remove-PkgshiftTemporaryPath -Path $binaryTemporary
    Remove-PkgshiftTemporaryPath -Path $skillTemporary
    Remove-PkgshiftTemporaryPath -Path $temporaryRoot
  }
}

if ($MyInvocation.InvocationName -ne '.') {
  Invoke-PkgshiftInstaller -RequestedVersion $Version -InstallDirectory $To -SharedDataDirectory $DataDir
}
