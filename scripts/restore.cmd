@echo off
rem Restore a backup: restore.cmd cloud-project-2026-11-12_0300
rem Without a name lists the backups. The portal is stopped while restoring.
setlocal
cd /d "%~dp0.."
if "%~1"=="" (
  docker compose exec backup sh /usr/local/bin/backup.sh list
  echo.
  echo Usage: scripts\restore.cmd NAME
  exit /b 2
)
echo Restoring %~1 replaces ALL current data of the portal.
set /p OK=Type YES to continue: 
if /i not "%OK%"=="YES" exit /b 1
docker compose stop web backend
docker compose exec backup sh /usr/local/bin/backup.sh restore "%~1"
set RC=%errorlevel%
docker compose start backend web
exit /b %RC%
