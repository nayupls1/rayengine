# Windows PATH operations preserve the original spelling of unrelated entries.
Set-StrictMode -Version Latest

function Get-PathKey([string] $Entry) {
    $expanded = [Environment]::ExpandEnvironmentVariables($Entry.Trim().Trim('"'))
    return $expanded.Replace('/', '\').TrimEnd('\').ToUpperInvariant()
}

function Test-PathEntry([string] $Path, [string] $Entry) {
    $key = Get-PathKey $Entry
    foreach ($part in $Path.Split(';')) {
        if ((Get-PathKey $part) -eq $key) { return $true }
    }
    return $false
}

function Add-PathEntry([string] $Path, [string] $Entry) {
    if (Test-PathEntry $Path $Entry) { return $Path }
    if ([string]::IsNullOrEmpty($Path)) { return $Entry }
    # Preserve even a trailing empty entry in the existing PATH.
    return "$Path;$Entry"
}

function Remove-PathEntries([string] $Path, [string[]] $Entries) {
    $keys = @($Entries | ForEach-Object { Get-PathKey $_ })
    $kept = @($Path.Split(';') | Where-Object { (Get-PathKey $_) -notin $keys })
    return [string]::Join(';', $kept)
}

function Get-UserPath {
    $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment')
    try {
        if ($null -eq $key) { return '' }
        return [string] $key.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
    } finally { if ($null -ne $key) { $key.Dispose() } }
}

function Set-UserPath([string] $Path) {
    $key = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey('Environment')
    try {
        $kind = [Microsoft.Win32.RegistryValueKind]::ExpandString
        if ('Path' -in $key.GetValueNames()) { $kind = $key.GetValueKind('Path') }
        $key.SetValue('Path', $Path, $kind)
    } finally { $key.Dispose() }
    # Let Explorer refresh its environment for subsequent terminals.
    if (-not ('RayengineSetup.EnvironmentBroadcast' -as [type])) {
        Add-Type @'
using System;
using System.Runtime.InteropServices;
namespace RayengineSetup {
    public static class EnvironmentBroadcast {
        [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        public static extern IntPtr SendMessageTimeout(IntPtr window, uint message,
            UIntPtr wParam, string lParam, uint flags, uint timeout, out UIntPtr result);
    }
}
'@
    }
    $result = [UIntPtr]::Zero
    [void] [RayengineSetup.EnvironmentBroadcast]::SendMessageTimeout(
        [IntPtr] 0xffff, 0x1a, [UIntPtr]::Zero, 'Environment', 2, 5000, [ref] $result)
}

Export-ModuleMember -Function Get-PathKey, Test-PathEntry, Add-PathEntry, Remove-PathEntries, Get-UserPath, Set-UserPath
