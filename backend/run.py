#!/usr/bin/env python3
"""
Chronicle Keeper Backend Server
Entry point for running the FastAPI application
"""

import uvicorn
import sys
import os

# Add src to Python path
sys.path.insert(0, os.path.join(os.path.dirname(__file__), 'src'))

if __name__ == "__main__":
    uvicorn.run(
        "src.main:app",
        host="127.0.0.1", 
        port=8000,
        reload=True,
        log_level="info"
    )