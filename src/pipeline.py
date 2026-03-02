from langchain_text_splitters import RecursiveCharacterTextSplitter
from langchain_core.documents import Document
from src.schema import ForumPost

def process_forum_post(post: ForumPost) -> list[Document]:
    if not post.content.strip():
        return []

    # Chunking only applied to excessively long individual posts
    splitter = RecursiveCharacterTextSplitter(
        chunk_size=2000, 
        chunk_overlap=200,
        separators=["\n\n", "\n", ".", " "]
    )
    
    chunks = splitter.split_text(post.content)
    
    return [
        Document(
            page_content=chunk,
            metadata={
                "post_id": post.post_id,
                "thread_id": post.thread_id,
                "author": post.author,
                "timestamp": post.timestamp.isoformat()
            }
        ) for chunk in chunks
    ]
