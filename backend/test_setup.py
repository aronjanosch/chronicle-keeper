#!/usr/bin/env python3
"""
Simple test to verify Chronicle Keeper backend setup
"""

def test_imports():
    """Test that all required modules can be imported"""
    try:
        import fastapi
        print("✓ FastAPI imported successfully")
        
        import whisperx
        print("✓ WhisperX imported successfully")
        
        import google.generativeai
        print("✓ Google Generative AI imported successfully")
        
        import requests
        print("✓ Requests imported successfully")
        
        import pydantic
        print("✓ Pydantic imported successfully")
        
        import uvicorn
        print("✓ Uvicorn imported successfully")
        
        print("\n🎉 All core dependencies are available!")
        return True
        
    except ImportError as e:
        print(f"❌ Import error: {e}")
        return False

def test_api_structure():
    """Test that API modules can be imported"""
    import sys
    import os
    
    # Add src to path
    sys.path.insert(0, os.path.join(os.path.dirname(__file__), 'src'))
    
    try:
        from audio.extraction import extract_craig_zip
        print("✓ Audio extraction module imported")
        
        from audio.transcription import WhisperTranscriber
        print("✓ Audio transcription module imported")
        
        from llm.ollama import OllamaClient
        print("✓ Ollama client imported")
        
        from llm.gemini import GeminiClient
        print("✓ Gemini client imported")
        
        from storage.manager import ConfigManager, SessionManager
        print("✓ Storage managers imported")
        
        print("\n🎉 All API modules are working!")
        return True
        
    except ImportError as e:
        print(f"❌ Module import error: {e}")
        return False

if __name__ == "__main__":
    print("Chronicle Keeper Backend Setup Test")
    print("=" * 40)
    
    success = True
    
    print("\n1. Testing core dependencies...")
    success &= test_imports()
    
    print("\n2. Testing API modules...")
    success &= test_api_structure()
    
    if success:
        print("\n✅ Setup test completed successfully!")
        print("Backend is ready to run!")
    else:
        print("\n❌ Setup test failed!")
        print("Please check the error messages above.")