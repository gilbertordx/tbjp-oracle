import json
from src.schema import ForumPost
from src.pipeline import process_forum_post
from src.vector_store import TBJPVectorStore
from datetime import datetime


def parse_timestamp(item: dict) -> datetime | None:
    raw_timestamp = item.get("timestamp")
    if raw_timestamp:
        try:
            return datetime.fromisoformat(raw_timestamp)
        except (TypeError, ValueError):
            pass

    raw_date = item.get("date")
    if raw_date:
        try:
            normalized = " ".join(str(raw_date).split())
            return datetime.strptime(normalized, "%B %d, %Y at %I:%M %p")
        except (TypeError, ValueError):
            pass

    return None


def ingest_json():
    print("Loading JSON data...")
    with open("data/raw_forum_data.json", "r", encoding="utf-8") as f:
        raw_data = json.load(f)

    documents = []
    skipped_posts = 0
    print(f"Parsing {len(raw_data)} posts...")
    for item in raw_data:
        timestamp = parse_timestamp(item)
        if not timestamp:
            skipped_posts += 1
            print(
                "Skipping post due to invalid/missing timestamp and date: "
                f"post_id={item.get('post_id', '')}, date={item.get('date', '')}"
            )
            continue

        post = ForumPost(
            post_id=item["post_id"],
            thread_id=item["thread_id"],
            author=item.get("author", "Jordan Peters"),
            timestamp=timestamp,
            content=item.get("content", ""),
            tags=item.get("tags", [])
        )

        chunks = process_forum_post(post)
        metadata = {
            "post_id": item.get("post_id", ""),
            "thread_id": item.get("thread_id", ""),
            "author": item.get("author", "Jordan Peters"),
            "date": item.get("date", "Unknown Date"),
            "thread_title": item.get("thread_title", "Unknown Thread")
        }
        for chunk in chunks:
            chunk.metadata.update(metadata)
        documents.extend(chunks)

    print(f"Prepared {len(documents)} chunks. Skipped posts: {skipped_posts}")

    print("Initializing Vector Store...")
    db = TBJPVectorStore()

    batch_size = 5000
    total_batches = (len(documents) + batch_size - 1) // batch_size

    print(f"Ingesting in {total_batches} batches...")
    for i in range(0, len(documents), batch_size):
        batch = documents[i:i + batch_size]
        db.ingest_documents(batch)
        print(f"Batch {i//batch_size + 1}/{total_batches} complete.")

    print("Ingestion complete.")

if __name__ == "__main__":
    ingest_json()
