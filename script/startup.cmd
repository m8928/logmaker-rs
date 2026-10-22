@echo off
rem Starts LogMaker in a new console window, keeping data, plugins and logs
rem next to this script. Stop it with Ctrl+C in that window or by closing the
rem window; running logs and scenarios stop and senders flush first. If the
rem server cannot start, the window stays open and shows the error.
rem Extra options are passed on, e.g.  startup.cmd --port 8080
cd /d "%~dp0"
start "LogMaker" cmd /c "logmaker.exe --data-root data --plugin-root plugins %* || pause"
