param(
    [Parameter(Mandatory)][int]$AppProcessId,
    [Parameter(Mandatory)][long]$ParentWindowHandle,
    [Parameter(Mandatory)][ValidateSet('Cancel', 'Choose')][string]$Mode,
    [Parameter(Mandatory)][string]$FixtureRoot,
    [string]$Folder,
    [ValidateRange(1, 20)][int]$TimeoutSeconds = 20
)
$ErrorActionPreference = 'Stop'
# Test only. Never inspect names/values from the desktop or send global input.
# ValuePattern: https://learn.microsoft.com/en-us/dotnet/framework/ui-automation/add-content-to-a-text-box-using-ui-automation
$stage = 'initialize test-only native automation'
$failureReason = $null
$sendError = $null
$observedControls = [System.Collections.Generic.List[object]]::new()
function Stop-Probe([string]$Reason) { $script:failureReason = $Reason; throw 'Owned native folder probe failed.' }
try {
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public static class OwnedFolderProbe {
    public delegate bool EnumProc(IntPtr window, IntPtr data);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc callback, IntPtr data);
    [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr window);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")] public static extern bool IsWindowEnabled(IntPtr window);
    [DllImport("user32.dll")] public static extern bool IsChild(IntPtr parent, IntPtr child);
    [DllImport("user32.dll")] public static extern IntPtr GetDlgItem(IntPtr parent, int id);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window, out uint processId);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr window, StringBuilder name, int length);
    [DllImport("user32.dll")] static extern IntPtr GetWindow(IntPtr window, uint command);
    [DllImport("user32.dll", SetLastError=true)] public static extern IntPtr SendMessageTimeout(IntPtr window, uint message, UIntPtr wParam, IntPtr lParam, uint flags, uint timeout, out UIntPtr result);
    [DllImport("user32.dll", EntryPoint="SendMessageTimeoutW", CharSet=CharSet.Unicode, SetLastError=true)] public static extern IntPtr SendTextTimeout(IntPtr window, uint message, UIntPtr wParam, string text, uint flags, uint timeout, out UIntPtr result);
    [DllImport("user32.dll", EntryPoint="SendMessageTimeoutW", CharSet=CharSet.Unicode, SetLastError=true)] public static extern IntPtr ReadTextTimeout(IntPtr window, uint message, UIntPtr capacity, [Out] StringBuilder text, uint flags, uint timeout, out UIntPtr result);
    public static bool BelongsTo(IntPtr window, int processId) {
        uint owner;
        return IsWindow(window) && GetWindowThreadProcessId(window, out owner) != 0 && owner == processId;
    }
    public static string ClassName(IntPtr window) {
        var text = new StringBuilder(128);
        GetClassName(window, text, text.Capacity);
        return text.ToString();
    }
    public static bool IsOwnedDialog(IntPtr window, int processId, IntPtr parent) {
        if (!BelongsTo(window, processId) || !IsWindowVisible(window) || ClassName(window) != "#32770") return false;
        var next = window;
        for (int depth = 0; depth < 16; depth++) {
            next = GetWindow(next, 4); // GW_OWNER; reject an unrelated dialog even in this PID.
            if (next == IntPtr.Zero || !BelongsTo(next, processId)) return false;
            if (next == parent) return true;
        }
        return false;
    }
    public static IntPtr[] FindDialogs(int processId, IntPtr parent) {
        var matches = new List<IntPtr>();
        EnumWindows((window, data) => {
            // Filter PID before obtaining any class information; no other-window inventory.
            if (BelongsTo(window, processId) && IsOwnedDialog(window, processId, parent)) matches.Add(window);
            return true;
        }, IntPtr.Zero);
        return matches.ToArray();
    }
}
'@

    $stage = 'validate ownership'
    $appProcess = Get-Process -Id $AppProcessId
    if ($appProcess.ProcessName -ne 'shinatama-desktop') { Stop-Probe 'unexpected_process' }
    $parentHandle = [IntPtr]$ParentWindowHandle
    if (![OwnedFolderProbe]::BelongsTo($parentHandle, $AppProcessId)) { Stop-Probe 'parent_not_owned' }
    $fixture = [IO.Path]::GetFullPath($FixtureRoot).TrimEnd('\')
    $temporaryRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\')
    if ([IO.Path]::GetDirectoryName($fixture) -ine $temporaryRoot -or [IO.Path]::GetFileName($fixture) -notlike 'shinatama-native-picker-*') {
        Stop-Probe 'fixture_not_owned_temp_directory'
    }
    if ($Mode -eq 'Choose') {
        if (![IO.Path]::IsPathRooted($Folder)) { Stop-Probe 'folder_not_absolute' }
        $selectedFolder = (Resolve-Path -LiteralPath $Folder).ProviderPath.TrimEnd('\')
        if (!$selectedFolder.StartsWith($fixture + '\', [StringComparison]::OrdinalIgnoreCase) -or !(Test-Path -LiteralPath $selectedFolder -PathType Container)) {
            Stop-Probe 'folder_outside_owned_fixture'
        }
    }
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    $stage = 'wait for owned native folder dialog'
    $dialogHandle = [IntPtr]::Zero
    while ([DateTime]::UtcNow -lt $deadline) {
        if (![OwnedFolderProbe]::BelongsTo($parentHandle, $AppProcessId)) { Stop-Probe 'parent_closed' }
        $matches = @([OwnedFolderProbe]::FindDialogs($AppProcessId, $parentHandle))
        if ($matches.Count -gt 1) { Stop-Probe 'ambiguous_owned_dialogs' }
        if ($matches.Count -eq 1) { $dialogHandle = $matches[0]; break }
        Start-Sleep -Milliseconds 100
    }
    if ($dialogHandle -eq [IntPtr]::Zero) { Stop-Probe 'owned_dialog_deadline' }
    function Assert-OwnedDialog {
        if (![OwnedFolderProbe]::IsOwnedDialog($dialogHandle, $AppProcessId, $parentHandle)) { Stop-Probe 'dialog_ownership_changed' }
    }
    function Send-DialogCommand([int]$Command) {
        Assert-OwnedDialog
        [UIntPtr]$messageResult = [UIntPtr]::Zero
        # WM_COMMAND only to this verified dialog. No SendKeys, clipboard, or foreground-window input.
        $sent = [OwnedFolderProbe]::SendMessageTimeout($dialogHandle, 0x0111, [UIntPtr]::new([uint32]$Command), [IntPtr]::Zero, 3, 1000, [ref]$messageResult)
        $script:sendError = [Runtime.InteropServices.Marshal]::GetLastWin32Error()
        # Destruction while processing IDCANCEL/IDOK can make SendMessageTimeout return zero.
        if ($sent -eq [IntPtr]::Zero -and [OwnedFolderProbe]::IsWindow($dialogHandle)) { Stop-Probe 'dialog_command_not_acknowledged' }
    }
    $entryId = $null
    $entryMethod = $null
    if ($Mode -eq 'Choose') {
        $stage = 'find owned filename entry'
        Assert-OwnedDialog
        $root = [System.Windows.Automation.AutomationElement]::FromHandle($dialogHandle)
        if ($root.Current.ProcessId -ne $AppProcessId) { Stop-Probe 'uia_dialog_not_owned' }
        $walker = [System.Windows.Automation.TreeWalker]::RawViewWalker
        $queue = [System.Collections.Generic.Queue[object]]::new()
        $queue.Enqueue(@{ Element = $root; FileNameHost = $false; Depth = 0 })
        $candidates = [System.Collections.Generic.List[object]]::new()
        $visited = 0
        while ($queue.Count -gt 0 -and $visited -lt 512 -and [DateTime]::UtcNow -lt $deadline) {
            $item = $queue.Dequeue(); $element = $item.Element; $visited++
            $properties = $element.Current
            if ($properties.ProcessId -ne $AppProcessId) { continue }
            $id = $properties.AutomationId
            # Folder-only IFileDialog uses edit1152; file dialogs commonly use host1148/edit1001.
            $isFileNameHost = $item.FileNameHost -or $id -in @('1148', '1152', 'FileNameControlHost') -or $properties.ClassName -eq 'FileNameControlHost'
            $isEdit = $properties.ControlType -eq [System.Windows.Automation.ControlType]::Edit -or $properties.ClassName -eq 'Edit'
            if ($observedControls.Count -lt 96 -and $properties.ClassName -ne 'UIProperty') {
                # IDs/classes/types only. Never collect the control's Name, Value, or Text.
                $observedControls.Add(@{ automationId = $id; class = $properties.ClassName; type = $properties.ControlType.ProgrammaticName })
            }
            if ($isEdit -and $isFileNameHost -and $properties.IsEnabled -and !$properties.IsPassword -and !$properties.IsOffscreen) {
                $pattern = $null
                if ($element.TryGetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern, [ref]$pattern) -and !$pattern.Current.IsReadOnly) {
                    $candidates.Add(@{ Element = $element; Pattern = $pattern; Id = $id })
                }
            }
            if ($item.Depth -lt 16) {
                $child = $walker.GetFirstChild($element)
                while ($null -ne $child -and $queue.Count -lt 512) {
                    $queue.Enqueue(@{ Element = $child; FileNameHost = $isFileNameHost; Depth = $item.Depth + 1 })
                    $child = $walker.GetNextSibling($child)
                }
            }
        }
        if ($candidates.Count -gt 1) { Stop-Probe 'filename_entry_not_unambiguous' }
        $stage = 'set synthetic folder path'
        Assert-OwnedDialog
        if ($candidates.Count -eq 1) {
            $entry = $candidates[0]; $entryId = $entry.Id; $entryMethod = 'uia_value_pattern'
            if ($entry.Element.Current.ProcessId -ne $AppProcessId) { Stop-Probe 'filename_uia_ownership_changed' }
            $entryHandle = [IntPtr]$entry.Element.Current.NativeWindowHandle
            if ($entryHandle -ne [IntPtr]::Zero -and ![OwnedFolderProbe]::BelongsTo($entryHandle, $AppProcessId)) { Stop-Probe 'filename_hwnd_ownership_changed' }
            $entry.Pattern.SetValue($selectedFolder)
            # Read back only after overwriting with this test's synthetic path; never expose values.
            if ($entry.Pattern.Current.Value -cne $selectedFolder) { Stop-Probe 'synthetic_path_not_retained' }
        } else {
            # Some PowerShell/.NET hosts cannot load Microsoft's Win32 UIA proxies.
            # Use the exact native folder-name control ID, never an arbitrary edit or input.
            $entryHandle = [OwnedFolderProbe]::GetDlgItem($dialogHandle, 1152); $entryId = '1152'
            if ($entryHandle -eq [IntPtr]::Zero) {
                $hostHandle = [OwnedFolderProbe]::GetDlgItem($dialogHandle, 1148)
                if ($hostHandle -ne [IntPtr]::Zero -and [OwnedFolderProbe]::BelongsTo($hostHandle, $AppProcessId)) {
                    $entryHandle = [OwnedFolderProbe]::GetDlgItem($hostHandle, 1001); $entryId = '1148/1001'
                }
            }
            Assert-OwnedDialog
            if (![OwnedFolderProbe]::BelongsTo($entryHandle, $AppProcessId) -or ![OwnedFolderProbe]::IsChild($dialogHandle, $entryHandle) -or [OwnedFolderProbe]::ClassName($entryHandle) -ne 'Edit' -or ![OwnedFolderProbe]::IsWindowEnabled($entryHandle) -or ![OwnedFolderProbe]::IsWindowVisible($entryHandle)) {
                Stop-Probe 'native_filename_control_not_owned_edit'
            }
            $entryMethod = 'scoped_win32_edit'
            [UIntPtr]$textResult = [UIntPtr]::Zero
            $sent = [OwnedFolderProbe]::SendTextTimeout($entryHandle, 0x000C, [UIntPtr]::Zero, $selectedFolder, 3, 1000, [ref]$textResult)
            if ($sent -eq [IntPtr]::Zero -or $textResult -eq [UIntPtr]::Zero) { Stop-Probe 'synthetic_path_not_acknowledged' }
            Assert-OwnedDialog
            if (![OwnedFolderProbe]::BelongsTo($entryHandle, $AppProcessId) -or ![OwnedFolderProbe]::IsChild($dialogHandle, $entryHandle)) { Stop-Probe 'filename_hwnd_ownership_changed' }
            $readback = [Text.StringBuilder]::new($selectedFolder.Length + 2)
            # GetWindowText cannot retrieve an edit in another process. WM_GETTEXT is scoped
            # to the exact control just overwritten, bounded to this synthetic path's length.
            $read = [OwnedFolderProbe]::ReadTextTimeout($entryHandle, 0x000D, [UIntPtr]::new([uint32]$readback.Capacity), $readback, 3, 1000, [ref]$textResult)
            if ($read -eq [IntPtr]::Zero) { Stop-Probe 'synthetic_path_readback_deadline' }
            if ($readback.ToString() -cne $selectedFolder) { Stop-Probe 'synthetic_path_not_retained' }
        }
        $stage = 'accept synthetic folder'
        Send-DialogCommand 1 # IDOK
        # Some Windows folder dialogs navigate on the first accept and select on the second.
        $navigationDeadline = [DateTime]::UtcNow.AddMilliseconds(1500)
        while ([OwnedFolderProbe]::IsOwnedDialog($dialogHandle, $AppProcessId, $parentHandle) -and [DateTime]::UtcNow -lt $navigationDeadline) { Start-Sleep -Milliseconds 100 }
        if ([OwnedFolderProbe]::IsOwnedDialog($dialogHandle, $AppProcessId, $parentHandle)) { Send-DialogCommand 1 }
    } else {
        $stage = 'cancel owned native dialog'
        Send-DialogCommand 2 # IDCANCEL
    }
    $stage = 'wait for owned dialog to close'
    while ([OwnedFolderProbe]::IsOwnedDialog($dialogHandle, $AppProcessId, $parentHandle) -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 100 }
    # IFileDialog can retain a hidden HWND after completion. The Node driver additionally
    # requires the actual catalog_browse result, so hiding alone never qualifies the proof.
    if ([OwnedFolderProbe]::IsWindow($dialogHandle) -and [OwnedFolderProbe]::IsWindowVisible($dialogHandle)) { Stop-Probe 'dialog_did_not_close' }
    @{ status = 'passed'; mode = $Mode.ToLowerInvariant(); dialogClass = '#32770'; parentOwned = $true; nativeVisible = $false; entryAutomationId = $entryId; entryMethod = $entryMethod; controls = $observedControls.ToArray() } | ConvertTo-Json -Depth 4 -Compress
} catch {
    # A provider failure cannot produce a pass; omit exception messages that could include UI content.
    @{ status = 'failed'; mode = $Mode.ToLowerInvariant(); stage = $stage; failureReason = $failureReason; sendError = $sendError; failureType = $_.Exception.GetType().Name; visited = $visited; candidateCount = $candidates.Count; controls = $observedControls.ToArray() } | ConvertTo-Json -Depth 4 -Compress
    exit 1
}
