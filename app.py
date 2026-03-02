import os
from dotenv import load_dotenv
load_dotenv()

import streamlit as st
from src.vector_store import TBJPVectorStore
from src.llm_engine import TBJPOracleLLM

st.set_page_config(page_title="TBJP Oracle", page_icon="🏋️", layout="centered")

@st.cache_resource
def init_system():
    db = TBJPVectorStore()
    retriever = db.get_retriever(target_results=5)
    qa_chain = TBJPOracleLLM().build_qa_chain(retriever)
    return qa_chain

st.title("TBJP Oracle")
st.markdown("### Structural Bodybuilding & Coaching RAG POC")

try:
    qa_chain = init_system()
except Exception as e:
    st.error(f"Initialization failed: {e}")
    st.stop()

if "messages" not in st.session_state:
    st.session_state.messages = []

for message in st.session_state.messages:
    with st.chat_message(message["role"]):
        st.markdown(message["content"])

if prompt := st.chat_input("Ask the Oracle..."):
    st.session_state.messages.append({"role": "user", "content": prompt})
    with st.chat_message("user"):
        st.markdown(prompt)

    # Compile chat history for the Conversational Retrieval Chain
    chat_history = []
    user_msg = None
    for msg in st.session_state.messages[:-1]:
        if msg["role"] == "user":
            user_msg = msg["content"]
        elif msg["role"] == "assistant" and user_msg:
            chat_history.append((user_msg, msg["content"]))
            user_msg = None

    with st.chat_message("assistant"):
        with st.spinner("Synthesising Jordan's archives with conversational memory..."):
            try:
                response = qa_chain.invoke({"question": prompt, "chat_history": chat_history})
                answer = response.get("answer", "No answer generated.")
                st.markdown(answer)
                st.session_state.messages.append({"role": "assistant", "content": answer})
                
                # Expose the raw data and full metadata directly in the UI
                source_docs = response.get("source_documents", [])
                if source_docs:
                    for doc in source_docs:
                        post_id = doc.metadata.get('post_id', 'Unknown ID')
                        post_date = doc.metadata.get('date', 'Unknown Date')
                        thread_title = doc.metadata.get('thread_title', doc.metadata.get('title', 'Unknown Thread'))
                        
                        with st.expander(f"{post_date} | {thread_title} (ID: {post_id})"):
                            st.markdown(doc.page_content)

            except Exception as e:
                st.error(f"Query failed: {e}")
