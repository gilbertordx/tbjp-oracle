from __future__ import annotations

from datetime import datetime, timedelta, timezone

from src.pipeline import process_forum_post
from src.schema import ForumPost
from src.vector_store import TBJPVectorStore


def build_mock_posts() -> list[ForumPost]:
    now = datetime.now(timezone.utc)
    return [
        ForumPost(
            post_id="p001",
            thread_id="t_deadlift_swaps",
            author="coach_j",
            timestamp=now - timedelta(days=2),
            content=(
                "If conventional deadlifts are crushing your recovery, stop forcing them. "
                "Use RDLs as your hinge pattern, and add a 45-degree back extension for "
                "high-rep posterior chain volume. If you still want a heavy pull, run a "
                "Smith machine block pull from just below the knee."
            ),
            tags=["deadlift", "posterior-chain", "exercise-selection"],
        ),
        ForumPost(
            post_id="p002",
            thread_id="t_deadlift_swaps",
            author="coach_j",
            timestamp=now - timedelta(days=1, hours=10),
            content=(
                "For hypertrophy clients, I rarely need conventional deadlifts. "
                "A leg curl + RDL + back extension stack usually beats it for stimulus "
                "with less axial fatigue."
            ),
            parent_post_id="p001",
            tags=["hypertrophy", "fatigue-management"],
        ),
        ForumPost(
            post_id="p003",
            thread_id="t_squat",
            author="user_anon_22",
            timestamp=now - timedelta(days=1),
            content=(
                "My knees cave on hack squat when load gets heavy. "
                "Any cues for bracing and foot pressure?"
            ),
            tags=["hack-squat", "technique"],
        ),
    ]


def main():
    vector_store = TBJPVectorStore()
    documents = []
    for post in build_mock_posts():
        documents.extend(process_forum_post(post))

    vector_store.ingest_documents(documents)
    print(
        f"Seed complete: {len(documents)} chunks indexed "
        f"using backend={vector_store.backend}."
    )


if __name__ == "__main__":
    main()
