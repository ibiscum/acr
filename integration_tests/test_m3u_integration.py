#!/usr/bin/env python3
"""
Integration tests for M3U playlist parsing API
"""

from pathlib import Path
import threading
import time
from http.server import HTTPServer, SimpleHTTPRequestHandler

import pytest
import venv_bootstrap

TESTDATA_DIR = Path(__file__).parent / "testdata" / "m3u"


class M3UTestServer:
    """Test HTTP server for serving M3U playlist files."""

    def __init__(self, port=8123):
        self.port = port
        self.server = None
        self.thread = None

    def start(self):
        """Start the test server."""
        directory = str(TESTDATA_DIR)

        class Handler(SimpleHTTPRequestHandler):
            def __init__(self, *args, **kwargs):
                super().__init__(*args, directory=directory, **kwargs)

        self.server = HTTPServer(("localhost", self.port), Handler)
        self.thread = threading.Thread(target=self.server.serve_forever)
        self.thread.daemon = True
        self.thread.start()

    def stop(self):
        """Stop the test server."""
        if self.server:
            self.server.shutdown()
            self.server.server_close()
        if self.thread:
            self.thread.join(timeout=1)

    def get_file_url(self, filename):
        """Get the URL for a test file."""
        return f"http://localhost:{self.port}/{filename}"

@pytest.fixture
def m3u_server():
    """Fixture that provides a test M3U server."""
    server = M3UTestServer()
    server.start()
    time.sleep(0.5)  # Give server time to start
    yield server
    server.stop()


class TestM3UIntegration:
    """Integration tests for M3U playlist parsing API."""
    
    def test_parse_simple_m3u_playlist(self, m3u_server, generic_server):
        """Test parsing a simple M3U playlist with absolute URLs."""
        url = m3u_server.get_file_url("simple.m3u")
        
        response_data = generic_server.api_request(
            'POST',
            '/api/m3u/parse',
            json={"url": url}
        )
        
        # Validate response structure
        assert response_data["success"] is True
        assert response_data["url"] == url
        assert "timestamp" in response_data
        
        playlist = response_data["playlist"]
        assert playlist is not None
        assert playlist["count"] == 3
        assert playlist["is_extended"] is False
        
        entries = playlist["entries"]
        assert len(entries) == 3
        assert entries[0]["url"] == "http://example.com/song1.mp3"
        assert entries[0]["title"] is None
        assert entries[0]["duration"] is None
        assert entries[1]["url"] == "http://example.com/song2.mp3"
        assert entries[2]["url"] == "http://example.com/song3.mp3"
        
    def test_parse_extended_m3u_playlist(self, m3u_server, generic_server):
        """Test parsing an extended M3U playlist with metadata."""
        url = m3u_server.get_file_url("extended.m3u")
        
        response_data = generic_server.api_request(
            'POST',
            '/api/m3u/parse',
            json={"url": url}
        )
        
        # Validate response
        assert response_data["success"] is True
        
        playlist = response_data["playlist"]
        assert playlist["is_extended"] is True
        assert playlist["count"] == 4
        
        entries = playlist["entries"]
        assert len(entries) == 4
        
        # Check first entry with full metadata
        assert entries[0]["url"] == "http://example.com/song1.mp3"
        assert entries[0]["title"] == "Artist 1 - Song 1"
        assert entries[0]["duration"] == 180.0
        
        # Check second entry  
        assert entries[1]["url"] == "http://example.com/song2.mp3"
        assert entries[1]["title"] == "Artist 2 - Song 2"
        assert entries[1]["duration"] == 240.0
        
        # Check live stream entry (duration -1 is converted to None for unknown duration)
        assert entries[2]["url"] == "http://example.com/stream.m3u8"
        assert entries[2]["title"] == "Live Stream"
        assert entries[2]["duration"] is None  # -1 is converted to None
        
        # Check entry with no title
        assert entries[3]["url"] == "http://example.com/song_no_title.mp3"
        assert entries[3]["title"] is None
        assert entries[3]["duration"] == 200.0

    def test_parse_invalid_url(self, generic_server):
        """Test parsing with an invalid URL."""
        response_data = generic_server.api_request(
            'POST',
            '/api/m3u/parse',
            json={"url": "not-a-valid-url"},
            expect_error=True
        )
        
        assert response_data["success"] is False
        assert "error" in response_data
        assert response_data["error"] is not None

    def test_parse_empty_url(self, generic_server):
        """Test parsing with an empty URL."""
        response_data = generic_server.api_request(
            'POST',
            '/api/m3u/parse',
            json={"url": ""},
            expect_error=True
        )
        
        assert response_data["success"] is False
        assert "error" in response_data

    def test_parse_bytefm_real_playlist(self, generic_server):
        """Test parsing a real-world M3U playlist from byte.fm."""
        url = "http://www.byte.fm/stream/bytefmhq.m3u"
        
        response_data = generic_server.api_request(
            'POST',
            '/api/m3u/parse',
            json={"url": url}
        )
        
        # Validate response
        assert response_data["success"] is True
        assert response_data["url"] == url
        assert "timestamp" in response_data
        
        playlist = response_data["playlist"]
        assert playlist is not None
        assert playlist["count"] >= 1
        
        entries = playlist["entries"]
        assert len(entries) >= 1
        
        # Check the stream entry
        entry = entries[0]
        
        # Verify the URL is from the same domain (byte.fm)
        assert "byte.fm" in entry["url"]
