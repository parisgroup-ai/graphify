import json
from app.models.user import User

class LLMGateway:
    def __init__(self):
        self.model = "claude"

def call_llm(prompt):
    gateway = LLMGateway()
    return {"response": prompt}
