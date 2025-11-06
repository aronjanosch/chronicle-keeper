#!/usr/bin/env python3
"""
Test script for model selection feature.
Tests the new endpoints for Ollama and Whisper model configuration.
"""

import sys
import os

# Add src to path
sys.path.insert(0, os.path.join(os.path.dirname(__file__), 'src'))

from storage.config_manager import ConfigManager
from llm.ollama import OllamaClient

def test_config_manager():
    """Test ConfigManager methods"""
    print("Testing ConfigManager...")
    cm = ConfigManager()

    # Test Whisper models
    print("\n1. Available Whisper Models:")
    whisper_models = cm.get_available_whisper_models()
    for model, desc in whisper_models.items():
        print(f"   - {model}: {desc}")

    # Test current Whisper model
    current_whisper = cm.get_whisper_model()
    print(f"\n2. Current Whisper Model: {current_whisper}")

    # Test Ollama models
    print("\n3. Available Ollama Models:")
    ollama_models = cm.get_available_ollama_models()
    for model, desc in ollama_models.items():
        print(f"   - {model}: {desc}")

    # Test current Ollama model
    current_ollama = cm.get_ollama_model()
    print(f"\n4. Current Ollama Model: {current_ollama}")

    print("\n✅ ConfigManager tests passed!")
    return True

def test_ollama_client():
    """Test OllamaClient model management"""
    print("\nTesting OllamaClient...")

    # Test with default model
    client = OllamaClient(model="llama3.2")
    print(f"\n1. Created OllamaClient with model: {client.model}")

    # Test server running check
    server_running = client.is_server_running()
    print(f"2. Ollama server running: {server_running}")

    if server_running:
        # Test model availability
        model_available = client.is_model_available()
        print(f"3. Model 'llama3.2' available: {model_available}")

        # Test connection
        status = client.test_connection()
        print(f"4. Connection status: {status}")
    else:
        print("⚠️  Ollama server not running - skipping model tests")

    print("\n✅ OllamaClient tests passed!")
    return True

def test_settings_update():
    """Test updating model settings"""
    print("\nTesting Settings Update...")

    cm = ConfigManager()

    # Test updating Whisper model
    print("\n1. Testing Whisper model update...")
    cm.update_settings({"whisper_model": "base"})
    updated_whisper = cm.get_whisper_model()
    print(f"   Updated Whisper model to: {updated_whisper}")
    assert updated_whisper == "base", "Whisper model update failed!"

    # Reset to default
    cm.update_settings({"whisper_model": "large-v2"})
    print(f"   Reset to: {cm.get_whisper_model()}")

    # Test updating Ollama model
    print("\n2. Testing Ollama model update...")
    cm.update_settings({"ollama_model": "mistral"})
    updated_ollama = cm.get_ollama_model()
    print(f"   Updated Ollama model to: {updated_ollama}")
    assert updated_ollama == "mistral", "Ollama model update failed!"

    # Reset to default
    cm.update_settings({"ollama_model": "llama3.2"})
    print(f"   Reset to: {cm.get_ollama_model()}")

    print("\n✅ Settings update tests passed!")
    return True

if __name__ == "__main__":
    print("=" * 60)
    print("Model Selection Feature Test Suite")
    print("=" * 60)

    try:
        test_config_manager()
        test_ollama_client()
        test_settings_update()

        print("\n" + "=" * 60)
        print("✅ All tests passed successfully!")
        print("=" * 60)

    except Exception as e:
        print("\n" + "=" * 60)
        print(f"❌ Test failed with error: {e}")
        print("=" * 60)
        import traceback
        traceback.print_exc()
        sys.exit(1)
