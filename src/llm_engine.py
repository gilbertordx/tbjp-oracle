import os
from dotenv import load_dotenv
from langchain_google_genai import ChatGoogleGenerativeAI
from langchain_core.prompts import PromptTemplate

try:
    from langchain.chains import ConversationalRetrievalChain
except ImportError:
    from langchain_classic.chains import ConversationalRetrievalChain

load_dotenv()


class TBJPOracleLLM:
    def __init__(self):
        self.llm = ChatGoogleGenerativeAI(
            model="gemini-2.5-flash",
            temperature=0.3,
            max_tokens=2048
        )
        self.prompt_template = """
        You are the TBJP Oracle, an AI embodying the exact coaching philosophy and writing style of Jordan Peters (Trained by JP). 
        
        Your tone is highly analytical, deeply educational, and brutally honest about the reality of heavy tissue accrual. You are NOT a generic, shouting drill sergeant. You are a meticulous coach who bases every decision on logbook data, mechanical tension, and internal health markers (digestion, bloodwork). 
        
        When answering, synthesise the provided forum posts seamlessly. Speak like Jordan explaining a complex biomechanical or nutritional concept to a dedicated client. Use British English phrasing.
        
        Use ONLY the following pieces of retrieved forum context to answer the user's question. 
        If the provided context does not contain the answer, state clearly that the data is not in the logbook. Do not guess.

        Context:
        {context}

        Question: {question}
        Answer:
        """

    def build_qa_chain(self, retriever):
        PROMPT = PromptTemplate(
            template=self.prompt_template,
            input_variables=["context", "question"]
        )

        qa_chain = ConversationalRetrievalChain.from_llm(
            llm=self.llm,
            retriever=retriever,
            return_source_documents=True,
            combine_docs_chain_kwargs={"prompt": PROMPT}
        )
        return qa_chain
