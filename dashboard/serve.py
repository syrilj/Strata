#!/usr/bin/env python3
"""
Production-ready HTTP server for the dashboard.

Features:
- Security headers (CSP, X-Frame-Options, etc.)
- CORS support for development
- Gzip compression
- Proper MIME types
- Request logging
"""

import http.server
import socketserver
import webbrowser
import os
import sys
import gzip
from functools import partial
from io import BytesIO

PORT = int(os.environ.get('PORT', 3000))
HOST = os.environ.get('HOST', 'localhost')
OPEN_BROWSER = os.environ.get('NO_BROWSER', '').lower() != 'true'

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


class SecureHandler(http.server.SimpleHTTPRequestHandler):
    """HTTP handler with security headers and compression."""
    
    def __init__(self, *args, directory=None, **kwargs):
        super().__init__(*args, directory=directory, **kwargs)
    
    def end_headers(self):
        # Add security headers
        for header, value in SECURITY_HEADERS.items():
            self.send_header(header, value)
        
        # CORS for development
        self.send_header('Access-Control-Allow-Origin', '*')
        self.send_header('Access-Control-Allow-Methods', 'GET, OPTIONS')
        self.send_header('Access-Control-Allow-Headers', 'Content-Type')
        
        super().end_headers()
    
    def guess_type(self, path):
        """Return proper MIME type."""
        ext = os.path.splitext(path)[1].lower()
        return MIME_TYPES.get(ext, super().guess_type(path))
    
    def do_GET(self):
        """Handle GET with compression for text files."""
        # Check if client accepts gzip
        accept_encoding = self.headers.get('Accept-Encoding', '')
        
        if 'gzip' in accept_encoding:
            # Get the file path
            path = self.translate_path(self.path)
            
            if os.path.isfile(path):
                ext = os.path.splitext(path)[1].lower()
                
                # Compress text-based files
                if ext in ['.html', '.js', '.css', '.json', '.svg']:
                    try:
                        with open(path, 'rb') as f:
                            content = f.read()
                        
                        # Compress
                        buf = BytesIO()
                        with gzip.GzipFile(fileobj=buf, mode='wb') as gz:
                            gz.write(content)
                        compressed = buf.getvalue()
                        
                        self.send_response(200)
                        self.send_header('Content-Type', self.guess_type(path))
                        self.send_header('Content-Encoding', 'gzip')
                        self.send_header('Content-Length', len(compressed))
                        self.end_headers()
                        self.wfile.write(compressed)
                        return
                    except Exception:
                        pass  # Fall back to normal serving
        
        super().do_GET()
    
    def do_OPTIONS(self):
        """Handle CORS preflight."""
        self.send_response(200)
        self.end_headers()
    
    def log_message(self, format, *args):
        """Custom log format."""
        print(f"[{self.log_date_time_string()}] {args[0]}")


def main():
    os.chdir(os.path.dirname(os.path.abspath(__file__)))
    
    # Check if dist folder exists (production build)
    if os.path.exists('dist'):
        serve_dir = 'dist'
        print("📦 Serving production build from dist/")
    else:
        serve_dir = '.'
        print("🔧 Serving development files (run 'npm run build' for production)")
    
    handler = partial(SecureHandler, directory=serve_dir)
    
    with socketserver.TCPServer((HOST, PORT), handler) as httpd:
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
