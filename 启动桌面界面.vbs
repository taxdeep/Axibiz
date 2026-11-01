Set shell = CreateObject("WScript.Shell")
Set fs = CreateObject("Scripting.FileSystemObject")
root = fs.GetParentFolderName(WScript.ScriptFullName)
shell.CurrentDirectory = root
exe = root & "\dist\Axibiz Desktop Beta.exe"
If Not fs.FileExists(exe) Then exe = root & "\dist\Axibiz.exe"
If Not fs.FileExists(exe) Then exe = root & "\target\desktop\release\axibiz.exe"
If Not fs.FileExists(exe) Then exe = root & "\target\desktop\debug\axibiz.exe"
If Not fs.FileExists(exe) Then exe = root & "\target\debug\axibiz.exe"
If fs.FileExists(exe) Then
    cleanup = root & "\scripts\cleanup-running-builds.ps1"
    If fs.FileExists(cleanup) Then
        shell.Run "powershell.exe -NoProfile -ExecutionPolicy Bypass -File " & Chr(34) & cleanup & Chr(34), 0, True
    End If
    shell.Run Chr(34) & exe & Chr(34), 0, False
Else
    MsgBox "Build first: powershell -File build.ps1", 48, "Axibiz Desktop Beta"
End If
