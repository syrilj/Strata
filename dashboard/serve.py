#!/usr/bin/env python3
"""
Production-ready HTTP server for the Strata dashboard.

Features:
- Serves built SPA static files from dist/ with index.html fallback
- Reverse-proxies /api/* requests (GET, POST, PUT, DELETE, OPTIONS) to COORDINATOR_API_URL
- Security headers (X-Content-Type-Options, X-Frame-Options, etc.)
- CORS support
- Gzip compression for text assets
- Proper MIME types
"""

import gzip
import http.server
import os
import socketserver
import sys
import urllib.error
import urllib.parse
import urllib.request
import webbrowser
from functools import partial
from io import BytesIO

PORT = int(os.environ.get('PORT', 3000))
HOST = os.environ.get('HOST', 'localhost')
OPEN_BROWSER = os.environ.get('NO_BROWSER', '').lower() != 'true'
COORDINATOR_API_URL = os.environ.get('COORDINATOR_API_URL', 'http://127.0.0.1:51051').rstrip('/')

# Security headers
SECURITY_HEADERS = {
    'X-Content-Type-Options': 'nosniff',
    'X-Frame-Options': 'DENY',
    'X-XSS-Protection': '1; mode=block',
    'Referrer-Policy': 'strict-origin-when-cross-origin',
    'Permissions-Policy': 'geolocation=(), microphone=(), camera=()',
}

# MIME types
MIME_TYPES = {
    '.html': 'text/html; charset=utf-8',
    '.js': 'application/javascript; charset=utf-8',
    '.mjs': 'application/javascript; charset=utf-8',
    '.css': 'text/css; charset=utf-8',
    '.json': 'application/json; charset=utf-8',
    '.svg': 'image/svg+xml',
    '.png': 'image/png',
    '.ico': 'image/x-icon',
    '.woff': 'font/woff',
    '.woff2': 'font/woff2',
}

HOP_BY_HOP_HEADERS = {
    'connection',
    'keep-alive',
    'proxy-authenticate',
    'proxy-authorization',
    'te',
    'trailers',
    'transfer-encoding',
    'upgrade',
    'host',
}


class ThreadingHTTPServer(socketserver.ThreadingMixIn, http.server.HTTPServer):
    daemon_threads = True
    allow_reuse_address = True


class SecureHandler(http.server.SimpleHTTPRequestHandler):
    """HTTP handler with static SPA fallback, /api reverse proxy, security headers, and compression."""

    def __init__(self, *args, directory=None, **kwargs):
        super().__init__(*args, directory=directory, **kwargs)

    def end_headers(self):
        for header, value in SECURITY_HEADERS.items():
            self.send_header(header, value)

        self.send_header('Access-Control-Allow-Origin', '*')
        self.send_header('Access-Control-Allow-Methods', 'GET, POST, PUT, DELETE, OPTIONS')
        self.send_header('Access-Control-Allow-Headers', 'Content-Type, Authorization')

        super().end_headers()

    def guess_type(self, path):
        """Return proper MIME type."""
        ext = os.path.splitext(path)[1].lower()
        return MIME_TYPES.get(ext, super().guess_type(path))

    def _is_api_request(self) -> bool:
        parsed = urllib.parse.urlsplit(self.path)
        return parsed.path == '/api' or parsed.path.startswith('/api/')

    def _proxy_api(self, method: str):
        target_url = f'{COORDINATOR_API_URL}{self.path}'
        content_length = int(self.headers.get('Content-Length', 0) or 0)
        body = self.rfile.read(content_length) if content_length > 0 else None

        forward_headers = {}
        for key, value in self.headers.items():
            if key.lower() not in HOP_BY_HOP_HEADERS and key.lower() != 'content-length':
                forward_headers[key] = value

        req = urllib.request.Request(
            target_url,
            data=body,
            headers=forward_headers,
            method=method,
        )

        try:
            with urllib.request.urlopen(req, timeout=15) as resp:
                resp_body = resp.read()
                self.send_response(resp.status)
                for key, value in resp.headers.items():
                    lower = key.lower()
                    if lower not in HOP_BY_HOP_HEADERS and not lower.startswith('access-control-'):
                        if lower == 'content-length':
                            continue
                        self.send_header(key, value)
                self.send_header('Content-Length', str(len(resp_body)))
                self.end_headers()
                if resp_body:
                    self.wfile.write(resp_body)
        except urllib.error.HTTPError as err:
            err_body = err.read()
            self.send_response(err.code)
            for key, value in err.headers.items():
                lower = key.lower()
                if lower not in HOP_BY_HOP_HEADERS and not lower.startswith('access-control-'):
                    if lower == 'content-length':
                        continue
                    self.send_header(key, value)
            self.send_header('Content-Length', str(len(err_body)))
            self.end_headers()
            if err_body:
                self.wfile.write(err_body)
        except urllib.error.URLError as err:
            payload = (
                f'{{"error":"Upstream coordinator unreachable","detail":"{str(err.reason)}"}}'
            ).encode('utf-8')
            self.send_response(502)
            self.send_header('Content-Type', 'application/json; charset=utf-8')
            self.send_header('Content-Length', str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)

    def _serve_spa_or_static(self):
        parsed = urllib.parse.urlsplit(self.path)
        translated = self.translate_path(parsed.path)

        # SPA fallback to index.html for client-side routes when file doesn't exist
        if not os.path.exists(translated):
            index_path = os.path.join(self.directory or '.', 'index.html')
            if os.path.isfile(index_path):
                self.path = '/index.html'
                translated = index_path

        accept_encoding = self.headers.get('Accept-Encoding', '')
        if 'gzip' in accept_encoding and os.path.isfile(translated):
            ext = os.path.splitext(translated)[1].lower()
            if ext in ['.html', '.js', '.css', '.json', '.svg']:
                try:
                    with open(translated, 'rb') as f:
                        content = f.read()

                    buf = BytesIO()
                    with gzip.GzipFile(fileobj=buf, mode='wb') as gz:
                        gz.write(content)
                    compressed = buf.getvalue()

                    self.send_response(200)
                    self.send_header('Content-Type', self.guess_type(translated))
                    self.send_header('Content-Encoding', 'gzip')
                    self.send_header('Content-Length', str(len(compressed)))
                    self.end_headers()
                    self.wfile.write(compressed)
                    return
                except Exception:
                    pass

        super().do_GET()

    def do_GET(self):
        if self._is_api_request():
            self._proxy_api('GET')
            return
        self._serve_spa_or_static()

    def do_POST(self):
        if self._is_api_request():
            self._proxy_api('POST')
            return
        self.send_error(405, 'Method Not Allowed')

    def do_PUT(self):
        if self._is_api_request():
            self._proxy_api('PUT')
            return
        self.send_error(405, 'Method Not Allowed')

    def do_DELETE(self):
        if self._is_api_request():
            self._proxy_api('DELETE')
            return
        self.send_error(405, 'Method Not Allowed')

    def do_OPTIONS(self):
        if self._is_api_request():
            self._proxy_api('OPTIONS')
            return
        self.send_response(200)
        self.end_headers()

    def log_message(self, format, *args):
        """Custom log format."""
        print(f"[{self.log_date_time_string()}] {args[0]}")


def main():
    os.chdir(os.path.dirname(os.path.abspath(__file__)))

    if os.path.exists('dist'):
        serve_dir = 'dist'
        print("📦 Serving production build from dist/")
    else:
        serve_dir = '.'
        print("🔧 Serving development files (run 'npm run build' for production)")

    print(f"🔌 Proxying /api/* -> {COORDINATOR_API_URL}")
    handler = partial(SecureHandler, directory=serve_dir)

    with ThreadingHTTPServer((HOST, PORT), handler) as httpd:
        url = f"http://{HOST}:{PORT}"
        print(f"🚀 Dashboard server running at {url}")
        print("Press Ctrl+C to stop\n")

        if OPEN_BROWSER:
            webbrowser.open(url)

        try:
            httpd.serve_forever()
        except KeyboardInterrupt:
            print("\n👋 Server stopped")
            sys.exit(0)


if __name__ == "__main__":
    main()
