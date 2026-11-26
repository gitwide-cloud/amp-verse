Option Explicit
Dim files, shell, env, app, profile
Set files = CreateObject("Scripting.FileSystemObject")
Set shell = CreateObject("WScript.Shell")
Set env = shell.Environment("PROCESS")
app = files.GetParentFolderName(WScript.ScriptFullName)
profile = env("LOCALAPPDATA") & "\AMPVerse\V1Preview"
env("AMP_V1_MODE") = "1"
env("APPDATA") = profile & "\AppData"
env("AMP_VERSE_CONFIG_DIR") = profile & "\settings"
env("AMP_DOCUMENTS_DIR") = profile & "\Documents"
env("AMP_CONTROL_BIND") = "0.0.0.0:8790"
env("AMP_CAST_ADDRESS") = "127.0.0.1:8787"
env("AMP_CAST_BIND") = "0.0.0.0:8787"
env("AMP_CAST_CONFIG_DIR") = profile & "\settings\cast-host"
env("AMP_ENGINE_EXE") = app & "\addons\amp-engine.exe"
env("AMP_STEM_PYTHON") = app & "\addons\stems\Scripts\python.exe"
app = Replace(app, "/", "\")
profile = Replace(profile, "/", "\")
shell.CurrentDirectory = app
If Not IsRunning(app & "\amp-player.exe") Then
 If files.FileExists(app & "\amp-player.next.exe") Then
  On Error Resume Next
  files.CopyFile app & "\amp-player.next.exe", app & "\amp-player.exe", True
  If Err.Number = 0 Then
   files.DeleteFile app & "\amp-player.next.exe", True
   If files.FileExists(app & "\PENDING_UPDATE.txt") Then files.DeleteFile app & "\PENDING_UPDATE.txt", True
  End If
  Err.Clear
  On Error GoTo 0
 End If
 shell.Run Quote(app & "\amp-player.exe"), 1, False
End If
Function Quote(value)
 Quote = Chr(34) & value & Chr(34)
End Function
Function IsRunning(executable)
 Dim process
 IsRunning = False
 For Each process In GetObject("winmgmts:\\.\root\cimv2").ExecQuery("SELECT ExecutablePath FROM Win32_Process WHERE Name='amp-player.exe' OR Name='amp-cast.exe'")
  If Not IsNull(process.ExecutablePath) Then
   If LCase(process.ExecutablePath) = LCase(executable) Then IsRunning = True
  End If
 Next
End Function
