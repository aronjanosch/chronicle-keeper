import requests
import json
import subprocess
import time
import logging
from typing import Optional

logger = logging.getLogger(__name__)

class OllamaClient:
    def __init__(self, base_url: str = "http://127.0.0.1:11434", model: str = "llama3.2"):
        """
        Initialize Ollama client
        
        Args:
            base_url: Ollama server URL
            model: Model name to use (default: llama3.2)
        """
        self.base_url = base_url
        self.model = model
        self.api_url = f"{base_url}/api"
        
    def is_server_running(self) -> bool:
        """Check if Ollama server is running"""
        try:
            response = requests.get(f"{self.base_url}/api/tags", timeout=5)
            return response.status_code == 200
        except requests.RequestException:
            return False
    
    def ensure_server_running(self) -> bool:
        """Ensure Ollama server is running, try to start if not"""
        if self.is_server_running():
            return True
            
        logger.info("Ollama server not running, attempting to start...")
        try:
            # Try to start Ollama (assuming it's in PATH)
            subprocess.Popen(
                ["ollama", "serve"], 
                stdout=subprocess.DEVNULL, 
                stderr=subprocess.DEVNULL
            )
            
            # Wait for server to start
            for _ in range(10):  # Wait up to 10 seconds
                time.sleep(1)
                if self.is_server_running():
                    logger.info("Ollama server started successfully")
                    return True
            
            logger.error("Failed to start Ollama server within timeout")
            return False
            
        except FileNotFoundError:
            logger.error("Ollama not found in PATH. Please install Ollama.")
            return False
        except Exception as e:
            logger.error(f"Error starting Ollama: {e}")
            return False
    
    def is_model_available(self) -> bool:
        """Check if the specified model is available"""
        try:
            response = requests.get(f"{self.api_url}/tags")
            if response.status_code == 200:
                models = response.json().get("models", [])
                return any(model["name"].startswith(self.model) for model in models)
            return False
        except requests.RequestException:
            return False
    
    def pull_model(self) -> bool:
        """Pull the model if not available"""
        logger.info(f"Pulling model {self.model}...")
        try:
            response = requests.post(
                f"{self.api_url}/pull",
                json={"name": self.model},
                stream=True,
                timeout=300  # 5 minute timeout for model download
            )
            
            if response.status_code == 200:
                # Stream the download progress
                for line in response.iter_lines():
                    if line:
                        data = json.loads(line)
                        if "status" in data:
                            logger.info(f"Model pull: {data['status']}")
                        if data.get("status") == "success":
                            return True
            
            return False
            
        except requests.RequestException as e:
            logger.error(f"Error pulling model: {e}")
            return False
    
    def ensure_model_ready(self) -> bool:
        """Ensure model is available, pull if necessary"""
        if not self.ensure_server_running():
            return False
            
        if self.is_model_available():
            return True
            
        return self.pull_model()
    
    def generate_summary(self, transcript: str, system_prompt: str) -> str:
        """
        Generate session summary using Ollama
        
        Args:
            transcript: The session transcript
            system_prompt: System prompt for summarization
            
        Returns:
            Generated summary
        """
        if not self.ensure_model_ready():
            raise Exception("Ollama model not available")
        
        # Prepare the prompt
        full_prompt = f"{system_prompt}\n\nTranscript:\n{transcript}"
        
        try:
            response = requests.post(
                f"{self.api_url}/generate",
                json={
                    "model": self.model,
                    "prompt": full_prompt,
                    "stream": False,
                    "options": {
                        "temperature": 0.7,
                        "top_p": 0.9,
                        "max_tokens": 2048
                    }
                },
                timeout=120  # 2 minute timeout
            )
            
            if response.status_code == 200:
                result = response.json()
                return result.get("response", "").strip()
            else:
                raise Exception(f"Ollama API error: {response.status_code}")
                
        except requests.RequestException as e:
            logger.error(f"Error calling Ollama API: {e}")
            raise Exception(f"Failed to generate summary: {str(e)}")
    
    def test_connection(self) -> dict:
        """Test connection and return status info"""
        status = {
            "server_running": self.is_server_running(),
            "model_available": False,
            "error": None
        }
        
        if status["server_running"]:
            status["model_available"] = self.is_model_available()
        else:
            status["error"] = "Ollama server not running"
        
        return status