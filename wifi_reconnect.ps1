param(
    [string]$Ssid = "MARKETHOUSE_2.4GHz",
    [string]$Password = "Mh58981658",
    [int]$CheckIntervalSeconds = 15
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Write-Log {
    param(
        [string]$Message,
        [string]$Level = "INFO"
    )

    $timestamp = Get-Date -Format "yyyy-MM-dd HH:mm:ss"
    Write-Host "[$timestamp] [$Level] $Message"
}

function Get-WifiStatus {
    $output = netsh wlan show interfaces | Out-String
    $state = $null
    $ssid = $null

    foreach ($line in ($output -split "`r?`n")) {
        if (-not $state -and $line -match "^\s*State\s*:\s*(.+?)\s*$") {
            $state = $Matches[1].Trim()
            continue
        }

        if (-not $ssid -and $line -match "^\s*SSID\s*:\s*(.+?)\s*$") {
            $ssid = $Matches[1].Trim()
            continue
        }
    }

    [PSCustomObject]@{
        State = $state
        SSID  = $ssid
    }
}

function Test-WifiProfileExists {
    param(
        [Parameter(Mandatory = $true)]
        [string]$ProfileName
    )

    $output = netsh wlan show profiles | Out-String
    foreach ($line in ($output -split "`r?`n")) {
        if ($line -match "^\s*(All User Profile|User Profile)\s*:\s*(.+?)\s*$") {
            if ($Matches[2].Trim() -eq $ProfileName) {
                return $true
            }
        }
    }

    return $false
}

function Ensure-WifiProfile {
    param(
        [Parameter(Mandatory = $true)]
        [string]$ProfileSsid,
        [Parameter(Mandatory = $true)]
        [string]$ProfilePassword
    )

    if (Test-WifiProfileExists -ProfileName $ProfileSsid) {
        Write-Log "Wi-Fi profile '$ProfileSsid' already exists."
        return
    }

    Write-Log "Creating Wi-Fi profile for '$ProfileSsid'."

    $escapedSsid = [System.Security.SecurityElement]::Escape($ProfileSsid)
    $escapedPassword = [System.Security.SecurityElement]::Escape($ProfilePassword)

    $profileXml = @"
<?xml version="1.0"?>
<WLANProfile xmlns="http://www.microsoft.com/networking/WLAN/profile/v1">
    <name>$escapedSsid</name>
    <SSIDConfig>
        <SSID>
            <name>$escapedSsid</name>
        </SSID>
    </SSIDConfig>
    <connectionType>ESS</connectionType>
    <connectionMode>auto</connectionMode>
    <MSM>
        <security>
            <authEncryption>
                <authentication>WPA2PSK</authentication>
                <encryption>AES</encryption>
                <useOneX>false</useOneX>
            </authEncryption>
            <sharedKey>
                <keyType>passPhrase</keyType>
                <protected>false</protected>
                <keyMaterial>$escapedPassword</keyMaterial>
            </sharedKey>
        </security>
    </MSM>
</WLANProfile>
"@

    $safeSsid = ($ProfileSsid -replace '[^\w\.-]', "_")
    $profilePath = Join-Path -Path ([System.IO.Path]::GetTempPath()) -ChildPath "wifi_profile_$safeSsid.xml"

    try {
        Set-Content -Path $profilePath -Value $profileXml -Encoding UTF8
        netsh wlan add profile filename="$profilePath" user=current | Out-Null
        if ($LASTEXITCODE -ne 0) {
            throw "Failed to add Wi-Fi profile '$ProfileSsid'. netsh exit code: $LASTEXITCODE"
        }
    }
    finally {
        if (Test-Path $profilePath) {
            Remove-Item -Path $profilePath -Force
        }
    }
}

Ensure-WifiProfile -ProfileSsid $Ssid -ProfilePassword $Password
netsh wlan set profileparameter name="$Ssid" connectionmode=auto | Out-Null

Write-Log "Monitoring Wi-Fi and reconnecting to '$Ssid' when needed. Press Ctrl+C to stop."

while ($true) {
    $status = Get-WifiStatus
    $isOnTargetNetwork = ($status.State -match "connected") -and ($status.SSID -eq $Ssid)

    if (-not $isOnTargetNetwork) {
        Write-Log "Current state: '$($status.State)', SSID: '$($status.SSID)'. Reconnecting to '$Ssid'."
        netsh wlan connect name="$Ssid" ssid="$Ssid" | Out-Null

        if ($LASTEXITCODE -eq 0) {
            Write-Log "Reconnect command sent."
        }
        else {
            Write-Log "Reconnect command failed (exit code: $LASTEXITCODE)." "WARN"
        }
    }

    Start-Sleep -Seconds $CheckIntervalSeconds
}
