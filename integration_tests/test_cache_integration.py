#!/usr/bin/env python3
"""
Integration tests for Cache Statistics API
These tests verify the cache statistics endpoint functionality
"""

import venv_bootstrap
import pytest
import json
import time
import uuid


def test_cache_stats_basic_response(cache_server):
    """Test that the cache stats endpoint returns a valid response structure"""
    response = cache_server.api_request('GET', '/api/cache/stats')

    assert isinstance(response, dict)
    assert response['success'] is True
    assert response['stats'] is not None
    assert response['message'] is None

    # Verify the attribute cache stats structure
    stats = response['stats']
    assert isinstance(stats, dict)
    assert 'disk_entries' in stats
    assert 'memory_entries' in stats
    assert 'memory_bytes' in stats
    assert 'memory_limit_bytes' in stats

    # Verify data types
    assert isinstance(stats['disk_entries'], int)
    assert isinstance(stats['memory_entries'], int)
    assert isinstance(stats['memory_bytes'], int)
    assert isinstance(stats['memory_limit_bytes'], int)

    # Verify the image cache stats structure (should be present)
    assert 'image_cache_stats' in response
    if response['image_cache_stats'] is not None:
        image_stats = response['image_cache_stats']
        assert isinstance(image_stats, dict)
        assert 'total_images' in image_stats
        assert 'total_size' in image_stats
        assert 'last_updated' in image_stats

        # Verify data types
        assert isinstance(image_stats['total_images'], int)
        assert isinstance(image_stats['total_size'], int)
        assert isinstance(image_stats['last_updated'], int)


def test_cache_stats_memory_limit_configuration(cache_server):
    """Test that memory limit is properly configured and returned"""
    response = cache_server.api_request('GET', '/api/cache/stats')

    assert response['success'] is True
    assert response['stats'] is not None
    stats = response['stats']

    # test_config_cache.json configures attribute_cache.memory_limit to 2MB
    assert stats['memory_limit_bytes'] == 2 * 1024 * 1024
    assert stats['memory_bytes'] <= stats['memory_limit_bytes']


def test_cache_stats_initial_state(cache_server):
    """Test cache stats in initial state"""
    response = cache_server.api_request('GET', '/api/cache/stats')

    assert response['success'] is True
    stats = response['stats']

    # Initially, cache should be empty
    assert stats['disk_entries'] >= 0  # Could be 0 or have some entries
    assert stats['memory_entries'] == 0  # Memory cache should start empty
    assert stats['memory_bytes'] == 0  # No memory usage initially
    assert stats['memory_limit_bytes'] > 0  # Limit should be configured


def test_cache_stats_multiple_requests(cache_server):
    """Test that multiple requests return consistent format"""
    # Make multiple requests to the cache stats endpoint
    required_fields = {'success', 'stats', 'image_cache_stats', 'message'}
    required_stats_fields = {'disk_entries', 'memory_entries', 'memory_bytes', 'memory_limit_bytes'}

    for i in range(3):
        time.sleep(0.1)  # Small delay between requests
        response = cache_server.api_request('GET', '/api/cache/stats')

        assert isinstance(response, dict)
        assert set(response.keys()) == required_fields
        assert response['success'] is True

        stats = response['stats']
        assert isinstance(stats, dict)
        assert required_stats_fields.issubset(stats.keys())
        assert stats['memory_bytes'] <= stats['memory_limit_bytes']


def test_cache_stats_non_negative_values(cache_server):
    """Test that all numeric values in cache stats are non-negative"""
    response = cache_server.api_request('GET', '/api/cache/stats')

    assert response['success'] is True
    stats = response['stats']

    # All values should be non-negative
    assert stats['disk_entries'] >= 0
    assert stats['memory_entries'] >= 0
    assert stats['memory_bytes'] >= 0
    assert stats['memory_limit_bytes'] > 0  # Should be positive (not just non-negative)


def test_cache_stats_memory_usage_invariant(cache_server):
    """Test cache stats memory usage invariant (memory_bytes <= memory_limit_bytes)"""
    response = cache_server.api_request('GET', '/api/cache/stats')

    assert response['success'] is True
    stats = response['stats']

    # Memory bytes should not exceed the memory limit
    assert stats['memory_bytes'] <= stats['memory_limit_bytes']

    # If there are memory entries, there should be some memory usage (unless entries are empty)
    # This is a soft check since entries could theoretically be empty strings
    if stats['memory_entries'] > 0:
        # We don't enforce memory_bytes > 0 because entries could be empty strings
        pass


def test_cache_stats_response_format(cache_server):
    """Test that the cache stats response has the correct JSON format"""
    response = cache_server.api_request('GET', '/api/cache/stats')

    # Check top-level structure
    required_fields = ['success', 'stats', 'image_cache_stats', 'message']
    for field in required_fields:
        assert field in response, f"Missing required field: {field}"

    # Check stats structure
    stats = response['stats']
    required_stats_fields = ['disk_entries', 'memory_entries', 'memory_bytes', 'memory_limit_bytes']
    for field in required_stats_fields:
        assert field in stats, f"Missing required stats field: {field}"

    # Check image cache stats structure (if present)
    if response['image_cache_stats'] is not None:
        image_stats = response['image_cache_stats']
        required_image_stats_fields = ['total_images', 'total_size', 'last_updated']
        for field in required_image_stats_fields:
            assert field in image_stats, f"Missing required image cache stats field: {field}"

    # Check that there are no unexpected extra fields at the top level
    expected_fields = {'success', 'stats', 'image_cache_stats', 'message'}
    actual_fields = set(response.keys())
    extra_fields = actual_fields - expected_fields
    assert len(extra_fields) == 0, f"Unexpected extra fields in response: {extra_fields}"


def test_cache_stats_partial_failure_when_image_stats_unavailable(cache_failure_server):
    """Inject backend failure and verify cache stats endpoint degrades predictably"""
    response = cache_failure_server.api_request('GET', '/api/cache/stats')

    assert isinstance(response, dict)
    assert response['success'] is True
    assert response['stats'] is not None
    assert response['image_cache_stats'] is None
    assert response['message'] == 'Failed to retrieve image cache statistics'

    stats = response['stats']
    assert isinstance(stats, dict)
    assert 'disk_entries' in stats
    assert 'memory_entries' in stats
    assert 'memory_bytes' in stats
    assert 'memory_limit_bytes' in stats


def test_cache_stats_partial_failure_when_attribute_stats_unavailable(cache_failure_attribute_server):
    """Inject attribute stats failure and verify symmetric degraded response"""
    response = cache_failure_attribute_server.api_request('GET', '/api/cache/stats')

    assert isinstance(response, dict)
    assert response['success'] is True
    assert response['stats'] is None
    assert response['image_cache_stats'] is not None
    assert response['message'] == 'Failed to retrieve attribute cache statistics'

    image_stats = response['image_cache_stats']
    assert isinstance(image_stats, dict)
    assert 'total_images' in image_stats
    assert 'total_size' in image_stats
    assert 'last_updated' in image_stats
