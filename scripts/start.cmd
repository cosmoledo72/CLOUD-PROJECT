@echo off
rem Start CLOUD-PROJECT with Docker.
rem   start.cmd         http://localhost:8080 (no domain)
rem   start.cmd local   developer PC: https://cloud-project.online with local certificates
rem                     (once: hosts-local.cmd as administrator, then trust-local-cert.cmd)
rem   start.cmd prod    office server: https://cloud-project.online with Let's Encrypt
setlocal
cd /d "%~dp0.."
if not exist .env (
  echo .env not found. Copy .env.example to .env and set the secrets first.
  exit /b 1
)
set FILES=-f docker-compose.yml
if /i "%~1"=="prod" set FILES=-f docker-compose.yml -f docker-compose.prod.yml
if /i "%~1"=="local" set FILES=-f docker-compose.yml -f docker-compose.prod.yml -f docker-compose.local.yml
docker compose %FILES% up -d --build --remove-orphans
if errorlevel 1 exit /b 1
docker compose %FILES% ps
echo.
if /i "%~1"=="" (
  echo Portal: http://localhost:8080   Documents: http://localhost:8081
) else (
  echo Portal: https://cloud-project.online   Documents: https://docs.cloud-project.online
)
if /i "%~1"=="local" echo First time on this PC: scripts\hosts-local.cmd ^(as administrator^) and scripts\trust-local-cert.cmd
echo ONLYOFFICE needs 1-2 minutes after the first start.
pause
