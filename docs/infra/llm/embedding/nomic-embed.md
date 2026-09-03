使用方法
重要提示：文本提示必须包含任务指令前缀，以指示模型执行的任务。

例如，如果您正在实现RAG应用程序，您应该将文档嵌入为 search_document: <此处文本> 并将用户查询嵌入为 search_query: <此处文本>。

任务指令前缀
search_document
目的：将文本作为数据集中的文档进行嵌入
此前缀用于将文本作为文档进行嵌入，例如作为RAG索引的文档。

from sentence_transformers import SentenceTransformer

model = SentenceTransformer("nomic-ai/nomic-embed-text-v1", trust_remote_code=True)
sentences = ['search_document: TSNE is a dimensionality reduction algorithm created by Laurens van Der Maaten']
embeddings = model.encode(sentences)
print(embeddings)
search_query
目的：将文本作为需要回答的问题进行嵌入
此前缀用于将文本作为可以从数据集中找到答案的问题进行嵌入，例如作为由RAG应用程序解答的查询。

from sentence_transformers import SentenceTransformer

model = SentenceTransformer("nomic-ai/nomic-embed-text-v1", trust_remote_code=True)
sentences = ['search_query: Who is Laurens van Der Maaten?']
embeddings = model.encode(sentences)
print(embeddings)
clustering
目的：将文本嵌入以便将其分组到集群中
此前缀用于将文本嵌入以将其分组成集群，发现共同主题或移除语义重复项。

from sentence_transformers import SentenceTransformer

model = SentenceTransformer("nomic-ai/nomic-embed-text-v1", trust_remote_code=True)
sentences = ['clustering: the quick brown fox']
embeddings = model.encode(sentences)
print(embeddings)
classification
目的：将文本嵌入以便对其进行分类
此前缀用于将文本嵌入成向量，这些向量将作为分类模型的特征使用。

from sentence_transformers import SentenceTransformer

model = SentenceTransformer("nomic-ai/nomic-embed-text-v1", trust_remote_code=True)
sentences = ['classification: the quick brown fox']
embeddings = model.encode(sentences)
print(embeddings)

-- https://www.modelscope.cn/models/nomic-ai/nomic-embed-text-v1
