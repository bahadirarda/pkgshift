Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repositoryRoot = Split-Path -Parent $PSScriptRoot
. (Join-Path $repositoryRoot "site\install.ps1")

function Assert-PkgshiftTest {
  param(
    [Parameter(Mandatory)][bool] $Condition,
    [Parameter(Mandatory)][string] $Message
  )
  if (-not $Condition) {
    throw "Windows installer test failed: $Message"
  }
}

function Publish-MockArchive {
  param(
    [Parameter(Mandatory)][string] $Staging,
    [Parameter(Mandatory)][string] $ArchivePath,
    [Parameter(Mandatory)][string] $ChecksumPath
  )
  if (Test-Path -LiteralPath $ArchivePath) {
    Remove-Item -LiteralPath $ArchivePath -Force
  }
  Compress-Archive -Path $Staging -DestinationPath $ArchivePath
  $hash = (Get-FileHash -LiteralPath $ArchivePath -Algorithm SHA256).Hash.ToLowerInvariant()
  Set-Content -LiteralPath $ChecksumPath -Value "$hash  $([System.IO.Path]::GetFileName($ArchivePath))"
}

$root = Join-Path ([System.IO.Path]::GetTempPath()) "pkgshift-windows-installer-test-$([guid]::NewGuid().ToString('N'))"
try {
  $version = "v0.20260817.5"
  $target = "x86_64-pc-windows-msvc"
  $stagingName = "pkgshift-$version-$target"
  $releaseDirectory = Join-Path $root "release"
  $staging = Join-Path $root $stagingName
  $skillSource = Join-Path $staging "skills\pkgshift"
  $installDirectory = Join-Path $root "bin"
  $dataDirectory = Join-Path $root "data"
  New-Item -ItemType Directory -Force -Path $releaseDirectory, $skillSource | Out-Null

  $fakeBinarySource = @'
using System;
using System.IO;

public static class PkgshiftInstallerFixture
{
    public static int Main(string[] args)
    {
        if (args.Length == 1 && args[0] == "--version") return 0;
        if (args.Length > 0 && args[0] == "skill")
        {
            var root = Environment.GetEnvironmentVariable("PKGSHIFT_DATA_DIR");
            if (!String.IsNullOrEmpty(root) && File.Exists(Path.Combine(root, "skills", "pkgshift", "SKILL.md"))) return 0;
        }
        return 2;
    }
}
'@
  Add-Type `
    -TypeDefinition $fakeBinarySource `
    -OutputAssembly (Join-Path $staging "pkgshift.exe") `
    -OutputType ConsoleApplication
  Set-Content -LiteralPath (Join-Path $skillSource "SKILL.md") -Value @"
---
name: pkgshift
description: Windows installer fixture one.
---
"@
  [ordered]@{
    name = "pkgshift"
    version = $version.Substring(1)
    buildId = "$($version.Substring(1))+sha.000000000000"
    tag = $version
    commit = "0000000000000000000000000000000000000000"
    commitDate = "2026-08-17"
    target = $target
  } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $staging "release.json")

  $archiveName = "$stagingName.zip"
  $archivePath = Join-Path $releaseDirectory $archiveName
  $checksumPath = Join-Path $releaseDirectory "SHA256SUMS"
  Publish-MockArchive -Staging $staging -ArchivePath $archivePath -ChecksumPath $checksumPath

  $script:PkgshiftMockReleaseDirectory = $releaseDirectory
  function Invoke-PkgshiftDownload {
    param(
      [Parameter(Mandatory)][string] $Uri,
      [Parameter(Mandatory)][string] $OutFile
    )
    $name = [System.IO.Path]::GetFileName(([uri] $Uri).AbsolutePath)
    Copy-Item -LiteralPath (Join-Path $script:PkgshiftMockReleaseDirectory $name) -Destination $OutFile
  }

  $first = @(Invoke-PkgshiftInstaller `
    -RequestedVersion $version `
    -InstallDirectory $installDirectory `
    -SharedDataDirectory $dataDirectory)
  $destinationBinary = Join-Path $installDirectory "pkgshift.exe"
  $destinationSkill = Join-Path $dataDirectory "skills\pkgshift"
  Assert-PkgshiftTest -Condition (Test-Path -LiteralPath $destinationBinary -PathType Leaf) -Message "binary was not installed"
  Assert-PkgshiftTest -Condition (Test-Path -LiteralPath (Join-Path $destinationSkill "SKILL.md") -PathType Leaf) -Message "Skill data was not installed"
  Assert-PkgshiftTest -Condition (($first -join "`n") -like "*installed portable Agent Skill data*") -Message "success output omitted Skill data"

  Set-Content -LiteralPath (Join-Path $destinationSkill "stale.md") -Value "stale"
  Set-Content -LiteralPath (Join-Path $skillSource "SKILL.md") -Value @"
---
name: pkgshift
description: Windows installer fixture two.
---
"@
  Publish-MockArchive -Staging $staging -ArchivePath $archivePath -ChecksumPath $checksumPath
  Invoke-PkgshiftInstaller `
    -RequestedVersion $version `
    -InstallDirectory $installDirectory `
    -SharedDataDirectory $dataDirectory | Out-Null
  Assert-PkgshiftTest -Condition (-not (Test-Path -LiteralPath (Join-Path $destinationSkill "stale.md"))) -Message "atomic refresh retained stale Skill data"
  Assert-PkgshiftTest -Condition ((Get-Content -LiteralPath (Join-Path $destinationSkill "SKILL.md") -Raw) -like "*fixture two*") -Message "atomic refresh did not install new Skill data"

  $installedHash = (Get-FileHash -LiteralPath $destinationBinary -Algorithm SHA256).Hash
  Add-Content -LiteralPath $archivePath -Value "corruption"
  $checksumRejected = $false
  try {
    Invoke-PkgshiftInstaller `
      -RequestedVersion $version `
      -InstallDirectory $installDirectory `
      -SharedDataDirectory $dataDirectory | Out-Null
  } catch {
    $checksumRejected = $_.Exception.Message -like "*checksum verification failed*"
  }
  Assert-PkgshiftTest -Condition $checksumRejected -Message "corrupted archive was not rejected"
  Assert-PkgshiftTest -Condition ((Get-FileHash -LiteralPath $destinationBinary -Algorithm SHA256).Hash -eq $installedHash) -Message "checksum failure changed the installed binary"

  Write-Output "Windows installer acceptance passed."
} finally {
  if (Test-Path -LiteralPath $root) {
    Remove-Item -LiteralPath $root -Recurse -Force
  }
}
