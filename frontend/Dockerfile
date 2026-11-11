# Build the SvelteKit SPA.
FROM node:22-alpine AS build
WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci
COPY . .
RUN npm run build

# Serve the static bundle and reverse-proxy /api to the Rust backend.
FROM nginx:1.27-alpine
# Rendered to /etc/nginx/conf.d/default.conf at start with REAL_IP_FROM substituted.
COPY nginx.conf /etc/nginx/templates/default.conf.template
ENV REAL_IP_FROM=127.0.0.1
COPY --from=build /app/build /usr/share/nginx/html
EXPOSE 80
