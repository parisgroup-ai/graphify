import os
from app.services.llm import call_llm
from app.models.user import User

def main():
    user = User("test")
    result = call_llm("hello")
    call_llm("again")
    return result
