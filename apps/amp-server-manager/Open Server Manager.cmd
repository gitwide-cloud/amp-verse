@echo off
setlocal
set "MANAGER_PROFILE=%LOCALAPPDATA%\AMPVerse\ServerManager\manager.json"
if not exist "%LOCALAPPDATA%\AMPVerse\ServerManager" mkdir "%LOCALAPPDATA%\AMPVerse\ServerManager"
if not exist "%MANAGER_PROFILE%" "%~dp0amp-server-manager.exe" --configure "%MANAGER_PROFILE%"
if exist "%MANAGER_PROFILE%" start "AMP Verse Server" "%~dp0amp-server-manager.exe" --window "%MANAGER_PROFILE%"
