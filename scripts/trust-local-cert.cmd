@echo off
rem Trust Caddy's local certificate authority for the current Windows user, so
rem https://cloud-project.online opens without warnings (Edge, Chrome; Firefox
rem uses the Windows store when security.enterprise_roots.enabled = true).
rem Run after "start.cmd local". Repeat if the Docker volume caddy-data is deleted.
setlocal
cd /d "%~dp0.."
set RC=0
set FILES=-f docker-compose.yml -f docker-compose.prod.yml -f docker-compose.local.yml
docker compose %FILES% cp caddy:/data/caddy/pki/authorities/local/root.crt deploy\caddy-local-root.crt
if errorlevel 1 (
  echo Could not read the certificate. Start the portal first: scripts\start.cmd local
  set RC=1
  goto done
)
certutil -user -addstore Root deploy\caddy-local-root.crt
if errorlevel 1 (
  set RC=1
  goto done
)
echo Done. Close all browser windows and open https://cloud-project.online
:done
echo.
pause
exit /b %RC%
