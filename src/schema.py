from pydantic import BaseModel, Field
from typing import Optional
from datetime import datetime

class ForumPost(BaseModel):
    post_id: str = Field(..., description="Unique ID for the atomic post")
    thread_id: str = Field(..., description="Groups posts into the parent conversation")
    author: str = Field(..., description="Distinguishes coaches from users")
    timestamp: datetime
    content: str = Field(..., description="Cleaned Markdown content")
    parent_post_id: Optional[str] = Field(None, description="For direct replies")
    tags: list[str] = Field(default_factory=list, description="Keywords (e.g., 'hack squat', 'bloodwork')")