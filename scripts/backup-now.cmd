@echo off
rem Make a backup right away and list all backups (folder .\backups).
setlocal
cd /d "%~dp0.."
docker compose exec backup sh /usr/local/bin/backup.sh now
if errorlevel 1 exit /b 1
docker compose exec backup sh /usr/local/bin/backup.sh list
